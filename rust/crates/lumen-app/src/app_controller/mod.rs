//! Central application controller (port of `app_controller.py`).
//!
//! The Python controller is a `QObject`: Qt signals fan state out to the UI,
//! Qt timers drive periodic work and QThreads run the blocking connection
//! flows. In the headless Rust port one owner task drives the controller:
//!
//! * signals become an [`AppEvent`] queue (`subscribe()`);
//! * queued cross-thread calls become [`ControllerTask`] closures posted to
//!   `submit()` (the same "run it on the owner later" semantics as
//!   `QMetaObject.invokeMethod(..., QueuedConnection)`);
//! * Qt timers become deadline checks inside [`AppController::run_event_loop`];
//! * the transition worker thread becomes the owner task itself — the UI
//!   never shares its thread, so blocking connect/disconnect work keeps the
//!   generation-guarded queued-completion semantics without a second thread.
//!
//! Every Python method has a 1:1 Rust counterpart; submodules hold the
//! grouped implementations (`persistence`, `startup_admin`, `config_paths`,
//! `connection_flow`, `proxy_wiring`, `subscriptions`, `workers`, `logging`,
//! `lock`, `diagnostics`).

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll, Waker};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use lumen_core::bounded_logs::LogRing;
use lumen_core::constants::{data_dir, log_dir, runtime_dir};
use lumen_core::models::{AppSettings, AppState, Node, RoutingSettings};
use lumen_core::routing_presets::{normalize_regional_preset, normalize_routing_preset_for_region};
use lumen_core::secret_scrubber::StreamingSecretScrubber;
use lumen_core::storage::StateStorage;
use lumen_core::traffic_history::TrafficHistoryStorage;
use lumen_core::zapret_server_scope::{ZapretServerScope, resolve_server_ips};
use lumen_engines::core_resource_updater::{
    ResourceUpdateEvent, ResourceUpdateResult, ResourceUpdateWorker,
};
use lumen_engines::discord_proxy_manager::DiscordProxyManager;
use lumen_engines::singbox::manager::{SingBoxManager, SingboxEvent};
use lumen_engines::xray::core_updater::XrayCoreUpdateResult;
use lumen_engines::xray::manager::{XrayEvent, XrayManager};
use lumen_engines::xray::operations::{
    ActiveSessionView, OperationsController, SessionCapture,
};
use lumen_engines::zapret_manager::{ZapretEvent, ZapretManager};
use lumen_platform::network_monitor::NetworkMonitor;
use lumen_platform::power_reconnect::PowerReconnectController;
use lumen_platform::proxy_manager::ProxyManager;

use crate::auto_switch_service::{
    AutoSwitchConnection, AutoSwitchEvent, AutoSwitchMetrics, AutoSwitchReason, AutoSwitchService,
};
use crate::connection_service::ConnectionController;
use crate::live_metrics_worker::LiveMetricsSample;
use crate::runtime_services::{
    ConnectionRuntime, MetricsUpdate, RuntimeController, ShutdownController, handle_unexpected_disconnect,
};
use crate::session_state::ActiveSessionSnapshot;
use crate::signature_service::SignatureService;
use crate::transition_engine::{
    TransitionContext, compute_transition_action, needs_transition, transition_status_text,
};
use crate::update_service::XrayUpdateService;
use crate::worker_service::{TestEvent, WorkerService};
use crate::xray_runtime_service::XrayRuntimeConfig;

mod config_paths;
mod connection_flow;
mod diagnostics;
mod lock;
mod logging;
mod persistence;
mod proxy_wiring;
mod startup_admin;
mod subscriptions;
mod workers;

#[cfg(test)]
mod tests;

pub use persistence::{LoadState, apply_dns_defaults_update_once, apply_masque_direct_update_once};

/// Deferred work executed on the controller's owner task, mirroring the
/// Python `QMetaObject.invokeMethod(..., QueuedConnection)` re-entry from
/// worker threads.
pub type ControllerTask = Box<dyn FnOnce(&mut AppController) + Send>;

