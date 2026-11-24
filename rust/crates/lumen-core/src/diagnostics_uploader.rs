//! Port of `windows/xray_fluent/diagnostics_uploader.py` plus
//! `logging_setup.configure_diagnostics_upload`.
//!
//! Bounded consent-generation-scoped transport; no I/O on the GUI thread.
//! Revocation invalidates queued/new work without joining network threads.
//! Already DISPATCHED requests cannot be unsent. The built-in HMAC is a
//! protocol tag, NOT client authentication.

use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use parking_lot::{Condvar, Mutex};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::constants::{APP_VERSION, DIAGNOSTICS_SECRET};
use crate::diagnostics::{MAX_BUNDLE_BYTES, safe_install_id, safe_version, sanitize_diagnostic_zip};
use crate::logging::{self, LogRecord};
use crate::paths::get_install_id;
use crate::pyjson;
use crate::secret_scrubber::scrub_text;

pub const EVENT_TIMEOUT: Duration = Duration::from_secs(10);
pub const BUNDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const BATCH_MAX: usize = 50;
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const QUEUE_MAX: usize = 1000;
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15 * 60);
const EVENT_FIELD_MAX_CHARS: usize = 16 * 1024;
const SEND_SLOTS_MAX: usize = 2;
const BUNDLE_THREADS_MAX: usize = 2;
const GENERATIONS_MAX: usize = 256;

fn user_agent() -> String {
    format!("Lumen-Diagnostics/{APP_VERSION}")
}

trait ConsentClient: Send + Sync {
    fn cancel_for_consent(&self);
}

struct ConsentState {
    enabled: bool,
    epoch: u64,
    generations: VecDeque<(String, u64)>,
    clients: Vec<Weak<dyn ConsentClient>>,
}

static STATE: LazyLock<Mutex<ConsentState>> = LazyLock::new(|| {
    Mutex::new(ConsentState { enabled: false, epoch: 0, generations: VecDeque::new(), clients: Vec::new() })
});
static SEND_SLOTS: AtomicUsize = AtomicUsize::new(0);
static BUNDLE_ACTIVE: LazyLock<(Mutex<usize>, Condvar)> = LazyLock::new(|| (Mutex::new(0), Condvar::new()));

/// Capture BEFORE scheduling a job; pass it to [`upload_bundle`].
pub fn get_upload_epoch() -> u64 {
    STATE.lock().epoch
}

pub fn uploads_allowed(epoch: u64) -> bool {
    let state = STATE.lock();
    state.enabled && epoch == state.epoch
}

/// Invalidate old work even on a quick OFF -> ON, without joining workers.
pub fn set_uploads_enabled(enabled: bool) -> u64 {
    let (epoch, clients) = {
        let mut state = STATE.lock();
        state.epoch += 1;
        state.enabled = enabled;
        state.clients.retain(|client| client.strong_count() > 0);
        let clients: Vec<Arc<dyn ConsentClient>> = state.clients.iter().filter_map(Weak::upgrade).collect();
        (state.epoch, clients)
    };
    for client in clients {
        client.cancel_for_consent();
    }
    epoch
}

/// `os.path.normcase(os.path.abspath(path))` on Windows.
pub fn path_key(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    absolute.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Bind recent exports to consent; delayed jobs must retain explicit epochs.
pub fn mark_bundle_generation(path: &Path, epoch: u64) {
    let key = path_key(path);
    let mut state = STATE.lock();
    state.generations.retain(|(existing, _)| existing != &key);
    state.generations.push_back((key, epoch));
    while state.generations.len() > GENERATIONS_MAX {
        state.generations.pop_front();
    }
}

fn register(client: Weak<dyn ConsentClient>) {
    STATE.lock().clients.push(client);
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub fn sanitize_event(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return json!({"msg": truncate_chars(&scrub_text(value), EVENT_FIELD_MAX_CHARS)});
    };
    let mut event = Map::new();
    for key in ["ts", "level", "domain", "logger", "msg", "exc", "stack"] {
        if let Some(Value::String(item)) = object.get(key) {
            let scrubbed = scrub_text(&Value::String(item.clone()));
            event.insert(key.into(), Value::String(truncate_chars(&scrubbed, EVENT_FIELD_MAX_CHARS)));
        }
    }
    if event.is_empty() {
        return json!({"msg": "[UNFORMATTABLE LOG EVENT]"});
    }
    Value::Object(event)
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("Unknown diagnostics payload")]
    Unknown,
    #[error("Invalid heartbeat install_id")]
    InvalidHeartbeat,
}

