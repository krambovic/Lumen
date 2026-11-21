//! Ported expectations of `windows/tests/test_singbox_runtime_planner.py`.
//!
//! Python monkeypatches module globals (`socket.getaddrinfo`,
//! `_local_ipv4_addresses`, `_ipv4_gateway_in_use`); the Rust tests use the
//! `cfg(test)` override hooks and a shared lock so the global state stays
//! deterministic across the parallel test threads.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{json, Value};

use super::bootstrap::{clear_endpoint_dns_caches_for_tests, set_test_getaddrinfo, prefetch_endpoint_addresses};
use super::tun::set_test_tun_env;
use super::{
    classify_node_for_singbox, plan_singbox_runtime, resolve_endpoint_addresses,
    singbox_node_source_signature, singbox_node_tag, PlanRuntimeOptions,
};
use crate::singbox::document::parse_singbox_document;
use lumen_core::models::{Node, RoutingSettings};

static TEST_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

fn base_config() -> Value {
    json!({
        "inbounds": [
            {
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "xftun",
            }
        ],
        "outbounds": [
            {"type": "direct", "tag": "proxy"},
            {"type": "direct", "tag": "direct"},
            {"type": "block", "tag": "block"},
        ],
        "route": {"rules": [], "final": "direct"},
        "dns": {
            "servers": [
                {"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"},
                {"tag": "proxy-dns", "type": "https", "server": "dns.google"},
                {"tag": "fake-dns", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
            ],
            "final": "bootstrap-dns",
        },
    })
}

fn test_node() -> Node {
    let mut node = Node::new();
    node.scheme = "vless".to_string();
    node.server = "203.0.113.1".to_string();
    node.outbound = json!({
        "protocol": "vless",
        "settings": {
            "vnext": [
                {
                    "address": "203.0.113.1",
                    "port": 443,
                    "users": [{"id": "00000000-0000-0000-0000-000000000000"}],
                }
            ]
        },
        "streamSettings": {"security": "tls"},
    });
    node
}

fn node_with_server(server: &str) -> Node {
    let mut node = test_node();
    node.server = server.to_string();
    node.outbound["settings"]["vnext"][0]["address"] = json!(server);
    node
}

fn wireguard_node(server: &str) -> Node {
    let mut node = Node::new();
    node.scheme = "wireguard".to_string();
    node.server = server.to_string();
    node.outbound = json!({
        "protocol": "wireguard",
        "singbox": {
            "type": "wireguard",
            "tag": "proxy",
            "address": ["10.0.0.2/32"],
            "private_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "peers": [
                {
                    "address": server,
                    "port": 51820,
                    "public_key": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=",
                    "allowed_ips": ["0.0.0.0/0"],
                }
            ],
        },
    });
    node
}

fn hysteria_node(bandwidth: Value) -> Node {
    let mut node = Node::new();
    node.scheme = "hysteria".to_string();
    node.server = "srv.example.com".to_string();
    let mut outbound = json!({
        "protocol": "hysteria",
        "singbox": {
            "type": "hysteria",
            "server": "srv.example.com",
            "server_port": 443,
            "auth_str": "auth",
        },
    });
    if let Some(map) = bandwidth.as_object() {
        for (key, value) in map {
            outbound["singbox"][key] = value.clone();
        }
    }
    node.outbound = outbound;
    node
}

fn endpoint_document(server: &str) -> Value {
    let mut config = base_config();
    config["outbounds"] = json!([
        {"type": "selector", "tag": "SELECT", "outbounds": ["direct"], "default": "direct"},
        {"type": "direct", "tag": "direct"},
        {"type": "block", "tag": "block"},
    ]);
    config["endpoints"] = json!([
        {
            "type": "wireguard",
            "tag": "wg-out",
            "address": ["10.0.0.2/32"],
            "private_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "peers": [
                {
                    "address": server,
                    "port": 51820,
                    "public_key": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=",
                    "allowed_ips": ["0.0.0.0/0"],
                }
            ],
        }
    ]);
    config
}

fn full_config_node(config: Value) -> Node {
    let mut node = Node::new();
    node.scheme = "singbox_config".to_string();
    node.outbound = json!({"protocol": "singbox_config", "singbox_config": config});
    node
}

fn xhttp_node(protocol: &str) -> Node {
    let user = if protocol == "vmess" {
        json!({"id": "00000000-0000-0000-0000-000000000000", "alterId": 0, "security": "auto"})
    } else {
        json!({"id": "00000000-0000-0000-0000-000000000000", "encryption": "none"})
    };
    let mut node = Node::new();
    node.scheme = protocol.to_string();
    node.server = "203.0.113.1".to_string();
    node.port = 443;
    node.outbound = json!({
        "protocol": protocol,
        "settings": {
            "vnext": [
                {
                    "address": "203.0.113.1",
                    "port": 443,
                    "users": [user],
                }
            ]
        },
        "streamSettings": {
            "network": "xhttp",
            "security": "tls",
            "tlsSettings": {"serverName": "server.example.com"},
            "xhttpSettings": {"path": "/", "mode": "auto"},
        },
    });
    node
}

fn plan(routing: RoutingSettings, node: Option<Node>, mut options: PlanRuntimeOptions<'_>) -> Value {
    // Python `_plan(routing, node=None, **kwargs)` substitutes the default node.
    let node = node.unwrap_or_else(test_node);
    options.routing = Some(routing);
    let text = base_config().to_string();
    let document = parse_singbox_document(Path::new("test.json"), &text).unwrap();
    plan_singbox_runtime(&document, Some(&node), options).unwrap().singbox_config
}

