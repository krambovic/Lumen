//! Traffic history with asynchronous atomic persistence (`traffic_history.py`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

const MAX_REASONABLE_BYTES_PER_SECOND: u64 = 2 * 1024 * 1024 * 1024;
const MIN_REASONABLE_SESSION_BYTES: u64 = 2 * 1024u64.pow(4);
const HARD_REASONABLE_SESSION_BYTES: u64 = 1024u64.pow(5);
const ROUTES: [&str; 4] = ["proxy", "direct", "mixed", "unknown"];

fn counter(value: &Value) -> u64 {
    match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0).max(0.0) as u64,
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProcessTrafficEntry {
    pub upload: u64,
    pub download: u64,
    pub route: String,
    pub proxy_bytes: u64,
    pub direct_bytes: u64,
    pub unknown_bytes: u64,
}

impl ProcessTrafficEntry {
    pub fn to_dict(&self) -> Value {
        json!({
            "upload": self.upload,
            "download": self.download,
            "route": self.route,
            "proxy_bytes": self.proxy_bytes,
            "direct_bytes": self.direct_bytes,
            "unknown_bytes": self.unknown_bytes,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        let mut entry = Self {
            upload: counter(data.get("upload").unwrap_or(&json!(0))),
            download: counter(data.get("download").unwrap_or(&json!(0))),
            route: data
                .get("route")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            proxy_bytes: counter(data.get("proxy_bytes").unwrap_or(&json!(0))),
            direct_bytes: counter(data.get("direct_bytes").unwrap_or(&json!(0))),
            unknown_bytes: counter(data.get("unknown_bytes").unwrap_or(&json!(0))),
        };
        if !["proxy_bytes", "direct_bytes", "unknown_bytes"]
            .iter()
            .any(|k| data.get(*k).is_some())
        {
            // Legacy mixed totals cannot be split honestly; retain as unknown.
            let kind = if entry.route == "proxy" || entry.route == "direct" {
                entry.route.clone()
            } else {
                "unknown".to_string()
            };
            let total = entry.upload + entry.download;
            match kind.as_str() {
                "proxy" => entry.proxy_bytes = total,
                "direct" => entry.direct_bytes = total,
                _ => entry.unknown_bytes = total,
            }
        }
        entry
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TrafficSession {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub node_name: String,
    pub mode: String,
    pub total_upload: u64,
    pub total_download: u64,
    pub processes: BTreeMap<String, ProcessTrafficEntry>,
}

impl TrafficSession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ..Default::default()
        }
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "id": self.id,
            "started_at": self.started_at,
            "ended_at": self.ended_at.clone().map(Value::String).unwrap_or(Value::Null),
            "node_name": self.node_name,
            "mode": self.mode,
            "total_upload": self.total_upload,
            "total_download": self.total_download,
            "processes": self
                .processes
                .iter()
                .map(|(k, v)| (k.clone(), v.to_dict()))
                .collect::<BTreeMap<_, _>>(),
        })
    }

    pub fn from_dict(data: &Value) -> anyhow::Result<Self> {
        let processes_raw = data
            .get("processes")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("invalid process history"))?;
        Ok(Self {
            id: data
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            started_at: data
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ended_at: data
                .get("ended_at")
                .filter(|v| v.is_string())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            node_name: data
                .get("node_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            mode: data
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            total_upload: counter(data.get("total_upload").unwrap_or(&json!(0))),
            total_download: counter(data.get("total_download").unwrap_or(&json!(0))),
            processes: processes_raw
                .iter()
                .filter(|(_, v)| v.is_object())
                .map(|(k, v)| (k.clone(), ProcessTrafficEntry::from_dict(v)))
                .collect(),
        })
    }
}

/// Per-process update payload: (upload, download, route) or with routed bytes.
#[derive(Debug, Clone)]
pub struct ProcessStatsUpdate {
    pub upload: u64,
    pub download: u64,
    pub route: String,
    pub proxy_bytes: u64,
    pub direct_bytes: u64,
    pub unknown_bytes: u64,
}

enum WriterCommand {
    Submit { generation: u64, payload: String },
    Close,
}