pub fn safe_payload(payload: &Value) -> Result<Value, PayloadError> {
    let kind = payload.get("kind").and_then(Value::as_str).filter(|_| payload.is_object());
    let kind = match kind {
        Some(kind @ ("error-batch" | "heartbeat")) => kind,
        _ => return Err(PayloadError::Unknown),
    };
    let mut safe = Map::new();
    safe.insert("kind".into(), json!(kind));
    safe.insert("app_version".into(), json!(safe_version(payload.get("app_version").unwrap_or(&Value::Null))));
    let install_id = safe_install_id(payload.get("install_id").unwrap_or(&Value::Null));
    if let Some(install_id) = &install_id {
        safe.insert("install_id".into(), json!(install_id));
        safe.insert("platform".into(), json!("windows"));
    }
    if kind == "heartbeat" {
        if install_id.is_none() {
            return Err(PayloadError::InvalidHeartbeat);
        }
        let ts = match payload.get("ts") {
            Some(Value::Number(n)) if n.is_u64() => n.as_u64().unwrap_or(0),
            _ => 0,
        };
        safe.insert("ts".into(), json!(ts));
    } else {
        let events = match payload.get("events") {
            Some(Value::Array(events)) => events.iter().take(BATCH_MAX).map(sanitize_event).collect(),
            _ => Vec::new(),
        };
        safe.insert("events".into(), Value::Array(events));
    }
    Ok(Value::Object(safe))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Legacy protocol tag only, not authentication or authorization.
pub fn sign_headers(body: &[u8]) -> Vec<(String, String)> {
    if DIAGNOSTICS_SECRET.is_empty() {
        return Vec::new();
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string();
    let digest = hex(&Sha256::digest(body));
    let mut mac = Hmac::<Sha256>::new_from_slice(DIAGNOSTICS_SECRET.as_bytes()).expect("hmac accepts any key");
    mac.update(format!("{ts}.{digest}").as_bytes());
    let signature = hex(&mac.finalize().into_bytes());
    vec![("X-Diag-Timestamp".into(), ts), ("X-Diag-Signature".into(), signature)]
}

/// The `https` (or `all`) system proxy usable as a connection-failure fallback.
pub fn fallback_proxy() -> Option<String> {
    let configured = lumen_platform::inet_settings::system_proxies();
    let proxy = configured.get("https").or_else(|| configured.get("all"))?.clone();
    let proxy = if proxy.contains("://") { proxy } else { String::from("http://") + &proxy };
    let parsed = url::Url::parse(&proxy).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none_or(str::is_empty) || parsed.fragment().is_some() {
        return None;
    }
    Some(proxy)
}

fn is_loopback(parsed: &url::Url) -> bool {
    match parsed.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip == std::net::Ipv4Addr::new(127, 0, 0, 1),
        Some(url::Host::Ipv6(ip)) => ip == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    }
}

struct SlotGuard;

impl SlotGuard {
    fn acquire() -> Option<Self> {
        SEND_SLOTS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < SEND_SLOTS_MAX).then_some(n + 1))
            .ok()
            .map(|_| SlotGuard)
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        SEND_SLOTS.fetch_sub(1, Ordering::AcqRel);
    }
}

enum PostOutcome {
    Delivered,
    Rejected,
    ConnectFailed,
}

fn post(client: &reqwest::blocking::Client, url: &str, body: &[u8], headers: &[(String, String)]) -> PostOutcome {
    let mut request = client.post(url).body(body.to_vec());
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    match request.send() {
        Ok(response) if response.status().is_success() => PostOutcome::Delivered,
        Ok(_) => PostOutcome::Rejected,
        Err(error) if error.is_connect() => PostOutcome::ConnectFailed,
        Err(_) => PostOutcome::Rejected,
    }
}