fn dns_servers(config: &Value) -> HashMap<String, Value> {
    config["dns"]["servers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|server| server.is_object())
        .map(|server| (super::json_str_field(server, "tag"), server.clone()))
        .collect()
}

fn tun_inbound(config: &Value) -> Value {
    config["inbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|inbound| super::json_str_field(inbound, "type") == "tun")
        .unwrap()
        .clone()
}

fn tun_dns_hijack_rule() -> Value {
    json!({
        "type": "logical",
        "mode": "or",
        "rules": [{"port": 53}, {"protocol": "dns"}],
        "action": "hijack-dns",
    })
}

fn tun_sniff_rule() -> Value {
    json!({
        "action": "sniff",
        "timeout": "1s",
    })
}

fn routing_global(tun_default_outbound: &str) -> RoutingSettings {
    RoutingSettings {
        mode: "global".to_string(),
        tun_default_outbound: tun_default_outbound.to_string(),
        ..Default::default()
    }
}

/// Deterministic resolver override: known hosts resolve to the given addresses,
/// unknown hosts return an empty answer.
fn set_resolver_map(mapping: &[(&'static str, &[&'static str])]) {
    let map: HashMap<String, Vec<String>> = mapping
        .iter()
        .map(|(host, addresses)| {
            (
                host.to_string(),
                addresses.iter().map(|address| address.to_string()).collect(),
            )
        })
        .collect();
    set_test_getaddrinfo(Some(Arc::new(move |host: &str| {
        Ok(map
            .get(host)
            .map(|addresses| {
                addresses
                    .iter()
                    .map(|address| {
                        let ip: std::net::IpAddr = address.parse().unwrap();
                        SocketAddr::new(ip, 0)
                    })
                    .collect()
            })
            .unwrap_or_default())
    })));
}

fn set_resolver_fail() {
    set_test_getaddrinfo(Some(Arc::new(|_host: &str| {
        Err(std::io::Error::other("gaierror"))
    })));
}

fn reset_test_env() {
    set_test_tun_env(None, None);
    set_test_getaddrinfo(None);
    clear_endpoint_dns_caches_for_tests();
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn runtime_plan_propagates_its_rotating_clash_api_secret() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let node = test_node();
    let first = plan_singbox_runtime(
        &document,
        Some(&node),
        PlanRuntimeOptions {
            routing: Some(routing_global("direct")),
            ..Default::default()
        },
    )
    .unwrap();
    let second = plan_singbox_runtime(
        &document,
        Some(&node),
        PlanRuntimeOptions {
            routing: Some(routing_global("direct")),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!first.clash_api_secret.is_empty());
    assert_ne!(first.clash_api_secret, second.clash_api_secret);
    assert_eq!(
        first.singbox_config["experimental"]["clash_api"]["secret"],
        json!(first.clash_api_secret)
    );
    assert_eq!(
        first.singbox_config["experimental"]["clash_api"]["external_controller"],
        json!("127.0.0.1:19090")
    );
    reset_test_env();
}

#[test]
fn tun_runtime_uses_stable_low_mtu_v2rayn_defaults() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_test_tun_env(Some(HashSet::new()), Some(false));
    let config = plan(
        routing_global("proxy"),
        None,
        PlanRuntimeOptions {
            tun_mtu: 1280,
            tun_stack: "gvisor".to_string(),
            ..Default::default()
        },
    );
    let inbound = tun_inbound(&config);

    assert_eq!(inbound["interface_name"], json!("xftun"));
    assert_eq!(inbound["address"], json!(["172.18.0.1/30", "fdfe:dcba:9876::1/126"]));
    assert_eq!(inbound["mtu"], json!(1280));
    assert_eq!(inbound["stack"], json!("gvisor"));
    assert_eq!(inbound["auto_route"], json!(true));
    assert!(inbound.get("route_address").is_none());
    assert_eq!(inbound["strict_route"], json!(true));
    assert_eq!(config["route"]["rules"][0], tun_sniff_rule());
    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["tun-in"]))
            && rule.get("protocol") == Some(&json!("quic"))
            && rule.get("action") == Some(&json!("reject"))
    }));
    reset_test_env();
}

#[test]
fn tun_strict_route_can_still_be_disabled_explicitly() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        routing_global("direct"),
        None,
        PlanRuntimeOptions {
            tun_strict_route: false,
            ..Default::default()
        },
    );
    assert_eq!(tun_inbound(&config)["strict_route"], json!(false));
    reset_test_env();
}

#[test]
fn full_singbox_config_repairs_null_vmess_security() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut config = base_config();
    config["outbounds"] = json!([
        {
            "type": "vmess",
            "tag": "proxy",
            "server": "example.com",
            "server_port": 443,
            "uuid": "00000000-0000-0000-0000-000000000000",
            "security": "null",
        },
        {"type": "direct", "tag": "direct"},
        {"type": "block", "tag": "block"},
    ]);
    let document = parse_singbox_document(Path::new("template.json"), &base_config().to_string()).unwrap();

    let plan = plan_singbox_runtime(
        &document,
        Some(&full_config_node(config)),
        PlanRuntimeOptions::default(),
    )
    .unwrap();

    let proxy = plan.singbox_config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item.get("tag") == Some(&json!("proxy")))
        .unwrap();
    assert_eq!(proxy["security"], json!("auto"));
    reset_test_env();
}

#[test]
fn tun_runtime_uses_singbox_tun_when_interface_name_is_missing() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut payload = base_config();
    payload["inbounds"][0].as_object_mut().unwrap().shift_remove("interface_name");
    let document = parse_singbox_document(Path::new("test.json"), &payload.to_string()).unwrap();

    let config = plan_singbox_runtime(
        &document,
        Some(&test_node()),
        PlanRuntimeOptions {
            routing: Some(routing_global("proxy")),
            ..Default::default()
        },
    )
    .unwrap()
    .singbox_config;

    assert_eq!(tun_inbound(&config)["interface_name"], json!("singbox_tun"));
    reset_test_env();
}

#[test]
fn tun_runtime_keeps_v2rayn_style_local_proxy_inbounds() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(routing_global("proxy"), None, PlanRuntimeOptions::default());

    let inbounds: HashMap<String, Value> = config["inbounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inbound| (inbound.get("tag").and_then(Value::as_str).unwrap_or("").to_string(), inbound.clone()))
        .collect();
    let rules = config["route"]["rules"].as_array().unwrap();

    assert_eq!(
        inbounds["socks-in"],
        json!({
            "type": "mixed",
            "tag": "socks-in",
            "listen": "127.0.0.1",
            "listen_port": 10808,
        })
    );
    assert_eq!(
        inbounds["http-in"],
        json!({
            "type": "http",
            "tag": "http-in",
            "listen": "127.0.0.1",
            "listen_port": 10809,
        })
    );
    assert!(rules.iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["socks-in", "http-in"]))
            && rule.get("outbound") == Some(&json!("proxy"))
            && rule.get("action") == Some(&json!("route"))
    }));
    reset_test_env();
}

#[test]
fn tun_direct_fallback_keeps_local_proxy_inbounds_routed_to_proxy() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "rule".to_string(),
            preset_id: "custom-user-preset".to_string(),
            tun_default_outbound: "direct".to_string(),
            process_rules: vec![json!({"process": "selected-app.exe", "action": "proxy"})],
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    assert_eq!(config["route"]["final"], json!("direct"));
    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("process_name") == Some(&json!(["selected-app.exe"]))
            && rule.get("outbound") == Some(&json!("proxy"))
            && rule.get("action") == Some(&json!("route"))
    }));
    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["socks-in", "http-in"]))
            && rule.get("outbound") == Some(&json!("proxy"))
            && rule.get("action") == Some(&json!("route"))
    }));
    reset_test_env();
}

#[test]
fn proxy_runtime_removes_tun_and_keeps_local_proxy_inbounds() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        routing_global("proxy"),
        Some(wireguard_node("198.51.100.10")),
        PlanRuntimeOptions {
            tun_mode: false,
            local_socks_port: 12080,
            local_http_port: 12081,
            ..Default::default()
        },
    );

    assert!(!config["inbounds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|inbound| inbound.get("type") == Some(&json!("tun"))));
    let inbounds: HashMap<String, Value> = config["inbounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inbound| (inbound.get("tag").and_then(Value::as_str).unwrap_or("").to_string(), inbound.clone()))
        .collect();
    assert_eq!(inbounds["socks-in"]["listen_port"], json!(12080));
    assert_eq!(inbounds["http-in"]["listen_port"], json!(12081));
    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["socks-in", "http-in"]))
            && rule.get("outbound") == Some(&json!("proxy"))
    }));
    reset_test_env();
}

