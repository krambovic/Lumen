//! Domain-separated bounded asynchronous logging; only the worker touches files.
//!
//! Port of `windows/xray_fluent/logging_setup.py`. Files written under `log_dir`:
//! `core.log`, `app.log`, `traffic.log` (human format, per domain) and
//! `errors.log` (JSON lines, WARNING+, diagnostic-filtered). Each file rotates
//! at 2 MiB keeping 3 backups (`name.1` .. `name.3`).
//!
//! Diagnostics upload (`diagnostics_uploader`) attaches through
//! [`set_upload_sink`]; it receives WARNING+ records that pass the domain,
//! diagnostic and engine-noise filters, already formatted as UTC JSON lines.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::thread::{JoinHandle, ThreadId};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use parking_lot::{Condvar, Mutex};
use regex::Regex;
use serde_json::Value;

use crate::log_utils::is_routine_core_log;
use crate::secret_scrubber::scrub_text;

pub const ROOT_LOGGER_NAME: &str = "xray_fluent";
pub const CORE_LOGGER_NAME: &str = "xray_fluent.core";
pub const TRAFFIC_LOGGER_NAME: &str = "xray_fluent.traffic";
pub const APP_LOGGER_NAME: &str = "xray_fluent.app";
const CORE_CHILDREN: &[&str] = &["xray_fluent.zapret_manager"];
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const BACKUP_COUNT: usize = 3;
const FILE_QUEUE_MAX: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Debug = 10,
    Info = 20,
    Warning = 30,
    Error = 40,
    Critical = 50,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
            Level::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogRecord {
    pub created: DateTime<Utc>,
    pub level: Level,
    pub logger: String,
    pub msg: String,
    pub exc_text: Option<String>,
    pub domain: String,
}

impl LogRecord {
    pub fn new(logger: &str, level: Level, msg: &str) -> Self {
        Self {
            created: Utc::now(),
            level,
            logger: logger.to_string(),
            msg: msg.to_string(),
            exc_text: None,
            domain: String::new(),
        }
    }

    /// Scrubbed copy with a resolved domain (equivalent of `prepare_record` +
    /// `_DomainFilter`).
    pub fn prepared(&self) -> LogRecord {
        let mut safe = self.clone();
        safe.msg = scrub(&self.msg);
        safe.exc_text = self.exc_text.as_deref().map(scrub);
        safe.domain = if matches!(self.domain.as_str(), "app" | "core" | "traffic") {
            self.domain.clone()
        } else {
            domain_for(&self.logger).to_string()
        };
        safe
    }
}

fn scrub(text: &str) -> String {
    scrub_text(&Value::String(text.to_string()))
}

pub fn domain_for(name: &str) -> &'static str {
    if name == CORE_LOGGER_NAME || name.starts_with("xray_fluent.core.") || CORE_CHILDREN.contains(&name) {
        return "core";
    }
    if name == TRAFFIC_LOGGER_NAME || name.starts_with("xray_fluent.traffic.") {
        return "traffic";
    }
    "app"
}

/// `_DiagnosticFilter`: drop transient connection chatter from error reports.
pub fn diagnostic_filter(record: &LogRecord) -> bool {
    let msg = record.msg.to_lowercase();
    let noisy = [
        "connection:",
        "handshake",
        "dial tcp",
        "unexpected http response status",
        "unexpected response status",
    ]
    .iter()
    .any(|t| msg.contains(t));
    !(noisy || is_routine_core_log(&msg))
}

static ENGINE_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[(info|warning|error|debug)\]").unwrap());
const ENGINE_TOKENS: &[&str] = &["common/errors", "infra/conf", "deprecated", "migrate to"];
const APP_TOKENS: &[&str] = &[
    "права администратора",
    "прав администратора",
    "повышенными правами",
    "windivert могут работать нестабильно",
    "select a server first",
    "сначала выберите сервер",
];