struct WriterState {
    latest: u64,
    durable: u64,
    failed: u64,
    last_error: String,
    closed: bool,
}

/// One lazy writer thread per storage; only the newest pending payload is kept.
struct HistoryWriter {
    tx: Option<Sender<WriterCommand>>,
    handle: Option<JoinHandle<()>>,
    state: Arc<Mutex<WriterState>>,
}

impl HistoryWriter {
    fn new(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<WriterCommand>();
        let state = Arc::new(Mutex::new(WriterState {
            latest: 0,
            durable: 0,
            failed: 0,
            last_error: String::new(),
            closed: false,
        }));
        let thread_state = Arc::clone(&state);
        let handle = std::thread::Builder::new()
            .name("lumen-history-writer".into())
            .spawn(move || {
                loop {
                    match rx.recv() {
                        Ok(WriterCommand::Submit { generation, payload }) => {
                            {
                                let mut s = thread_state.lock().unwrap();
                                if generation > s.latest {
                                    s.latest = generation;
                                }
                            }
                            let superseded = {
                                let s = thread_state.lock().unwrap();
                                generation < s.latest
                            };
                            if !superseded {
                                if write_snapshot(&path, &payload) {
                                    let mut s = thread_state.lock().unwrap();
                                    s.durable = s.durable.max(generation);
                                    s.last_error.clear();
                                } else {
                                    let mut s = thread_state.lock().unwrap();
                                    s.failed = s.failed.max(generation);
                                    s.last_error = "traffic history write failed".to_string();
                                }
                            }
                        }
                        Ok(WriterCommand::Close) | Err(_) => {
                            return;
                        }
                    }
                }
            })
            .ok();
        Self {
            tx: Some(tx),
            handle,
            state,
        }
    }