/// Outgoing controller signals (Python `pyqtSignal` surface).
#[derive(Debug, Clone)]
pub enum AppEvent {
    NodesChanged,
    SelectionChanged(Option<Node>),
    ConnectionChanged(bool),
    ConnectionStatus { phase: String, message: String },
    RoutingChanged,
    SettingsChanged(AppSettings),
    SubscriptionsChanged,
    LogLine(String),
    Status { level: &'static str, message: String },
    AdminRelaunchRequested,
    BulkTaskProgress { task: String, current: usize, total: usize, completed: bool },
    PingUpdated { node_id: String, ms: Option<i64> },
    SpeedUpdated { node_id: String, mbps: Option<f64>, alive: bool },
    SpeedNodeProgress { node_id: String, percent: u32 },
    SpeedTestCancelled { completed: usize, total: usize },
    ConnectivityTestDone { ok: bool, message: String, elapsed_ms: Option<u64> },
    LiveMetricsUpdated(LiveMetricsEvent),
    XrayUpdateResult(XrayCoreUpdateResult),
    XrayUpdateProgress(u32),
    ResourceUpdateResult(ResourceUpdateResult),
    ResourceUpdateProgress { kind: String, percent: u32 },
    LockStateChanged(bool),
    PassphraseRequired,
    SaveFailed(String),
    StartupDisabled(u64),
    AdminSettingApplied { enabled: bool, elevated: bool, error: String },
    AutoSwitchTriggered(String),
    TransitionStateChanged { active: bool, text: String },
}

/// Python `live_metrics_updated` payload (dict sample or an empty reset).
#[derive(Debug, Clone)]
pub enum LiveMetricsEvent {
    Sample(Arc<LiveMetricsSample>),
    Reset,
}

/// Completions posted back to the owner task from background threads.
pub(crate) enum InternalEvent {
    /// `request_transition` from a worker context (queued signal).
    Transition(String),
    /// Country IP resolution finished (NodeService flow).
    CountriesResolved(BTreeMap<String, String>),
    /// Zapret trusted-DoH server lookup finished.
    ZapretServerIps {
        purpose: ZapretResolve,
        host: String,
        ips: Vec<String>,
        deadline: Instant,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ZapretResolve {
    /// Warm the trusted cache; results are never surfaced.
    Prefetch,
    /// Apply the bound preset for a connected server.
    Binding { token: u64 },
    /// Resume a transition once the bound server address is known.
    PreConnect { action: &'static str, reason: String, generation: u64 },
}

/// The transition bookkeeping fields of Python `AppController.__init__`.
#[derive(Default)]
pub(crate) struct TransitionState {
    pub active: bool,
    pub scheduled: bool,
    pub pending: bool,
    pub reason: String,
    pub generation: u64,
    pub blocked_signature: String,
    pub resume_reconnect_pending: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum TransitionTick {
    Drain,
    Execute { action: &'static str, reason: String, generation: u64 },
}

#[derive(Debug, Clone)]
pub(crate) enum DelayedAction {
    DiscordProxy,
    SyncZapret,
    CountryDetectBatch,
    StartCountryIpResolution,
    /// The bounded Zapret pre-connect lookup expired without a result.
    ZapretPreConnectTimeout { action: &'static str, reason: String, generation: u64 },
    ZapretBindingTimeout { token: u64 },
}

/// Deadline bookkeeping for the Python `QTimer` fields.
#[derive(Default)]
pub(crate) struct Timers {
    pub save_pending: bool,
    pub save_deadline: Option<Instant>,
    pub transition: Option<(Instant, TransitionTick)>,
    pub lock_check: Option<Instant>,
    pub core_log_flush: Option<Instant>,
    pub startup_sync: Option<Instant>,
    pub delayed: Vec<(Instant, DelayedAction)>,
    pub country_detect: CountryDetect,
}

/// Batched country detection from a saved profile (GUI-thread slices).
#[derive(Default)]
pub(crate) struct CountryDetect {
    pub node_ids: Vec<String>,
    pub cursor: usize,
    pub changed: bool,
}

/// Pending bounded trusted-DoH lookups (`_zapret_waits` bookkeeping).
#[derive(Default)]
pub(crate) struct ZapretWaits {
    /// `(action, reason, generation, deadline)`
    pub pre_connect: Option<(&'static str, String, u64, Instant)>,
    /// `(token, node_id, preset, label, host, deadline)`
    pub binding: Option<(u64, String, String, String, String, Instant)>,
}

/// Single-worker Zapret DNS pool (`ThreadPoolExecutor(max_workers=1)`).
pub(crate) struct ZapretDnsPool {
    sender: Option<UnboundedSender<(String, Instant, ZapretResolve)>>,
    thread: Option<JoinHandle<()>>,
}

impl ZapretDnsPool {
    fn submit(
        &mut self,
        internal_tx: UnboundedSender<InternalEvent>,
        host: String,
        deadline: Instant,
        purpose: ZapretResolve,
    ) {
        if self.sender.is_none() {
            let (sender, mut receiver) = unbounded_channel();
            self.sender = Some(sender);
            if let Ok(thread) = std::thread::Builder::new()
                .name("zapret-dns".into())
                .spawn(move || {
                    while let Some((host, deadline, purpose)) = receiver.blocking_recv() {
                        let ips = resolve_server_ips(&host);
                        let _ = internal_tx.send(InternalEvent::ZapretServerIps {
                            purpose,
                            host,
                            ips,
                            deadline,
                        });
                    }
                })
            {
                self.thread = Some(thread);
            }
        }
        if let Some(sender) = self.sender.as_ref() {
            let _ = sender.send((host, deadline, purpose));
        }
    }
}

impl Drop for ZapretDnsPool {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One tracked regional/app resource update worker.
pub(crate) struct ResourceUpdateEntry {
    pub kind: String,
    pub region: String,
    pub apply_update: bool,
    pub worker: ResourceUpdateWorker,
}

/// All four signatures computed in one profile-inspection pass.
pub(crate) struct Signatures {
    pub routing: String,
    pub transition: String,
    pub xray_layer: String,
    pub tun_layer: String,
}

/// Owns all application state, engine managers, services and the event loop.
pub struct AppController {
    // ── signal/completion plumbing ─────────────────────────────────────────
    events: tokio::sync::broadcast::Sender<AppEvent>,
    tasks: (UnboundedSender<ControllerTask>, UnboundedReceiver<ControllerTask>),
    internal: (UnboundedSender<InternalEvent>, UnboundedReceiver<InternalEvent>),
    stop_requested: bool,

    // ── core state ─────────────────────────────────────────────────────────
    pub state: AppState,
    pub storage: Arc<StateStorage>,
    pub(crate) load_state: LoadState,
    pub connected: bool,
    pub locked: bool,
    pub runtime: ConnectionRuntime,
    pub(crate) transitions: TransitionState,
    pub(crate) timers: Timers,

    // ── engine managers and OS integrations ────────────────────────────────
    pub xray: XrayManager,
    pub singbox: SingBoxManager,
    pub zapret: ZapretManager,
    pub proxy: ProxyManager,
    pub discord_proxy: DiscordProxyManager,
    pub(crate) network_monitor: Option<NetworkMonitor>,

    // ── logging ────────────────────────────────────────────────────────────
    pub(crate) recent_logs: Arc<LogRing<String>>,
    pub(crate) core_log_streams: BTreeMap<&'static str, StreamingSecretScrubber>,
    pub(crate) pending_core_logs: Arc<LogRing<(&'static str, String)>>,
    pub(crate) routine_log_count: u64,
    pub(crate) tun_log_count: u64,
    pub(crate) repeated_connection_failures: BTreeMap<(String, String), (f64, u64)>,

    // ── profiles ───────────────────────────────────────────────────────────
    pub(crate) profiles: std::cell::RefCell<crate::profile_service::ProfileService>,
    pub(crate) singbox_cache: crate::config_documents::SingboxDocumentCache,
    pub(crate) parsed_singbox_document:
        Option<lumen_engines::singbox::document::ParsedSingboxDocument>,

    // ── services ───────────────────────────────────────────────────────────
    pub(crate) workers: WorkerService,
    pub(crate) updates: XrayUpdateService,
    pub(crate) subscriptions: crate::subscription_dispatcher::SubscriptionDispatcher,
    pub(crate) auto_switch: AutoSwitchService,
    pub(crate) country_resolver: crate::node_runtime_service::CountryResolver,
    pub(crate) resource_updates: Vec<ResourceUpdateEntry>,
    pub(crate) pending_resource_disconnects: Vec<tokio::sync::oneshot::Sender<bool>>,
    pub(crate) traffic_history: TrafficHistoryStorage,
    pub(crate) save_queue: persistence::SaveQueue,
    pub(crate) background_tasks: startup_admin::BackgroundTasks,
    pub(crate) windows_prefs: startup_admin::WindowsPrefsQueue,

    // ── connection bookkeeping ─────────────────────────────────────────────
    pub(crate) active_core: String,
    pub(crate) active_session: Option<ActiveSessionSnapshot>,
    pub(crate) protect_ss_port: i64,
    pub(crate) protect_ss_password: String,
    pub(crate) xray_api_port: i64,

    // ── update / resource bookkeeping ──────────────────────────────────────
    pub(crate) pending_regional_preset: String,
    pub(crate) reconnect_after_resource_updates: bool,

    // ── zapret server binding ──────────────────────────────────────────────
    pub(crate) zapret_scope_token: u64,
    pub(crate) zapret_desired_scope: Option<ZapretServerScope>,
    pub(crate) zapret_dns: ZapretDnsPool,
    pub(crate) zapret_binding_warned_node: String,
    pub(crate) zapret_autostart_done: bool,
    pub(crate) zapret_reconnect_signature:
        Option<(String, String, String, Vec<String>, i64, bool, bool)>,
    pub(crate) zapret_waits: ZapretWaits,

    // ── shutdown / startup bookkeeping ─────────────────────────────────────
    pub(crate) shutting_down: Arc<AtomicBool>,
    pub(crate) system_shutdown: bool,
    pub(crate) deferred_services_started: bool,
    pub(crate) startup_sync_active: bool,
    pub(crate) startup_generation: Arc<AtomicU64>,
    pub(crate) startup_io_lock: Arc<parking_lot::Mutex<()>>,
    pub(crate) diagnostics_exports: Vec<JoinHandle<()>>,
}

/// Poll a future once without parking (no external `futures` dependency).
pub(crate) fn poll_once<F: Future>(fut: F) -> Option<F::Output> {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut fut = pin!(fut);
    match fut.as_mut().poll(&mut context) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// Clone a metrics sample into an owned `Arc` for the signal channel
/// (`LiveMetricsSample` itself is not `Clone` in `live_metrics_worker`).
fn clone_sample(sample: &LiveMetricsSample) -> LiveMetricsSample {
    LiveMetricsSample {
        up_bps: sample.up_bps,
        down_bps: sample.down_bps,
        process_stats: sample
            .process_stats
            .as_ref()
            .map(|stats| stats.iter().map(Clone::clone).collect()),
        connection_events: sample.connection_events.clone(),
        traffic_available: sample.traffic_available,
        traffic_reason: sample.traffic_reason.clone(),
        traffic_checked_at: sample.traffic_checked_at,
        upload_total: sample.upload_total,
        download_total: sample.download_total,
        health: sample.health.clone(),
    }
}

impl Default for AppController {
    fn default() -> Self {
        Self::new()
    }
}

impl AppController {
    /// Full constructor: configures logging, reconciles a stale system proxy
    /// and installs the trusted bootstrap-DNS cache, exactly like
    /// `AppController.__init__`.
    pub fn new() -> Self {
        Self::with_storage(StateStorage::new(lumen_core::constants::state_file()))
    }

    /// Constructor with an explicit state file (tests and alternative roots).
    pub fn with_storage(storage: StateStorage) -> Self {
        let (task_tx, task_rx) = unbounded_channel();
        let (internal_tx, internal_rx) = unbounded_channel();
        let (event_tx, _) = tokio::sync::broadcast::channel(1024);

        // Python configures logging in `__init__` before any other work.
        let _ = lumen_core::logging::configure_logging(&log_dir());

        let controller = Self {
            events: event_tx,
            tasks: (task_tx, task_rx),
            internal: (internal_tx, internal_rx),
            stop_requested: false,
            state: AppState::default(),
            storage: Arc::new(storage),
            load_state: LoadState::Unloaded,
            connected: false,
            locked: false,
            runtime: ConnectionRuntime::default(),
            transitions: TransitionState::default(),
            timers: Timers {
                core_log_flush: Some(Instant::now() + Duration::from_millis(50)),
                ..Timers::default()
            },
            xray: XrayManager::new(),
            singbox: SingBoxManager::new(),
            zapret: ZapretManager::new(),
            proxy: ProxyManager::new(runtime_dir()),
            discord_proxy: DiscordProxyManager::new(),
            network_monitor: None,
            recent_logs: Arc::new(LogRing::new(5000).expect("recent log ring")),
            core_log_streams: BTreeMap::from([
                ("xray", StreamingSecretScrubber::new()),
                ("singbox", StreamingSecretScrubber::new()),
                ("zapret", StreamingSecretScrubber::new()),
            ]),
            pending_core_logs: Arc::new(LogRing::new(2000).expect("core log ring")),
            routine_log_count: 0,
            tun_log_count: 0,
            repeated_connection_failures: BTreeMap::new(),
            profiles: std::cell::RefCell::new(crate::profile_service::ProfileService::default()),
            singbox_cache: crate::config_documents::SingboxDocumentCache::default(),
            parsed_singbox_document: None,
            workers: WorkerService::default(),
            updates: XrayUpdateService::default(),
            subscriptions: crate::subscription_dispatcher::SubscriptionDispatcher::default(),
            auto_switch: AutoSwitchService::default(),
            country_resolver: crate::node_runtime_service::CountryResolver::default(),
            resource_updates: Vec::new(),
            pending_resource_disconnects: Vec::new(),
            traffic_history: TrafficHistoryStorage::new(),
            save_queue: persistence::SaveQueue::default(),
            background_tasks: startup_admin::BackgroundTasks::default(),
            windows_prefs: startup_admin::WindowsPrefsQueue::default(),
            active_core: "xray".into(),
            active_session: None,
            protect_ss_port: 0,
            protect_ss_password: String::new(),
            xray_api_port: 0,
            pending_regional_preset: String::new(),
            reconnect_after_resource_updates: false,
            zapret_scope_token: 0,
            zapret_desired_scope: None,
            zapret_dns: ZapretDnsPool {
                sender: None,
                thread: None,
            },
            zapret_binding_warned_node: String::new(),
            zapret_autostart_done: false,
            zapret_reconnect_signature: None,
            zapret_waits: ZapretWaits::default(),
            shutting_down: Arc::new(AtomicBool::new(false)),
            system_shutdown: false,
            deferred_services_started: false,
            startup_sync_active: false,
            startup_generation: Arc::new(AtomicU64::new(0)),
            startup_io_lock: Arc::new(parking_lot::Mutex::new(())),
            diagnostics_exports: Vec::new(),
        };

        // `bootstrap_dns.configure_cache(DATA_DIR / "bootstrap-lkg.json")`.
        lumen_core::config::bootstrap_dns::configure_cache(Some(
            data_dir().join("bootstrap-lkg.json"),
        ));

        // A previous crash can leave WinINET pointing at Lumen's dead port.
        // Reconcile after logging is configured so recovery failures are visible.
        match controller.proxy.reconcile_stale_state() {
            Ok(true) => {
                controller.log_raw("[proxy] Restored system proxy after an interrupted session");
            }
            Err(error) => {
                controller.log_raw(&format!(
                    "[app] Could not restore system proxy after an interrupted session: {error}"
                ));
            }
            Ok(false) => {}
        }
        controller.log_previous_native_crash();
        controller
    }

    /// Receive half for UI observers (Python signal connections). Late
    /// subscribers miss earlier signals, like re-connected Qt signals.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    /// Queue work onto the owner task (Python queued connections).
    pub fn submit(&self, task: ControllerTask) {
        let _ = self.tasks.0.send(task);
    }

    pub(crate) fn emit(&self, event: AppEvent) {
        let _ = self.events.send(event);
    }

    pub(crate) fn queue_internal(&self, event: InternalEvent) {
        let _ = self.internal.0.send(event);
    }

    /// Stop the loop once the current iteration drains.
    pub fn request_event_loop_stop(&mut self) {
        self.stop_requested = true;
    }

    /// Spawn a blocking job on a named thread; re-entry is queued through
    /// [`ControllerTask`]s. Tracked for the shutdown join deadline.
    pub(crate) fn spawn_io(
        &mut self,
        name: &'static str,
        job: impl FnOnce(&UnboundedSender<ControllerTask>) + Send + 'static,
    ) {
        if self.shutting_down() {
            return;
        }
        let tasks = self.tasks.0.clone();
        if let Ok(thread) = std::thread::Builder::new().name(name.into()).spawn(move || {
            job(&tasks);
        }) {
            self.background_tasks.push(name, thread);
        }
    }

    // ── event loop ─────────────────────────────────────────────────────────

    /// Drive everything: queued tasks, completions, engine events, metrics,
    /// timers and transitions. Mirrors the Qt event loop that owned the
    /// Python controller.
    pub async fn run_event_loop(&mut self) {
        let mut xray_events = self.xray.subscribe();
        let mut singbox_events = self.singbox.subscribe();
        let mut zapret_events = self.zapret.subscribe();
        loop {
            if self.stop_requested {
                return;
            }
            // 1. queued calls from other threads (Qt queued connections).
            while let Ok(task) = self.tasks.1.try_recv() {
                task(self);
            }
            // 2. background completions.
            while let Some(event) = self.internal.1.try_recv().ok() {
                self.handle_internal(event).await;
            }
            // 3. pollable worker completions.
            self.poll_workers();
            self.poll_updates();
            self.poll_subscriptions();
            self.poll_resource_updates();
            self.poll_metrics();
            self.poll_manager_events(&mut xray_events, &mut singbox_events, &mut zapret_events);
            // 4. timers (save flush, transitions, lock, logs, refresh).
            self.process_timers().await;
            if self.stop_requested {
                return;
            }
            // 5. park until the next deadline or completion.
            let deadline = self.next_wake();
            let mut completion = None;
            let mut queued_task = None;
            tokio::select! {
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {}
                event = self.internal.1.recv() => {
                    completion = event;
                }
                task = self.tasks.1.recv() => {
                    queued_task = task;
                }
            }
            if let Some(task) = queued_task {
                task(self);
            }
            if let Some(event) = completion {
                self.handle_internal(event).await;
            }
        }
    }

    async fn handle_internal(&mut self, event: InternalEvent) {
        match event {
            InternalEvent::Transition(reason) => self.request_transition(&reason),
            InternalEvent::CountriesResolved(results) => {
                self.on_countries_resolved(&results);
            }
            InternalEvent::ZapretServerIps {
                purpose,
                host,
                ips,
                deadline,
            } => {
                if Instant::now() > deadline {
                    return;
                }
                match purpose {
                    ZapretResolve::Prefetch => {}
                    ZapretResolve::Binding { token } => {
                        self.on_zapret_binding_ips(token, &host, ips);
                    }
                    ZapretResolve::PreConnect { action, reason, generation } => {
                        if self.zapret_waits.pre_connect.take().is_some()
                            && self.transitions.active
                            && !self.shutting_down()
                        {
                            self.start_transition_worker(action, &reason, generation);
                        }
                    }
                }
            }
        }
    }

    fn poll_workers(&mut self) {
        while let Some(message) = poll_once(self.workers.next_worker_event()).flatten() {
            self.handle_worker_message(message);
        }
    }

    fn poll_updates(&mut self) {
        while let Some(event) = poll_once(self.updates.next_worker_event()).flatten() {
            self.handle_xray_update_event(event);
        }
    }

    fn poll_subscriptions(&mut self) {
        while let Some(event) = poll_once(self.subscriptions.next_worker_event()).flatten() {
            self.handle_subscription_dispatch(event);
        }
    }

    fn poll_resource_updates(&mut self) {
        let mut index = 0;
        while index < self.resource_updates.len() {
            let mut done = None;
            loop {
                let entry = &mut self.resource_updates[index];
                match entry.worker.events.try_recv() {
                    Ok(ResourceUpdateEvent::Progress(percent)) => {
                        let kind = entry.kind.clone();
                        self.emit(AppEvent::ResourceUpdateProgress { kind, percent });
                    }
                    Ok(ResourceUpdateEvent::RequestDisconnect(confirm)) => {
                        self.on_resource_update_disconnect_request(confirm);
                    }
                    Ok(ResourceUpdateEvent::Done(result)) => {
                        done = Some(result);
                        break;
                    }
                    Err(_) => break,
                }
            }
            if let Some(result) = done {
                let entry = self.resource_updates.swap_remove(index);
                self.on_resource_update_done(entry, result);
                continue;
            }
            index += 1;
        }
    }

    fn poll_metrics(&mut self) {
        while let Some(sample) = poll_once(self.runtime.next_metrics()).flatten() {
            let sample = Arc::new(sample);
            crate::runtime_services::on_live_metrics(self, &sample);
        }
    }

    fn poll_manager_events(
        &mut self,
        xray_events: &mut tokio::sync::broadcast::Receiver<XrayEvent>,
        singbox_events: &mut tokio::sync::broadcast::Receiver<SingboxEvent>,
        zapret_events: &mut tokio::sync::broadcast::Receiver<ZapretEvent>,
    ) {
        while let Ok(event) = xray_events.try_recv() {
            match event {
                XrayEvent::Log(line) => self.queue_core_log("xray", &line),
                XrayEvent::Error(message) => self.on_xray_error(&message),
                XrayEvent::StateChanged(running) => self.on_core_state_changed(running),
                XrayEvent::Stopped(code) => self.on_core_stopped("xray", code as i64),
                XrayEvent::Started => {}
            }
        }
        while let Ok(event) = singbox_events.try_recv() {
            match event {
                SingboxEvent::Log(line) => self.queue_core_log("singbox", &line),
                SingboxEvent::Error(message) => self.on_singbox_error(&message),
                SingboxEvent::StateChanged(running) => self.on_core_state_changed(running),
                SingboxEvent::Stopped(code) => self.on_core_stopped("singbox", code as i64),
                SingboxEvent::Started => {}
            }
        }
        while let Ok(event) = zapret_events.try_recv() {
            match event {
                ZapretEvent::Log(line) => self.queue_core_log("zapret", &line),
                ZapretEvent::Stopped => self.on_zapret_stopped_for_binding(),
                ZapretEvent::Error(message) => self.log(&format!("[zapret] {message}")),
                ZapretEvent::Started => {}
            }
        }
    }

    /// Earliest deadline across every timer, or a small default tick.
    fn next_wake(&self) -> Instant {
        let now = Instant::now();
        let default = now + Duration::from_millis(50);
        let mut earliest = default;
        let consider = |deadline: Instant, earliest: &mut Instant| {
            if deadline < *earliest {
                *earliest = deadline;
            }
        };
        if let Some(deadline) = self.timers.save_deadline {
            consider(deadline, &mut earliest);
        }
        if let Some((deadline, _)) = self.timers.transition.as_ref() {
            consider(*deadline, &mut earliest);
        }
        if let Some(deadline) = self.timers.lock_check {
            consider(deadline, &mut earliest);
        }
        if let Some(deadline) = self.timers.core_log_flush {
            consider(deadline, &mut earliest);
        }
        if let Some(deadline) = self.timers.startup_sync {
            consider(deadline, &mut earliest);
        }
        for (deadline, _) in &self.timers.delayed {
            consider(*deadline, &mut earliest);
        }
        if let Some(deadline) = self.subscriptions.auto_refresh_deadline() {
            consider(deadline, &mut earliest);
        }
        if let Some((_, _, _, _, _, deadline)) = self.zapret_waits.binding {
            consider(deadline, &mut earliest);
        }
        if let Some((_, _, _, deadline)) = self.zapret_waits.pre_connect {
            consider(deadline, &mut earliest);
        }
        earliest
    }
}

// ── shared state helpers used by every submodule ────────────────────────────

impl AppController {
    pub fn profile_loaded(&self) -> bool {
        self.load_state == LoadState::Loaded
    }

    pub fn selected_node(&self) -> Option<Node> {
        crate::node_runtime_service::get_node_by_id(
            &self.state.nodes,
            self.state.selected_node_id.as_deref(),
        )
        .cloned()
    }

    pub(crate) fn get_node_by_id(&self, node_id: Option<&str>) -> Option<Node> {
        crate::node_runtime_service::get_node_by_id(&self.state.nodes, node_id).cloned()
    }

    /// Python `_set_connection_status`.
    pub(crate) fn set_connection_status(&mut self, phase: &str, message: &str, level: &str) {
        self.emit(AppEvent::ConnectionStatus {
            phase: phase.into(),
            message: message.into(),
        });
        self.emit(AppEvent::Status {
            level: normalize_level(level),
            message: message.into(),
        });
    }

    /// Python `status.emit(level, message)`.
    pub(crate) fn emit_status(&mut self, level: &str, message: &str) {
        self.emit(AppEvent::Status {
            level: normalize_level(level),
            message: message.into(),
        });
    }

    /// Python `_compute_connected_state`.
    pub(crate) fn compute_connected_state(&self) -> bool {
        if self.active_core == "singbox" {
            if let Some(session) = self.active_session.as_ref()
                && session.hybrid
            {
                return self.singbox.is_running() && self.xray.is_running();
            }
            return self.singbox.is_running();
        }
        self.xray.is_running()
    }

    /// Python `_refresh_connected_state` returning `(previous, connected)`.
    pub fn refresh_connected_state(&mut self) -> (bool, bool) {
        let previous = self.connected;
        self.connected = self.compute_connected_state();
        (previous, self.connected)
    }

    pub(crate) fn shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    pub(crate) fn startup_settings_generation(&self) -> u64 {
        self.startup_generation.load(Ordering::SeqCst)
    }

    /// Python `is_singbox_editor_mode`.
    pub fn is_singbox_editor_mode(&self) -> bool {
        self.is_singbox_editor_mode_with(&self.state.settings)
    }

    pub(crate) fn is_singbox_editor_mode_with(&self, settings: &AppSettings) -> bool {
        // TUN runs sing-box, except when Xray is the only core (Xray's own TUN).
        settings.tun_mode && settings.core_preference != "xray"
    }

    /// Python `uses_xray_raw_config`.
    pub fn uses_xray_raw_config(&self) -> bool {
        !self.is_singbox_editor_mode()
    }

    /// Python `_system_proxy_bypass_lan`.
    pub(crate) fn system_proxy_bypass_lan(&self) -> bool {
        self.state.settings.system_proxy_bypass_lan
    }

    /// Python `_routing_signature`.
    pub(crate) fn routing_signature(&mut self, routing: Option<&RoutingSettings>) -> String {
        let mut service = SignatureService::from_state(&self.state);
        if let Some(routing) = routing {
            service.routing = routing;
        }
        service.routing_signature()
    }

    /// Compute every signature in one profile-inspection pass (the profile
    /// inspections may create or sync the active profile files, exactly like
    /// the Python controller).
    pub(crate) fn compute_transition_signatures(
        &mut self,
        node: Option<&Node>,
        settings: Option<&AppSettings>,
        routing: Option<&RoutingSettings>,
    ) -> Signatures {
        let xray_profile = self.xray_profile_inspection();
        let singbox_profile = self.singbox_document_state().ok();
        let mut service = SignatureService::from_state(&self.state);
        service.xray_profile = xray_profile.as_ref();
        service.singbox_profile = singbox_profile.as_ref();
        if let Some(node) = node {
            service.selected_node = Some(node);
        }
        if let Some(settings) = settings {
            service.settings = settings;
        }
        if let Some(routing) = routing {
            service.routing = routing;
        }
        Signatures {
            routing: service.routing_signature(),
            transition: service.transition_signature(node, true).unwrap_or_default(),
            xray_layer: service.xray_layer_signature(node).unwrap_or_default(),
            tun_layer: service.tun_layer_signature().unwrap_or_default(),
        }
    }

    /// Python `_transition_signature`.
    pub(crate) fn transition_signature(&mut self) -> String {
        self.transition_signature_for(None, None, None)
    }

    pub(crate) fn transition_signature_for(
        &mut self,
        node: Option<&Node>,
        settings: Option<&AppSettings>,
        routing: Option<&RoutingSettings>,
    ) -> String {
        self.compute_transition_signatures(node, settings, routing)
            .transition
    }

    /// Python `_xray_layer_signature`.
    pub(crate) fn xray_layer_signature(&mut self) -> String {
        self.compute_transition_signatures(None, None, None)
            .xray_layer
    }

    /// Python `_can_connect_without_selected_node` (read-only so the
    /// `ConnectionController` trait can call it through `&self`; the
    /// ensure/create side effects run in the neighbouring signature and
    /// runtime flows).
    pub(crate) fn can_connect_without_selected_node(&self) -> bool {
        if self.is_singbox_editor_mode() {
            return self
                .readonly_singbox_has_proxy_outbound()
                .map(|has_proxy| !has_proxy)
                .unwrap_or(true);
        }
        if self.uses_xray_raw_config() {
            return self
                .readonly_xray_has_proxy_outbound()
                .map(|has_proxy| !has_proxy)
                .unwrap_or(true);
        }
        false
    }

    /// Python `owned_core_process_pids`: main and temporary core PIDs owned by
    /// this Lumen instance. The ported managers keep their child handles
    /// private, so the PID set is currently empty and ownership filtering
    /// relies on the canonical executable paths below.
    pub fn owned_core_process_pids(&self) -> HashSet<u32> {
        HashSet::new()
    }

    /// Python `owned_core_executable_paths`: configured core paths used by
    /// this Lumen instance (the primary ownership signal for conflict scans).
    pub fn owned_core_executable_paths(&self) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();
        let settings = self.state.settings.clone();
        for (configured, default) in [
            (
                settings.xray_path.clone(),
                lumen_core::constants::xray_path_default(),
            ),
            (
                settings.singbox_path.clone(),
                lumen_core::constants::singbox_path_default(),
            ),
        ] {
            let resolved = lumen_core::path_utils::resolve_configured_path(
                &configured,
                Some(&default),
                true,
                false,
            )
            .unwrap_or(default);
            paths.insert(resolved);
        }
        paths
    }
}

fn normalize_level(level: &str) -> &'static str {
    match level {
        "success" => "success",
        "error" => "error",
        "warning" => "warning",
        _ => "info",
    }
}

// ── transition machinery ────────────────────────────────────────────────────

impl AppController {
    /// Python `_needs_transition`.
    pub(crate) fn needs_transition_now(&mut self) -> bool {
        let node = self.selected_node();
        let current_signature = self.transition_signature();
        let can_connect_without = self.can_connect_without_selected_node();
        let context = TransitionContext {
            desired_connected: self.runtime.desired_connected,
            locked: self.locked,
            has_selected_node: node.is_some(),
            can_connect_without_selected_node: can_connect_without,
            connected: self.connected,
            blocked_transition_signature: &self.transitions.blocked_signature,
            current_transition_signature: &current_signature,
            active_session: self.active_session.as_ref(),
            can_apply_proxy_runtime_change: false,
            can_tun_hot_swap: false,
            can_proxy_hot_swap: false,
        };
        needs_transition(&context)
    }

    /// Python `_can_apply_proxy_runtime_change`.
    pub(crate) fn can_apply_proxy_runtime_change(&mut self) -> bool {
        let Some(session) = self.active_session.clone() else {
            return false;
        };
        let settings_tun_mode = self.state.settings.tun_mode;
        let current_signature = self.xray_layer_signature();
        let proxy_enabled = self.state.settings.enable_system_proxy;
        let bypass_lan = self.system_proxy_bypass_lan();
        crate::transition_engine::can_apply_proxy_runtime_change(
            &session,
            settings_tun_mode,
            &current_signature,
            proxy_enabled,
            bypass_lan,
        )
    }

    /// Python `_can_proxy_hot_swap`.
    pub(crate) fn can_proxy_hot_swap(&mut self) -> bool {
        let Some(session) = self.active_session.clone() else {
            return false;
        };
        let settings = self.state.settings.clone();
        let preference = crate::node_runtime_service::core_preference_of(Some(&settings));
        let node = self.selected_node();
        if crate::node_runtime_service::proxy_core_for_node(node.as_ref(), preference) != "xray" {
            return false;
        }
        let inspection = self.inspect_active_xray_config();
        let (socks_port, http_port) = inspection
            .map(|inspection| (inspection.socks_port, inspection.http_port))
            .unwrap_or((0, 0));
        crate::transition_engine::can_proxy_hot_swap(
            &session,
            settings.tun_mode,
            socks_port,
            http_port,
            &self.xray_layer_signature(),
        )
    }

    /// Python `_can_tun_hot_swap`.
    pub(crate) fn can_tun_hot_swap(&mut self) -> bool {
        let Some(session) = self.active_session.clone() else {
            return false;
        };
        let settings = self.state.settings.clone();
        let node = self.selected_node();
        if node.is_none() || session.node_id == node.as_ref().map(|node| node.id.clone()) {
            return false;
        }
        let routing = self.state.routing.clone();
        let signature = self
            .compute_transition_signatures(node.as_ref(), Some(&settings), Some(&routing))
            .tun_layer;
        crate::transition_engine::can_tun_hot_swap(
            &session,
            settings.tun_mode,
            node.is_some(),
            &signature,
        )
    }

    /// Python `_compute_transition_action`.
    pub(crate) fn compute_transition_action_now(&mut self) -> Option<&'static str> {
        if self.transitions.resume_reconnect_pending {
            if !self.runtime.desired_connected || self.locked {
                return None;
            }
            return Some(if self.connected { "reconnect" } else { "connect" });
        }
        let node = self.selected_node();
        let session = self.active_session.clone();
        let current_signature = self.transition_signature();
        let can_connect_without = self.can_connect_without_selected_node();
        let (can_apply, can_tun, can_proxy) = if session.is_some() {
            (
                self.can_apply_proxy_runtime_change(),
                self.can_tun_hot_swap(),
                self.can_proxy_hot_swap(),
            )
        } else {
            (false, false, false)
        };
        let context = TransitionContext {
            desired_connected: self.runtime.desired_connected,
            locked: self.locked,
            has_selected_node: node.is_some(),
            can_connect_without_selected_node: can_connect_without,
            connected: self.connected,
            blocked_transition_signature: &self.transitions.blocked_signature,
            current_transition_signature: &current_signature,
            active_session: session.as_ref(),
            can_apply_proxy_runtime_change: can_apply,
            can_tun_hot_swap: can_tun,
            can_proxy_hot_swap: can_proxy,
        };
        compute_transition_action(&context)
    }

    /// Python `_emit_no_transition_feedback`.
    pub(crate) fn emit_no_transition_feedback(&mut self) {
        if !self.runtime.desired_connected {
            return;
        }
        let node = self.selected_node();
        if self.locked {
            self.set_connection_status(
                "error",
                "Приложение заблокировано. Разблокируйте для подключения.",
                "warning",
            );
            return;
        }
        if node.is_none() && !self.can_connect_without_selected_node() {
            self.set_connection_status("error", "Сначала выберите сервер.", "warning");
            return;
        }
        if !self.transitions.blocked_signature.is_empty()
            && self.transition_signature() == self.transitions.blocked_signature
        {
            self.set_connection_status(
                "error",
                "Предыдущая попытка подключения уже завершилась ошибкой. Измените сервер или настройки и попробуйте снова.",
                "warning",
            );
        }
    }

    /// Python `_request_transition` (owner-task entry; worker contexts post
    /// `InternalEvent::Transition` through `request_transition_queued`).
    pub fn request_transition(&mut self, reason: &str) {
        if self.shutting_down() || self.system_shutdown {
            return;
        }
        self.transitions.blocked_signature = String::new();
        self.transitions.pending = true;
        self.transitions.reason = reason.into();
        self.transitions.generation += 1;
        if self.transitions.active || self.transitions.scheduled {
            return;
        }
        self.transitions.scheduled = true;
        self.timers.transition =
            Some((Instant::now() + Duration::from_millis(120), TransitionTick::Drain));
    }

    /// `request_transition` from a non-owner thread (queued signal).
    pub fn request_transition_queued(&self, reason: &str) {
        self.queue_internal(InternalEvent::Transition(reason.into()));
    }

    /// Python `_drain_transition_queue`.
    pub(crate) fn drain_transition_queue(&mut self) {
        if self.shutting_down() {
            self.transitions.scheduled = false;
            self.transitions.pending = false;
            return;
        }
        self.transitions.scheduled = false;
        if self.transitions.active {
            return;
        }
        if !self.transitions.pending && !self.needs_transition_now() {
            self.emit(AppEvent::TransitionStateChanged {
                active: false,
                text: String::new(),
            });
            return;
        }
        let action = self.compute_transition_action_now();
        let Some(action) = action else {
            self.transitions.resume_reconnect_pending = false;
            self.runtime.auto_switch_transitioning = false;
            self.auto_switch.finish_transition();
            self.emit_no_transition_feedback();
            self.transitions.pending = false;
            self.emit(AppEvent::TransitionStateChanged {
                active: false,
                text: String::new(),
            });
            self.resolve_pending_update_disconnects();
            return;
        };
        self.transitions.pending = false;
        self.transitions.resume_reconnect_pending = false;
        let reason = if self.transitions.reason.is_empty() {
            action.to_owned()
        } else {
            self.transitions.reason.clone()
        };
        self.transitions.active = true;
        self.emit(AppEvent::TransitionStateChanged {
            active: true,
            text: transition_status_text(action).to_owned(),
        });
        let generation = self.transitions.generation;
        self.timers.transition = Some((
            Instant::now() + Duration::from_millis(16),
            TransitionTick::Execute { action, reason, generation },
        ));
    }

    /// Python `_execute_transition_action`.
    pub(crate) fn execute_transition_action(
        &mut self,
        action: &'static str,
        reason: &str,
        generation: u64,
    ) {
        if !self.transitions.active {
            return;
        }
        if reason == "system resume" && !self.runtime.desired_connected {
            self.on_transition_action_complete(true, action, reason, generation);
            return;
        }
        let host = match self.zapret_pre_connect_host(action) {
            Ok(host) => host,
            // Zapret must never block a connect.
            Err(error) => {
                self.log(&format!(
                    "[zapret] не удалось подготовить Zapret для сервера: {error}"
                ));
                String::new()
            }
        };
        if !host.is_empty() {
            // winws2 has to be running before the core dials the bound server,
            // otherwise the first handshake goes out without the strategy.
            self.log(&format!("[zapret] определяю IP сервера {host} перед подключением"));
            self.begin_zapret_pre_connect_wait(&host, action, reason, generation);
            return;
        }
        self.start_transition_worker(action, reason, generation);
    }

    /// Python `_start_transition_worker`.
    pub(crate) fn start_transition_worker(
        &mut self,
        action: &'static str,
        reason: &str,
        generation: u64,
    ) {
        if let Err(error) = self.prepare_zapret_scope_before_connect(action) {
            // Zapret must never block a connect.
            self.log(&format!(
                "[zapret] не удалось подготовить Zapret для сервера: {error}"
            ));
        }
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_transition_action(action, reason)
        }))
        .unwrap_or_else(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|value| value.to_string()))
                .unwrap_or_else(|| "panic".into());
            self.log(&format!("[transition] worker error: {message}"));
            false
        });
        self.on_transition_action_complete(ok, action, reason, generation);
    }

    /// Python `_run_transition_action`.
    pub(crate) fn run_transition_action(&mut self, action: &str, reason: &str) -> bool {
        match action {
            "disconnect" => self.disconnect_current(true, true, false),
            "connect" => self.connect_selected(false),
            "proxy_update" => self.apply_proxy_runtime_change(),
            "proxy_hot_swap" => self.restart_proxy_core(reason),
            "tun_hot_swap" => self.hot_swap_node(reason),
            _ => self.reconnect(reason),
        }
    }

    /// Python `_on_transition_action_complete`.
    pub(crate) fn on_transition_action_complete(
        &mut self,
        ok: bool,
        action: &str,
        _reason: &str,
        generation: u64,
    ) {
        // Any finished transition ends the auto-switch window: leaving the flag
        // set would make the next unexpected drop evaluate the kill-switch as
        // inactive and tear down the system proxy (fail-open).
        self.runtime.auto_switch_transitioning = false;
        self.auto_switch.finish_transition();
        if self.shutting_down() {
            self.transitions.active = false;
            return;
        }
        let (was_connected, is_connected) = self.refresh_connected_state();
        if was_connected != is_connected {
            self.emit(AppEvent::ConnectionChanged(is_connected));
            // Never start a worker without the session snapshot: a core can
            // briefly report running after a failed transition, before its
            // Clash secret / Xray inbound tags were captured.
            self.on_metrics_request(is_connected && self.active_session.is_some());
        } else if ok
            && is_connected
            && self.active_session.is_some()
            && !self.runtime.metrics_running()
        {
            // The core's state_changed signal can arrive before
            // connect_selected() has captured the active session. Start the
            // worker once the Clash bearer secret / Xray inbound tags are
            // available; otherwise it would run forever with an empty metrics
            // contract and history would remain at zero.
            self.start_metrics_worker();
        }
        let settings = self.state.settings.clone();
        if ok
            && matches!(action, "proxy_hot_swap" | "connect" | "reconnect")
            && self.connected
            && settings.discord_proxy_enabled
            && !settings.tun_mode
        {
            self.schedule_delayed(Duration::from_millis(250), DelayedAction::DiscordProxy);
        }
        if ok || was_connected != is_connected || self.zapret.server_scope().is_some() {
            self.schedule_delayed(Duration::ZERO, DelayedAction::SyncZapret);
        }
        if ok {
            self.transitions.blocked_signature = String::new();
        } else {
            self.transitions.blocked_signature = self.transition_signature();
            self.runtime.desired_connected = self.connected;
        }
        self.transitions.active = false;
        if generation != self.transitions.generation {
            self.transitions.pending = true;
        }
        if self.transitions.pending || self.needs_transition_now() {
            self.transitions.scheduled = true;
            self.timers.transition =
                Some((Instant::now() + Duration::from_millis(50), TransitionTick::Drain));
        } else {
            self.emit(AppEvent::TransitionStateChanged {
                active: false,
                text: String::new(),
            });
        }
        self.resolve_pending_update_disconnects();
    }

    /// Python `_on_metrics_request`.
    pub(crate) fn on_metrics_request(&mut self, start: bool) {
        if self.shutting_down() {
            return;
        }
        if start {
            self.start_metrics_worker();
        } else {
            self.stop_metrics_worker();
        }
    }

    /// Python `request_resume_reconnect`.
    pub fn request_resume_reconnect(&mut self, reason: &str) -> bool {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return false;
        }
        let settings = self.state.settings.clone();
        if self.shutting_down() || self.system_shutdown || self.locked {
            return false;
        }
        if !settings.reconnect_after_sleep {
            return false;
        }
        if !self.runtime.desired_connected {
            return false;
        }
        self.transitions.resume_reconnect_pending = true;
        self.request_transition(reason);
        true
    }