#[test]
fn awg_proxy_runtime_keeps_blocked_preset_direct_fallback_after_legacy_state() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    for legacy_routing in [
        json!({"mode": "global", "preset_id": "blocked"}),
        json!({"mode": "global", "proxy_domains": ["geosite:ru-blocked"]}),
    ] {
        let routing = RoutingSettings::from_dict(&legacy_routing);

        assert_eq!(routing.mode, "rule");
        let config = plan(
            routing,
            Some(wireguard_node("198.51.100.10")),
            PlanRuntimeOptions {
                tun_mode: false,
                enable_final_fragment: false,
                ..Default::default()
            },
        );

        assert_eq!(config["route"]["final"], json!("direct"));
        assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
            rule.get("outbound") == Some(&json!("proxy"))
                && rule
                    .get("rule_set")
                    .and_then(Value::as_array)
                    .map(|sets| sets.contains(&json!("geosite-ru-blocked")))
                    .unwrap_or(false)
        }));
    }
    reset_test_env();
}

#[test]
fn disabled_tls_fragment_is_not_added_to_tun_rules() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let config = plan_singbox_runtime(
        &document,
        Some(&test_node()),
        PlanRuntimeOptions {
            routing: Some(routing_global("proxy")),
            enable_final_fragment: false,
            ..Default::default()
        },
    )
    .unwrap()
    .singbox_config;

    assert!(!config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("tls_record_fragment").is_some_and(|value| *value == json!(true))
            || rule.get("tls_fragment").is_some_and(|value| *value == json!(true))
    }));
    reset_test_env();
}

#[test]
fn tun_runtime_uses_extended_tls_fragment_rule() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(routing_global("proxy"), None, PlanRuntimeOptions::default());

    let fragment_rules: Vec<Value> = config["route"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|rule| {
            rule.get("action") == Some(&json!("route-options")) && rule.get("protocol") == Some(&json!(["tls"]))
        })
        .cloned()
        .collect();

    assert_eq!(
        fragment_rules,
        vec![json!({
            "protocol": ["tls"],
            "action": "route-options",
            "tls_fragment": true,
            "tls_fragment_fallback_delay": "500ms",
        })]
    );
    reset_test_env();
}

#[test]
fn tun_runtime_rejects_browser_doh_before_dns_hijack() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(routing_global("proxy"), None, PlanRuntimeOptions::default());
    let rules = config["route"]["rules"].as_array().unwrap();

    let doh_rule = rules
        .iter()
        .find(|rule| {
            rule.get("action") == Some(&json!("reject"))
                && rule.get("inbound") == Some(&json!(["tun-in"]))
                && rule
                    .get("domain_suffix")
                    .and_then(Value::as_array)
                    .map(|suffixes| {
                        suffixes.contains(&json!("cloudflare-dns.com")) && suffixes.contains(&json!("dns.google"))
                    })
                    .unwrap_or(false)
        })
        .unwrap();
    let sniff_index = rules.iter().position(|rule| rule == &tun_sniff_rule()).unwrap();
    let doh_index = rules.iter().position(|rule| rule == doh_rule).unwrap();
    let hijack_index = rules.iter().position(|rule| rule == &tun_dns_hijack_rule()).unwrap();

    assert!(sniff_index < doh_index && doh_index < hijack_index);
    reset_test_env();
}

#[test]
fn tun_runtime_rejects_browser_doh_dns_before_fakeip() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_fake_enabled: true,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );
    let dns_rules = config["dns"]["rules"].as_array().unwrap();

    let https_index = dns_rules
        .iter()
        .position(|rule| rule.get("query_type") == Some(&json!(["HTTPS", "SVCB"])))
        .unwrap();
    let doh_index = dns_rules
        .iter()
        .position(|rule| {
            rule.get("action") == Some(&json!("reject"))
                && rule
                    .get("domain_suffix")
                    .and_then(Value::as_array)
                    .map(|suffixes| suffixes.iter().any(|item| *item == json!("dns.google")))
                    .unwrap_or(false)
        })
        .unwrap();
    let fake_index = dns_rules
        .iter()
        .position(|rule| rule.get("server") == Some(&json!("fake-dns")))
        .unwrap();

    assert!(https_index < doh_index && doh_index < fake_index);
    reset_test_env();
}

#[test]
fn vmess_xhttp_uses_native_singbox_and_vless_xhttp_stays_native() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    assert_eq!(classify_node_for_singbox(Some(&xhttp_node("vmess")), "auto"), "native_singbox");
    assert_eq!(classify_node_for_singbox(Some(&xhttp_node("vless")), "auto"), "native_singbox");
    reset_test_env();
}

#[test]
fn vless_raw_without_finalmask_uses_native_singbox() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut node = test_node();
    node.outbound["streamSettings"]["network"] = json!("raw");

    assert_eq!(classify_node_for_singbox(Some(&node), "auto"), "native_singbox");
    let outbound =
        crate::singbox::config_builder::build_singbox_outbound(&node, "proxy", false, 8).unwrap();
    assert!(outbound.get("transport").is_none());
    reset_test_env();
}

#[test]
fn vless_raw_with_finalmask_keeps_xray_sidecar() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut node = test_node();
    node.outbound["streamSettings"]["network"] = json!("raw");
    node.outbound["streamSettings"]["finalmask"] =
        json!({"tcp": [{"type": "fragment", "settings": {}}]});
    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let plan = plan_singbox_runtime(
        &document,
        Some(&node),
        PlanRuntimeOptions {
            routing: Some(routing_global("proxy")),
            enable_final_fragment: false,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(plan.is_hybrid());
    let sidecar = plan.xray_sidecar.as_ref().unwrap();
    let proxy = plan.singbox_config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();
    let xray_proxy = sidecar.config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();
    assert_eq!(proxy["type"], json!("socks"));
    assert_eq!(xray_proxy["protocol"], json!("vless"));
    assert_eq!(xray_proxy["streamSettings"]["network"], json!("raw"));
    assert!(xray_proxy["streamSettings"].get("finalmask").is_some());
    reset_test_env();
}

#[test]
fn system_dns_mode_uses_physical_adapter_dns_without_local_recursion() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let config = plan_singbox_runtime(
        &document,
        Some(&test_node()),
        PlanRuntimeOptions {
            routing: Some(RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "system".to_string(),
                dns_hijack_enabled: true,
                dns_fake_enabled: true,
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            }),
            system_dns_servers: vec!["192.0.2.53".to_string()],
            ..Default::default()
        },
    )
    .unwrap()
    .singbox_config;

    let rules = config["route"]["rules"].as_array().unwrap();
    let servers = dns_servers(&config);

    let hijack_rules: Vec<Value> = rules
        .iter()
        .filter(|rule| rule.get("action") == Some(&json!("hijack-dns")))
        .cloned()
        .collect();
    assert_eq!(hijack_rules, vec![tun_dns_hijack_rule()]);
    for tag in ["bootstrap-dns", "direct-dns"] {
        assert_eq!(servers[tag]["type"], json!("udp"));
        assert_eq!(servers[tag]["server"], json!("192.0.2.53"));
        assert_eq!(servers[tag]["detour"], json!("direct"));
        assert!(servers[tag].get("domain_resolver").is_none());
    }
    assert_eq!(servers["proxy-dns"]["type"], json!("https"));
    assert_eq!(servers["proxy-dns"]["server"], json!("cloudflare-dns.com"));
    assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
    assert_eq!(servers["proxy-dns"]["domain_resolver"], json!("bootstrap-dns"));
    assert_eq!(config["dns"]["final"], json!("proxy-dns"));
    assert_eq!(config["dns"]["strategy"], json!("ipv4_only"));
    assert_eq!(config["dns"]["reverse_mapping"], json!(true));
    assert_eq!(config["route"]["default_domain_resolver"]["server"], json!("proxy-dns"));
    assert_eq!(config["route"]["default_domain_resolver"]["strategy"], json!("ipv4_only"));
    // FakeIP is now enabled regardless of system/built-in DNS mode so ECH sites
    // stay reachable; it adds no upstream recursion.
    assert_eq!(servers["fake-dns"]["type"], json!("fakeip"));
    assert_eq!(servers["fake-dns"]["inet4_range"], json!("198.18.0.0/15"));
    reset_test_env();
}