/// `_send`. `bundle_app_version` adds the bundle headers (`X-App-Version`,
/// `X-Filename`).
pub fn send(
    url: &str,
    body: &[u8],
    content_type: &str,
    timeout: Duration,
    bundle_app_version: Option<&str>,
    epoch: Option<u64>,
    cancel: Option<&AtomicBool>,
) -> bool {
    let epoch = epoch.unwrap_or_else(get_upload_epoch);
    let cancelled = || !uploads_allowed(epoch) || cancel.is_some_and(|flag| flag.load(Ordering::Acquire));
    if cancelled() {
        return false;
    }
    let Some(_slot) = SlotGuard::acquire() else { return false };
    let Ok(parsed) = url::Url::parse(url) else { return false };
    let loopback = is_loopback(&parsed);
    if parsed.host_str().is_none_or(str::is_empty)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || !(parsed.scheme() == "https" || parsed.scheme() == "http" && loopback)
    {
        return false;
    }
    let body: Vec<u8> = match content_type {
        "application/json" => {
            let Ok(value) = serde_json::from_slice::<Value>(body) else { return false };
            let Ok(safe) = safe_payload(&value) else { return false };
            pyjson::dumps(&safe, false, None).into_bytes()
        }
        "application/zip" => match sanitize_diagnostic_zip(body, &cancelled) {
            Ok(safe) => safe,
            Err(_) => return false,
        },
        _ => return false,
    };
    let mut headers = vec![("Content-Type".to_string(), content_type.to_string()), ("User-Agent".to_string(), user_agent())];
    if let Some(app_version) = bundle_app_version {
        headers.push(("X-App-Version".into(), safe_version(&Value::String(app_version.into()))));
        headers.push(("X-Filename".into(), "diagnostics.zip".into()));
    }
    headers.extend(sign_headers(&body));
    // Prefer direct transport; consult system settings only after a
    // connection failure. Redirects remain disabled on both routes.
    let Ok(direct) = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
    else {
        return false;
    };
    if cancelled() {
        return false;
    }
    match post(&direct, url, &body, &headers) {
        PostOutcome::Delivered => true,
        PostOutcome::Rejected => false,
        PostOutcome::ConnectFailed => {
            if loopback || cancelled() {
                return false;
            }
            let Some(proxy) = fallback_proxy() else { return false };
            if cancelled() {
                return false;
            }
            let Ok(proxy) = reqwest::Proxy::https(&proxy) else { return false };
            let Ok(proxied) = reqwest::blocking::Client::builder()
                .proxy(proxy)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(timeout)
                .build()
            else {
                return false;
            };
            if cancelled() {
                return false;
            }
            matches!(post(&proxied, url, &body, &headers), PostOutcome::Delivered)
        }
    }
}

