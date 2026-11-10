//! Per-process traffic aggregation from the Clash `/connections` snapshot
//! (port of `windows/xray_fluent/process_traffic_collector.py`).
//!
//! Aggregate one immutable /connections snapshot; no second fetch per tick.
//! The standalone polling API is retained. Workers should supply the document
//! they already fetched and their creation-time tracking_epoch. No metadata or
//! missing chain ever implies direct traffic. Errors do not close connections.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::{Map, Value};

use crate::win_proc_monitor::{monotonic_seconds, process_name_from_pid};

/// Mirror of `lumen_core::constants::SINGBOX_CLASH_API_PORT`; the crate
/// boundary (lumen-core depends on lumen-platform) forbids the import.
pub const SINGBOX_CLASH_API_PORT: u16 = 19090;

const HIDDEN_PROCESSES: [&str; 2] = ["xray.exe", "sing-box.exe"];
const MAX_REASONABLE_BYTES_PER_SEC: u64 = 256 * 1024 * 1024;

/// Route of one connection chain, matching the strings produced by the
/// Python `RouteClassifier` ("proxy" | "direct" | "unknown").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionRoute {
    Proxy,
    Direct,
    Unknown,
}

impl ConnectionRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionRoute::Proxy => "proxy",
            ConnectionRoute::Direct => "direct",
            ConnectionRoute::Unknown => "unknown",
        }
    }
}

/// Adapter over the routing graph (the Python `RouteClassifier` duck type).
pub trait ConnectionRouteClassifier {
    /// Classify one connection's `chains` list; an empty or unrecognized
    /// chain is `Unknown`, exactly like `RouteClassifier.classify`.
    fn classify(&self, chains: &[String]) -> ConnectionRoute;
}

/// `RouteClassifier(None)`: no nodes, every chain unknown.
pub struct UnroutedClassifier;

impl ConnectionRouteClassifier for UnroutedClassifier {
    fn classify(&self, _chains: &[String]) -> ConnectionRoute {
        ConnectionRoute::Unknown
    }
}

/// Glue to the lumen-core services that cannot be imported directly (lumen-core
/// depends on lumen-platform). lumen-app implements this once, forwarding to
/// `ClashApiClient`, `RouteClassifier::new` and `i18n::tr`.
pub trait CoreServices {
    /// `ClashApiClient(port, secret).get(path)`: None on MetricsApiError.
    fn fetch_document(&self, path: &str) -> Option<Value>;
    /// `RouteClassifier(document)`; None builds `RouteClassifier::default()`.
    fn build_classifier(&self, document: Option<&Value>) -> Box<dyn ConnectionRouteClassifier>;
    /// `i18n.tr(key, params)`; the default renders the Russian source text.
    fn translate(&self, key: &str, params: &[(&str, &str)]) -> String {
        default_translate(key, params)
    }
}