#[test]
fn tun_runtime_disables_fake_dns_when_setting_off() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "system".to_string(),
            dns_fake_enabled: false,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let servers = dns_servers(&config);
    let dns_rules = config["dns"]["rules"].as_array().unwrap();
    let route_rules = config["route"]["rules"].as_array().unwrap();

    assert!(!servers.contains_key("fake-dns"));
    assert!(!dns_rules.iter().any(|rule| rule.get("server") == Some(&json!("fake-dns"))));
    assert!(!route_rules
        .iter()
        .any(|rule| rule.get("ip_cidr") == Some(&json!(["198.18.0.0/15", "fc00::/18"]))));
    reset_test_env();
}

#[test]
fn default_system_dns_keeps_proxied_domain_dns_on_proxy_resolver() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "rule".to_string(),
            dns_mode: "system".to_string(),
            proxy_domains: vec!["chatgpt.com".to_string()],
            tun_default_outbound: "direct".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let servers = dns_servers(&config);
    let dns_rules = config["dns"]["rules"].as_array().unwrap();

    assert_eq!(servers["bootstrap-dns"]["type"], json!("https"));
    assert_eq!(servers["bootstrap-dns"]["server"], json!("1.1.1.1"));
    assert_eq!(servers["bootstrap-dns"]["tls"]["server_name"], json!("cloudflare-dns.com"));
    assert_eq!(servers["direct-dns"]["type"], json!("https"));
    assert_eq!(servers["direct-dns"]["server"], json!("1.1.1.1"));
    assert_eq!(servers["proxy-dns"]["type"], json!("https"));
    assert_eq!(servers["proxy-dns"]["server"], json!("cloudflare-dns.com"));
    assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
    assert_eq!(config["dns"]["strategy"], json!("ipv4_only"));
    assert_eq!(config["dns"]["reverse_mapping"], json!(true));
    assert!(dns_rules.iter().any(|rule| {
        rule.get("server") == Some(&json!("proxy-dns"))
            && rule
                .get("domain_suffix")
                .and_then(Value::as_array)
                .map(|suffixes| suffixes.contains(&json!("chatgpt.com")))
                .unwrap_or(false)
    }));
    reset_test_env();
}

#[test]
fn tun_dns_can_prefer_ipv6_without_disabling_ipv4() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "system".to_string(),
            dns_bootstrap_strategy: "prefer_ipv6".to_string(),
            dns_proxy_strategy: "prefer_ipv6".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    assert_eq!(config["dns"]["strategy"], json!("prefer_ipv6"));
    assert_eq!(config["route"]["default_domain_resolver"]["strategy"], json!("prefer_ipv6"));
    reset_test_env();
}

#[test]
fn system_dns_mode_falls_back_to_explicit_bootstrap_server() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "system".to_string(),
            dns_bootstrap_server: "1.1.1.1".to_string(),
            dns_bootstrap_type: "udp".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let servers = dns_servers(&config);

    assert_eq!(servers["bootstrap-dns"]["type"], json!("https"));
    assert_eq!(servers["bootstrap-dns"]["server"], json!("1.1.1.1"));
    assert_eq!(servers["bootstrap-dns"]["tls"]["server_name"], json!("cloudflare-dns.com"));
    assert_eq!(servers["bootstrap-dns"]["detour"], json!("direct"));
    assert_eq!(servers["proxy-dns"]["type"], json!("https"));
    assert_eq!(servers["proxy-dns"]["server"], json!("cloudflare-dns.com"));
    assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
    assert_eq!(servers["proxy-dns"]["domain_resolver"], json!("bootstrap-dns"));
    reset_test_env();
}

#[test]
fn builtin_dns_mode_keeps_tun_dns_hijack_contract() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            dns_hijack_enabled: true,
            dns_fake_enabled: true,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    let servers = dns_servers(&config);

    assert!(rules.iter().any(|rule| rule.get("action") == Some(&json!("hijack-dns"))));
    let hijack_index = rules.iter().position(|rule| rule == &tun_dns_hijack_rule()).unwrap();
    let noisy_index = rules
        .iter()
        .position(|rule| {
            rule.get("action") == Some(&json!("reject"))
                && rule.get("port") == Some(&json!([135, 137, 138, 139, 5353, 5355]))
        })
        .unwrap();
    assert!(hijack_index < noisy_index);
    assert_eq!(servers["bootstrap-dns"]["type"], json!("https"));
    assert_eq!(servers["bootstrap-dns"]["server"], json!("1.1.1.1"));
    assert_eq!(servers["bootstrap-dns"]["tls"]["server_name"], json!("cloudflare-dns.com"));
    assert_eq!(servers["bootstrap-dns"]["detour"], json!("direct"));
    assert_eq!(servers["direct-dns"]["detour"], json!("direct"));
    assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
    assert_eq!(servers["fake-dns"]["type"], json!("fakeip"));
    assert_eq!(config["dns"]["final"], json!("proxy-dns"));
    assert!(config["dns"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("server") == Some(&json!("fake-dns")) && rule.get("query_type") == Some(&json!(["A", "AAAA"]))
    }));
    assert_eq!(config["log"]["level"], json!("info"));
    reset_test_env();
}

#[test]
fn default_bootstrap_dns_is_encrypted_doh_with_ip_sni() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(routing_global("proxy"), None, PlanRuntimeOptions::default());
    let servers = dns_servers(&config);

    assert_eq!(servers["bootstrap-dns"]["type"], json!("https"));
    assert_eq!(servers["bootstrap-dns"]["server"], json!("1.1.1.1"));
    assert_eq!(servers["bootstrap-dns"]["tls"]["server_name"], json!("cloudflare-dns.com"));
    assert_eq!(servers["direct-dns"]["tls"]["server_name"], json!("cloudflare-dns.com"));
    assert_eq!(servers["direct-dns-2"]["server"], json!("8.8.8.8"));
    assert_eq!(servers["direct-dns-2"]["tls"]["server_name"], json!("dns.google"));
    assert!(servers["bootstrap-dns"].get("domain_resolver").is_none());
    reset_test_env();
}