    fn submit(&self, generation: u64, payload: String) {
        let mut state = self.state.lock().unwrap();
        if state.closed || generation <= state.latest {
            return;
        }
        state.latest = generation;
        drop(state);
        if let Some(tx) = &self.tx {
            let _ = tx.send(WriterCommand::Submit { generation, payload });
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let (latest, durable, failed) = {
                let s = self.state.lock().unwrap();
                (s.latest, s.durable, s.failed)
            };
            if durable >= latest {
                return true;
            }
            if failed >= latest {
                return false;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn close(&mut self, timeout: Duration) -> bool {
        {
            let mut s = self.state.lock().unwrap();
            s.closed = true;
        }
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(WriterCommand::Close);
        }
        if let Some(handle) = self.handle.take() {
            let deadline = std::time::Instant::now() + timeout;
            while !handle.is_finished() {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = handle.join();
        }
        true
    }
}

fn write_snapshot(path: &Path, payload: &str) -> bool {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let staged = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        Uuid::new_v4().simple()
    ));
    let result = (|| -> std::io::Result<()> {
        fs::write(&staged, payload)?;
        fs::rename(&staged, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result.is_ok()
}

fn parse_iso_datetime(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if value.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn session_day_key(started_at: &str) -> String {
    match parse_iso_datetime(started_at) {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => {
            if started_at.len() >= 10
                && started_at.as_bytes().get(4) == Some(&b'-')
                && started_at.as_bytes().get(7) == Some(&b'-')
            {
                started_at[..10].to_string()
            } else {
                String::new()
            }
        }
    }
}

/// Thread-safe history with asynchronous atomic persistence.
pub struct TrafficHistoryStorage {
    inner: Mutex<HistoryInner>,
    writer: Mutex<HistoryWriter>,
    generation: AtomicU64,
}

struct HistoryInner {
    sessions: Vec<TrafficSession>,
    daily_totals: BTreeMap<String, (u64, u64)>,
    current_session: Option<usize>,
    closed: bool,
}

impl TrafficHistoryStorage {
    pub fn new() -> Self {
        Self::with_path(crate::constants::data_dir().join("traffic_history.json"))
    }

    /// Python `_load`: an unreadable or non-object history file is moved
    /// aside (`<name>.corrupt-<UTC stamp>-<id>`) instead of being overwritten
    /// by the next save.
    pub fn with_path(path: PathBuf) -> Self {
        let mut sessions: Vec<TrafficSession> = Vec::new();
        if path.exists() {
            let parsed = fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str::<Value>(&data).ok())
                .filter(Value::is_object);
            match parsed {
                Some(parsed) => {
                    if let Some(list) = parsed.get("sessions").and_then(|v| v.as_array()) {
                        for item in list {
                            if let Ok(session) = TrafficSession::from_dict(item) {
                                sessions.push(session);
                            }
                        }
                    }
                }
                None => Self::quarantine_corrupt_file(&path),
            }
        }
        let storage = Self {
            inner: Mutex::new(HistoryInner {
                sessions,
                daily_totals: BTreeMap::new(),
                current_session: None,
                closed: false,
            }),
            writer: Mutex::new(HistoryWriter::new(path.clone())),
            generation: AtomicU64::new(0),
        };
        // Sanitize loaded sessions and refresh daily totals.
        let mut inner = storage.inner.lock().unwrap();
        for session in &mut inner.sessions {
            Self::sanitize_session(session);
        }
        inner.daily_totals = Self::build_daily_totals(&inner.sessions);
        drop(inner);
        storage.save_locked();
        storage
    }

    fn quarantine_corrupt_file(path: &std::path::Path) {
        let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let id = uuid::Uuid::new_v4().simple().to_string();
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = fs::rename(path, path.with_file_name(format!("{name}.corrupt-{stamp}-{}", &id[..8])));
    }

    fn build_daily_totals(
        sessions: &[TrafficSession],
    ) -> BTreeMap<String, (u64, u64)> {
        let mut totals = BTreeMap::new();
        for session in sessions {
            let key = session_day_key(&session.started_at);
            if key.is_empty() {
                continue;
            }
            let entry = totals.entry(key).or_insert((0u64, 0u64));
            entry.0 += session.total_upload;
            entry.1 += session.total_download;
        }
        totals
    }

    fn snapshot(&self, inner: &HistoryInner) -> String {
        #[derive(Serialize)]
        struct DailyTotal {
            upload: u64,
            download: u64,
        }

        #[derive(Serialize)]
        struct Snapshot<'a> {
            sessions: &'a [TrafficSession],
            daily_totals: BTreeMap<&'a str, DailyTotal>,
        }

        let payload = Snapshot {
            sessions: &inner.sessions,
            daily_totals: inner
                .daily_totals
                .iter()
                .map(|(date, &(upload, download))| (date.as_str(), DailyTotal { upload, download }))
                .collect(),
        };
        serde_json::to_string(&payload).unwrap_or_default()
    }

    fn save_locked(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.daily_totals = Self::build_daily_totals(&inner.sessions);
        let snapshot = self.snapshot(&inner);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        drop(inner);
        self.writer.lock().unwrap().submit(generation, snapshot);
    }

    pub fn start_session(&self, node_name: &str, mode: &str) -> String {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return String::new();
        }
        Self::end_session_locked(&mut inner);
        let session = TrafficSession {
            started_at: chrono::Utc::now().to_rfc3339(),
            node_name: node_name.to_string(),
            mode: mode.to_string(),
            ..TrafficSession::new()
        };
        let id = session.id.clone();
        inner.current_session = Some(inner.sessions.len());
        inner.sessions.push(session);
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();
        inner.sessions.retain(|s| s.started_at >= cutoff);
        // Re-anchor the current session after retention pruning.
        inner.current_session = Some(inner.sessions.len() - 1);
        drop(inner);
        self.save_locked();
        id
    }

    /// Accept per-process stats: cumulative bytes, not rates.
    pub fn update_session(&self, process_stats: &BTreeMap<String, ProcessStatsUpdate>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return;
        }
        let Some(index) = inner.current_session else {
            return;
        };
        let session = &mut inner.sessions[index];
        for (exe, stats) in process_stats {
            let route = if ROUTES.contains(&stats.route.as_str()) {
                stats.route.clone()
            } else {
                "unknown".to_string()
            };
            let entry = session
                .processes
                .entry(exe.clone())
                .or_insert_with(|| ProcessTrafficEntry {
                    route: route.clone(),
                    ..Default::default()
                });
            entry.upload = stats.upload;
            entry.download = stats.download;
            if entry.route != route {
                entry.route = if entry.route == "unknown" || route == "unknown" {
                    "unknown".to_string()
                } else {
                    "mixed".to_string()
                };
            }
            entry.proxy_bytes = stats.proxy_bytes;
            entry.direct_bytes = stats.direct_bytes;
            entry.unknown_bytes = stats.unknown_bytes;
        }
        let session = &mut inner.sessions[index];
        Self::sanitize_session(session);
    }

    fn session_duration_seconds(started_at: &str, ended_at: Option<&String>) -> u64 {
        let started = parse_iso_datetime(started_at);
        let ended = ended_at
            .and_then(|v| parse_iso_datetime(v))
            .unwrap_or_else(chrono::Utc::now);
        match started {
            Some(s) => (ended - s).num_seconds().max(0) as u64,
            None => 0,
        }
    }

    fn session_limit_bytes(started_at: &str, ended_at: Option<&String>) -> u64 {
        let seconds = Self::session_duration_seconds(started_at, ended_at);
        (MIN_REASONABLE_SESSION_BYTES)
            .min(HARD_REASONABLE_SESSION_BYTES)
            .max(seconds.saturating_mul(MAX_REASONABLE_BYTES_PER_SECOND))
    }

    fn sanitize_session(session: &mut TrafficSession) {
        let limit = Self::session_limit_bytes(&session.started_at, session.ended_at.as_ref());
        for entry in session.processes.values_mut() {
            entry.upload = if entry.upload <= limit { entry.upload } else { 0 };
            entry.download = if entry.download <= limit { entry.download } else { 0 };
            if !ROUTES.contains(&entry.route.as_str()) {
                entry.route = "unknown".to_string();
            }
            let total = entry.upload + entry.download;
            let routed = entry.proxy_bytes + entry.direct_bytes + entry.unknown_bytes;
            if routed > total {
                entry.proxy_bytes = 0;
                entry.direct_bytes = 0;
                entry.unknown_bytes = total;
                entry.route = "unknown".to_string();
            } else if routed < total {
                entry.unknown_bytes += total - routed;
            }
            if entry.unknown_bytes > 0 {
                entry.route = "unknown".to_string();
            }
        }
        if session.processes.is_empty() {
            session.total_upload = if session.total_upload <= limit {
                session.total_upload
            } else {
                0
            };
            session.total_download = if session.total_download <= limit {
                session.total_download
            } else {
                0
            };
        } else {
            let up: u64 = session.processes.values().map(|e| e.upload).sum();
            let down: u64 = session.processes.values().map(|e| e.download).sum();
            session.total_upload = if up <= limit { up } else { 0 };
            session.total_download = if down <= limit { down } else { 0 };
        }
    }

    fn end_session_locked(inner: &mut HistoryInner) {
        if let Some(index) = inner.current_session.take() {
            if let Some(session) = inner.sessions.get_mut(index) {
                session.ended_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }
    }

    pub fn end_session(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.closed {
                Self::end_session_locked(&mut inner);
            } else {
                return;
            }
        }
        self.save_locked();
    }

    pub fn save_periodic(&self) {
        let inner = self.inner.lock().unwrap();
        if inner.current_session.is_some() && !inner.closed {
            drop(inner);
            self.save_locked();
        }
    }

    pub fn clear(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.sessions.clear();
            inner.daily_totals.clear();
            inner.current_session = None;
        }
        self.save_locked();
    }