/// `i18n.translate` behaviour for the source language (or a missing catalog
/// entry): the key itself with `{placeholder}` substitution.
pub fn default_translate(key: &str, params: &[(&str, &str)]) -> String {
    let mut text = key.to_string();
    for (name, value) in params {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Frozen snapshot of one process's aggregated traffic.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessTrafficSnapshot {
    pub exe: String,
    pub upload: u64,
    pub download: u64,
    pub connections: u64,
    pub total_connections: u64,
    /// proxy | direct | mixed | unknown
    pub route: String,
    pub proxy_bytes: u64,
    pub direct_bytes: u64,
    pub top_host: String,
    pub down_speed: f64,
    pub up_speed: f64,
    pub unknown_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RouteTotals {
    proxy: u64,
    direct: u64,
    unknown: u64,
}

impl RouteTotals {
    fn add(&mut self, route: ConnectionRoute, value: u64) {
        match route {
            ConnectionRoute::Proxy => self.proxy += value,
            ConnectionRoute::Direct => self.direct += value,
            ConnectionRoute::Unknown => self.unknown += value,
        }
    }

    fn get(&self, route: ConnectionRoute) -> u64 {
        match route {
            ConnectionRoute::Proxy => self.proxy,
            ConnectionRoute::Direct => self.direct,
            ConnectionRoute::Unknown => self.unknown,
        }
    }

    fn routes(&self) -> [ConnectionRoute; 3] {
        [
            ConnectionRoute::Proxy,
            ConnectionRoute::Direct,
            ConnectionRoute::Unknown,
        ]
    }
}

#[derive(Default)]
struct ProcEntry {
    upload: u64,
    download: u64,
    conns: u64,
    routes: HashSet<ConnectionRoute>,
    route_bytes: RouteTotals,
    /// insertion-ordered host totals, like the Python dict
    hosts: Vec<(String, u64)>,
}

impl ProcEntry {
    fn add_host(&mut self, host: &str, bytes: u64) {
        match self.hosts.iter_mut().find(|(name, _)| name == host) {
            Some((_, total)) => *total += bytes,
            None => self.hosts.push((host.to_string(), bytes)),
        }
    }

    fn top_host(&self) -> &str {
        // Python max(...) keeps the first maximum in insertion order; Rust
        // max_by_key returns the last one, so compare manually.
        let mut best: Option<(&String, &u64)> = None;
        for (name, total) in &self.hosts {
            match best {
                Some((_, current)) if *total <= *current => {}
                _ => best = Some((name, total)),
            }
        }
        best.map(|(name, _)| name.as_str()).unwrap_or("")
    }
}

#[derive(Default)]
struct CollectorState {
    generation: u64,
    seen_connections: HashMap<String, HashSet<String>>,
    conn_owner: HashMap<String, String>,
    conn_bytes: HashMap<String, (u64, u64)>,
    conn_raw_bytes: HashMap<String, (u64, u64)>,
    conn_route_bytes: HashMap<String, RouteTotals>,
    proc_total_connections: HashMap<String, u64>,
    proc_closed_bytes: HashMap<String, (u64, u64)>,
    /// Insertion order of `proc_closed_bytes` keys (Python dict semantics).
    proc_closed_order: Vec<String>,
    proc_closed_route_bytes: HashMap<String, RouteTotals>,
    proc_display_names: HashMap<String, String>,
    prev_proc_total: HashMap<String, (u64, u64)>,
    prev_time: Option<f64>,
}

fn state() -> &'static Mutex<CollectorState> {
    static STATE: OnceLock<Mutex<CollectorState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CollectorState::default()))
}

/// Python `by_proc.setdefault(exe, _empty_entry())` on an insertion-ordered dict.
fn by_proc_entry<'a>(by_proc: &'a mut Vec<(String, ProcEntry)>, exe: &str) -> &'a mut ProcEntry {
    match by_proc.iter().position(|(key, _)| key == exe) {
        Some(index) => &mut by_proc[index].1,
        None => {
            by_proc.push((exe.to_string(), ProcEntry::default()));
            &mut by_proc.last_mut().unwrap().1
        }
    }
}

pub fn connection_tracking_epoch() -> u64 {
    state().lock().generation
}

/// Invalidate even a fetch already in flight in a retiring worker.
pub fn reset_connection_tracking() {
    let mut state = state().lock();
    state.seen_connections.clear();
    state.conn_owner.clear();
    state.conn_bytes.clear();
    state.conn_raw_bytes.clear();
    state.conn_route_bytes.clear();
    state.proc_total_connections.clear();
    state.proc_closed_bytes.clear();
    state.proc_closed_order.clear();
    state.proc_closed_route_bytes.clear();
    state.proc_display_names.clear();
    state.prev_proc_total.clear();
    state.prev_time = None;
    state.generation += 1;
}