/// `_EngineNoiseFilter`: drop expected app warnings and raw engine lines.
pub fn engine_noise_filter(record: &LogRecord) -> bool {
    let msg = &record.msg;
    let low = msg.to_lowercase();
    if APP_TOKENS.iter().any(|t| low.contains(t)) {
        return false;
    }
    let domain = if record.domain.is_empty() { domain_for(&record.logger) } else { record.domain.as_str() };
    if domain != "core" {
        return true;
    }
    !(ENGINE_LINE_RE.is_match(msg) || ENGINE_TOKENS.iter().any(|t| low.contains(t)) || is_routine_core_log(msg))
}

/// `%(asctime)s | %(levelname)-7s | %(xdomain)-7s | %(name)s | %(message)s`.
pub fn format_human(record: &LogRecord) -> String {
    let ts = record.created.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S");
    let mut line = format!(
        "{ts} | {:<7} | {:<7} | {} | {}",
        record.level.name(),
        record.domain,
        record.logger,
        record.msg
    );
    if let Some(exc) = &record.exc_text {
        line.push('\n');
        line.push_str(exc);
    }
    scrub(&line)
}

fn json_str(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

/// JSON line identical to Python `json.dumps(payload, ensure_ascii=False)`.
pub fn format_json(record: &LogRecord, utc: bool) -> String {
    let ts = if utc {
        record.created.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    } else {
        record.created.with_timezone(&Local).format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
    };
    let mut out = format!(
        "{{\"ts\": {}, \"level\": {}, \"domain\": {}, \"logger\": {}, \"msg\": {}",
        json_str(&ts),
        json_str(record.level.name()),
        json_str(&record.domain),
        json_str(&record.logger),
        json_str(&record.msg)
    );
    if let Some(exc) = &record.exc_text {
        out.push_str(&format!(", \"exc\": {}", json_str(exc)));
    }
    out.push('}');
    out
}

#[derive(Clone, Copy)]
enum SinkFormat {
    Human,
    Json,
}

struct RotatingFileSink {
    path: PathBuf,
    domain: Option<&'static str>,
    level: Level,
    format: SinkFormat,
    stream: Option<File>,
    size: u64,
}

impl RotatingFileSink {
    fn new(path: PathBuf, domain: Option<&'static str>, level: Level, format: SinkFormat) -> Self {
        Self { path, domain, level, format, stream: None, size: 0 }
    }

    fn open(&mut self) -> std::io::Result<()> {
        if self.stream.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new().create(true).append(true).open(&self.path)?;
            self.size = file.metadata().map(|m| m.len()).unwrap_or(0);
            self.stream = Some(file);
        }
        Ok(())
    }

    fn backup(&self, index: usize) -> PathBuf {
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.path.with_file_name(format!("{name}.{index}"))
    }

    fn handle(&mut self, record: &LogRecord) -> std::io::Result<()> {
        match self.domain {
            Some(domain) if record.domain != domain => return Ok(()),
            None if !diagnostic_filter(record) => return Ok(()),
            _ => {}
        }
        let text = match self.format {
            SinkFormat::Human => format_human(record),
            SinkFormat::Json => format_json(record, false),
        };
        let data = format!("{text}\n").into_bytes();
        let result = (|| {
            self.open()?;
            if self.size > 0 && self.size + data.len() as u64 > MAX_BYTES {
                self.close();
                for index in (2..=BACKUP_COUNT).rev() {
                    let previous = self.backup(index - 1);
                    if previous.exists() {
                        replace_file(&previous, &self.backup(index))?;
                    }
                }
                replace_file(&self.path, &self.backup(1))?;
                self.open()?;
            }
            let stream = self.stream.as_mut().expect("opened");
            stream.write_all(&data)?;
            stream.flush()?;
            self.size += data.len() as u64;
            Ok(())
        })();
        if result.is_err() {
            self.close();
        }
        result
    }

    fn close(&mut self) {
        self.stream = None;
    }
}

fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    // std::fs::rename uses MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows,
    // matching Python's Path.replace.
    std::fs::rename(from, to)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoggingStats {
    pub accepted: u64,
    pub completed: u64,
    pub queued: u64,
    pub dropped: u64,
    pub sink_failures: u64,
}

struct QueueState {
    pending: VecDeque<(u64, LogRecord)>,
    stopping: bool,
    accepted: u64,
    completed: u64,
    dropped: u64,
    failures: u64,
    finished: bool,
}

struct Shared {
    state: Mutex<QueueState>,
    cond: Condvar,
    capacity: usize,
}

/// Drop newest on overflow; FIFO, snapshot flush barriers, bounded joins.
pub struct AsyncFileHandler {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    thread_id: ThreadId,
}

impl AsyncFileHandler {
    fn start(sinks: Vec<RotatingFileSink>, capacity: usize) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(QueueState {
                pending: VecDeque::new(),
                stopping: false,
                accepted: 0,
                completed: 0,
                dropped: 0,
                failures: 0,
                finished: false,
            }),
            cond: Condvar::new(),
            capacity: capacity.max(1),
        });
        let worker = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("lumen-file-logs".into())
            .spawn(move || run_worker(worker, sinks))
            .expect("spawn lumen-file-logs");
        let thread_id = handle.thread().id();
        Self { shared, thread: Mutex::new(Some(handle)), thread_id }
    }

    pub fn stats(&self) -> LoggingStats {
        let s = self.shared.state.lock();
        LoggingStats {
            accepted: s.accepted,
            completed: s.completed,
            queued: s.pending.len() as u64,
            dropped: s.dropped,
            sink_failures: s.failures,
        }
    }

    pub fn emit(&self, record: LogRecord) {
        let mut s = self.shared.state.lock();
        if s.stopping || s.pending.len() >= self.shared.capacity {
            s.dropped += 1;
            return;
        }
        s.accepted += 1;
        let seq = s.accepted;
        s.pending.push_back((seq, record));
        self.shared.cond.notify_all();
    }

    pub fn flush(&self, timeout: Duration) -> bool {
        if std::thread::current().id() == self.thread_id {
            return false;
        }
        let deadline = Instant::now() + timeout;
        let mut s = self.shared.state.lock();
        let target = s.accepted;
        while s.completed < target && !s.finished {
            if self.shared.cond.wait_until(&mut s, deadline).timed_out() {
                return s.completed >= target;
            }
        }
        s.completed >= target
    }

    pub fn is_alive(&self) -> bool {
        !self.shared.state.lock().finished
    }

    pub fn close(&self, wait: bool, timeout: Duration) {
        {
            let mut s = self.shared.state.lock();
            s.stopping = true;
            self.shared.cond.notify_all();
        }
        if wait && std::thread::current().id() != self.thread_id {
            let deadline = Instant::now() + timeout;
            let mut s = self.shared.state.lock();
            while !s.finished {
                if self.shared.cond.wait_until(&mut s, deadline).timed_out() {
                    break;
                }
            }
            let finished = s.finished;
            drop(s);
            if finished {
                if let Some(handle) = self.thread.lock().take() {
                    let _ = handle.join();
                }
            }
        }
    }
}

fn run_worker(shared: Arc<Shared>, mut sinks: Vec<RotatingFileSink>) {
    loop {
        let (seq, record) = {
            let mut s = shared.state.lock();
            while s.pending.is_empty() && !s.stopping {
                shared.cond.wait(&mut s);
            }
            match s.pending.pop_front() {
                Some(item) => item,
                None => break,
            }
        };
        let mut failures = 0;
        for sink in sinks.iter_mut() {
            if record.level < sink.level {
                continue;
            }
            if sink.handle(&record).is_err() {
                failures += 1;
            }
        }
        let mut s = shared.state.lock();
        s.failures += failures;
        s.completed = seq;
        shared.cond.notify_all();
    }
    for sink in sinks.iter_mut() {
        sink.close();
    }
    let mut s = shared.state.lock();
    s.finished = true;
    shared.cond.notify_all();
}

