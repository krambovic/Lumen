use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use lumen_core::i18n::tr;
use lumen_core::link_parser::{py_int_or, py_str_or, py_truthy};
use lumen_core::metrics_api::ClashApiClient;
use lumen_core::traffic_route_classifier::{RouteClass, RouteClassifier};
use lumen_engines::xray::stats_client::Stats;
use lumen_platform::process_traffic_collector::{
    ConnectionRoute, ConnectionRouteClassifier, CoreServices, ProcessTrafficSnapshot,
};
use lumen_platform::win_proc_monitor::ProxyProcessInfo;
use serde_json::Value;

const MAX_REASONABLE_BYTES_PER_SEC: u64 = 256 * 1024 * 1024;
const MAX_PLAUSIBLE_CONN_BYTES: u64 = 1024_u64.pow(5);

pub(super) fn metrics_idle_delay(interval: f64, wall: f64, cpu: f64) -> f64 {
    0.05_f64
        .max(interval - wall)
        .max((cpu.max(0.0) * 4.0).min(30.0))
        .max((wall - interval).clamp(0.0, 3.0))
}

pub(super) fn thread_cpu_seconds() -> f64 {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::FILETIME;
        use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // The thread pseudo-handle is valid only here and needs no close.
        if unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        }
        .is_ok()
        {
            let ticks = |time: FILETIME| {
                (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
            };
            return (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0;
        }
    }
    0.0
}

pub(super) fn xray_totals(stats: &Stats, tags: &[String]) -> Option<(u64, u64)> {
    tags.iter().try_fold((0_u64, 0_u64), |(up, down), tag| {
        let upload =
            u64::try_from(*stats.get(&format!("inbound>>>{tag}>>>traffic>>>uplink"))?).ok()?;
        let download =
            u64::try_from(*stats.get(&format!("inbound>>>{tag}>>>traffic>>>downlink"))?).ok()?;
        Some((up.checked_add(upload)?, down.checked_add(download)?))
    })
}

pub(super) fn sample_rates(
    previous: &mut Option<(u64, u64, f64)>,
    totals: Option<(u64, u64)>,
    now: f64,
) -> (f64, f64) {
    let Some((up, down)) = totals else {
        return (0.0, 0.0);
    };
    let Some((prev_up, prev_down, prev_at)) = previous.replace((up, down, now)) else {
        return (0.0, 0.0);
    };
    let dt = (now - prev_at).max(0.001);
    (
        up.saturating_sub(prev_up) as f64 / dt,
        down.saturating_sub(prev_down) as f64 / dt,
    )
}

#[derive(Default)]
struct ProxyCounters {
    input: u64,
    output: u64,
    download: u64,
    upload: u64,
}

#[derive(Default)]
pub(super) struct ProxyProcessTotals {
    previous_at: Option<f64>,
    by_exe: HashMap<String, ProxyCounters>,
}

impl ProxyProcessTotals {
    pub(super) fn collect(
        &mut self,
        processes: Vec<ProxyProcessInfo>,
        now: f64,
    ) -> Option<Vec<ProcessTrafficSnapshot>> {
        if processes.is_empty() {
            return None;
        }
        let dt = self
            .previous_at
            .replace(now)
            .map_or(2.0, |previous| (now - previous).max(0.5));
        let max_delta = (MAX_REASONABLE_BYTES_PER_SEC as f64 * dt) as u64;
        let mut result = Vec::with_capacity(processes.len());
        for process in processes {
            let previous = self.by_exe.get(&process.exe);
            let mut delta = (0, 0);
            let mut input = process.bytes_in;
            let mut output = process.bytes_out;
            let mut download = 0_u64;
            let mut upload = 0_u64;
            if let Some(previous) = previous {
                input = valid_counter(input, previous.input);
                output = valid_counter(output, previous.output);
                delta = (
                    bounded_delta(input, previous.input, max_delta),
                    bounded_delta(output, previous.output, max_delta),
                );
                download = previous.download.saturating_add(delta.0);
                upload = previous.upload.saturating_add(delta.1);
            } else {
                input = valid_counter(input, 0);
                output = valid_counter(output, 0);
            }
            self.by_exe.insert(
                process.exe.clone(),
                ProxyCounters {
                    input,
                    output,
                    download,
                    upload,
                },
            );
            result.push(ProcessTrafficSnapshot {
                exe: process.exe,
                upload,
                download,
                connections: process.connections,
                route: "unknown".into(),
                unknown_bytes: upload.saturating_add(download),
                down_speed: delta.0 as f64 / dt,
                up_speed: delta.1 as f64 / dt,
                ..Default::default()
            });
        }
        result.sort_by_key(|item| Reverse(item.upload.saturating_add(item.download)));
        Some(result)
    }
}