/// Python `str(value or "")` for JSON leaves.
fn python_str(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(true)) => "True".to_string(),
        Some(Value::Bool(false)) => "False".to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Python `_safe_int`: `max(0, int(value or 0))` with silent failures.
fn safe_int(value: Option<&Value>) -> u64 {
    match value {
        None | Some(Value::Null) => 0,
        Some(Value::Number(number)) => {
            if let Some(unsigned) = number.as_u64() {
                unsigned
            } else if let Some(signed) = number.as_i64() {
                signed.max(0) as u64
            } else {
                // Python int() truncates toward zero; negatives clamp to zero.
                number.as_f64().map(|f| f.max(0.0) as u64).unwrap_or(0)
            }
        }
        Some(Value::Bool(flag)) => *flag as u64,
        Some(Value::String(text)) => text.trim().parse::<i64>().map_or(0, |v| v.max(0) as u64),
        _ => 0,
    }
}

/// Python `value not in (None, "")`.
fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        _ => false,
    }
}

/// Python `_metadata_value`: exact keys first, then a case-insensitive scan.
fn metadata_value<'a>(meta: &'a Value, keys: &[&str]) -> &'a Value {
    let Some(map) = meta.as_object() else {
        return &Value::Null;
    };
    static NULL: OnceLock<Value> = OnceLock::new();
    for key in keys {
        if let Some(value) = map.get(*key) {
            if !is_empty_value(value) {
                return value;
            }
        }
    }
    for key in keys {
        let lowered = key.to_lowercase();
        for (name, value) in map {
            if name.to_lowercase() == lowered && !is_empty_value(value) {
                return value;
            }
        }
    }
    NULL.get_or_init(|| Value::Null)
}