    /// Python `toggle_connection`.
    pub fn toggle_connection(&mut self) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let current_target = if self.transitions.active || self.transitions.pending {
            self.runtime.desired_connected
        } else {
            self.connected
        };
        if !current_target
            && self.state.settings.tun_mode
            && !lumen_platform::startup::is_process_elevated()
        {
            self.emit_status(
                "warning",
                "VPN (TUN) недоступен без прав администратора. Переключитесь на режим прокси.",
            );
            return;
        }
        if current_target {
            self.transitions.resume_reconnect_pending = false;
        }
        self.runtime.desired_connected = !current_target;
        self.request_transition("toggle connection");
    }

    /// Python `switch_next_node`.
    pub fn switch_next_node(&mut self) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        self.switch_node_by(1);
    }

    /// Python `switch_prev_node`.
    pub fn switch_prev_node(&mut self) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        self.switch_node_by(-1);
    }

    fn switch_node_by(&mut self, direction: i64) {
        if self.state.nodes.is_empty() {
            return;
        }
        let current_id = self.state.selected_node_id.clone();
        let mut index = 0usize;
        if let Some(current_id) = current_id.as_deref()
            && let Some(position) = self
                .state
                .nodes
                .iter()
                .position(|node| node.id == current_id)
        {
            index = position;
        }
        let count = self.state.nodes.len() as i64;
        let next = (index as i64 + direction).rem_euclid(count) as usize;
        let next_id = self.state.nodes[next].id.clone();
        self.set_selected_node(&next_id);
    }

    /// Python `update_routing`.
    pub fn update_routing(&mut self, routing: RoutingSettings, restart_runtime: bool) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let mut routing = routing;
        if !lumen_core::constants::ROUTING_MODES.contains(&routing.mode.as_str()) {
            routing.mode = "rule".into();
        }
        self.state.routing = routing;
        self.emit(AppEvent::RoutingChanged);
        self.schedule_save();

        if restart_runtime && (self.connected || self.runtime.desired_connected) {
            self.request_transition("routing changed");
        }
    }

    /// Python `apply_routing_preset`.
    pub fn apply_routing_preset(&mut self, preset_id: &str, restart_runtime: bool) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let region = normalize_regional_preset(&self.state.settings.regional_preset);
        let requested_preset = preset_id.trim().to_lowercase();
        let preset_id = normalize_routing_preset_for_region(&requested_preset, &region);
        if requested_preset != preset_id {
            self.log(&format!(
                "[routing] mapped incompatible preset for {region}: {requested_preset} -> {preset_id}"
            ));
        }
        let before = self.routing_signature(None);
        let routing =
            lumen_core::routing_presets::build_routing_preset(&self.state.routing, &preset_id);
        let after = self.routing_signature(Some(&routing));
        if before == after {
            self.log(&format!("[routing] preset {preset_id} already active"));
            return;
        }
        self.log(&format!("[routing] applying dashboard preset: {preset_id}"));
        self.update_routing(routing, restart_runtime);
    }

    /// Python `_commit_regional_preset`.
    pub(crate) fn commit_regional_preset(&mut self, region: &str, restart_runtime: bool) -> bool {
        // Atomically apply a region and its equivalent built-in route preset.
        let region = normalize_regional_preset(region);
        let old_region = normalize_regional_preset(&self.state.settings.regional_preset.clone());
        let current_preset = self.state.routing.preset_id.clone();
        let target_preset =
            lumen_core::routing_presets::equivalent_regional_preset(&current_preset, &region);
        let routing =
            lumen_core::routing_presets::build_routing_preset(&self.state.routing, &target_preset);
        let routing_changed = self.state.routing.to_dict() != routing.to_dict();
        if old_region == region && !routing_changed {
            return false;
        }
        self.state.settings.regional_preset = region.clone();
        self.state.routing = routing;
        self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        // The regional quick-preset model also listens to routing changes, even
        // when the currently selected preset remains "global".
        if routing_changed || old_region != region {
            self.emit(AppEvent::RoutingChanged);
        }
        self.schedule_save();
        self.log(&format!(
            "[routing] regional profile committed: {old_region} -> {region}, preset={target_preset}"
        ));

        if restart_runtime && (self.connected || self.runtime.desired_connected) {
            self.request_transition("regional routing profile changed");
        }
        true
    }

    /// Python `request_regional_preset_change`. Returns `applied`,
    /// `downloading`, `unchanged`, `pending` or `busy` for the UI bridge.
    pub fn request_regional_preset_change(&mut self, value: &str) -> &'static str {
        let region = normalize_regional_preset(value);
        let current_region = normalize_regional_preset(&self.state.settings.regional_preset);
        if !self.pending_regional_preset.is_empty() {
            return if self.pending_regional_preset == region {
                "pending"
            } else {
                "busy"
            };
        }
        if lumen_engines::core_resource_updater::regional_geodata_installed(&region, None, None) {
            if current_region == region {
                return "unchanged";
            }
            self.commit_regional_preset(&region, true);
            return "applied";
        }

        self.pending_regional_preset = region.clone();
        if !self.run_resource_update("geodata", true, Some(&region)) {
            self.pending_regional_preset = String::new();
            return "busy";
        }
        "downloading"
    }
}

