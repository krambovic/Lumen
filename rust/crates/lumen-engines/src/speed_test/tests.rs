//! Ports of `windows/tests/test_speed_test_worker.py`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use lumen_core::constants::SPEED_TEST_TIMEOUT;
use lumen_core::models::Node;

use super::*;
use tunnel::{TunnelGetResult, parse_status_line};

fn test_emit() -> EmitFn {
    Arc::new(|_event: SpeedTestEvent| {})
}

fn test_node(id: &str, scheme: &str, outbound: Value) -> Node {
    Node {
        id: id.into(),
        scheme: scheme.into(),
        outbound,
        ..Node::new()
    }
}

#[test]
fn exposes_temporary_core_ids_until_unregistered() -> std::io::Result<()> {
    let worker = SpeedTestWorker::new(Vec::new(), "xray.exe");
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("--list")
        .stdout(std::process::Stdio::null())
        .spawn()?;
    let id = child.id();
    child.wait()?;
    let child = Arc::new(Mutex::new(child));

    worker.register_process(&child);
    assert_eq!(worker.process_ids(), [id]);
    worker.unregister_process(&child);
    assert!(worker.process_ids().is_empty());
    Ok(())
}

#[test]
fn configured_speed_concurrency_is_not_capped_at_six() {
    assert_eq!(resolve_speed_test_concurrency(30, 10), 10);
}

#[test]
fn auto_speed_concurrency_uses_default_ten() {
    assert_eq!(resolve_speed_test_concurrency(30, 0), 10);
}

#[test]
fn speed_concurrency_never_exceeds_node_count() {
    assert_eq!(resolve_speed_test_concurrency(3, 10), 3);
}

#[test]
fn xray_auto_speed_config_preserves_balancer_and_observatory() {
    let full_config = json!({
        "outbounds": [
            {
                "tag": "proxy-1",
                "protocol": "vless",
                "settings": {"vnext": [{"address": "one.example", "port": 443}]},
            },
            {"tag": "direct", "protocol": "freedom"},
        ],
        "observatory": {"subjectSelector": ["proxy"], "probeInterval": "30s"},
        "routing": {
            "balancers": [{"tag": "auto", "selector": ["proxy"], "strategy": {"type": "leastPing"}}],
            "rules": [{"type": "field", "network": "tcp,udp", "balancerTag": "auto"}],
        },
    });
    let node = test_node(
        "auto",
        "auto",
        json!({"protocol": "xray_config", "xray_config": full_config.clone()}),
    );
    let worker = SpeedTestWorker::new(vec![node.clone()], "xray.exe");

    let config = worker
        .build_config(&SpeedTestTarget {
            node: node.clone(),
            http_port: 19080,
        })
        .unwrap();

    assert_eq!(config["observatory"], full_config["observatory"]);
    assert_eq!(
        config["routing"]["balancers"],
        full_config["routing"]["balancers"]
    );
    assert_eq!(
        config["routing"]["rules"],
        json!([{"type": "field", "inboundTag": ["speed-http"], "balancerTag": "auto"}])
    );
    assert_eq!(config["inbounds"][0]["port"], json!(19080));
}