/// `ntpath.basename`, including the drive-relative `"C:file"` quirk.
fn ntpath_basename(path: &str) -> &str {
    let path = if path.as_bytes().get(1) == Some(&b':') {
        &path[2..]
    } else {
        path
    };
    match path.rfind(['/', '\\']) {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Python `_process_name_from_metadata`: (lowercased key, display name).
fn process_name_from_metadata(
    meta: &Value,
    pid_names: &mut HashMap<String, String>,
    services: Option<&dyn CoreServices>,
) -> (String, String) {
    let process_path = python_str(Some(metadata_value(
        meta,
        &["processPath", "process_path", "process"],
    )))
    .trim()
    .to_string();
    let explicit_name = python_str(Some(metadata_value(
        meta,
        &["processName", "process_name", "program", "exe"],
    )))
    .trim()
    .to_string();
    if !process_path.is_empty() || !explicit_name.is_empty() {
        let source = if process_path.is_empty() {
            explicit_name.as_str()
        } else {
            process_path.as_str()
        };
        let mut name = ntpath_basename(source).trim().to_string();
        if name.is_empty() {
            name = source.to_string();
        }
        return (name.to_lowercase(), name);
    }
    // win_proc_monitor owns the PID cache and keys it by process creation
    // time. A second PID-only cache here would return the previous program
    // after Windows recycles the numeric PID.
    let pid = metadata_value(meta, &["processID", "processId", "pid", "uid"]);
    let pid_key = python_str(Some(pid)).trim().to_string();
    if !pid_key.is_empty() {
        if !pid_names.contains_key(&pid_key) {
            pid_names.insert(pid_key.clone(), process_name_from_pid(&pid_key));
        }
        let name = pid_names[&pid_key].clone();
        if !name.is_empty() {
            return (name.to_lowercase(), name);
        }
    }
    let host = python_str(Some(metadata_value(
        meta,
        &["host", "destinationIP", "destination_ip", "dstIP", "dst_ip"],
    )))
    .trim()
    .to_string();
    if !host.is_empty() {
        let display = match services {
            Some(services) => services.translate("Системный трафик ({addr})", &[("addr", &host)]),
            None => default_translate("Системный трафик ({addr})", &[("addr", &host)]),
        };
        return (format!("system:{host}").to_lowercase(), display);
    }
    let display = match services {
        Some(services) => services.translate("Системный трафик", &[]),
        None => default_translate("Системный трафик", &[]),
    };
    ("system:unknown".to_string(), display)
}

/// Python `_validated_connection_bytes`.
fn validated_connection_bytes(
    state: &mut CollectorState,
    cid: &str,
    raw_up: u64,
    raw_down: u64,
    max_delta: u64,
) -> (u64, u64) {
    if cid.is_empty() {
        return (0, 0); // no stable identity -> cannot invent cumulative deltas
    }
    let previous = state.conn_bytes.get(cid).copied().unwrap_or((0, 0));
    let (up, down) = match state.conn_raw_bytes.get(cid).copied() {
        None => (0, 0), // first sample establishes a baseline, not session traffic
        Some((raw_previous_up, raw_previous_down)) => {
            let delta_up = raw_up.saturating_sub(raw_previous_up);
            let delta_down = raw_down.saturating_sub(raw_previous_down);
            (
                previous.0 + if delta_up <= max_delta { delta_up } else { 0 },
                previous.1 + if delta_down <= max_delta { delta_down } else { 0 },
            )
        }
    };
    state.conn_raw_bytes.insert(cid.to_string(), (raw_up, raw_down));
    state.conn_bytes.insert(cid.to_string(), (up, down));
    (up, down)
}

/// Python `_close_connection`: move a connection's totals into the closed
/// buckets of its owning process.
fn close_connection(state: &mut CollectorState, cid: &str) {
    let (up, down) = state.conn_bytes.remove(cid).unwrap_or((0, 0));
    state.conn_raw_bytes.remove(cid);
    let route_totals = state.conn_route_bytes.remove(cid).unwrap_or_default();
    let Some(exe) = state.conn_owner.remove(cid) else {
        return;
    };
    if !state.proc_closed_bytes.contains_key(&exe) {
        state.proc_closed_order.push(exe.clone());
    }
    let closed = state
        .proc_closed_bytes
        .entry(exe.clone())
        .or_insert((0, 0));
    closed.0 += up;
    closed.1 += down;
    let closed_routes = state
        .proc_closed_route_bytes
        .entry(exe.clone())
        .or_default();
    closed_routes.proxy += route_totals.proxy;
    closed_routes.direct += route_totals.direct;
    closed_routes.unknown += route_totals.unknown;
    if let Some(seen) = state.seen_connections.get_mut(&exe) {
        seen.remove(cid);
        if seen.is_empty() {
            state.seen_connections.remove(&exe);
        }
    }
}

/// Aggregate one immutable `/connections` snapshot; no second fetch per tick.
///
/// The standalone polling API is retained. Workers should supply the document
/// they already fetched and their creation-time tracking_epoch. No metadata or
/// missing chain ever implies direct traffic. Errors do not close connections.
#[allow(clippy::too_many_arguments)]
pub fn collect_process_stats(
    clash_api_port: u16,
    clash_api_secret: &str,
    connections_document: Option<&Value>,
    outbound_graph: Option<&Value>,
    tracking_epoch: Option<u64>,
    services: Option<&dyn CoreServices>,
) -> Vec<ProcessTrafficSnapshot> {
    let _ = clash_api_port; // used only by the standalone polling API below
    let epoch = tracking_epoch.unwrap_or_else(connection_tracking_epoch);

    let fetched_connections;
    let mut fetched_graph: Option<Value> = None;
    let data: &Value = match connections_document {
        Some(document) => document,
        None => {
            if clash_api_secret.is_empty() {
                return Vec::new();
            }
            let Some(services) = services else {
                return Vec::new();
            };
            let Some(document) = services.fetch_document("/connections") else {
                return Vec::new();
            };
            if outbound_graph.is_none() {
                // A failed /proxies read keeps the graph empty (MetricsApiError
                // swallowed), never implies direct traffic.
                fetched_graph = services.fetch_document("/proxies");
            }
            fetched_connections = document;
            &fetched_connections
        }
    };
    if !data.is_object() {
        return Vec::new();
    }
    let Some(connections) = data.get("connections").and_then(Value::as_array) else {
        return Vec::new();
    };
    let graph: Box<dyn ConnectionRouteClassifier> = match services {
        Some(services) => {
            services.build_classifier(outbound_graph.or(fetched_graph.as_ref()))
        }
        None => Box::new(UnroutedClassifier),
    };

    let mut state = state().lock();
    if epoch != state.generation {
        return Vec::new();
    }
    let now = monotonic_seconds();
    let dt = match state.prev_time {
        Some(previous) if previous > 0.0 => (now - previous).max(0.5),
        _ => 2.0,
    };
    state.prev_time = Some(now);
    let max_delta = (MAX_REASONABLE_BYTES_PER_SEC as f64 * dt) as u64;

    let empty_meta = Value::Object(Map::new());
    let mut active_ids: HashSet<String> = HashSet::new();
    // Python by_proc is a dict: insertion order feeds the stable result sort.
    let mut by_proc: Vec<(String, ProcEntry)> = Vec::new();
    let mut pid_names: HashMap<String, String> = HashMap::new(); // snapshot-local, never stale across PID generations
    for conn in connections {
        if !conn.is_object() {
            continue;
        }
        let meta = match conn.get("metadata") {
            Some(meta) if meta.is_object() => meta,
            _ => &empty_meta,
        };
        let (exe, display_exe) = process_name_from_metadata(meta, &mut pid_names, services);
        if HIDDEN_PROCESSES.contains(&exe.as_str()) {
            continue;
        }
        state.proc_display_names.insert(exe.clone(), display_exe);
        let entry = by_proc_entry(&mut by_proc, &exe);
        let cid = python_str(conn.get("id"));
        if !cid.is_empty() && active_ids.contains(&cid) {
            continue; // malformed duplicate must not inflate active totals
        }
        if !cid.is_empty() {
            let owner_conflict = match state.conn_owner.get(&cid) {
                Some(owner) => *owner != exe,
                None => false,
            };
            if owner_conflict {
                close_connection(&mut state, &cid);
            }
        }
        let previous = state.conn_bytes.get(&cid).copied().unwrap_or((0, 0));
        let (up, down) = validated_connection_bytes(
            &mut state,
            &cid,
            safe_int(conn.get("upload")),
            safe_int(conn.get("download")),
            max_delta,
        );
        // A non-list chain or a chain holding non-strings is Unknown, matching
        // `RouteClassifier.classify`.
        let route = match conn
            .get("chains")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().map(str::to_string).ok_or(()))
                    .collect::<Result<Vec<String>, _>>()
            })
            .transpose()
        {
            Ok(Some(chains)) if !chains.is_empty() => graph.classify(&chains),
            _ => ConnectionRoute::Unknown,
        };
        if !cid.is_empty() {
            active_ids.insert(cid.clone());
            if !state.conn_owner.contains_key(&cid) {
                *state.proc_total_connections.entry(exe.clone()).or_insert(0) += 1;
            }
            state
                .seen_connections
                .entry(exe.clone())
                .or_default()
                .insert(cid.clone());
            state.conn_owner.insert(cid.clone(), exe.clone());
            let route_totals = state.conn_route_bytes.entry(cid.clone()).or_default();
            let delta = (up.saturating_sub(previous.0)).saturating_add(down.saturating_sub(previous.1));
            route_totals.add(route, delta);
            for kind in route_totals.routes() {
                *entry.route_bytes_slot_mut(kind) += route_totals.get(kind);
            }
        }
        entry.upload += up;
        entry.download += down;
        entry.conns += 1;
        entry.routes.insert(route);
        let host = python_str(Some(metadata_value(meta, &["host", "destinationIP"])));
        if !host.is_empty() {
            entry.add_host(&host, up.saturating_add(down));
        }
    }

    let stale: Vec<String> = state
        .conn_bytes
        .keys()
        .filter(|cid| !active_ids.contains(*cid))
        .cloned()
        .collect();
    for cid in stale {
        close_connection(&mut state, &cid);
    }
    // Closed-only processes stay visible with zero speed, preserving both
    // overall and route-specific session totals after the last socket ends.
    let closed_exes: Vec<String> = state.proc_closed_order.clone();
    for exe in closed_exes {
        by_proc_entry(&mut by_proc, &exe);
    }

    let mut result: Vec<ProcessTrafficSnapshot> = Vec::new();
    for (exe, entry) in by_proc.iter_mut() {
        let (closed_up, closed_down) = state
            .proc_closed_bytes
            .get(exe)
            .copied()
            .unwrap_or((0, 0));
        let up = entry.upload + closed_up;
        let down = entry.download + closed_down;
        if let Some(closed_routes) = state.proc_closed_route_bytes.get(exe) {
            entry.route_bytes.proxy += closed_routes.proxy;
            entry.route_bytes.direct += closed_routes.direct;
            entry.route_bytes.unknown += closed_routes.unknown;
        }
        let routes: HashSet<ConnectionRoute> = entry
            .routes
            .iter()
            .copied()
            .chain(
                [
                    ConnectionRoute::Proxy,
                    ConnectionRoute::Direct,
                    ConnectionRoute::Unknown,
                ]
                .into_iter()
                .filter(|kind| entry.route_bytes.get(*kind) > 0),
            )
            .collect();
        let route = if routes.is_empty() || routes.contains(&ConnectionRoute::Unknown) {
            "unknown"
        } else if routes.len() > 1 {
            "mixed"
        } else {
            routes.iter().next().map(|route| route.as_str()).unwrap_or("unknown")
        };
        let (previous_up, previous_down) = state
            .prev_proc_total
            .get(exe)
            .copied()
            .unwrap_or((up, down));
        state.prev_proc_total.insert(exe.clone(), (up, down));
        result.push(ProcessTrafficSnapshot {
            exe: state
                .proc_display_names
                .get(exe)
                .cloned()
                .unwrap_or_else(|| exe.clone()),
            upload: up,
            download: down,
            connections: entry.conns,
            total_connections: *state.proc_total_connections.get(exe).unwrap_or(&0),
            route: route.to_string(),
            proxy_bytes: entry.route_bytes.proxy,
            direct_bytes: entry.route_bytes.direct,
            unknown_bytes: entry.route_bytes.unknown,
            top_host: entry.top_host().to_string(),
            down_speed: (down as f64 - previous_down as f64).max(0.0) / dt,
            up_speed: (up as f64 - previous_up as f64).max(0.0) / dt,
        });
    }
    result.sort_by(|left, right| {
        (right.upload + right.download).cmp(&(left.upload + left.download))
    });
    result
}