// ── session capture ─────────────────────────────────────────────────────────

impl AppController {
    /// Python `_capture_active_session`.
    pub(crate) fn capture_active_session(&mut self, node: Option<&Node>, capture: SessionCapture) {
        let settings = self.state.settings.clone();
        let socks_port = capture.socks_port.unwrap_or(settings.local_socks_port);
        let http_port = capture.http_port.unwrap_or(settings.local_http_port);
        let xray_inbound_tags = capture.xray_inbound_tags.unwrap_or_default();
        let ping_host = if capture.ping_host.is_empty() {
            node.map(|node| node.server.clone()).unwrap_or_default()
        } else {
            capture.ping_host.clone()
        };
        let ping_port = if capture.ping_port <= 0 {
            node.map(|node| node.port).unwrap_or(0)
        } else {
            capture.ping_port
        };
        let proxy_bypass_lan = if capture.tun {
            self.state.routing.bypass_lan
        } else {
            self.system_proxy_bypass_lan()
        };
        let signatures = self.compute_transition_signatures(node, None, None);
        self.active_session = Some(ActiveSessionSnapshot {
            node_id: node.map(|node| node.id.clone()),
            node_server: node.map(|node| node.server.clone()).unwrap_or_default(),
            active_core: capture.core.clone(),
            tun_mode: capture.tun,
            proxy_enabled: settings.enable_system_proxy,
            proxy_bypass_lan,
            xray_path: settings.xray_path.clone(),
            singbox_path: settings.singbox_path.clone(),
            socks_port,
            http_port,
            routing_signature: signatures.routing,
            transition_signature: signatures.transition,
            xray_layer_signature: signatures.xray_layer,
            tun_layer_signature: signatures.tun_layer,
            hybrid: capture.hybrid,
            api_port: capture.api_port,
            clash_api_secret: capture.clash_api_secret,
            xray_inbound_tags,
            sidecar_relay_port: capture.sidecar_relay_port,
            protect_ss_port: capture.protect_ss_port,
            protect_ss_password: capture.protect_ss_password,
            ping_host,
            ping_port,
            clash_api_selector: capture.clash_api_selector,
            clash_api_node_signatures: capture.clash_api_node_signatures,
            health_proxy_url: capture.health_proxy_url,
        });
        self.auto_switch.mark_session_started();
        self.transitions.blocked_signature = String::new();
    }