#[test]
fn auto_probe_uses_main_balancer_and_preserves_loopback_rescue() {
    let config = json!({
        "outbounds": [
            {"tag": "proxy-1", "protocol": "vless"},
            {"tag": "rescue", "protocol": "loopback", "settings": {"inboundTag": "rescue-in"}},
            {"tag": "block", "protocol": "blackhole"},
        ],
        "routing": {
            "balancers": [
                {"tag": "main", "selector": ["proxy-"], "fallbackTag": "rescue"},
                {"tag": "backup", "selector": ["proxy-"], "fallbackTag": "block"},
            ],
            "rules": [
                {"type": "field", "inboundTag": ["rescue-in"], "balancerTag": "backup"},
                {"type": "field", "network": "tcp,udp", "balancerTag": "main"},
            ],
        },
    });
    let rescue_rule = config["routing"]["rules"][0].clone();
    let node = test_node(
        "auto",
        "auto",
        json!({"protocol": "xray_config", "xray_config": config}),
    );
    let worker = SpeedTestWorker::new(vec![node.clone()], "xray.exe");
    let result = worker
        .build_config(&SpeedTestTarget {
            node: node.clone(),
            http_port: 19080,
        })
        .unwrap();
    assert_eq!(
        result["routing"]["rules"],
        json!([
            {"type": "field", "inboundTag": ["speed-http"], "balancerTag": "main"},
            rescue_rule,
        ])
    );
    // A freedom fallback bypasses the probe and must be rejected.
    let mut mutated = node.clone();
    mutated.outbound["xray_config"]["outbounds"][2]["protocol"] = json!("freedom");
    let error = worker
        .build_config(&SpeedTestTarget {
            node: mutated,
            http_port: 19080,
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot bypass"), "{error}");
}

/// The Python test monkeypatches `HTTPSConnection`; here the same expectations
/// run against a fake `WarmTunnel` with an injectable clock.
#[test]
fn http_get_excludes_handshakes_and_warmup() {
    let cases: [(bool, u16, i64); 3] = [(false, 204, 40), (true, 204, -1), (false, 302, -1)];
    for (will_close, status, expected) in cases {
        let clock = Arc::new(Mutex::new(0.0f64));
        let node = Node {
            id: "one".into(),
            server: "node.example".into(),
            port: 443,
            ..Node::new()
        };
        let worker = SpeedTestWorker::with_options(
            vec![node.clone()],
            "xray.exe",
            None,
            SPEED_TEST_TIMEOUT,
            "ping",
            "",
            0,
            false,
            "",
            true,
        );
        let emit = test_emit();
        let target = SpeedTestTarget {
            node,
            http_port: 19080,
        };

        let recorded: Arc<Mutex<Option<(String, u16, String, u16)>>> = Arc::new(Mutex::new(None));
        let closed = Arc::new(Mutex::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let mut connector = {
            let clock = Arc::clone(&clock);
            let recorded = Arc::clone(&recorded);
            let closed = Arc::clone(&closed);
            let requests = Arc::clone(&requests);
            move |proxy_host: &str,
                  proxy_port: u16,
                  host: &str,
                  port: u16,
                  _timeout: f64|
                  -> Box<dyn WarmTunnel + Send> {
                *recorded.lock().unwrap() =
                    Some((proxy_host.to_string(), proxy_port, host.to_string(), port));
                Box::new(FakeTunnel {
                    clock: Arc::clone(&clock),
                    requests: Arc::clone(&requests),
                    status,
                    will_close,
                    sock_open: false,
                    closed: Arc::clone(&closed),
                })
            }
        };
        let clock_fn = {
            let clock = Arc::clone(&clock);
            move || *clock.lock().unwrap()
        };

        let mut measure = |target: &SpeedTestTarget| -> i64 {
            worker.warm_http_ping_with(target, &emit, &clock_fn, &mut connector)
        };
        let mut sleep = |_delay: f64| {};
        assert_eq!(
            worker.real_ping_with(&target, &clock_fn, &mut sleep, &mut measure),
            expected,
            "case status={status} will_close={will_close}"
        );
        let recorded = recorded
            .lock()
            .unwrap()
            .clone()
            .expect("one tunnel created");
        assert_eq!(recorded.0, "127.0.0.1");
        assert_eq!(recorded.1, 19080);
        assert_eq!(recorded.2, "www.google.com");
        assert_eq!(recorded.3, 443);
        assert!(*closed.lock().unwrap(), "tunnel closed");
        assert!(worker.registry_is_empty(), "response registry drained");
        let requests = requests.load(Ordering::SeqCst);
        assert_eq!(requests, if expected > 0 { 3 } else { 1 });
    }
}

struct FakeTunnel {
    clock: Arc<Mutex<f64>>,
    requests: Arc<AtomicUsize>,
    status: u16,
    will_close: bool,
    sock_open: bool,
    closed: Arc<Mutex<bool>>,
}

impl WarmTunnel for FakeTunnel {
    fn connect(&mut self) -> anyhow::Result<()> {
        // CONNECT and TLS never enter the measured samples.
        *self.clock.lock().unwrap() += 2.0;
        self.sock_open = true;
        Ok(())
    }

    fn sock_closed(&self) -> bool {
        !self.sock_open
    }

    fn get(&mut self, path: &str) -> anyhow::Result<TunnelGetResult> {
        assert_eq!(path, "/generate_204");
        assert!(self.sock_open);
        let durations = [0.9, 0.06, 0.04];
        let index = self.requests.fetch_add(1, Ordering::SeqCst);
        *self.clock.lock().unwrap() += durations.get(index).copied().unwrap_or(0.04);
        Ok(TunnelGetResult {
            status: self.status,
            will_close: self.will_close,
            fully_consumed: true,
        })
    }

    fn close(&mut self) {
        *self.closed.lock().unwrap() = true;
    }
}

/// Python `test_auto_ping_retries_observer_startup_outside_measurement`.
#[test]
fn auto_ping_retries_observer_startup_outside_measurement() {
    let node = test_node("auto", "auto", json!({"protocol": "xray_config"}));
    let worker = SpeedTestWorker::with_options(
        vec![node.clone()],
        "xray.exe",
        None,
        SPEED_TEST_TIMEOUT,
        "ping",
        "",
        0,
        false,
        "",
        true,
    );
    let clock = Arc::new(Mutex::new(0.0f64));
    let clock_fn = {
        let clock = Arc::clone(&clock);
        move || *clock.lock().unwrap()
    };
    let results = Mutex::new(vec![-1i64, -1, 45]);
    let mut measure = |_: &SpeedTestTarget| -> i64 {
        let mut queue = results.lock().unwrap();
        if queue.is_empty() {
            -1
        } else {
            queue.remove(0)
        }
    };
    let sleep_clock = Arc::clone(&clock);
    let mut sleep = move |delay: f64| *sleep_clock.lock().unwrap() += delay;
    let target = SpeedTestTarget {
        node,
        http_port: 19080,
    };

    assert_eq!(
        worker.real_ping_with(&target, &clock_fn, &mut sleep, &mut measure),
        45
    );
    let elapsed = *clock.lock().unwrap();
    assert!((elapsed - 0.4).abs() < 1e-9, "clock {elapsed}");

    let mut measure = |_: &SpeedTestTarget| -> i64 { -1 };
    assert_eq!(
        worker.real_ping_with(&target, &clock_fn, &mut sleep, &mut measure),
        -1
    );
    let elapsed = *clock.lock().unwrap();
    assert!((12.0..=12.7).contains(&elapsed), "clock {elapsed}");
}

#[test]
fn profile_hosts_walks_the_outbound_payload() {
    let mut node = test_node(
        "n",
        "vless",
        json!({
            "protocol": "vless",
            "settings": {"vnext": [{"address": "a.example", "port": 443}]},
            "streamSettings": {"wsSettings": {"headers": {"Host": "cdn.example"}}},
        }),
    );
    node.server = "node.example".into();
    // Full-profile nodes never fall back to endpoint ping against node.server.
    let mut full = node.clone();
    full.outbound = json!({"protocol": "xray_config"});
    full.server = "fallback.example".into();
    assert_eq!(profile_hosts(&full), Vec::<String>::new());

    let hosts = profile_hosts(&node);
    assert_eq!(hosts, vec!["a.example", "node.example"]);
}

#[test]
fn tunnel_status_line_parsing_matches_http_client() {
    assert_eq!(
        parse_status_line("HTTP/1.1 200 Connection established\r\n").unwrap(),
        200
    );
    assert_eq!(parse_status_line("HTTP/1.0 204 No Content\n").unwrap(), 204);
    assert!(parse_status_line("garbage").is_err());
}