    pub fn flush(&self, timeout: Duration) -> bool {
        self.save_locked();
        self.writer.lock().unwrap().flush(timeout)
    }

    pub fn close(&self, timeout: Duration) -> bool {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.closed {
                Self::end_session_locked(&mut inner);
                inner.closed = true;
                inner.daily_totals = Self::build_daily_totals(&inner.sessions);
                let snapshot = self.snapshot(&inner);
                let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                drop(inner);
                let mut writer = self.writer.lock().unwrap();
                writer.submit(generation, snapshot);
                return writer.close(timeout);
            }
        }
        self.writer.lock().unwrap().close(timeout)
    }

    pub fn last_save_error(&self) -> String {
        self.writer.lock().unwrap().state.lock().unwrap().last_error.clone()
    }

    pub fn get_sessions(&self, days: i64) -> Vec<TrafficSession> {
        self.with_sessions(days, |sessions| sessions.cloned().collect())
    }

    /// Read sessions while their storage lock is held. The callback may
    /// borrow each session but must not re-enter this storage.
    pub fn with_sessions<T>(
        &self,
        days: i64,
        read: impl FnOnce(&mut dyn Iterator<Item = &TrafficSession>) -> T,
    ) -> T {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let inner = self.inner.lock().unwrap();
        read(&mut inner.sessions.iter().filter(|session| session.started_at >= cutoff))
    }

    pub fn get_daily_totals(&self, days: i64) -> BTreeMap<String, (u64, u64)> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string();
        let inner = self.inner.lock().unwrap();
        let totals = Self::build_daily_totals(&inner.sessions);
        totals
            .into_iter()
            .filter(|(key, _)| *key >= cutoff)
            .collect()
    }

    pub fn get_process_totals(
        &self,
        days: i64,
    ) -> BTreeMap<String, BTreeMap<String, Value>> {
        let totals = self.with_sessions(days, |sessions| {
            let mut totals: BTreeMap<String, ProcessTrafficEntry> = BTreeMap::new();
            for session in sessions {
                for (exe, entry) in &session.processes {
                    let Some(result) = totals.get_mut(exe) else {
                        totals.insert(exe.clone(), entry.clone());
                        continue;
                    };
                    result.upload += entry.upload;
                    result.download += entry.download;
                    result.proxy_bytes += entry.proxy_bytes;
                    result.direct_bytes += entry.direct_bytes;
                    result.unknown_bytes += entry.unknown_bytes;
                    if entry.route != result.route {
                        let route = if entry.route == "unknown" || result.route == "unknown" {
                            "unknown"
                        } else {
                            "mixed"
                        };
                        if result.route != route {
                            result.route = route.into();
                        }
                    }
                }
            }
            totals
        });
        totals
            .into_iter()
            .map(|(exe, entry)| {
                let value = BTreeMap::from([
                    ("upload".into(), json!(entry.upload)),
                    ("download".into(), json!(entry.download)),
                    ("route".into(), json!(entry.route)),
                    ("proxy_bytes".into(), json!(entry.proxy_bytes)),
                    ("direct_bytes".into(), json!(entry.direct_bytes)),
                    ("unknown_bytes".into(), json!(entry.unknown_bytes)),
                ]);
                (exe, value)
            })
            .collect()
    }

    pub fn current_session(&self) -> Option<TrafficSession> {
        let inner = self.inner.lock().unwrap();
        inner.current_session.and_then(|i| inner.sessions.get(i).cloned())
    }
}