    pub(crate) fn active_session_view(&self) -> Option<ActiveSessionView> {
        self.active_session.as_ref().map(|session| ActiveSessionView {
            node_id: session.node_id.clone(),
            active_core: session.active_core.clone(),
            tun_mode: session.tun_mode,
            hybrid: session.hybrid,
            api_port: session.api_port,
            socks_port: session.socks_port,
            http_port: session.http_port,
            clash_api_selector: session.clash_api_selector.clone(),
            clash_api_secret: session.clash_api_secret.clone(),
            clash_api_node_signatures: session.clash_api_node_signatures.clone(),
            xray_inbound_tags: session.xray_inbound_tags.clone(),
            health_proxy_url: session.health_proxy_url.clone(),
        })
    }
}

// ── trait implementations ───────────────────────────────────────────────────

impl OperationsController for AppController {
    type Runtime = XrayRuntimeConfig;

    fn set_active_core(&mut self, core: &str) {
        self.active_core = core.into();
    }

    fn set_switching(&mut self, switching: bool) {
        self.runtime.switching = switching;
    }

    fn xray_api_port(&self) -> i64 {
        self.xray_api_port
    }

    fn set_xray_api_port(&mut self, port: i64) {
        self.xray_api_port = port;
    }

    fn protect_ss_port(&self) -> i64 {
        self.protect_ss_port
    }

