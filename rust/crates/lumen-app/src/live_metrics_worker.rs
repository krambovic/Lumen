//! Poll traffic and active-profile health on a dedicated background thread.

mod sampling;

use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lumen_core::active_profile_health::{DEFAULT_HEALTH_URL, HealthSample, probe_active_profile};
use lumen_core::constants::{DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT, SINGBOX_CLASH_API_PORT};
use lumen_core::http_utils::WarmHttpProbe;
use lumen_core::metrics_api::{ClashApiClient, monotonic_seconds};
use lumen_engines::xray::stats_client::XrayStatsClient;
use lumen_platform::process_traffic_collector::{
    ProcessTrafficSnapshot, collect_process_stats, connection_tracking_epoch,
};
use lumen_platform::win_proc_monitor::get_proxy_connections;
use tokio::sync::{mpsc, watch};

use sampling::{
    CollectorServices, ProxyProcessTotals, metrics_idle_delay, new_connection_events, sample_rates,
    thread_cpu_seconds, xray_totals,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum MetricsMode {
    #[default]
    Xray,
    Singbox,
    Unsupported,
}

#[derive(Clone)]
pub struct MetricsProfile {
    pub id: String,
    pub outbound_tag: String,
    /// HTTP listener guaranteed to route the active profile.
    pub health_proxy_url: String,
}

impl Default for MetricsProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            outbound_tag: "proxy".into(),
            health_proxy_url: String::new(),
        }
    }
}

pub struct LiveMetricsOptions {
    pub mode: MetricsMode,
    pub xray_api_port: i64,
    pub xray_inbound_tags: Vec<String>,
    pub clash_api_port: u16,
    pub clash_api_secret: String,
    pub socks_port: u16,
    pub http_port: u16,
    pub interval: Duration,
    pub health_interval: Duration,
    pub process_stats_interval: Duration,
    pub process_stats_enabled: bool,
    pub profile: MetricsProfile,
}

impl Default for LiveMetricsOptions {
    fn default() -> Self {
        Self {
            mode: MetricsMode::Xray,
            xray_api_port: 0,
            xray_inbound_tags: Vec::new(),
            clash_api_port: SINGBOX_CLASH_API_PORT,
            clash_api_secret: String::new(),
            socks_port: DEFAULT_SOCKS_PORT,
            http_port: DEFAULT_HTTP_PORT,
            interval: Duration::from_secs(1),
            health_interval: Duration::from_secs(3),
            process_stats_interval: Duration::from_secs(4),
            process_stats_enabled: true,
            profile: MetricsProfile::default(),
        }
    }
}

#[derive(Debug)]
pub struct LiveMetricsSample {
    pub up_bps: f64,
    pub down_bps: f64,
    pub process_stats: Option<Vec<ProcessTrafficSnapshot>>,
    pub connection_events: Vec<String>,
    pub traffic_available: bool,
    pub traffic_reason: String,
    pub traffic_checked_at: f64,
    pub upload_total: Option<u64>,
    pub download_total: Option<u64>,
    pub health: HealthSample,
}

struct Control {
    stop: watch::Sender<bool>,
    profile: watch::Sender<MetricsProfile>,
    process_stats_enabled: AtomicBool,
    xray: XrayStatsClient,
    clash: ClashApiClient,
}

pub struct LiveMetricsWorker {
    pub samples: mpsc::UnboundedReceiver<LiveMetricsSample>,
    control: Arc<Control>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

impl LiveMetricsWorker {
    pub fn start(options: LiveMetricsOptions) -> io::Result<Self> {
        let (samples, receiver) = mpsc::unbounded_channel();
        let control = Arc::new(Control {
            stop: watch::Sender::new(false),
            profile: watch::Sender::new(options.profile.clone()),
            process_stats_enabled: AtomicBool::new(options.process_stats_enabled),
            xray: XrayStatsClient::new(options.xray_api_port),
            clash: ClashApiClient::new(options.clash_api_port, &options.clash_api_secret),
        });
        let tracking_epoch = connection_tracking_epoch();
        let worker_control = Arc::clone(&control);
        let thread = thread::Builder::new()
            .name("lumen-live-metrics".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(run(options, &worker_control, tracking_epoch, samples));
                worker_control.xray.close();
                worker_control.clash.close();
                Ok(())
            })?;
        Ok(Self {
            samples: receiver,
            control,
            thread: Some(thread),
        })
    }

    pub fn set_process_stats_enabled(&self, enabled: bool) {
        self.control
            .process_stats_enabled
            .store(enabled, Ordering::Relaxed);
    }

    pub fn set_active_profile(&self, profile: MetricsProfile) {
        self.control.profile.send_replace(profile);
    }

    pub fn stop(&self) {
        self.control.stop.send_replace(true);
        self.control.clash.close();
        self.control.xray.close();
    }