pub type UploadSink = Arc<dyn Fn(&LogRecord, &str) + Send + Sync>;

struct Global {
    configured: bool,
    file_handler: Option<Arc<AsyncFileHandler>>,
    upload_sink: Option<UploadSink>,
}

static GLOBAL: LazyLock<Mutex<Global>> =
    LazyLock::new(|| Mutex::new(Global { configured: false, file_handler: None, upload_sink: None }));

#[derive(Debug, thiserror::Error)]
#[error("Previous file logger is still draining")]
pub struct StillDraining;

/// Install once; all file creation and rotation is deferred to the worker.
pub fn configure_logging(log_dir: &Path) -> Result<(), StillDraining> {
    let mut g = GLOBAL.lock();
    if g.configured {
        return Ok(());
    }
    if let Some(handler) = &g.file_handler {
        if handler.is_alive() {
            return Err(StillDraining);
        }
    }
    let mut sinks: Vec<RotatingFileSink> = ["core", "app", "traffic"]
        .into_iter()
        .map(|d| RotatingFileSink::new(log_dir.join(format!("{d}.log")), Some(d), Level::Debug, SinkFormat::Human))
        .collect();
    sinks.push(RotatingFileSink::new(log_dir.join("errors.log"), None, Level::Warning, SinkFormat::Json));
    g.file_handler = Some(Arc::new(AsyncFileHandler::start(sinks, FILE_QUEUE_MAX)));
    g.configured = true;
    Ok(())
}

/// Attach (or with `None` detach) the diagnostics uploader.
pub fn set_upload_sink(sink: Option<UploadSink>) {
    GLOBAL.lock().upload_sink = sink;
}

pub fn flush_logging(timeout: Duration) -> bool {
    let handler = GLOBAL.lock().file_handler.clone();
    handler.map_or(true, |h| h.flush(timeout))
}

pub fn logging_stats() -> LoggingStats {
    let handler = GLOBAL.lock().file_handler.clone();
    handler.map(|h| h.stats()).unwrap_or_default()
}

/// Detach immediately; `false` means a worker still owns pending file I/O.
pub fn shutdown_logging(wait: bool, timeout: Duration) -> bool {
    let handler = {
        let mut g = GLOBAL.lock();
        g.upload_sink = None;
        g.configured = false;
        g.file_handler.clone()
    };
    let Some(handler) = handler else { return true };
    handler.close(wait, timeout);
    !handler.is_alive()
}

/// Emit a record through the configured handlers.
pub fn emit(record: LogRecord) {
    let (handler, upload) = {
        let g = GLOBAL.lock();
        (g.file_handler.clone().filter(|_| g.configured), g.upload_sink.clone())
    };
    if handler.is_none() && upload.is_none() {
        return;
    }
    let safe = record.prepared();
    if let Some(upload) = upload {
        if safe.level >= Level::Warning && diagnostic_filter(&safe) && engine_noise_filter(&safe) {
            upload(&safe, &format_json(&safe, true));
        }
    }
    if let Some(handler) = handler {
        handler.emit(safe);
    }
}

#[derive(Debug, Clone)]
pub struct Logger {
    name: String,
}