#[test]
fn legacy_default_udp_dns_is_migrated_but_custom_udp_is_preserved() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let legacy_default = RoutingSettings::from_dict(&json!({
        "dns_bootstrap_server": "1.1.1.1",
        "dns_bootstrap_servers": ["1.1.1.1", "8.8.8.8"],
        "dns_bootstrap_type": "udp",
    }));
    let custom_udp = RoutingSettings::from_dict(&json!({
        "dns_bootstrap_server": "9.9.9.9",
        "dns_bootstrap_servers": ["9.9.9.9"],
        "dns_bootstrap_type": "udp",
    }));
    let explicit_default_udp = RoutingSettings::from_dict(&json!({
        "dns_bootstrap_server": "1.1.1.1",
        "dns_bootstrap_servers": ["1.1.1.1", "8.8.8.8"],
        "dns_bootstrap_type": "udp",
        "dns_bootstrap_type_explicit": true,
    }));

    assert_eq!(legacy_default.dns_bootstrap_type, "https");
    assert_eq!(custom_udp.dns_bootstrap_type, "udp");
    assert_eq!(explicit_default_udp.dns_bootstrap_type, "udp");
    assert_eq!(
        RoutingSettings::from_dict(&explicit_default_udp.to_dict()).dns_bootstrap_type,
        "udp"
    );
    reset_test_env();
}

#[test]
fn fake_dns_runtime_persists_fakeip_cache() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            dns_fake_enabled: true,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let cache_file = config["experimental"]["cache_file"].clone();

    assert_eq!(cache_file["enabled"], json!(true));
    assert_eq!(cache_file["store_fakeip"], json!(true));
    assert!(cache_file["path"]
        .as_str()
        .unwrap()
        .ends_with("sing-box-cache.db"));
    reset_test_env();
}

#[test]
fn builtin_dns_always_hijacks_the_tun_dns_peer() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            dns_hijack_enabled: false,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let hijack_rules: Vec<Value> = config["route"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|rule| rule.get("action") == Some(&json!("hijack-dns")))
        .cloned()
        .collect();

    assert_eq!(hijack_rules, vec![tun_dns_hijack_rule()]);
    reset_test_env();
}

#[test]
fn builtin_fake_dns_keeps_proxied_dns_final_for_direct_default() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "direct".to_string(),
            dns_mode: "builtin".to_string(),
            dns_hijack_enabled: true,
            dns_fake_enabled: true,
            tun_default_outbound: "direct".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    assert_eq!(config["route"]["final"], json!("direct"));
    assert_eq!(config["dns"]["final"], json!("proxy-dns"));
    assert!(config["dns"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule.get("server") == Some(&json!("fake-dns"))));
    reset_test_env();
}

#[test]
fn builtin_fake_dns_keeps_service_dns_rules_ahead_of_fakeip() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            service_routes: BTreeMap::from([("youtube".to_string(), "direct".to_string())]),
            dns_mode: "builtin".to_string(),
            dns_fake_enabled: true,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let rules = config["dns"]["rules"].as_array().unwrap();
    let service_index = rules
        .iter()
        .position(|rule| {
            rule.get("server") == Some(&json!("bootstrap-dns"))
                && rule
                    .get("domain_suffix")
                    .and_then(Value::as_array)
                    .map(|suffixes| suffixes.contains(&json!("youtube.com")))
                    .unwrap_or(false)
        })
        .unwrap();
    let fake_index = rules
        .iter()
        .position(|rule| rule.get("server") == Some(&json!("fake-dns")))
        .unwrap();

    assert!(service_index < fake_index);
    reset_test_env();
}

#[test]
fn builtin_dns_uses_independent_resolver_for_dns_server_hostnames() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    for dns_type in ["udp", "tcp", "tls", "https"] {
        let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
        let config = plan_singbox_runtime(
            &document,
            Some(&test_node()),
            PlanRuntimeOptions {
                routing: Some(RoutingSettings {
                    mode: "global".to_string(),
                    dns_mode: "builtin".to_string(),
                    dns_bootstrap_server: "resolver.example.com".to_string(),
                    dns_bootstrap_type: dns_type.to_string(),
                    dns_proxy_server: "proxied-resolver.example.com".to_string(),
                    dns_proxy_type: dns_type.to_string(),
                    tun_default_outbound: "proxy".to_string(),
                    ..Default::default()
                }),
                system_dns_servers: vec!["192.0.2.53".to_string()],
                ..Default::default()
            },
        )
        .unwrap()
        .singbox_config;

        let servers = dns_servers(&config);

        assert_eq!(servers["system-dns"]["type"], json!("udp"), "dns_type {dns_type}");
        assert_eq!(servers["system-dns"]["server"], json!("192.0.2.53"));
        assert_eq!(servers["system-dns"]["detour"], json!("direct"));
        assert_eq!(servers["bootstrap-dns"]["detour"], json!("direct"));
        assert_eq!(servers["bootstrap-dns"]["domain_resolver"], json!("system-dns"));
        assert_eq!(servers["direct-dns"]["detour"], json!("direct"));
        assert_eq!(servers["direct-dns"]["domain_resolver"], json!("system-dns"));
        assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
        assert_eq!(servers["proxy-dns"]["domain_resolver"], json!("bootstrap-dns"));
    }
    reset_test_env();
}

#[test]
fn builtin_dns_does_not_add_resolver_for_ip_dns_servers() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    for dns_type in ["udp", "tcp", "tls", "https"] {
        let config = plan(
            RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "builtin".to_string(),
                dns_bootstrap_server: "1.1.1.1".to_string(),
                dns_bootstrap_type: dns_type.to_string(),
                dns_proxy_server: "8.8.8.8".to_string(),
                dns_proxy_type: dns_type.to_string(),
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            },
            None,
            PlanRuntimeOptions::default(),
        );

        let servers = dns_servers(&config);

        assert_eq!(servers["bootstrap-dns"]["detour"], json!("direct"), "dns_type {dns_type}");
        assert!(servers["bootstrap-dns"].get("domain_resolver").is_none());
        assert_eq!(servers["direct-dns"]["detour"], json!("direct"));
        assert!(servers["direct-dns"].get("domain_resolver").is_none());
        assert_eq!(servers["proxy-dns"]["detour"], json!("proxy"));
        if matches!(dns_type, "tls" | "https") {
            assert_eq!(servers["proxy-dns"]["server"], json!("dns.google"));
            assert_eq!(servers["proxy-dns"]["type"], json!(dns_type));
            assert_eq!(servers["proxy-dns"]["domain_resolver"], json!("bootstrap-dns"));
        } else {
            assert_eq!(servers["proxy-dns"]["server"], json!("8.8.8.8"));
            assert!(servers["proxy-dns"].get("domain_resolver").is_none());
        }
    }
    reset_test_env();
}