    fn set_protect_ss_port(&mut self, port: i64) {
        self.protect_ss_port = port;
    }

    fn protect_ss_password(&self) -> String {
        self.protect_ss_password.clone()
    }

    fn set_protect_ss_password(&mut self, password: &str) {
        self.protect_ss_password = password.into();
    }

    fn selected_node(&self) -> Option<Node> {
        self.selected_node()
    }

    fn selected_node_mut(&mut self) -> Option<&mut Node> {
        let id = self.state.selected_node_id.clone()?;
        self.state.nodes.iter_mut().find(|node| node.id == id)
    }

    fn settings(&self) -> &AppSettings {
        &self.state.settings
    }

    fn routing(&self) -> RoutingSettings {
        self.state.routing.clone()
    }

    fn active_session(&self) -> Option<ActiveSessionView> {
        self.active_session_view()
    }

    fn log(&mut self, line: &str) {
        self.log(line);
    }

    fn set_connection_status(&mut self, phase: &str, message: &str, level: &str) {
        self.set_connection_status(phase, message, level);
    }

    fn emit_metrics_request(&mut self, connected: bool) {
        self.on_metrics_request(connected);
    }

    fn emit_connection_changed(&mut self, connected: bool) {
        self.emit(AppEvent::ConnectionChanged(connected));
    }

    fn save(&mut self) {
        self.save();
    }

    fn refresh_connected_state(&mut self) -> (bool, bool) {
        self.refresh_connected_state()
    }

    fn handle_unexpected_disconnect(&mut self) {
        handle_unexpected_disconnect(self);
    }