    /// Stop and wait for the background thread, including any in-flight process scan.
    pub fn join(mut self) -> io::Result<()> {
        self.stop();
        self.thread
            .take()
            .expect("metrics worker thread")
            .join()
            .map_err(|_| io::Error::other("live metrics thread panicked"))?
    }
}

impl Drop for LiveMetricsWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn run(
    mut options: LiveMetricsOptions,
    control: &Control,
    tracking_epoch: u64,
    samples: mpsc::UnboundedSender<LiveMetricsSample>,
) {
    let mut stop = control.stop.subscribe();
    let mut profiles = control.profile.subscribe();
    let mut profile = profiles.borrow_and_update().clone();
    let mut health = HealthSample {
        profile_id: profile.id.clone(),
        ..HealthSample::default()
    };
    let mut warm_probe = WarmHttpProbe::new();
    let interval = options.interval.as_secs_f64().max(0.25);
    let health_interval = options.health_interval.as_secs_f64().max(1.0);
    let process_interval = options.process_stats_interval.as_secs_f64().max(1.0);
    let mut seen_tags = HashSet::new();
    options.xray_inbound_tags = options
        .xray_inbound_tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty() && seen_tags.insert(*tag))
        .map(str::to_owned)
        .collect();
    let mut previous = None;
    let mut last_process = f64::NEG_INFINITY;
    let mut process_totals = ProxyProcessTotals::default();
    let mut outbound_graph = None;
    let mut seen_connections = HashSet::new();

    while !*stop.borrow() {
        let started = Instant::now();
        let cpu_started = thread_cpu_seconds();
        let mut connections = None;
        let mut connection_events = Vec::new();
        let totals = match options.mode {
            MetricsMode::Xray if options.xray_inbound_tags.is_empty() => {
                Err("active Xray inbound tags unavailable".into())
            }
            MetricsMode::Xray => match control.xray.query().await {
                Some(stats) => xray_totals(&stats, &options.xray_inbound_tags)
                    .ok_or_else(|| "active Xray inbound counters unavailable".into()),
                None => Err(control.xray.reason()),
            },
            MetricsMode::Singbox => match control.clash.get("/connections", 0.8) {
                Ok(document) => {
                    connection_events = new_connection_events(
                        &document,
                        outbound_graph.as_ref(),
                        &mut seen_connections,
                    );
                    let totals = document["uploadTotal"]
                        .as_u64()
                        .zip(document["downloadTotal"].as_u64());
                    connections = Some(document);
                    totals.ok_or_else(|| "Clash API traffic totals missing or invalid".into())
                }
                Err(error) => Err(error.to_string()),
            },
            MetricsMode::Unsupported => Err("metrics unsupported for active core".into()),
        };
        let sampled_at = monotonic_seconds();
        let (up_bps, down_bps) =
            sample_rates(&mut previous, totals.as_ref().ok().copied(), sampled_at);
        if *stop.borrow() {
            break;
        }
        if profiles.has_changed().unwrap_or(false) {
            refresh_profile(&mut profiles, &mut profile, &mut health, &mut warm_probe);
        }
        if health.checked_at == 0.0 || sampled_at - health.checked_at >= health_interval {
            let clash = (options.mode == MetricsMode::Singbox).then_some(&control.clash);
            if let Some(client) = clash {
                outbound_graph = client.get("/proxies", 0.8).ok();
            }
            let sample = tokio::select! {
                biased;
                _ = stop.wait_for(|stopped| *stopped) => break,
                _ = profiles.changed() => None,
                sample = probe_active_profile(&profile.id, clash, &profile.outbound_tag,
                    outbound_graph.as_ref(), &profile.health_proxy_url, DEFAULT_HEALTH_URL,
                    1.2, Some(&mut warm_probe)) => Some(sample),
            };
            if let Some(sample) = sample {
                health = sample;
            } else {
                refresh_profile(&mut profiles, &mut profile, &mut health, &mut warm_probe);
            }
        }
        if *stop.borrow() {
            break;
        }
        let process_stats = if control.process_stats_enabled.load(Ordering::Relaxed)
            && sampled_at - last_process >= process_interval
        {
            let stats = match options.mode {
                MetricsMode::Singbox => connections.as_ref().map(|document| {
                    collect_process_stats(
                        options.clash_api_port,
                        &options.clash_api_secret,
                        Some(document),
                        outbound_graph.as_ref(),
                        Some(tracking_epoch),
                        Some(&CollectorServices(&control.clash)),
                    )
                }),
                MetricsMode::Xray => process_totals.collect(
                    get_proxy_connections(options.socks_port, options.http_port),
                    monotonic_seconds(),
                ),
                MetricsMode::Unsupported => None,
            };
            last_process = monotonic_seconds();
            stats
        } else {
            None
        };
        if *stop.borrow() {
            break;
        }
        if profiles.has_changed().unwrap_or(false) {
            refresh_profile(&mut profiles, &mut profile, &mut health, &mut warm_probe);
        }
        let (upload_total, download_total, traffic_reason) = match totals {
            Ok((up, down)) => (Some(up), Some(down), String::new()),
            Err(error) => (None, None, error),
        };
        let sample = LiveMetricsSample {
            up_bps,
            down_bps,
            process_stats,
            connection_events,
            traffic_available: upload_total.is_some(),
            traffic_reason,
            traffic_checked_at: if upload_total.is_some() {
                sampled_at
            } else {
                0.0
            },
            upload_total,
            download_total,
            health: health.at_time(monotonic_seconds()),
        };
        if samples.send(sample).is_err() {
            break;
        }
        let delay = metrics_idle_delay(
            interval,
            started.elapsed().as_secs_f64(),
            thread_cpu_seconds() - cpu_started,
        );
        tokio::select! {
            _ = stop.wait_for(|stopped| *stopped) => break,
            _ = tokio::time::sleep(Duration::from_secs_f64(delay)) => {},
        }
    }
}