/// Waits up to `timeout` for `finished`, then joins (or detaches) `handle`.
fn join_with_timeout(handle: JoinHandle<()>, finished: &(Mutex<bool>, Condvar), timeout: Duration) {
    if handle.thread().id() == std::thread::current().id() {
        return;
    }
    let deadline = Instant::now() + timeout;
    let mut done = finished.0.lock();
    while !*done {
        if finished.1.wait_until(&mut done, deadline).timed_out() {
            break;
        }
    }
    if *done {
        drop(done);
        let _ = handle.join();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UploaderStats {
    pub accepted: u64,
    pub dropped: u64,
    pub queued: usize,
}

struct HandlerInner {
    url: String,
    app_version: String,
    epoch: u64,
    flush_interval: Duration,
    queue: Mutex<VecDeque<Option<Value>>>,
    available: Condvar,
    stop: AtomicBool,
    accepted: AtomicU64,
    dropped: AtomicU64,
    finished: (Mutex<bool>, Condvar),
}

impl HandlerInner {
    fn active(&self) -> bool {
        !self.stop.load(Ordering::Acquire) && uploads_allowed(self.epoch)
    }

    fn discard_pending(&self, queue: &mut VecDeque<Option<Value>>) {
        for item in queue.drain(..) {
            if item.is_some() {
                self.dropped.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    fn post_events(&self, batch: Vec<Value>) -> bool {
        if !self.active() {
            return false;
        }
        let payload = json!({
            "kind": "error-batch",
            "app_version": self.app_version,
            "install_id": get_install_id(),
            "events": batch,
        });
        let Ok(safe) = safe_payload(&payload) else { return false };
        let body = pyjson::dumps(&safe, false, None);
        send(&self.url, body.as_bytes(), "application/json", EVENT_TIMEOUT, None, Some(self.epoch), Some(&self.stop))
    }

    fn run(&self) {
        let mut batch: Vec<Value> = Vec::new();
        let mut last = Instant::now();
        while self.active() {
            let elapsed = last.elapsed();
            let remaining = self.flush_interval.saturating_sub(elapsed).max(Duration::from_millis(10));
            let wait = remaining.min(Duration::from_millis(200));
            let item = {
                let mut queue = self.queue.lock();
                if queue.is_empty() {
                    self.available.wait_for(&mut queue, wait);
                }
                queue.pop_front()
            };
            match item {
                Some(None) => break,
                Some(Some(event)) => batch.push(event),
                None => {}
            }
            let due = last.elapsed() >= self.flush_interval;
            if !batch.is_empty() && (batch.len() >= BATCH_MAX || due) {
                let count = batch.len() as u64;
                if !self.post_events(std::mem::take(&mut batch)) {
                    self.dropped.fetch_add(count, Ordering::AcqRel);
                }
                last = Instant::now();
            }
        }
        self.dropped.fetch_add(batch.len() as u64, Ordering::AcqRel);
        let mut queue = self.queue.lock();
        self.discard_pending(&mut queue);
    }
}

impl ConsentClient for HandlerInner {
    fn cancel_for_consent(&self) {
        let mut queue = self.queue.lock();
        self.stop.store(true, Ordering::Release);
        self.discard_pending(&mut queue);
        queue.push_back(None);
        self.available.notify_all();
    }
}

/// Background WARNING+ log batch uploader (`HttpDiagnosticsHandler`).
pub struct HttpDiagnosticsHandler {
    inner: Arc<HandlerInner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl HttpDiagnosticsHandler {
    pub fn new(url: &str, app_version: &str, flush_interval: Duration) -> std::io::Result<Arc<Self>> {
        let inner = Arc::new(HandlerInner {
            url: url.to_string(),
            app_version: app_version.to_string(),
            epoch: get_upload_epoch(),
            flush_interval: flush_interval.max(Duration::from_millis(10)),
            queue: Mutex::new(VecDeque::new()),
            available: Condvar::new(),
            stop: AtomicBool::new(false),
            accepted: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            finished: (Mutex::new(false), Condvar::new()),
        });
        let weak: Weak<dyn ConsentClient> = Arc::downgrade(&inner) as Weak<dyn ConsentClient>;
        register(weak);
        let worker = inner.clone();
        let thread = std::thread::Builder::new().name("diag-uploader".into()).spawn(move || {
            worker.run();
            *worker.finished.0.lock() = true;
            worker.finished.1.notify_all();
        })?;
        Ok(Arc::new(Self { inner, thread: Mutex::new(Some(thread)) }))
    }

    pub fn stats(&self) -> UploaderStats {
        UploaderStats {
            accepted: self.inner.accepted.load(Ordering::Acquire),
            dropped: self.inner.dropped.load(Ordering::Acquire),
            queued: self.inner.queue.lock().iter().filter(|item| item.is_some()).count(),
        }
    }

    /// `emit`: `formatted` is the UTC JSON-lines rendering of the record.
    pub fn emit(&self, formatted: &str) {
        if !self.inner.active() {
            return;
        }
        let formatted = scrub_text(&Value::String(formatted.to_string()));
        let event = match serde_json::from_str::<Value>(&formatted) {
            Ok(value) => sanitize_event(&value),
            Err(_) => json!({"msg": truncate_chars(&formatted, EVENT_FIELD_MAX_CHARS)}),
        };
        let mut queue = self.inner.queue.lock();
        if !self.inner.active() {
            return;
        }
        if queue.len() >= QUEUE_MAX {
            self.inner.dropped.fetch_add(1, Ordering::AcqRel);
            return;
        }
        queue.push_back(Some(event));
        self.inner.accepted.fetch_add(1, Ordering::AcqRel);
        self.inner.available.notify_one();
    }

    pub fn close(&self, wait: bool, timeout: Duration) {
        self.inner.cancel_for_consent();
        let handle = self.thread.lock().take();
        if let Some(handle) = handle {
            if wait {
                join_with_timeout(handle, &self.inner.finished, timeout);
            }
        }
    }
}

/// Scrub old archives in a bounded worker; send only in the captured epoch.
pub fn upload_bundle(url: &str, zip_path: &Path, app_version: &str, epoch: Option<u64>) {
    if url.is_empty() {
        return;
    }
    let (epoch, recorded) = {
        let state = STATE.lock();
        let key = path_key(zip_path);
        let recorded = state.generations.iter().find(|(k, _)| k == &key).map(|(_, e)| *e);
        let epoch = match (epoch, recorded) {
            (Some(epoch), _) => epoch,
            (None, Some(recorded)) => recorded,
            (None, None) => state.epoch,
        };
        (epoch, recorded)
    };
    if !uploads_allowed(epoch) || recorded.is_some_and(|recorded| recorded != epoch) {
        return;
    }
    {
        let mut active = BUNDLE_ACTIVE.0.lock();
        if *active >= BUNDLE_THREADS_MAX {
            return;
        }
        *active += 1;
    }
    let url = url.to_string();
    let zip_path = zip_path.to_path_buf();
    let app_version = app_version.to_string();
    let spawned = std::thread::Builder::new().name("diag-bundle-upload".into()).spawn(move || {
        let job = || -> Option<()> {
            if !uploads_allowed(epoch) {
                return None;
            }
            let mut data = Vec::new();
            std::fs::File::open(&zip_path).ok()?.take(MAX_BUNDLE_BYTES as u64 + 1).read_to_end(&mut data).ok()?;
            let safe = sanitize_diagnostic_zip(&data, &|| !uploads_allowed(epoch)).ok()?;
            if uploads_allowed(epoch) {
                send(&url, &safe, "application/zip", BUNDLE_TIMEOUT, Some(&app_version), Some(epoch), None);
            }
            Some(())
        };
        let _ = job();
        let mut active = BUNDLE_ACTIVE.0.lock();
        *active = active.saturating_sub(1);
        BUNDLE_ACTIVE.1.notify_all();
    });
    if spawned.is_err() {
        let mut active = BUNDLE_ACTIVE.0.lock();
        *active = active.saturating_sub(1);
        BUNDLE_ACTIVE.1.notify_all();
    }
}

/// Explicit worker/test drain ONLY; never used by GUI consent-off.
pub fn wait_for_bundle_uploads(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut active = BUNDLE_ACTIVE.0.lock();
    while *active > 0 {
        if BUNDLE_ACTIVE.1.wait_until(&mut active, deadline).timed_out() {
            break;
        }
    }
}

struct HeartbeatInner {
    url: String,
    app_version: String,
    epoch: u64,
    interval: Duration,
    stop: AtomicBool,
    wake: (Mutex<()>, Condvar),
    finished: (Mutex<bool>, Condvar),
}

impl HeartbeatInner {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    fn ping(&self) {
        if self.stopped() || !uploads_allowed(self.epoch) {
            return;
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let payload = json!({
            "kind": "heartbeat",
            "app_version": self.app_version,
            "install_id": get_install_id(),
            "ts": ts,
        });
        if let Ok(safe) = safe_payload(&payload) {
            let body = pyjson::dumps(&safe, false, None);
            send(&self.url, body.as_bytes(), "application/json", EVENT_TIMEOUT, None, Some(self.epoch), Some(&self.stop));
        }
    }

    fn run(&self) {
        while !self.stopped() && uploads_allowed(self.epoch) {
            self.ping();
            let mut guard = self.wake.0.lock();
            if !self.stopped() {
                self.wake.1.wait_for(&mut guard, self.interval);
            }
        }
    }

    fn signal_stop(&self) {
        let _guard = self.wake.0.lock();
        self.stop.store(true, Ordering::Release);
        self.wake.1.notify_all();
    }
}

impl ConsentClient for HeartbeatInner {
    fn cancel_for_consent(&self) {
        self.signal_stop();
    }
}

/// Consent-scoped activity ping with a persistent random installation ID.
pub struct HeartbeatSender {
    inner: Arc<HeartbeatInner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl HeartbeatSender {
    pub fn new(url: &str, app_version: &str, interval: Duration) -> std::io::Result<Self> {
        let inner = Arc::new(HeartbeatInner {
            url: url.to_string(),
            app_version: app_version.to_string(),
            epoch: get_upload_epoch(),
            interval: interval.max(Duration::from_millis(10)),
            stop: AtomicBool::new(false),
            wake: (Mutex::new(()), Condvar::new()),
            finished: (Mutex::new(false), Condvar::new()),
        });
        register(Arc::downgrade(&inner) as Weak<dyn ConsentClient>);
        let worker = inner.clone();
        let thread = std::thread::Builder::new().name("diag-heartbeat".into()).spawn(move || {
            worker.run();
            *worker.finished.0.lock() = true;
            worker.finished.1.notify_all();
        })?;
        Ok(Self { inner, thread: Mutex::new(Some(thread)) })
    }

    pub fn stop(&self, wait: bool, timeout: Duration) {
        self.inner.signal_stop();
        let handle = self.thread.lock().take();
        if let Some(handle) = handle {
            if wait {
                join_with_timeout(handle, &self.inner.finished, timeout);
            }
        }
    }
}

struct UploadGlobals {
    handler: Option<Arc<HttpDiagnosticsHandler>>,
    heartbeat: Option<HeartbeatSender>,
}

static UPLOAD: LazyLock<Mutex<UploadGlobals>> =
    LazyLock::new(|| Mutex::new(UploadGlobals { handler: None, heartbeat: None }));

/// `logging_setup.configure_diagnostics_upload`: an empty `upload_url`
/// revokes consent and detaches everything.
pub fn configure_diagnostics_upload(upload_url: &str, app_version: &str) {
    let mut globals = UPLOAD.lock();
    set_uploads_enabled(!upload_url.is_empty());
    logging::set_upload_sink(None);
    if let Some(handler) = globals.handler.take() {
        handler.close(false, Duration::ZERO);
    }
    if let Some(heartbeat) = globals.heartbeat.take() {
        heartbeat.stop(false, Duration::ZERO);
    }
    if upload_url.is_empty() {
        return;
    }
    let started = HttpDiagnosticsHandler::new(upload_url, app_version, FLUSH_INTERVAL).and_then(|handler| {
        let heartbeat = HeartbeatSender::new(upload_url, app_version, HEARTBEAT_INTERVAL);
        match heartbeat {
            Ok(heartbeat) => Ok((handler, heartbeat)),
            Err(error) => {
                handler.close(false, Duration::ZERO);
                Err(error)
            }
        }
    });
    match started {
        Ok((handler, heartbeat)) => {
            let sink_handler = handler.clone();
            logging::set_upload_sink(Some(Arc::new(move |_record: &LogRecord, formatted: &str| {
                sink_handler.emit(formatted);
            })));
            globals.handler = Some(handler);
            globals.heartbeat = Some(heartbeat);
        }
        Err(_) => {
            set_uploads_enabled(false);
            drop(globals);
            logging::root_logger().warning("[app] diagnostics auto-upload init failed");
        }
    }
}

/// Stats of the active log uploader, if any.
pub fn uploader_stats() -> Option<UploaderStats> {
    UPLOAD.lock().handler.as_ref().map(|handler| handler.stats())
}

#[cfg(test)]
pub(crate) static TEST_EPOCH_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_preserves_prerelease_version() {
        for kind in ["heartbeat", "error-batch"] {
            let payload = json!({
                "kind": kind,
                "app_version": "3.0.0-pre.01",
                "install_id": "0123456789abcdef0123456789abcdef",
                "ts": 1,
                "events": [{"msg": "error"}],
            });
            let safe = safe_payload(&payload).unwrap();
            assert_eq!(safe["app_version"], "3.0.0-pre.01", "{kind}");
        }
    }

    #[test]
    fn payload_sanitizing() {
        assert!(safe_payload(&json!({"kind": "other"})).is_err());
        assert!(safe_payload(&json!({"kind": "heartbeat", "install_id": "bad"})).is_err());
        let id = "0123456789abcdef0123456789abcdef";
        let beat = safe_payload(&json!({"kind": "heartbeat", "install_id": id, "ts": -5, "app_version": "2.1.4"})).unwrap();
        assert_eq!(beat["ts"], json!(0));
        assert_eq!(beat["platform"], json!("windows"));
        assert_eq!(beat["app_version"], json!("2.1.4"));
        let batch = safe_payload(&json!({"kind": "error-batch", "events": [{"msg": "x", "extra": "y"}, 5]})).unwrap();
        assert_eq!(batch["events"][0], json!({"msg": "x"}));
        assert_eq!(batch["events"][1], json!({"msg": "5"}));
        assert_eq!(sanitize_event(&json!({"other": 1})), json!({"msg": "[UNFORMATTABLE LOG EVENT]"}));
    }

    #[test]
    fn send_rejects_unsafe_urls_and_revoked_epochs() {
        let _epoch_guard = TEST_EPOCH_LOCK.lock();
        let epoch = set_uploads_enabled(true);
        assert!(!send("http://example.com/x", b"{}", "application/json", EVENT_TIMEOUT, None, Some(epoch), None));
        assert!(!send("https://u:p@example.com/x", b"{}", "application/json", EVENT_TIMEOUT, None, Some(epoch), None));
        assert!(!send("https://example.com/x", b"{}", "text/plain", EVENT_TIMEOUT, None, Some(epoch), None));
        let revoked = set_uploads_enabled(false);
        assert!(!uploads_allowed(revoked));
        assert!(!uploads_allowed(epoch));
    }

    #[test]
    fn generations_are_bounded() {
        for index in 0..300 {
            mark_bundle_generation(Path::new(&format!("bundle-{index}.zip")), index);
        }
        assert_eq!(STATE.lock().generations.len(), GENERATIONS_MAX);
    }
}