fn valid_counter(value: u64, previous: u64) -> u64 {
    if value <= MAX_PLAUSIBLE_CONN_BYTES {
        value
    } else {
        previous
    }
}

fn bounded_delta(current: u64, previous: u64, max: u64) -> u64 {
    let delta = current.saturating_sub(previous);
    if delta <= max { delta } else { 0 }
}

pub(super) struct CollectorServices<'a>(pub &'a ClashApiClient);

impl CoreServices for CollectorServices<'_> {
    fn fetch_document(&self, path: &str) -> Option<Value> {
        self.0.get(path, 0.8).ok()
    }

    fn build_classifier(&self, document: Option<&Value>) -> Box<dyn ConnectionRouteClassifier> {
        Box::new(CollectorClassifier(
            document.map(RouteClassifier::new).unwrap_or_default(),
        ))
    }

    fn translate(&self, key: &str, params: &[(&str, &str)]) -> String {
        tr(key, params)
    }
}

struct CollectorClassifier(RouteClassifier);

impl ConnectionRouteClassifier for CollectorClassifier {
    fn classify(&self, chains: &[String]) -> ConnectionRoute {
        map_route(
            self.0
                .classify(&chains.iter().map(String::as_str).collect::<Vec<_>>()),
        )
    }
}

fn map_route(route: RouteClass) -> ConnectionRoute {
    match route {
        RouteClass::Proxy => ConnectionRoute::Proxy,
        RouteClass::Direct => ConnectionRoute::Direct,
        RouteClass::Unknown => ConnectionRoute::Unknown,
    }
}

pub(super) fn new_connection_events(
    document: &Value,
    graph: Option<&Value>,
    seen: &mut HashSet<String>,
) -> Vec<String> {
    let Some(connections) = document["connections"].as_array() else {
        return Vec::new();
    };
    let mut classifier = None;
    let mut current = HashSet::new();
    let mut events = Vec::new();
    let mut hidden = 0;
    for connection in connections {
        let id = py_str_or(connection.get("id"), "");
        if id.is_empty() {
            continue;
        }
        let already_seen = seen.contains(&id);
        current.insert(id);
        if already_seen {
            continue;
        }
        let metadata = &connection["metadata"];
        let host = metadata
            .get("host")
            .filter(|value| py_truthy(value))
            .or_else(|| metadata.get("destinationIP"));
        let host: String = py_str_or(host, "").trim().chars().take(200).collect();
        if host.is_empty() {
            continue;
        }
        if events.len() == 6 {
            hidden += 1;
            continue;
        }
        let route = graph
            .and_then(|graph| {
                let chain: Option<Vec<_>> = connection["chains"]
                    .as_array()?
                    .iter()
                    .map(Value::as_str)
                    .collect();
                Some(map_route(
                    classifier
                        .get_or_insert_with(|| RouteClassifier::new(graph))
                        .classify(&chain?),
                ))
            })
            .unwrap_or(ConnectionRoute::Unknown);
        let destination = format_endpoint(host, metadata.get("destinationPort"));
        let source: String = py_str_or(metadata.get("sourceIP"), "")
            .trim()
            .chars()
            .take(100)
            .collect();
        let source = if source.is_empty() {
            String::new()
        } else {
            format!(
                "from {} ",
                format_endpoint(source, metadata.get("sourcePort"))
            )
        };
        let network = py_str_or(metadata.get("network"), "").to_lowercase();
        let network = match network.as_str() {
            "tcp" => "tcp",
            "udp" => "udp",
            _ => "connection",
        };
        events.push(format!(
            "[singbox] {source}accepted {network}:{destination} [{}]",
            route.as_str()
        ));
    }
    *seen = current;
    if hidden > 0 {
        events.push(format!(
            "[singbox] {hidden} additional connections hidden in this sample"
        ));
    }
    events
}

fn format_endpoint(mut host: String, port: Option<&Value>) -> String {
    if host.contains(':') && !host.starts_with('[') {
        host = format!("[{host}]");
    }
    let port = py_int_or(port, 0).unwrap_or_default();
    if (1..=65535).contains(&port) {
        host = format!("{host}:{port}");
    }
    host
}