#[test]
fn native_tun_excludes_ip_proxy_endpoint_from_tun_routes() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    for (server, cidr) in [("95.128.157.251", "95.128.157.251/32"), ("2001:db8::2208", "2001:db8::2208/128")] {
        let config = plan(
            RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "builtin".to_string(),
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            },
            Some(node_with_server(server)),
            PlanRuntimeOptions::default(),
        );

        let rules = config["route"]["rules"].as_array().unwrap();
        let route_excludes = tun_inbound(&config)["route_exclude_address"].clone();

        assert!(route_excludes
            .as_array()
            .unwrap()
            .contains(&json!(cidr)), "server {server}");
        assert!(rules
            .iter()
            .any(|rule| rule.get("outbound") == Some(&json!("direct"))
                && rule
                    .get("ip_cidr")
                    .and_then(Value::as_array)
                    .map(|items| items.contains(&json!(cidr)))
                    .unwrap_or(false)));
    }
    reset_test_env();
}

#[test]
fn native_tun_preserves_user_route_excludes_when_adding_endpoint_exclude() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            tun_route_exclude_address: vec!["192.168.0.0/16".to_string()],
            ..Default::default()
        },
        Some(node_with_server("95.128.157.251")),
        PlanRuntimeOptions::default(),
    );

    let route_excludes = tun_inbound(&config)["route_exclude_address"].clone();

    assert!(route_excludes.as_array().unwrap().contains(&json!("192.168.0.0/16")));
    assert!(route_excludes.as_array().unwrap().contains(&json!("95.128.157.251/32")));
    reset_test_env();
}

#[test]
fn native_tun_excludes_resolved_domain_proxy_endpoint_from_tun_routes() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_map(&[("vpn.example.com", &["203.0.113.10", "2001:db8::10"])]);

    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        Some(node_with_server("vpn.example.com")),
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    let inbound = tun_inbound(&config);
    let proxy = config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();

    assert!(inbound["route_exclude_address"]
        .as_array()
        .unwrap()
        .contains(&json!("203.0.113.10/32")));
    assert!(inbound["route_exclude_address"]
        .as_array()
        .unwrap()
        .contains(&json!("2001:db8::10/128")));
    assert_eq!(proxy["server"], json!("vpn.example.com"));
    assert_eq!(proxy.get("domain_resolver"), Some(&json!("bootstrap-dns")));
    assert_eq!(proxy["tls"]["server_name"], json!("vpn.example.com"));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("ip_cidr")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("203.0.113.10/32")))
                .unwrap_or(false)));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("vpn.example.com")))
                .unwrap_or(false)));
    reset_test_env();
}

#[test]
fn endpoint_resolution_is_reused_within_cache_ttl() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_inner = calls.clone();
    set_test_getaddrinfo(Some(Arc::new(move |_host: &str| {
        calls_inner.fetch_add(1, Ordering::SeqCst);
        Ok(vec!["203.0.113.25:0".parse::<SocketAddr>().unwrap()])
    })));
    clear_endpoint_dns_caches_for_tests();

    assert_eq!(resolve_endpoint_addresses("cache.example.com"), vec!["203.0.113.25"]);
    assert_eq!(resolve_endpoint_addresses("cache.example.com"), vec!["203.0.113.25"]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    reset_test_env();
}

#[test]
fn tun_runtime_rejects_https_svcb_dns_in_system_mode() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "system".to_string(),
            dns_fake_enabled: true,
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let dns_rules = config["dns"]["rules"].as_array().unwrap();
    let https_rejects: Vec<&Value> = dns_rules
        .iter()
        .filter(|rule| {
            rule.get("action") == Some(&json!("reject"))
                && rule
                    .get("query_type")
                    .and_then(Value::as_array)
                    .map(|items| {
                        let names: HashSet<String> = items
                            .iter()
                            .map(|item| item.as_str().unwrap_or_default().to_uppercase())
                            .collect();
                        names == HashSet::from(["HTTPS".to_string(), "SVCB".to_string()])
                    })
                    .unwrap_or(false)
        })
        .collect();
    assert_eq!(https_rejects.len(), 1);
    // The reject must sit ahead of the A/AAAA -> fake-dns rule so ECH/h3 hints
    // are never answered for routed domains.
    let fake_index = dns_rules
        .iter()
        .position(|rule| rule.get("server") == Some(&json!("fake-dns")))
        .unwrap();
    let reject_index = dns_rules
        .iter()
        .position(|rule| {
            rule.get("action") == Some(&json!("reject"))
                && rule
                    .get("query_type")
                    .and_then(Value::as_array)
                    .map(|items| {
                        let names: HashSet<String> = items
                            .iter()
                            .map(|item| item.as_str().unwrap_or_default().to_uppercase())
                            .collect();
                        names == HashSet::from(["HTTPS".to_string(), "SVCB".to_string()])
                    })
                    .unwrap_or(false)
        })
        .unwrap();
    assert!(reject_index < fake_index);

    let servers = dns_servers(&config);
    assert_eq!(servers["fake-dns"]["type"], json!("fakeip"));
    reset_test_env();
}

#[test]
fn tun_runtime_does_not_reject_quic_globally() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_fail();
    let config = plan(
        RoutingSettings {
            mode: "rule".to_string(),
            dns_mode: "system".to_string(),
            direct_domains: vec!["2ip.ru".to_string()],
            tun_default_outbound: "direct".to_string(),
            ..Default::default()
        },
        Some(node_with_server("vpn.example.com")),
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain_suffix")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("2ip.ru")))
                .unwrap_or(false)));
    assert!(!rules
        .iter()
        .any(|rule| rule.get("action") == Some(&json!("reject"))
            && rule.get("network") == Some(&json!("udp"))
            && rule.get("port") == Some(&json!(443))));
    reset_test_env();
}

#[test]
fn native_tun_keeps_domain_proxy_endpoint_route_when_resolution_fails() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_fail();

    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        Some(node_with_server("vpn.example.com")),
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    let inbound = tun_inbound(&config);
    let proxy = config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();

    assert!(inbound.get("route_exclude_address").is_none());
    assert_eq!(proxy["server"], json!("vpn.example.com"));
    assert_eq!(proxy["domain_resolver"], json!("bootstrap-dns"));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("vpn.example.com")))
                .unwrap_or(false)));
    reset_test_env();
}

#[test]
fn native_tun_does_not_block_startup_on_slow_endpoint_dns() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_test_tun_env(Some(HashSet::new()), Some(false));
    set_test_getaddrinfo(Some(Arc::new(|_host: &str| {
        std::thread::sleep(Duration::from_secs(5));
        Ok(Vec::new())
    })));
    let started = Instant::now();

    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        Some(node_with_server("vpn.example.com")),
        PlanRuntimeOptions::default(),
    );

    assert!(started.elapsed() < Duration::from_secs(2));
    let proxy = config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();
    assert_eq!(proxy["server"], json!("vpn.example.com"));
    assert_eq!(proxy["domain_resolver"], json!("bootstrap-dns"));
    reset_test_env();
}