impl Default for TrafficHistoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod quarantine_tests {
    use super::*;

    #[test]
    fn borrowed_sessions_keep_period_filter_and_match_disk_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let storage = TrafficHistoryStorage::with_path(path.clone());
        let id = storage.start_session("Example", "proxy");
        assert!(storage.flush(Duration::from_secs(3)));
        storage.with_sessions(1, |sessions| {
            let recent = sessions.collect::<Vec<_>>();
            let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(recent.len(), 1);
            assert_eq!(recent[0].id, id);
            assert_eq!(saved["sessions"][0], recent[0].to_dict());
            assert!(saved["daily_totals"].is_object());
        });
        storage.with_sessions(0, |sessions| assert_eq!(sessions.count(), 0));
    }

    /// Python test_audit_traffic_history / test_persistence_and_startup.
    #[test]
    fn corrupt_history_is_kept_aside_and_new_history_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("corrupt.json");
        fs::write(&target, r#"{"sessions": [{"#).unwrap();
        let storage = TrafficHistoryStorage::with_path(target.clone());
        storage.start_session("new", "proxy");
        assert!(storage.flush(Duration::from_secs(3)));
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("corrupt.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), r#"{"sessions": [{"#);
        let saved: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(saved["sessions"][0]["node_name"], "new");
    }
}