fn refresh_profile(
    profiles: &mut watch::Receiver<MetricsProfile>,
    profile: &mut MetricsProfile,
    health: &mut HealthSample,
    warm_probe: &mut WarmHttpProbe,
) {
    *profile = profiles.borrow_and_update().clone();
    *health = HealthSample {
        profile_id: profile.id.clone(),
        ..HealthSample::default()
    };
    warm_probe.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_platform::win_proc_monitor::ProxyProcessInfo;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};

    #[test]
    fn slow_io_and_expensive_cpu_both_get_a_cooldown() {
        for (wall, cpu, expected) in [
            (0.02, 0.01, 0.98),
            (1.2, 0.8, 3.2),
            (3.0, 2.0, 8.0),
            (5.0, 0.001, 3.0),
        ] {
            assert!((metrics_idle_delay(1.0, wall, cpu) - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn rates_keep_last_valid_baseline_and_treat_counter_resets_as_zero() {
        let mut previous = None;
        assert_eq!(
            sample_rates(&mut previous, Some((10, 100)), 1.0),
            (0.0, 0.0)
        );
        assert_eq!(sample_rates(&mut previous, None, 2.0), (0.0, 0.0));
        assert_eq!(
            sample_rates(&mut previous, Some((20, 160)), 3.0),
            (5.0, 30.0)
        );
        assert_eq!(sample_rates(&mut previous, Some((2, 4)), 4.0), (0.0, 0.0));
    }

    #[test]
    fn connection_logs_use_existing_snapshot_and_are_bounded() {
        let graph = json!({"proxies": {"proxy": {"type": "VLESS"}}});
        let document = json!({"connections": (0..10).map(|index| json!({
            "id": format!("conn-{index}"),
            "metadata": {"host": format!("site-{index}.example"), "destinationPort": 443,
                "sourceIP": "127.0.0.1", "sourcePort": 60000 + index, "network": "tcp",
                "processPath": "C:/private/process.exe"},
            "chains": ["proxy"],
        })).collect::<Vec<_>>()});
        let mut seen = HashSet::new();
        let events = new_connection_events(&document, Some(&graph), &mut seen);
        assert_eq!(events.len(), 7);
        assert_eq!(
            events[0],
            "[singbox] from 127.0.0.1:60000 accepted tcp:site-0.example:443 [proxy]"
        );
        assert_eq!(
            events[6],
            "[singbox] 4 additional connections hidden in this sample"
        );
        assert!(events.iter().all(|event| !event.contains("process.exe")));
        assert!(new_connection_events(&document, Some(&graph), &mut seen).is_empty());
        assert!(
            new_connection_events(&json!({"connections": []}), Some(&graph), &mut seen).is_empty()
        );
        assert_eq!(new_connection_events(&document, None, &mut seen).len(), 7);
    }

    #[test]
    fn xray_counts_only_exact_active_inbound_pairs() {
        let mut stats = [
            ("inbound>>>http>>>traffic>>>uplink".into(), 10),
            ("inbound>>>http>>>traffic>>>downlink".into(), 30),
            ("inbound>>>socks>>>traffic>>>uplink".into(), 20),
            ("inbound>>>socks>>>traffic>>>downlink".into(), 40),
            ("inbound>>>health>>>traffic>>>uplink".into(), 900),
        ]
        .into_iter()
        .collect();
        let tags = ["http".into(), "socks".into()];
        assert_eq!(xray_totals(&stats, &tags), Some((30, 70)));
        stats.remove("inbound>>>socks>>>traffic>>>downlink");
        assert_eq!(xray_totals(&stats, &tags), None);
    }

    #[test]
    fn proxy_process_snapshots_reject_implausible_counters_and_deltas() {
        let mut tracker = ProxyProcessTotals::default();
        let mut sample = |now, input, output| {
            tracker
                .collect(
                    vec![ProxyProcessInfo {
                        exe: "browser.exe".into(),
                        connections: 2,
                        bytes_in: input,
                        bytes_out: output,
                        ..Default::default()
                    }],
                    now,
                )
                .unwrap()
                .pop()
                .unwrap()
        };
        assert_eq!(sample(1.0, 200, 100).unknown_bytes, 0);
        assert_eq!(sample(2.0, 300, 120).unknown_bytes, 120);
        assert_eq!(sample(3.0, u64::MAX, 400_000_000).unknown_bytes, 120);
        assert_eq!(sample(4.0, 10, 5).unknown_bytes, 120);
        let result = sample(5.0, 30, 15);
        assert_eq!(
            (result.download, result.upload, result.connections),
            (120, 30, 2)
        );
        assert_eq!(result.route, "unknown");
    }

    async fn read_request(stream: &mut TcpStream) -> io::Result<String> {
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        loop {
            let before = request.len();
            if reader.read_line(&mut request).await? == 0 || &request[before..] == "\r\n" {
                return Ok(request);
            }
        }
    }

    #[tokio::test]
    async fn polls_loopback_and_changes_health_profile_without_restarting() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let (record, mut requests) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await?;
                let request = read_request(&mut stream).await?;
                assert!(request.contains("Authorization: Bearer test-secret\r\n"));
                let path = request.split_whitespace().nth(1).unwrap();
                if record.send(path.to_owned()).is_err() {
                    return io::Result::Ok(());
                }
                let body = match path.split('?').next().unwrap() {
                    "/connections" => r#"{"uploadTotal":11,"downloadTotal":31,"connections":[]}"#,
                    "/proxies" => {
                        r#"{"proxies":{"proxy":{"type":"VLESS"},"backup":{"type":"Trojan"}}}"#
                    }
                    "/proxies/proxy/delay" | "/proxies/backup/delay" => r#"{"delay":12}"#,
                    other => panic!("unexpected request: {other}"),
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await?;
            }
        });
        let mut worker = LiveMetricsWorker::start(LiveMetricsOptions {
            mode: MetricsMode::Singbox,
            clash_api_port: port,
            clash_api_secret: "test-secret".into(),
            interval: Duration::from_millis(250),
            health_interval: Duration::from_secs(60),
            process_stats_enabled: false,
            profile: MetricsProfile {
                id: "first".into(),
                ..Default::default()
            },
            ..LiveMetricsOptions::default()
        })?;
        let first = tokio::time::timeout(Duration::from_secs(4), worker.samples.recv())
            .await?
            .unwrap();
        assert!(first.traffic_available);
        assert_eq!(
            (first.upload_total, first.download_total),
            (Some(11), Some(31))
        );
        assert_eq!(
            (
                first.health.profile_id.as_str(),
                first.health.status.as_str()
            ),
            ("first", "HEALTHY")
        );
        assert!(first.process_stats.is_none());
        assert_eq!(requests.try_recv()?, "/connections");
        assert_eq!(requests.try_recv()?, "/proxies");
        assert!(requests.try_recv()?.starts_with("/proxies/proxy/delay?"));

        worker.set_active_profile(MetricsProfile {
            id: "second".into(),
            outbound_tag: "backup".into(),
            ..Default::default()
        });
        worker.set_process_stats_enabled(true);
        let second = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let sample = worker.samples.recv().await.unwrap();
                if sample.health.profile_id == "second" && sample.health.status == "HEALTHY" {
                    break sample;
                }
            }
        })
        .await?;
        worker.join()?;
        server.abort();
        assert!(second.process_stats.is_some());
        let mut paths = Vec::new();
        while let Ok(path) = requests.try_recv() {
            paths.push(path);
        }
        assert!(
            paths
                .iter()
                .any(|path| path.starts_with("/proxies/backup/delay?"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn stopping_closes_a_pending_clash_read() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let mut worker = LiveMetricsWorker::start(LiveMetricsOptions {
            mode: MetricsMode::Singbox,
            clash_api_port: listener.local_addr()?.port(),
            clash_api_secret: "test-secret".into(),
            ..LiveMetricsOptions::default()
        })?;
        let (mut stream, _) =
            tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
        let request =
            tokio::time::timeout(Duration::from_secs(2), read_request(&mut stream)).await??;
        assert!(request.starts_with("GET /connections "));
        worker.stop();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), worker.samples.recv())
                .await?
                .is_none()
        );
        assert_eq!(stream.read(&mut [0]).await?, 0);
        worker.join()?;
        Ok(())
    }
}