    fn system_proxy_bypass_lan(&self) -> bool {
        self.system_proxy_bypass_lan()
    }

    fn xray_is_running(&self) -> bool {
        self.xray.is_running()
    }

    fn xray_validate_config(
        &mut self,
        xray_path: &str,
        config: &serde_json::Value,
    ) -> (bool, String) {
        self.xray.validate_config(xray_path, config)
    }

    fn xray_start(&mut self, xray_path: &str, config: &serde_json::Value, prevalidated: bool) -> bool {
        self.xray.start(xray_path, config, prevalidated)
    }

    fn xray_stop(&mut self, expected: bool, fast: bool) -> bool {
        self.xray.stop(expected, fast)
    }

    fn singbox_is_running(&self) -> bool {
        self.singbox.is_running()
    }

    fn singbox_validate_config(
        &mut self,
        singbox_path: &str,
        config: &serde_json::Value,
    ) -> (bool, String) {
        self.singbox.validate_config(singbox_path, config)
    }

    fn singbox_stop(&mut self, expected: bool, fast: bool) -> bool {
        self.singbox.stop(expected, fast)
    }

    fn proxy_enable(
        &mut self,
        http_port: i64,
        socks_port: i64,
        bypass_lan: bool,
        configure_firefox: bool,
    ) -> Result<(), String> {
        self.proxy_enable(http_port, socks_port, bypass_lan, configure_firefox)
    }

    fn proxy_disable(&mut self, restore_previous: bool) {
        self.proxy_disable(restore_previous);
    }

    fn build_runtime_xray_config(
        &mut self,
        node: Option<&Node>,
    ) -> Result<XrayRuntimeConfig, String> {
        self.build_runtime_xray_config(node)
    }

    fn plan_runtime_singbox(
        &mut self,
        node: Option<&Node>,
        tun_mode: bool,
        hot_switch: bool,
    ) -> Result<lumen_engines::singbox::runtime_planner::SingboxRuntimePlan, String> {
        self.plan_runtime_singbox(node, tun_mode, hot_switch)
    }

    fn start_singbox_runtime_plan(
        &mut self,
        plan: &lumen_engines::singbox::runtime_planner::SingboxRuntimePlan,
    ) -> bool {
        self.start_singbox_runtime_plan(plan)
    }

    fn infer_singbox_ping_target(
        &self,
        payload: &serde_json::Value,
        node: Option<&Node>,
    ) -> (String, i64) {
        crate::runtime_introspection::infer_singbox_ping_target(payload, node)
    }

    fn capture_active_session(&mut self, node: Option<&Node>, capture: SessionCapture) {
        self.capture_active_session(node, capture);
    }
}

impl RuntimeController for AppController {
    fn runtime(&self) -> &ConnectionRuntime {
        &self.runtime
    }

    fn runtime_mut(&mut self) -> &mut ConnectionRuntime {
        &mut self.runtime
    }

    fn active_core(&self) -> &str {
        &self.active_core
    }

    fn clear_active_session(&mut self) {
        self.active_session = None;
    }

    fn reset_auto_switch(&mut self, reset_cooldown: bool, reset_cycle: bool) {
        self.auto_switch
            .reset_for_connection(reset_cooldown, reset_cycle);
    }

    fn emit_status(&mut self, level: &str, message: &str) {
        self.emit_status(level, message);
    }

    fn emit_live_metrics(&mut self, update: MetricsUpdate<'_>) {
        let event = match update {
            MetricsUpdate::Sample(sample) => LiveMetricsEvent::Sample(Arc::new(clone_sample(sample))),
            MetricsUpdate::Reset => LiveMetricsEvent::Reset,
        };
        self.emit(AppEvent::LiveMetricsUpdated(event));
    }

    fn check_auto_switch(&mut self, sample: &LiveMetricsSample) {
        let health = sample.health.clone();
        let metrics = AutoSwitchMetrics {
            down_bps: sample.down_bps,
            up_bps: sample.up_bps,
            health: Some(&health),
        };
        let mut desired = self.runtime.desired_connected;
        let mut events = Vec::new();
        {
            let mut connection = AutoSwitchConnection {
                state: &mut self.state,
                active_session: self.active_session.as_ref(),
                connected: self.connected,
                switching: self.runtime.switching,
                reconnecting: self.runtime.reconnecting,
                desired_connected: &mut desired,
            };
            self.auto_switch
                .check_auto_switch(&mut connection, metrics, |event, _| events.push(event));
        }
        self.runtime.desired_connected = desired;
        for event in events {
            match event {
                AutoSwitchEvent::Log(line) => self.log(&line),
                AutoSwitchEvent::Status { level, message } => self.emit_status(level, message),
                AutoSwitchEvent::Triggered(name) => {
                    self.emit(AppEvent::AutoSwitchTriggered(name));
                }
                AutoSwitchEvent::SelectionChanged(_) => {
                    self.emit(AppEvent::SelectionChanged(self.selected_node()));
                }
                AutoSwitchEvent::SaveRequested => self.save(),
                AutoSwitchEvent::TransitionRequested(reason) => {
                    self.runtime.desired_connected = true;
                    self.request_transition(match reason {
                        AutoSwitchReason::ProfileHealth => "auto-switch: profile HTTP health",
                        AutoSwitchReason::SpeedDrop => "auto-switch: speed drop",
                    });
                }
            }
        }
    }

    fn start_traffic_session(&mut self, label: &str, core: &str) {
        self.traffic_history.start_session(label, core);
    }

    fn end_traffic_session(&mut self) {
        self.traffic_history.end_session();
    }

    fn update_traffic_session(
        &mut self,
        stats: &BTreeMap<String, lumen_core::traffic_history::ProcessStatsUpdate>,
    ) {
        self.traffic_history.update_session(stats);
    }

    fn save_traffic_periodic(&mut self) {
        self.traffic_history.save_periodic();
    }

    fn effective_proxy_ports(&self) -> (i64, i64) {
        self.effective_proxy_ports_for_metrics()
    }
}

impl ConnectionController for AppController {
    fn locked(&self) -> bool {
        self.locked
    }

    fn owned_core_process_pids(&self) -> HashSet<u32> {
        self.owned_core_process_pids()
    }

    fn owned_core_executable_paths(&self) -> HashSet<PathBuf> {
        self.owned_core_executable_paths()
    }

    fn is_singbox_editor_mode(&self) -> bool {
        self.is_singbox_editor_mode()
    }

    fn uses_xray_raw_config(&self) -> bool {
        self.uses_xray_raw_config()
    }

    fn can_connect_without_selected_node(&self) -> bool {
        self.can_connect_without_selected_node()
    }

    fn active_singbox_config_name(&self) -> String {
        self.readonly_config_name(
            "sing-box",
            lumen_core::constants::SINGBOX_DEFAULT_CONFIG_NAME,
            "sing-box",
            &self.state.settings.singbox_config_file.clone(),
        )
    }

    fn active_xray_config_name(&self) -> String {
        self.readonly_config_name(
            "xray",
            lumen_core::constants::XRAY_DEFAULT_CONFIG_NAME,
            "xray",
            &self.state.settings.xray_config_file.clone(),
        )
    }

    fn schedule_save(&mut self) {
        self.schedule_save();
    }

    fn reset_tun_log_count(&mut self) {
        self.tun_log_count = 0;
    }

    fn disable_discord_proxy(&mut self) -> String {
        let result = self.discord_proxy.disable();
        result.message
    }
}

impl ShutdownController for AppController {
    fn stop_network_monitor(&mut self) {
        if let Some(monitor) = self.network_monitor.as_mut() {
            monitor.stop();
        }
    }

    fn stop_lock_timer(&mut self) {
        self.timers.lock_check = None;
    }

    fn cancel_background_workers(&mut self) {
        self.workers.stop_workers();
        self.subscriptions.shutdown(|_| {});
        // Dropping the service cancels the Xray update worker (Drop impl).
        self.updates = XrayUpdateService::default();
        for entry in self.resource_updates.drain(..) {
            entry.worker.cancel();
        }
        self.pending_resource_disconnects.clear();
        self.timers.delayed.clear();
        self.zapret_waits.binding = None;
        self.zapret_waits.pre_connect = None;
        self.zapret_dns.sender.take();
    }

    fn pending_background_workers(&mut self) -> Vec<&'static str> {
        let mut pending = Vec::new();
        if self.workers.has_pending_workers() {
            pending.push("measurement worker");
        }
        if self.background_tasks.has_pending() {
            pending.push("background task");
        }
        if !self.diagnostics_exports.is_empty() {
            pending.push("diagnostics export");
        }
        if !self.resource_updates.is_empty() {
            pending.push("resource update");
        }
        pending
    }

    fn zapret_is_running(&self) -> bool {
        self.zapret.running()
    }

    fn stop_zapret(&mut self, fast: bool) {
        self.zapret.stop_with(fast, true, true);
    }
}

impl PowerReconnectController for AppController {
    fn reconnect_after_sleep(&self) -> bool {
        self.state.settings.reconnect_after_sleep
    }

    fn locked(&self) -> bool {
        self.locked
    }

    fn shutting_down(&self) -> bool {
        self.shutting_down()
    }

    fn system_shutdown(&self) -> bool {
        self.system_shutdown
    }

    fn desired_connected(&self) -> Option<bool> {
        Some(self.runtime.desired_connected)
    }

    fn connected(&self) -> bool {
        self.connected
    }

    fn has_active_session(&self) -> bool {
        self.active_session.is_some()
    }

    fn transition_active(&self) -> bool {
        self.transitions.active
    }

    fn transition_pending(&self) -> bool {
        self.transitions.pending || self.transitions.scheduled
    }

    fn set_system_shutdown(&mut self, value: bool) {
        self.system_shutdown = value;
        self.runtime.system_shutdown = value;
    }