impl Logger {
    pub fn named(name: &str) -> Self {
        Self { name: name.to_string() }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn log(&self, level: Level, msg: impl AsRef<str>) {
        emit(LogRecord::new(&self.name, level, msg.as_ref()));
    }
    pub fn log_exc(&self, level: Level, msg: impl AsRef<str>, exc: impl AsRef<str>) {
        let mut record = LogRecord::new(&self.name, level, msg.as_ref());
        record.exc_text = Some(exc.as_ref().to_string());
        emit(record);
    }
    pub fn debug(&self, msg: impl AsRef<str>) {
        self.log(Level::Debug, msg)
    }
    pub fn info(&self, msg: impl AsRef<str>) {
        self.log(Level::Info, msg)
    }
    pub fn warning(&self, msg: impl AsRef<str>) {
        self.log(Level::Warning, msg)
    }
    pub fn error(&self, msg: impl AsRef<str>) {
        self.log(Level::Error, msg)
    }
    pub fn critical(&self, msg: impl AsRef<str>) {
        self.log(Level::Critical, msg)
    }
}

pub fn get_logger(domain: &str) -> Logger {
    Logger::named(match domain {
        "core" => CORE_LOGGER_NAME,
        "traffic" => TRAFFIC_LOGGER_NAME,
        _ => APP_LOGGER_NAME,
    })
}

/// Root (`xray_fluent`) logger, used for module-level messages.
pub fn root_logger() -> Logger {
    Logger::named(ROOT_LOGGER_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        assert_eq!(domain_for("xray_fluent.core"), "core");
        assert_eq!(domain_for("xray_fluent.core.xray"), "core");
        assert_eq!(domain_for("xray_fluent.zapret_manager"), "core");
        assert_eq!(domain_for("xray_fluent.traffic"), "traffic");
        assert_eq!(domain_for("xray_fluent.app"), "app");
        assert_eq!(domain_for("xray_fluent.corex"), "app");
    }

    #[test]
    fn json_format_matches_python_layout() {
        let mut r = LogRecord::new(APP_LOGGER_NAME, Level::Warning, "привет \"x\"");
        r.created = DateTime::parse_from_rfc3339("2026-01-02T03:04:05.678Z").unwrap().with_timezone(&Utc);
        let r = r.prepared();
        assert_eq!(
            format_json(&r, true),
            "{\"ts\": \"2026-01-02T03:04:05.678Z\", \"level\": \"WARNING\", \"domain\": \"app\", \"logger\": \"xray_fluent.app\", \"msg\": \"привет \\\"x\\\"\"}"
        );
        assert!(format_human(&r).contains(" | WARNING | app     | xray_fluent.app | "));
    }

    #[test]
    fn filters() {
        let core = LogRecord::new(CORE_LOGGER_NAME, Level::Warning, "[Warning] common/errors: x").prepared();
        assert!(!engine_noise_filter(&core));
        let app = LogRecord::new(APP_LOGGER_NAME, Level::Warning, "Сначала выберите сервер").prepared();
        assert!(!engine_noise_filter(&app));
        let hs = LogRecord::new(APP_LOGGER_NAME, Level::Error, "tls handshake eof").prepared();
        assert!(!diagnostic_filter(&hs));
    }

    #[test]
    fn async_handler_writes_and_rotates() {
        let dir = std::env::temp_dir().join(format!("lumen-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sinks = vec![
            RotatingFileSink::new(dir.join("app.log"), Some("app"), Level::Debug, SinkFormat::Human),
            RotatingFileSink::new(dir.join("errors.log"), None, Level::Warning, SinkFormat::Json),
        ];
        let h = AsyncFileHandler::start(sinks, 16);
        h.emit(LogRecord::new(APP_LOGGER_NAME, Level::Info, "hello").prepared());
        h.emit(LogRecord::new(APP_LOGGER_NAME, Level::Error, "boom").prepared());
        assert!(h.flush(Duration::from_secs(5)));
        let app = std::fs::read_to_string(dir.join("app.log")).unwrap();
        assert!(app.contains("hello") && app.contains("boom"));
        let errors = std::fs::read_to_string(dir.join("errors.log")).unwrap();
        assert!(errors.contains("\"msg\": \"boom\"") && !errors.contains("hello"));
        h.close(true, Duration::from_secs(5));
        assert!(!h.is_alive());
        assert_eq!(h.stats().completed, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