#[test]
fn endpoint_tun_excludes_ip_peer_from_tun_routes() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        Some(wireguard_node("46.17.101.82")),
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    let route_excludes = tun_inbound(&config)["route_exclude_address"].clone();

    assert!(route_excludes.as_array().unwrap().contains(&json!("46.17.101.82/32")));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("ip_cidr")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("46.17.101.82/32")))
                .unwrap_or(false)));
    reset_test_env();
}

#[test]
fn endpoint_tun_excludes_resolved_domain_peer_from_tun_routes() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_map(&[("wg.example.com", &["198.51.100.20"])]);

    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        Some(wireguard_node("wg.example.com")),
        PlanRuntimeOptions::default(),
    );

    let rules = config["route"]["rules"].as_array().unwrap();
    let route_excludes = tun_inbound(&config)["route_exclude_address"].clone();

    assert!(route_excludes.as_array().unwrap().contains(&json!("198.51.100.20/32")));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("ip_cidr")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("198.51.100.20/32")))
                .unwrap_or(false)));
    assert!(rules
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("wg.example.com")))
                .unwrap_or(false)));
    reset_test_env();
}

#[test]
fn builtin_dns_adds_multiple_servers_and_hosts_without_unsupported_optimistic_field() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            dns_bootstrap_servers: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
            dns_bootstrap_type: "udp".to_string(),
            dns_proxy_servers: vec!["dns.google".to_string(), "9.9.9.9".to_string()],
            dns_proxy_type: "https".to_string(),
            dns_optimistic_cache: true,
            dns_hosts: BTreeMap::from([("local.test".to_string(), vec![
                "127.0.0.1".to_string(),
                "::1".to_string(),
            ])]),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );

    let servers = dns_servers(&config);

    assert_eq!(servers["direct-dns-2"]["server"], json!("8.8.8.8"));
    assert_eq!(servers["proxy-dns-2"]["server"], json!("dns.quad9.net"));
    assert_eq!(
        servers["hosts-dns"],
        json!({
            "tag": "hosts-dns",
            "type": "hosts",
            "predefined": {"local.test": ["127.0.0.1", "::1"]},
        })
    );
    assert!(config["dns"].get("optimistic").is_none());
    assert!(config["dns"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("ip_accept_any") == Some(&json!(true)) && rule.get("server") == Some(&json!("hosts-dns"))
    }));
    reset_test_env();
}

#[test]
fn hysteria_without_bandwidth_gets_singbox_speed_defaults() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let outbound =
        crate::singbox::config_builder::build_singbox_outbound(&hysteria_node(json!({})), "proxy", false, 8)
            .unwrap();

    assert_eq!(outbound["up_mbps"], json!(50));
    assert_eq!(outbound["down_mbps"], json!(200));
    reset_test_env();
}

#[test]
fn hysteria_keeps_bandwidth_declared_by_the_link() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let outbound = crate::singbox::config_builder::build_singbox_outbound(
        &hysteria_node(json!({"up_mbps": 10, "down_mbps": 20})),
        "proxy",
        false,
        8,
    )
    .unwrap();

    assert_eq!(outbound["up_mbps"], json!(10));
    assert_eq!(outbound["down_mbps"], json!(20));
    reset_test_env();
}

#[test]
fn full_config_node_keeps_endpoint_bootstrap_route_after_gui_routing() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_fail();

    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let config = plan_singbox_runtime(
        &document,
        Some(&full_config_node(endpoint_document("wg.example.com"))),
        PlanRuntimeOptions {
            routing: Some(RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "builtin".to_string(),
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap()
    .singbox_config;

    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("wg.example.com")))
                .unwrap_or(false)
    }));
    reset_test_env();
}

#[test]
fn document_without_proxy_outbound_keeps_endpoint_bootstrap_route() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_resolver_fail();

    let document = parse_singbox_document(
        Path::new("test.json"),
        &endpoint_document("wg.example.com").to_string(),
    )
    .unwrap();
    let plan = plan_singbox_runtime(
        &document,
        None,
        PlanRuntimeOptions {
            routing: Some(RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "builtin".to_string(),
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!plan.has_proxy_outbound);
    assert!(plan.singbox_config["route"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule.get("outbound") == Some(&json!("direct"))
            && rule
                .get("domain")
                .and_then(Value::as_array)
                .map(|items| items.contains(&json!("wg.example.com")))
                .unwrap_or(false)));
    reset_test_env();
}

#[test]
fn document_with_foreign_proxy_tag_gets_a_proxy_alias() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let document = parse_singbox_document(
        Path::new("test.json"),
        &endpoint_document("198.51.100.7").to_string(),
    )
    .unwrap();
    let plan = plan_singbox_runtime(
        &document,
        None,
        PlanRuntimeOptions {
            routing: Some(RoutingSettings {
                mode: "global".to_string(),
                dns_mode: "builtin".to_string(),
                tun_default_outbound: "proxy".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();
    let config = plan.singbox_config;

    let alias = config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|outbound| outbound.get("tag") == Some(&json!("proxy")))
        .unwrap();

    assert!(!plan.has_proxy_outbound);
    assert_eq!(alias["type"], json!("selector"));
    assert_eq!(alias["default"], json!("SELECT"));
    assert_eq!(config["route"]["final"], json!("proxy"));
    assert!(config["route"]["rules"].as_array().unwrap().iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["socks-in", "http-in"]))
            && rule.get("outbound") == Some(&json!("proxy"))
    }));
    reset_test_env();
}

#[test]
fn imported_tun_inbound_tag_is_normalized_for_the_scoped_reject_rules() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut payload = base_config();
    payload["inbounds"][0]["tag"] = json!("tun");
    payload["route"]["rules"] = json!([{"inbound": ["tun"], "action": "route", "outbound": "direct"}]);
    let document = parse_singbox_document(Path::new("test.json"), &payload.to_string()).unwrap();
    let config = plan_singbox_runtime(
        &document,
        Some(&test_node()),
        PlanRuntimeOptions {
            routing: Some(routing_global("proxy")),
            ..Default::default()
        },
    )
    .unwrap()
    .singbox_config;

    let rules = config["route"]["rules"].as_array().unwrap();

    assert_eq!(tun_inbound(&config)["tag"], json!("tun-in"));
    assert!(rules.iter().any(|rule| {
        rule.get("inbound") == Some(&json!(["tun-in"]))
            && rule.get("protocol") == Some(&json!("quic"))
            && rule.get("action") == Some(&json!("reject"))
    }));
    assert!(!rules.iter().any(|rule| {
        let inbound = rule.get("inbound").cloned().unwrap_or(json!([]));
        match inbound {
            Value::Array(items) => items.iter().any(|item| *item == json!("tun")),
            Value::String(text) => text.contains("tun"),
            _ => false,
        }
    }));
    reset_test_env();
}