    fn stop_active_connection_processes(
        &mut self,
        disable_proxy: bool,
        fast: bool,
    ) -> Result<(), String> {
        crate::runtime_services::stop_active_connection_processes(self, disable_proxy, fast);
        Ok(())
    }

    fn request_resume_reconnect(&mut self, reason: &str) -> bool {
        self.request_resume_reconnect(reason)
    }

    fn log(&mut self, message: &str) {
        self.log(message);
    }
}

// ── shutdown ────────────────────────────────────────────────────────────────

impl AppController {
    /// Python `shutdown`.
    pub async fn shutdown(&mut self, deadline: Option<Instant>) {
        if self.shutting_down() {
            return;
        }
        self.shutting_down.store(true, Ordering::SeqCst);
        let deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(5));
        self.log("[app] AppController shutting down...");
        self.timers.startup_sync = None;
        self.timers.core_log_flush = None;
        self.timers.transition = None;
        self.transitions.pending = false;
        self.transitions.scheduled = false;
        self.runtime.desired_connected = false;
        self.transitions.resume_reconnect_pending = false;
        // Pending update disconnects are acknowledged negatively, whatever the
        // live connection state (Python `_confirm_update_disconnect(w, False)`).
        let connection = self.xray_update_connection();
        let mut update_events = Vec::new();
        self.updates
            .resolve_pending_disconnect(connection, |event| update_events.push(event));
        for event in update_events {
            self.dispatch_xray_update_event(event);
        }
        let confirms = std::mem::take(&mut self.pending_resource_disconnects);
        for confirm in confirms {
            let _ = confirm.send(false);
        }
        self.reconnect_after_resource_updates = false;
        if !self.system_shutdown {
            self.background_tasks.join_until(deadline);
            self.join_transition_worker(deadline);
        }
        crate::runtime_services::shutdown(self, Some(deadline)).await;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !self.traffic_history.close(remaining) {
            self.log(&format!(
                "[history] final write pending or failed: {}",
                self.traffic_history.last_save_error()
            ));
        }
        self.flush_state_saves(remaining);
        self.save_queue.shutdown();
        self.zapret_dns.sender.take();
        lumen_core::diagnostics_uploader::configure_diagnostics_upload("", lumen_core::constants::APP_VERSION);
    }

    /// Python `finalize_shutdown`.
    pub fn finalize_shutdown(&mut self) {
        // A transition that was already in native IO can finish after the first
        // deadline. Stop only our own runtimes once that transition is drained.
        crate::runtime_services::stop_active_connection_processes(self, true, true);
        if self.zapret.running() {
            self.zapret.stop_with(true, true, true);
        }
        self.timers.core_log_flush = None;
        while !self.pending_core_logs.is_empty() {
            self.flush_core_logs();
        }
        if !lumen_core::logging::shutdown_logging(true, Duration::from_secs(1)) {
            // A final file flush is still pending; nothing further can log.
        }
        self.request_event_loop_stop();
    }

    /// Python `has_pending_shutdown_work`.
    pub fn has_pending_shutdown_work(&mut self) -> bool {
        self.save_queue.has_pending()
            || self.background_tasks.has_pending()
            || self.transitions.active
            || self.workers.has_pending_workers()
            || self.updates.is_running()
            || !self.resource_updates.is_empty()
            || !self.diagnostics_exports.is_empty()
    }

    /// Python `_join_transition_worker` (the worker is the owner task).
    pub(crate) fn join_transition_worker(&mut self, deadline: Instant) {
        while self.transitions.active {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.log("[app] Transition worker did not stop before shutdown timeout");
                return;
            }
            std::thread::sleep(Duration::from_millis(10).min(remaining));
        }
    }
}

// ── timer processing and completion wiring ──────────────────────────────────

impl AppController {
    pub(crate) fn schedule_delayed(&mut self, delay: Duration, action: DelayedAction) {
        self.timers.delayed.push((Instant::now() + delay, action));
    }

    /// Drain every due timer. Runs on the owner task; transition work keeps
    /// the Python worker-thread serialization through the active flag.
    pub(crate) async fn process_timers(&mut self) {
        let now = Instant::now();
        // Save flush (single-shot, 250 ms).
        if let Some(deadline) = self.timers.save_deadline
            && now >= deadline
        {
            self.timers.save_deadline = None;
            self.flush_scheduled_save();
        }
        // Transition timer.
        if let Some((deadline, tick)) = self.timers.transition.clone()
            && now >= deadline
        {
            self.timers.transition = None;
            match tick {
                TransitionTick::Drain => self.drain_transition_queue(),
                TransitionTick::Execute { action, reason, generation } => {
                    self.execute_transition_action(action, &reason, generation);
                }
            }
        }
        // Core log flush (interval, 50 ms).
        if let Some(deadline) = self.timers.core_log_flush
            && now >= deadline
        {
            self.timers.core_log_flush = Some(now + Duration::from_millis(50));
            self.flush_core_logs();
        }
        // Auto lock (interval, 15 s, started with deferred services).
        if let Some(deadline) = self.timers.lock_check
            && now >= deadline
        {
            self.timers.lock_check = Some(now + Duration::from_millis(15_000));
            self.check_auto_lock();
        }
        // Startup state sync (interval, 120 s).
        if let Some(deadline) = self.timers.startup_sync
            && now >= deadline
        {
            self.timers.startup_sync = Some(now + Duration::from_millis(120_000));
            self.schedule_startup_state_sync();
        }
        // Delayed single-shots.
        let due: Vec<DelayedAction> = self
            .timers
            .delayed
            .iter()
            .filter(|(deadline, _)| *deadline <= now)
            .map(|(_, action)| action.clone())
            .collect();
        if !due.is_empty() {
            self.timers.delayed.retain(|(deadline, _)| *deadline > now);
            for action in due {
                self.run_delayed_action(action).await;
            }
        }
        // Subscription auto refresh.
        let proxy_url = if self.connected {
            self.get_effective_http_proxy_url()
        } else {
            None
        };
        let context = crate::subscription_dispatcher::SubscriptionDispatchContext {
            profile_loaded: self.profile_loaded(),
            locked: self.locked,
            effective_proxy_url: proxy_url.as_deref(),
        };
        self.subscriptions
            .tick_auto_refresh(&self.state, context, now, chrono::Utc::now());
        self.background_tasks.reap_finished();
        for error in self.runtime.reap_metrics() {
            self.log(&format!("[metrics] worker failed: {error}"));
        }
    }

    async fn run_delayed_action(&mut self, action: DelayedAction) {
        match action {
            DelayedAction::DiscordProxy => self.apply_discord_proxy().await,
            DelayedAction::SyncZapret => self.sync_zapret_server_binding(),
            DelayedAction::CountryDetectBatch => self.process_country_detection_batch(),
            DelayedAction::StartCountryIpResolution => self.start_country_ip_resolution(),
            DelayedAction::ZapretPreConnectTimeout { action, reason, generation } => {
                if self
                    .zapret_waits
                    .pre_connect
                    .take()
                    .is_some()
                    && self.transitions.active
                    && !self.shutting_down()
                {
                    self.start_transition_worker(action, &reason, generation);
                }
            }
            DelayedAction::ZapretBindingTimeout { token } => {
                let pending = self
                    .zapret_waits
                    .binding
                    .take()
                    .filter(|(pending, ..)| *pending == token);
                if let Some((_, _, _, _, host, _)) = pending {
                    self.on_zapret_binding_ips(token, &host, Vec::new());
                }
            }
        }
    }
}

// ── worker completion handlers (used by the event loop) ─────────────────────

impl AppController {
    pub(crate) fn handle_worker_message(&mut self, message: crate::worker_service::WorkerMessage) {
        let mut events = Vec::new();
        self.workers
            .handle_worker_event(&mut self.state, message, |event| events.push(event));
        for event in events {
            self.handle_test_event(&event);
        }
    }

    pub(crate) fn handle_test_event(&mut self, event: &TestEvent) {
        match event {
            TestEvent::Status { level, message } => self.emit_status(level, message),
            TestEvent::Log(line) => self.log(line),
            TestEvent::PingUpdated { node_id, ms } => {
                self.emit(AppEvent::PingUpdated {
                    node_id: node_id.clone(),
                    ms: *ms,
                });
            }
            TestEvent::SpeedUpdated {
                node_id,
                mbps,
                alive,
            } => {
                self.emit(AppEvent::SpeedUpdated {
                    node_id: node_id.clone(),
                    mbps: *mbps,
                    alive: *alive,
                });
            }
            TestEvent::SpeedNodeProgress { node_id, percent } => {
                self.emit(AppEvent::SpeedNodeProgress {
                    node_id: node_id.clone(),
                    percent: *percent,
                });
            }
            TestEvent::Progress {
                kind,
                completed,
                total,
                finished,
            } => {
                self.emit(AppEvent::BulkTaskProgress {
                    task: match kind {
                        crate::worker_service::TestKind::Ping => "ping".into(),
                        crate::worker_service::TestKind::Speed => "speed".into(),
                    },
                    current: *completed,
                    total: *total,
                    completed: *finished,
                });
            }
            TestEvent::SpeedCancelled { completed, total } => {
                self.emit(AppEvent::SpeedTestCancelled {
                    completed: *completed,
                    total: *total,
                });
            }
            TestEvent::ConnectivityDone {
                ok,
                message,
                elapsed_ms,
            } => {
                self.emit(AppEvent::ConnectivityTestDone {
                    ok: *ok,
                    message: message.clone(),
                    elapsed_ms: *elapsed_ms,
                });
            }
            TestEvent::SaveRequested => self.save(),
            TestEvent::ScheduleSave => self.schedule_save(),
        }
    }
}