impl ProcEntry {
    fn route_bytes_slot_mut(&mut self, route: ConnectionRoute) -> &mut u64 {
        match route {
            ConnectionRoute::Proxy => &mut self.route_bytes.proxy,
            ConnectionRoute::Direct => &mut self.route_bytes.direct,
            ConnectionRoute::Unknown => &mut self.route_bytes.unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct MockClassifier;

    impl ConnectionRouteClassifier for MockClassifier {
        fn classify(&self, chains: &[String]) -> ConnectionRoute {
            match chains.first().map(String::as_str) {
                Some("proxy") => ConnectionRoute::Proxy,
                Some("direct") => ConnectionRoute::Direct,
                _ => ConnectionRoute::Unknown,
            }
        }
    }

    struct MockServices {
        documents: HashMap<String, Value>,
    }

    impl MockServices {
        fn new(documents: Value) -> Self {
            let mut map = HashMap::new();
            map.insert("/connections".to_string(), documents);
            Self { documents: map }
        }
    }

    impl CoreServices for MockServices {
        fn fetch_document(&self, path: &str) -> Option<Value> {
            self.documents.get(path).cloned()
        }
        fn build_classifier(&self, _document: Option<&Value>) -> Box<dyn ConnectionRouteClassifier> {
            Box::new(MockClassifier)
        }
    }

    /// Python tests run sequentially against one module-level state; Rust
    /// runs them in parallel threads, so state-touching tests share this lock.
    static TEST_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn epoch() -> u64 {
        // A fresh generation isolates each test from other tests' mutations.
        reset_connection_tracking();
        connection_tracking_epoch()
    }

    #[test]
    fn first_sample_establishes_a_baseline_then_counts_deltas() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let epoch = epoch();
        let document = json!({"connections": [
            {"id": "c1", "upload": 100, "download": 200, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\Apps\\Chrome\\chrome.exe"}},
            {"id": "c2", "upload": 10, "download": 20, "chains": ["direct"],
             "metadata": {"processPath": "C:\\Apps\\Chrome\\chrome.exe"}},
        ]});
        let first = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&document),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        assert_eq!(first.len(), 1);
        let snapshot = &first[0];
        assert_eq!(snapshot.exe, "chrome.exe");
        assert_eq!((snapshot.upload, snapshot.download), (0, 0));
        assert_eq!(snapshot.connections, 2);
        assert_eq!(snapshot.total_connections, 2);
        assert_eq!(snapshot.route, "mixed");

        let grown = json!({"connections": [
            {"id": "c1", "upload": 350, "download": 680, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\Apps\\Chrome\\chrome.exe"}},
        ]});
        let second = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&grown),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        assert_eq!(second.len(), 1);
        let snapshot = &second[0];
        assert_eq!((snapshot.upload, snapshot.download), (250, 480));
        assert_eq!(snapshot.connections, 1);
        assert_eq!(snapshot.route, "proxy");
        assert!(snapshot.up_speed > 0.0);
    }

    #[test]
    fn closed_connections_keep_their_session_totals() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let epoch = epoch();
        let document = json!({"connections": [
            {"id": "c1", "upload": 100, "download": 200, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\Apps\\Chrome\\chrome.exe"}},
        ]});
        collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&document),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        let grown = json!({"connections": [
            {"id": "c1", "upload": 150, "download": 250, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\Apps\\Chrome\\chrome.exe"}},
        ]});
        let second = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&grown),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        assert_eq!((second[0].upload, second[0].download), (50, 50));

        // The socket closes: the process stays visible with zero speed.
        let empty = json!({"connections": []});
        let third = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&empty),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        assert_eq!(third.len(), 1);
        assert_eq!((third[0].upload, third[0].download), (50, 50));
        assert_eq!(third[0].connections, 0);
        assert_eq!(third[0].route, "proxy");
    }

    #[test]
    fn hidden_processes_and_unnamed_hosts() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let epoch = epoch();
        let document = json!({"connections": [
            {"id": "h1", "upload": 5, "download": 5, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\core\\xray.exe"}},
            {"id": "h2", "upload": 7, "download": 9, "chains": ["unknown"],
             "metadata": {"host": "example.com"}},
        ]});
        let result = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&document),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        assert_eq!(result.len(), 1);
        // The snapshot exposes the display name; the tracking key is
        // "system:example.com".
        assert_eq!(result[0].exe, "Системный трафик (example.com)");
        assert_eq!(result[0].top_host, "example.com");
        assert_eq!(result[0].unknown_bytes, 0); // baseline sample
        assert_eq!(result[0].route, "unknown");
    }

    #[test]
    fn epoch_mismatch_returns_nothing() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let stale = epoch();
        let document = json!({"connections": [
            {"id": "e1", "upload": 1, "download": 1, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\a.exe"}},
        ]});
        reset_connection_tracking();
        let result = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&document),
            None,
            Some(stale),
            Some(&MockServices::new(json!({}))),
        );
        assert!(result.is_empty());
    }

    #[test]
    fn standalone_polling_requires_secret_and_services() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let document = json!({"connections": [
            {"id": "p1", "upload": 1, "download": 1, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\a.exe"}},
        ]});
        let epoch = epoch();
        // No secret: nothing is fetched, like the Python guard.
        assert!(collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "",
            None,
            None,
            Some(epoch),
            Some(&MockServices::new(document.clone())),
        )
        .is_empty());
        // With a secret and services the document is fetched and aggregated.
        let result = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            None,
            None,
            Some(epoch),
            Some(&MockServices::new(document)),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].exe, "a.exe");
    }

    #[test]
    fn malformed_rows_and_duplicates_are_tolerated() {
        let _guard = TEST_STATE_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let epoch = epoch();
        let document = json!({"connections": [
            "not-an-object",
            {"id": "d1", "upload": 3, "download": 4, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\a.exe"}},
            {"id": "d1", "upload": 30, "download": 40, "chains": ["proxy"],
             "metadata": {"processPath": "C:\\a.exe"}},
            {"metadata": null},
        ]});
        let result = collect_process_stats(
            SINGBOX_CLASH_API_PORT,
            "secret",
            Some(&document),
            None,
            Some(epoch),
            Some(&MockServices::new(json!({}))),
        );
        // a.exe plus the metadata-less system:unknown entry.
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].exe, "a.exe");
        assert_eq!(result[0].connections, 1);
        assert_eq!((result[0].upload, result[0].download), (0, 0));
        assert_eq!(result[1].exe, "Системный трафик");
        assert_eq!(result[1].connections, 1);
    }

    #[test]
    fn ntpath_basename_matches_python() {
        assert_eq!(ntpath_basename(r"C:\Apps\chrome.exe"), "chrome.exe");
        assert_eq!(ntpath_basename(r"C:\dir\"), "");
        assert_eq!(ntpath_basename("/a/b/"), "");
        assert_eq!(ntpath_basename("C:file"), "file");
        assert_eq!(ntpath_basename("file.exe"), "file.exe");
        assert_eq!(ntpath_basename(""), "");
    }

    #[test]
    fn safe_int_and_metadata_lookups_mirror_python() {
        assert_eq!(safe_int(Some(&json!(12))), 12);
        assert_eq!(safe_int(Some(&json!(-5))), 0);
        assert_eq!(safe_int(Some(&json!(12.9))), 12);
        assert_eq!(safe_int(Some(&json!("42"))), 42);
        assert_eq!(safe_int(Some(&json!("bad"))), 0);
        assert_eq!(safe_int(Some(&json!(true))), 1);
        assert_eq!(safe_int(Some(&Value::Null)), 0);
        assert_eq!(safe_int(None), 0);

        let meta = json!({"ProcessPath": "C:\\x\\a.exe", "PID": 42});
        assert_eq!(
            metadata_value(&meta, &["processPath", "process_path"]),
            &json!("C:\\x\\a.exe")
        );
        // The case-insensitive second pass matches "PID" for the "pid" key.
        assert_eq!(metadata_value(&meta, &["pid"]), &json!(42));
        assert_eq!(metadata_value(&meta, &["processId"]), &Value::Null);
        assert_eq!(metadata_value(&meta, &["missing"]), &Value::Null);
    }

    #[test]
    fn default_translate_substitutes_placeholders() {
        assert_eq!(
            default_translate("Системный трафик ({addr})", &[("addr", "example.com")]),
            "Системный трафик (example.com)"
        );
        assert_eq!(default_translate("Системный трафик", &[]), "Системный трафик");
    }
}