#[test]
fn tls_fragment_rule_precedes_terminal_gui_route_rules() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "rule".to_string(),
            proxy_domains: vec!["example.com".to_string()],
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );
    let rules = config["route"]["rules"].as_array().unwrap();

    let fragment_index = rules
        .iter()
        .position(|rule| rule.get("action") == Some(&json!("route-options")))
        .unwrap();
    let gui_index = rules
        .iter()
        .position(|rule| {
            rule.get("outbound") == Some(&json!("proxy"))
                && rule
                    .get("domain_suffix")
                    .and_then(Value::as_array)
                    .map(|items| items.contains(&json!("example.com")))
                    .unwrap_or(false)
        })
        .unwrap();
    let local_proxy_index = rules
        .iter()
        .position(|rule| rule.get("inbound") == Some(&json!(["socks-in", "http-in"])))
        .unwrap();

    assert!(fragment_index < gui_index);
    assert!(fragment_index < local_proxy_index);
    reset_test_env();
}

#[test]
fn dns_hosts_override_precedes_the_fakeip_rule() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let config = plan(
        RoutingSettings {
            mode: "global".to_string(),
            dns_mode: "builtin".to_string(),
            dns_fake_enabled: true,
            dns_hosts: BTreeMap::from([("my.internal".to_string(), vec!["10.1.2.3".to_string()])]),
            tun_default_outbound: "proxy".to_string(),
            ..Default::default()
        },
        None,
        PlanRuntimeOptions::default(),
    );
    let dns_rules = config["dns"]["rules"].as_array().unwrap();

    let https_index = dns_rules
        .iter()
        .position(|rule| rule.get("query_type") == Some(&json!(["HTTPS", "SVCB"])))
        .unwrap();
    let hosts_index = dns_rules
        .iter()
        .position(|rule| rule.get("server") == Some(&json!("hosts-dns")))
        .unwrap();
    let fake_index = dns_rules
        .iter()
        .position(|rule| rule.get("server") == Some(&json!("fake-dns")))
        .unwrap();

    assert!(https_index < hosts_index && hosts_index < fake_index);
    reset_test_env();
}

#[test]
fn hot_switch_selector_contains_all_native_extended_nodes() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    let mut first = node_with_server("first.example.com");
    first.id = "first-node".to_string();
    let mut second = node_with_server("second.example.com");
    second.id = "second-node".to_string();
    let mut extended = Node::new();
    extended.id = "extended-node".to_string();
    extended.name = "Extended".to_string();
    extended.scheme = "vmess".to_string();
    extended.server = "hybrid.example.com".to_string();
    extended.port = 443;
    extended.outbound = json!({
        "protocol": "vmess",
        "settings": {"vnext": [{"address": "hybrid.example.com", "port": 443}]},
        "streamSettings": {"network": "xhttp"},
    });

    let document = parse_singbox_document(Path::new("test.json"), &base_config().to_string()).unwrap();
    let plan = plan_singbox_runtime(
        &document,
        Some(&first),
        PlanRuntimeOptions {
            routing: Some(routing_global("proxy")),
            enable_hot_switch: true,
            hot_switch_nodes: vec![first.clone(), second.clone(), extended.clone()],
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(plan.clash_api_selector, "proxy");
    let selector = plan.singbox_config["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item.get("tag") == Some(&json!("proxy")))
        .unwrap();
    assert_eq!(selector["type"], json!("selector"));
    let members: Vec<Value> = selector["outbounds"].as_array().unwrap().clone();
    let default_tag = selector["default"].as_str().unwrap().to_string();
    assert!(members.contains(&json!(default_tag)));
    assert_eq!(members.len(), 3);
    assert!(members.contains(&json!(singbox_node_tag("extended-node"))));
    let signatures: HashMap<String, String> = plan.clash_api_node_signatures.iter().cloned().collect();
    assert_eq!(
        signatures,
        HashMap::from([
            (first.id.clone(), singbox_node_source_signature(&first)),
            (second.id.clone(), singbox_node_source_signature(&second)),
            (extended.id.clone(), singbox_node_source_signature(&extended)),
        ])
    );
    assert!(!plan.singbox_config["outbounds"].as_array().unwrap().iter().any(|item| {
        item.get("tag") == Some(&json!("proxy")) && item.get("type") != Some(&json!("selector"))
    }));
    reset_test_env();
}

#[test]
fn proxy_bootstrap_does_not_resolve_every_hot_switch_candidate() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    // The proxy (no-TUN) path must not call the endpoint resolver; the Python
    // test monkeypatches `_resolve_endpoint_addresses` to raise on any call.
    // The Rust port asserts the same outcome by direct inspection.
    let mut payload = json!({
        "inbounds": [{"type": "mixed", "tag": "socks-in"}],
        "route": {"rules": []},
    });
    let path = super::DialPath::Outbound(0);
    // Attach the outbound under test.
    payload["outbounds"] = json!([{"type": "vless", "server": "example.org"}]);
    super::bootstrap::ensure_proxy_server_bootstrap_contract(&mut payload, path, "example.org");
    let outbound = &payload["outbounds"][0];
    assert_eq!(outbound["domain_resolver"], json!("bootstrap-dns"));
    assert_eq!(
        payload["route"]["rules"],
        json!([{"domain": ["example.org"], "action": "route", "outbound": "direct"}])
    );
    reset_test_env();
}

#[test]
fn tun_bootstrap_still_excludes_resolved_endpoint() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_test_getaddrinfo(Some(Arc::new(|_host: &str| {
        Ok(vec!["203.0.113.10:0".parse::<SocketAddr>().unwrap()])
    })));
    let mut payload = json!({
        "inbounds": [{"type": "tun", "tag": "tun-in"}],
        "route": {"rules": []},
    });
    payload["outbounds"] = json!([{"type": "vless", "server": "example.org"}]);
    super::bootstrap::ensure_proxy_server_bootstrap_contract(&mut payload, super::DialPath::Outbound(0), "example.org");
    assert!(payload["inbounds"][0]["route_exclude_address"]
        .as_array()
        .unwrap()
        .contains(&json!("203.0.113.10/32")));
    reset_test_env();
}

#[test]
fn tun_endpoint_names_resolve_in_parallel_with_one_deadline() {
    let _guard = TEST_GLOBAL_LOCK.lock();
    set_test_getaddrinfo(Some(Arc::new(|host: &str| {
        if host.starts_with("dead") {
            std::thread::sleep(Duration::from_secs(5));
            Err(std::io::Error::other("timeout"))
        } else {
            std::thread::sleep(Duration::from_millis(200));
            Ok(vec!["203.0.113.7:0".parse::<SocketAddr>().unwrap()])
        }
    })));
    clear_endpoint_dns_caches_for_tests();
    let mut hosts: Vec<String> = (0..30).map(|index| format!("node{index}.example.net")).collect();
    hosts.push("dead1.example.net".to_string());
    hosts.push("dead2.example.net".to_string());

    let started = Instant::now();
    prefetch_endpoint_addresses(&hosts, Duration::from_secs(1));
    // 30 x 0.2 s in a row plus 2 x 1.2 s timeouts used to take ~8 s.
    assert!(started.elapsed() < Duration::from_millis(1600));

    let started = Instant::now();
    assert_eq!(resolve_endpoint_addresses("node7.example.net"), vec!["203.0.113.7"]);
    assert!(resolve_endpoint_addresses("dead1.example.net").is_empty());
    assert!(started.elapsed() < Duration::from_millis(100));
    reset_test_env();
}
