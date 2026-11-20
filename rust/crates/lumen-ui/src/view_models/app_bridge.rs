//! Facade view-model that exposes the `AppController` to the egui shell
//! (port of `qml_app/bridge/app_bridge.py`).
//!
//! **Porting model.** The Python bridge was a `QObject` living on the GUI
//! thread: controller signals updated cached properties, actions invoked
//! controller methods directly. Here the bridge owns the controller behind a
//! mutex and keeps a dedicated pump thread that drives
//! [`AppController::run_event_loop`]; the UI thread performs actions by
//! locking the controller for the duration of the (synchronous) call, exactly
//! like the Python "run it on the controller thread" semantics. Where the
//! Python emitted a Qt signal, the bridge pushes a [`UiEvent`] onto its
//! outbox (or raises a change flag) for the shell to drain each frame.
//!
//! Flows that the Python bridge implemented on top of the controller
//! (subscription fetch batching, app-update worker threads) were already
//! absorbed by the ported controller (`subscription_dispatcher`,
//! `app_update_check`/`app_updater`), so the bridge only forwards results and
//! localized notices.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use lumen_app::app_controller::{AppController, AppEvent, ControllerTask, LiveMetricsEvent};
use lumen_app::profile_service::ProfileEditResult;
use lumen_core::bounded_logs::LogRing;
use lumen_core::constants::{
    APP_VERSION, DEFAULT_DISCORD_SOCKS_PORT, DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT,
    ONBOARDING_REVISION, RECOMMENDED_VPN_URL, SPEED_TEST_MAX_CONCURRENCY, singbox_path_default,
    xray_path_default,
};
use lumen_core::country_flags::{detect_country, get_flag_emoji};
use lumen_core::i18n::{
    active_map, available_languages, language_name, set_language, tr, translate_dynamic,
};
use lumen_core::models::{AppSettings, Node, RoutingSettings, normalize_core_preference};
use lumen_core::process_presets::PROCESS_PRESETS;
use lumen_core::release_notes::{LEGACY_PROFILE_VERSION, is_upgrade, notes_between, version_key};
use lumen_core::routing_presets::{
    BUILT_IN_ROUTING_PRESET_IDS, is_internal_preset_rule, normalize_regional_preset,
    regional_routing_preset_ids,
};
use lumen_core::routing_rule_import::{
    normalize_routing_address, parse_routing_rules, split_routing_addresses,
};
use lumen_core::service_presets::SERVICE_PRESETS;
use lumen_core::warp_generator::WARP_GROUP;
use lumen_core::zapret_strategies as strategies;
use lumen_platform::process_conflicts::{scan_network_conflicts, terminate_scanned_conflicts};
use lumen_platform::startup::is_process_elevated;
use lumen_platform::win_process_list::list_running_executables;

use crate::view_models::configs_helpers;
use crate::view_models::history_helpers;
use crate::view_models::log_model::{LogFilter, LogModel};
use crate::view_models::node_edit_helpers;
use crate::view_models::node_list_model::{
    DashboardNodeProxy, NodeListModel, server_display_name_without_country_prefix,
};
use crate::view_models::process_model::ProcessModel;
use crate::view_models::zapret_helpers;

/// Events the Python bridge emitted as Qt signals (shell-facing outbox).
#[derive(Debug, Clone)]
pub enum UiEvent {
    Toast {
        level: String,
        message: String,
    },
    ActionToast {
        level: String,
        message: String,
        action_id: String,
        action_label: String,
    },
    AutoSwitch(String),
    BulkTaskProgress {
        task: String,
        current: usize,
        total: usize,
        completed: bool,
    },
    ConnectivityResult {
        ok: bool,
        message: String,
        elapsed_ms: Option<u64>,
    },
    AppUpdateState(Value),
    XrayUpdateState(Value),
    ResourceUpdateState(Value),
    NetworkConflictRequested {
        mode: String,
        message: String,
        can_terminate: bool,
    },
    AdminRelaunchRequested,
    TrayNotify {
        title: String,
        message: String,
    },
    TrayMessageRequested,
    /// Share link rendered as PNG data URI (empty payload means the shell must
    /// render the link itself; see `node_export_link`).
    NodeQrReady {
        link: String,
        name: String,
    },
    NodeImported(String),
    SubscriptionImported(String),
    OpenDirectory(PathBuf),
}

/// Python property-change notify signals as explicit flags the shell reads
/// each frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiChanges {
    pub connected: bool,
    pub transition_busy: bool,
    pub transition_direction: bool,
    pub runtime: bool,
    pub metrics: bool,
    pub selection: bool,
    pub routing: bool,
    pub settings: bool,
    pub subscriptions: bool,
    pub node_filters: bool,
    pub node_sort: bool,
    pub node_filter: bool,
    pub node_table_layout: bool,
    pub zapret_table_layout: bool,
    pub selected_subscription: bool,
    pub locked: bool,
    pub tray_available: bool,
    pub quitting: bool,
    pub language: bool,
    pub subscription_importing: bool,
    pub subscription_import_status: bool,
    pub regional_preset_busy: bool,
    pub onboarding: bool,
    pub warp_generation: bool,
    pub tests_running: bool,
    pub updates_available: bool,
}

impl UiChanges {
    /// Clear every flag (the shell calls this after consuming a frame).
    pub fn clear(&mut self) {
        *self = UiChanges::default();
    }
}

#[derive(Debug, Clone)]
enum PendingToast {
    Toast {
        level: String,
        message: String,
    },
    ActionToast {
        level: String,
        message: String,
        action_id: String,
        action_label: String,
    },
}

/// Cached display payload of `_on_live_metrics`.
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub down_bps: f64,
    pub up_bps: f64,
    pub latency_ms: i64,
    pub traffic_available: bool,
    pub traffic_reason: String,
    pub health_status: String,
    pub health_reason: String,
}

/// Python `_zapret_binding_choices` rows.
#[derive(Debug, Clone)]
pub struct ZapretBindingRow {
    pub node_id: String,
    pub name: String,
    pub preset: String,
    pub missing_node: bool,
    pub missing_preset: bool,
    pub active: bool,
    pub strategy_mode: bool,
    pub kind_title: String,
    pub choices: Vec<(String, String)>,
    pub choice_index: i64,
    pub placeholder: String,
}

/// One Zapret server option (Python `zapretServerOptions`).
#[derive(Debug, Clone)]
pub struct ZapretServerOption {
    pub value: String,
    pub label: String,
}

/// One proxy-chain hop (Python `proxyChainPreview`).
#[derive(Debug, Clone)]
pub struct ProxyChainHop {
    pub role: String,
    pub label: String,
}

/// Python `AppBridge`.
pub struct AppBridge {
    controller: Arc<Mutex<AppController>>,
    /// Last per-frame reads, served while a transition holds the controller.
    read_cache: Mutex<std::collections::HashMap<String, Box<dyn std::any::Any + Send>>>,
    deferred_events: VecDeque<AppEvent>,
    /// Preset files listed from disk; the Zapret page asks for them several
    /// times per frame.
    zapret_presets_cache: Mutex<Option<(std::time::Instant, Vec<Value>)>>,
    runtime: Arc<tokio::runtime::Runtime>,
    event_rx: broadcast::Receiver<AppEvent>,
    pump: Option<PumpHandle>,

    pub node_model: NodeListModel,
    pub dashboard_nodes: DashboardNodeProxy,
    pub log_model: LogModel,
    pub log_filter: LogFilter,
    pub process_model: ProcessModel,
    pending_ui_logs: Arc<LogRing<String>>,

    outbox: VecDeque<UiEvent>,
    changes: UiChanges,

    // ── cached QML properties ──────────────────────────────────────────────
    connected: bool,
    busy: bool,
    transition_target_connected: bool,
    transition_connecting: bool,
    runtime_phase: String,
    runtime_message: String,
    metrics: MetricsSnapshot,
    session_upload_total: i64,
    session_download_total: i64,
    session_raw_upload_last: Option<i64>,
    session_raw_download_last: Option<i64>,
    selected_id: String,
    selected_name: String,
    selected_flag: String,
    selected_flag_country: String,
    selected_flag_source: String,
    selected_latency: i64,
    selected_protocol: String,
    selected_source: String,
    running_tests: HashSet<String>,
    manual_selection_in_progress: bool,
    routing_mode: String,
    routing_cached: RoutingSettings,
    tun_mode: bool,
    proxy_enabled: bool,
    discord_proxy: bool,
    theme: String,
    language: String,
    accent: String,
    filter_group: String,
    filter_text: String,
    sort_key: String,
    sort_asc: bool,
    tray_available: bool,
    quitting: bool,
    system_shutdown: bool,
    updates_available: bool,
    deferred_started: bool,
    toasts_ready: bool,
    pending_toasts: Vec<PendingToast>,
    pending_network_mode: String,
    pending_conflict_processes: Vec<Value>,
    sub_importing: bool,
    sub_import_status: String,
    pending_regional_preset: String,
    settings_cached: AppSettings,
    nodes_cached: Vec<Node>,
    manual_groups_cached: Vec<String>,
    subscriptions_cached: Vec<Value>,
    diagnostics_exporting: bool,
    warp_generating: std::sync::atomic::AtomicBool,
    warp_added: i64,
    shutdown_started: bool,
    settings_revision: u64,
    locked: bool,
    task_sender: tokio::sync::mpsc::UnboundedSender<ControllerTask>,
    power_state: lumen_platform::power_reconnect::PowerReconnectState,
    app_update: Option<lumen_app::app_update_check::AppUpdate>,
    update_check: Option<tokio::task::JoinHandle<()>>,
    update_check_silent: bool,
    /// Core versions for the Updates tab, probed off the UI thread
    /// (`xray -version` and `sing-box version` take hundreds of ms).
    component_versions: Arc<parking_lot::Mutex<Option<Value>>>,
    component_versions_started: bool,
    update_download: Option<lumen_app::app_updater::AppUpdateDownloader>,
    warp_cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

struct PumpHandle {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

// ── module-level helpers (Python free functions) ────────────────────────────

/// Python `_join_messages` — readable multi-line toast text: unique messages,
/// one per line.
pub fn join_messages(errors: &[String], limit: usize) -> String {
    let mut unique: Vec<String> = Vec::new();
    for error in errors {
        let text = error.trim().to_string();
        if !text.is_empty() && !unique.contains(&text) {
            unique.push(text);
        }
    }
    let mut shown: Vec<String> = unique.iter().take(limit).cloned().collect();
    if unique.len() > limit {
        shown.push(tr(
            "…и ещё {count}",
            &[("count", &(unique.len() - limit).to_string())],
        ));
    }
    shown.join("\n")
}

/// Python `_node_source_caption` — where the node came from: its subscription,
/// else a non-default group.
fn node_source_caption(subscriptions: &[Value], node: Option<&Node>) -> String {
    let Some(node) = node else {
        return String::new();
    };
    let sub_id = node.subscription_id.trim().to_string();
    if !sub_id.is_empty() {
        for sub in subscriptions {
            if sub
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
                == sub_id
            {
                let name = sub
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .or_else(|| {
                        sub.get("group")
                            .and_then(Value::as_str)
                            .filter(|text| !text.trim().is_empty())
                    });
                if let Some(name) = name {
                    return name.trim().to_string();
                }
            }
        }
    }
    let group = node.group.trim().to_string();
    if !group.is_empty() && group.to_lowercase() != "default" {
        group
    } else {
        String::new()
    }
}

const STATUS_REPLACEMENTS: [(&str, &str); 20] = [
    ("Запуск VPN...", "Starting VPN..."),
    ("Подключение VPN...", "Connecting VPN..."),
    ("VPN подключён", "VPN connected"),
    ("VPN отключён", "VPN disconnected"),
    ("Остановка прокси...", "Stopping proxy..."),
    ("Прокси подключён", "Proxy connected"),
    ("Прокси отключён", "Proxy disconnected"),
    ("Остановка VPN...", "Stopping VPN..."),
    (
        "Проверка обновлений sing-box...",
        "Checking sing-box updates...",
    ),
    ("Обновление sing-box...", "Updating sing-box..."),
    (
        "Проверка обновлений geoip/geosite...",
        "Checking geoip/geosite updates...",
    ),
    ("Обновление geoip/geosite...", "Updating geoip/geosite..."),
    (
        "geoip/geosite уже обновляются",
        "geoip/geosite update is already running",
    ),
    (
        "Проверка обновлений приложения...",
        "Checking app updates...",
    ),
    ("Приложение обновлено", "Application is up to date"),
    ("Обновление уже выполняется", "Update is already running"),
    (
        "Обновление Xray уже выполняется",
        "Xray update is already running",
    ),
    ("Обновление Xray...", "Updating Xray..."),
    ("Проверка обновлений Xray...", "Checking Xray updates..."),
    ("Новых серверов не найдено", "No new servers found"),
];

const STATUS_PREFIX_REPLACEMENTS: [(&str, &str); 27] = [
    ("Не удалось проверить droute: ", "Could not check droute: "),
    ("Не удалось обновить droute: ", "Could not update droute: "),
    ("Доступен droute ", "droute available: "),
    ("droute обновлен до ", "droute updated to "),
    ("droute актуален (", "droute is up to date ("),
    ("Запуск прокси: ", "Starting proxy: "),
    ("Подключено: ", "Connected: "),
    ("Запуск VPN: ", "Starting VPN: "),
    ("Подключение VPN: ", "Connecting VPN: "),
    ("Не удалось подключиться: ", "Could not connect: "),
    ("Не удалось отключиться: ", "Could not disconnect: "),
    ("Подписка обновлена: ", "Subscription updated: "),
    ("Подписки обновлены: ", "Subscriptions updated: "),
    ("Импортировано серверов: ", "Servers imported: "),
    ("Авто-обновление подписок: +", "Subscription auto update: +"),
    ("Доступно обновление v", "Update available: v"),
    (
        "Не удалось обновить geoip/geosite: ",
        "Could not update geoip/geosite: ",
    ),
    (
        "Не удалось проверить sing-box: ",
        "Could not check sing-box: ",
    ),
    (
        "Не удалось обновить sing-box: ",
        "Could not update sing-box: ",
    ),
    ("Ошибка загрузки: ", "Download error: "),
    ("Ошибка установки: ", "Install error: "),
    ("Доступно обновление Xray: ", "Xray update available: "),
    ("Xray core обновлён до ", "Xray core updated to "),
    ("sing-box обновлен до ", "sing-box updated to "),
    ("sing-box обновлён до ", "sing-box updated to "),
    (
        "Доступен sing-box extended ",
        "sing-box extended available: ",
    ),
    ("Некорректная ссылка lumen", "Invalid lumen link"),
];

/// Python `_longest_catalog_prefix` — longest catalog key that starts
/// `message` (the bucketed index from Python collapses to a linear scan here;
/// the catalogs are small and the result is cached by the caller).
fn longest_catalog_prefix(
    catalog: &std::collections::BTreeMap<String, String>,
    message: &str,
) -> String {
    let mut best: Option<&String> = None;
    for key in catalog.keys() {
        if key.is_empty() || !message.starts_with(key.as_str()) {
            continue;
        }
        match best {
            Some(current) if current.len() >= key.len() => {}
            _ => best = Some(key),
        }
    }
    best.cloned().unwrap_or_default()
}

/// Python `_localized_backend_message` — translate backend text while an
/// English UI language is active.
pub fn localized_backend_message(message: &str, english_ui: bool) -> String {
    if message.is_empty() || !english_ui {
        return message.to_string();
    }
    let catalog = active_map();
    if let Some(localized) = translate_dynamic(message) {
        return localized;
    }
    let catalog_prefix = longest_catalog_prefix(&catalog, message);
    if !catalog_prefix.is_empty() {
        let tail = &message[catalog_prefix.len()..];
        let localized_tail = translate_dynamic(tail);
        let tail = match localized_tail {
            Some(localized_tail) => localized_tail,
            None => tail.to_string(),
        };
        return format!(
            "{}{}",
            catalog.get(&catalog_prefix).cloned().unwrap_or_default(),
            tail
        );
    }
    for (ru, en) in STATUS_REPLACEMENTS {
        if message == ru {
            return catalog.get(ru).cloned().unwrap_or_else(|| en.to_string());
        }
    }
    for (ru, en) in STATUS_PREFIX_REPLACEMENTS {
        if message.starts_with(ru) {
            let tail = &message[ru.len()..];
            return format!(
                "{}{}",
                catalog.get(ru).cloned().unwrap_or_else(|| en.to_string()),
                tail
            );
        }
    }
    if message.starts_with("Xray core актуален (") && message.ends_with(')') {
        return format!(
            "{} {}",
            catalog
                .get("Xray core актуален")
                .cloned()
                .unwrap_or_else(|| "Xray core is up to date".to_string()),
            &message["Xray core актуален ".len()..]
        );
    }
    if message.starts_with("sing-box актуален (") && message.ends_with(')') {
        return format!(
            "{} {}",
            catalog
                .get("sing-box актуален")
                .cloned()
                .unwrap_or_else(|| "sing-box is up to date".to_string()),
            &message["sing-box актуален ".len()..]
        );
    }
    message.to_string()
}

/// Python `_localized_log_line` — preserve the severity prefix.
pub fn localized_log_line(line: &str, english_ui: bool) -> String {
    let level_prefixes = ["DEBUG:", "INFO:", "WARNING:", "ERROR:", "CRITICAL:"];
    for prefix in level_prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            let body = rest.trim_start();
            let body_start = line.len() - rest.len();
            return format!("{} {}", prefix, localized_backend_message(body, english_ui))
                .trim_end()
                .to_string()
                + if body_start == line.len() { "" } else { "" };
        }
    }
    localized_backend_message(line, english_ui)
}

/// Python `_accent_for_theme_preset`.
pub fn accent_for_theme_preset(value: &str) -> &'static str {
    match value.trim().to_lowercase().as_str() {
        "absolutely" => "#CC7D5E",
        "ayu" => "#D97706",
        "catppuccin" => "#CBA6F7",
        "codex" => "#2563EB",
        "dracula" => "#BD93F9",
        "everforest" => "#4F8A10",
        "github" => "#0969DA",
        "gruvbox" => "#B57614",
        "linear" => "#5E6AD2",
        "lobster" => "#E11D48",
        "material" => "#1976D2",
        "matrix" => "#16A34A",
        "midnight" => "#2563EB",
        "monokai" => "#679E00",
        "night-owl" => "#0E7CBF",
        "nord" => "#5E81AC",
        "notion" => "#2383E2",
        "one" => "#4078F2",
        "oscurange" => "#EA580C",
        "raycast" => "#FF3B30",
        "rose-pine" => "#907AA9",
        "sentry" => "#6E56CF",
        "solarized" => "#268BD2",
        "temple" => "#7A9F00",
        "tokyo-night" => "#34548A",
        "vercel" => "#111111",
        "vscode-plus" => "#007ACC",
        "xcode" => "#0066CC",
        _ => "#0078D4",
    }
}

// ── Flag SVG sprite (Python get_flag_svg_data_uri) ──────────────────────────

fn flag_sprite_symbols() -> &'static std::collections::HashMap<String, (String, String)> {
    use std::sync::OnceLock;
    static SYMBOLS: OnceLock<std::collections::HashMap<String, (String, String)>> = OnceLock::new();
    SYMBOLS.get_or_init(|| {
        let text = include_str!("../../../../../windows/xray_fluent/qml_app/qml/assets/flags.svg");
        let symbol_re = regex::Regex::new(
            r#"(?s)<symbol\s+[^>]*id="([A-Za-z]{2})"[^>]*viewBox="([^"]+)"[^>]*>(.*?)</symbol>"#,
        )
        .expect("symbol regex");
        symbol_re
            .captures_iter(text)
            .map(|captures| {
                (
                    captures[1].to_ascii_lowercase(),
                    (captures[2].to_owned(), captures[3].to_owned()),
                )
            })
            .collect()
    })
}

pub(crate) fn flag_svg(code: &str) -> Option<String> {
    let code = code.trim().to_ascii_lowercase();
    if code.len() != 2 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let (view_box, body) = flag_sprite_symbols().get(&code)?;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ns1=\"http://www.w3.org/1999/xlink\" viewBox=\"{view_box}\">{body}</svg>"
    ))
}

/// Python `get_flag_svg_data_uri`.
pub fn flag_svg_data_uri(code: &str) -> String {
    let Some(svg) = flag_svg(code) else {
        return String::new();
    };
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg.as_bytes())
    )
}

/// Python `is_mica_supported` — a registry-based approximation (the DWM
/// composition and high-contrast probes need extra Win32 features the egui
/// shell does not use).
pub fn is_mica_supported() -> bool {
    crate::shell::win_native::is_mica_supported()
}

/// Python `fit_window_geometry` (window_geometry.py) — fit a restored window
/// into one monitor's usable desktop area.
#[allow(clippy::too_many_arguments)]
pub fn fit_window_geometry(
    width: i64,
    height: i64,
    x: i64,
    y: i64,
    minimum_width: i64,
    minimum_height: i64,
    available_x: i64,
    available_y: i64,
    available_width: i64,
    available_height: i64,
    position_saved: bool,
) -> Value {
    let aw = available_width.max(1);
    let ah = available_height.max(1);
    let ax = available_x;
    let ay = available_y;

    let min_width = aw.min(minimum_width.max(1));
    let min_height = ah.min(minimum_height.max(1));
    let fitted_width = min_width.max(width.max(1).min(aw));
    let fitted_height = min_height.max(height.max(1).min(ah));

    let (fitted_x, fitted_y) = if position_saved {
        (
            x.max(ax).min(ax + aw - fitted_width),
            y.max(ay).min(ay + ah - fitted_height),
        )
    } else {
        (
            ax + ((aw - fitted_width).max(0) / 2),
            ay + ((ah - fitted_height).max(0) / 2),
        )
    };

    json!({"x": fitted_x, "y": fitted_y, "width": fitted_width, "height": fitted_height})
}

impl AppBridge {
    /// Python `AppBridge.__init__` — construct the controller, subscribe to
    /// its signal channel and initialize the models.
    pub fn new() -> Self {
        Self::with_controller(AppController::new())
    }

    /// Constructor with a pre-built controller (tests, custom storage roots).
    pub fn with_controller(controller: AppController) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("bridge tokio runtime");
        let event_rx = controller.subscribe();
        let task_sender = controller.task_sender();
        let locked = controller.locked;
        let language = "en".to_string();
        set_language(&language);
        Self {
            controller: Arc::new(Mutex::new(controller)),
            read_cache: Mutex::new(std::collections::HashMap::new()),
            deferred_events: VecDeque::new(),
            zapret_presets_cache: Mutex::new(None),
            runtime: Arc::new(runtime),
            event_rx,
            pump: None,
            node_model: NodeListModel::new(),
            dashboard_nodes: DashboardNodeProxy::new("manual"),
            log_model: LogModel::new(4000),
            log_filter: LogFilter::default(),
            process_model: ProcessModel::new(),
            pending_ui_logs: Arc::new(LogRing::new(2000).expect("pending ui log ring")),
            outbox: VecDeque::new(),
            changes: UiChanges::default(),
            connected: false,
            busy: false,
            transition_target_connected: false,
            transition_connecting: false,
            runtime_phase: String::new(),
            runtime_message: String::new(),
            metrics: MetricsSnapshot::default(),
            session_upload_total: 0,
            session_download_total: 0,
            session_raw_upload_last: None,
            session_raw_download_last: None,
            selected_id: String::new(),
            selected_name: String::new(),
            selected_flag: String::new(),
            selected_flag_country: String::new(),
            selected_flag_source: String::new(),
            selected_latency: -1,
            selected_protocol: String::new(),
            selected_source: String::new(),
            running_tests: HashSet::new(),
            manual_selection_in_progress: false,
            routing_mode: "rule".into(),
            routing_cached: RoutingSettings::default(),
            tun_mode: false,
            proxy_enabled: false,
            discord_proxy: false,
            theme: "dark".into(),
            language,
            accent: "#0078D4".into(),
            filter_group: String::new(),
            filter_text: String::new(),
            sort_key: "manual".into(),
            sort_asc: true,
            tray_available: false,
            quitting: false,
            system_shutdown: false,
            updates_available: false,
            deferred_started: false,
            toasts_ready: false,
            pending_toasts: Vec::new(),
            pending_network_mode: String::new(),
            pending_conflict_processes: Vec::new(),
            sub_importing: false,
            sub_import_status: String::new(),
            pending_regional_preset: String::new(),
            settings_cached: AppSettings::default(),
            nodes_cached: Vec::new(),
            manual_groups_cached: Vec::new(),
            subscriptions_cached: Vec::new(),
            diagnostics_exporting: false,
            warp_generating: std::sync::atomic::AtomicBool::new(false),
            warp_added: 0,
            shutdown_started: false,
            settings_revision: 0,
            locked,
            task_sender,
            power_state: Default::default(),
            app_update: None,
            update_check: None,
            update_check_silent: false,
            component_versions: Arc::default(),
            component_versions_started: false,
            update_download: None,
            warp_cancel: None,
        }
    }

    /// Escape hatch for the shell: a locked view of the controller.
    pub fn controller(&self) -> Arc<Mutex<AppController>> {
        self.controller.clone()
    }

    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Connect/disconnect transitions run on the controller thread and keep
    /// the controller locked while the cores start (seconds). Getters the
    /// shell calls every frame check this first and serve their last value
    /// instead of stalling the UI thread.
    fn controller_free(&self) -> bool {
        // No timed wait: several reads per frame would each add it to the frame.
        self.controller.try_lock().is_some()
    }

    fn cached_read<T: Clone + 'static>(&self, key: &str) -> Option<T> {
        self.read_cache.lock().get(key).and_then(|value| value.downcast_ref::<T>()).cloned()
    }

    fn remember_read<T: Send + 'static>(&self, key: String, value: T) {
        self.read_cache.lock().insert(key, Box::new(value));
    }

    /// Whether a running transition currently holds the controller.
    pub fn controller_busy(&self) -> bool {
        self.controller.is_locked()
    }

    /// Queue controller work (Python queued connections).
    pub fn submit(&self, task: ControllerTask) {
        let _ = self.task_sender.send(task);
    }

    /// Spawn the controller pump thread (`run_event_loop` owner). Idempotent.
    pub fn start_controller_loop(&mut self) {
        if self.pump.is_some() {
            return;
        }
        let controller = self.controller.clone();
        let runtime = self.runtime.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("controller-owner".into())
            .spawn(move || {
                let mut managers = controller.lock().manager_events();
                while !stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                    let running =
                        runtime.block_on(controller.lock().drive_event_loop_once(&mut managers));
                    if !running {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(8));
                }
            })
            .expect("controller pump thread");
        self.pump = Some(PumpHandle {
            stop,
            thread: Some(thread),
        });
    }

    /// Stop the pump thread after requesting the controller loop to end.
    pub fn stop_controller_loop(&mut self) {
        if let Some(mut pump) = self.pump.take() {
            self.controller.lock().request_event_loop_stop();
            pump.stop.store(true, std::sync::atomic::Ordering::Release);
            if let Some(thread) = pump.thread.take() {
                let _ = thread.join();
            }
        }
    }

    /// Python `_notify`.
    fn notify(&mut self, level: &str, message: &str) {
        if self.toasts_ready {
            self.outbox.push_back(UiEvent::Toast {
                level: level.to_string(),
                message: message.to_string(),
            });
        } else if self.pending_toasts.len() < 20 {
            self.pending_toasts.push(PendingToast::Toast {
                level: level.to_string(),
                message: message.to_string(),
            });
        }
    }

    /// Python `_notify_action`.
    fn notify_action(&mut self, level: &str, message: &str, action_id: &str, action_label: &str) {
        if self.toasts_ready {
            self.outbox.push_back(UiEvent::ActionToast {
                level: level.to_string(),
                message: message.to_string(),
                action_id: action_id.to_string(),
                action_label: action_label.to_string(),
            });
        } else if self.pending_toasts.len() < 20 {
            self.pending_toasts.push(PendingToast::ActionToast {
                level: level.to_string(),
                message: message.to_string(),
                action_id: action_id.to_string(),
                action_label: action_label.to_string(),
            });
        }
    }

    /// Python `_flush_pending_toasts`.
    pub fn flush_pending_toasts(&mut self) {
        self.toasts_ready = true;
        let pending = std::mem::take(&mut self.pending_toasts);
        for toast in pending {
            match toast {
                PendingToast::Toast { level, message } => {
                    self.outbox.push_back(UiEvent::Toast { level, message });
                }
                PendingToast::ActionToast {
                    level,
                    message,
                    action_id,
                    action_label,
                } => {
                    self.outbox.push_back(UiEvent::ActionToast {
                        level,
                        message,
                        action_id,
                        action_label,
                    });
                }
            }
        }
    }

    /// Drain the shell-facing event outbox (Python signal emissions).
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        self.outbox.drain(..).collect()
    }

    /// Read (and optionally clear) the per-frame change flags.
    pub fn changes(&self) -> UiChanges {
        self.changes
    }

    pub fn clear_changes(&mut self) {
        self.changes.clear();
    }

    fn english_ui(&self) -> bool {
        self.language.to_lowercase() != "ru"
    }

    fn localize(&self, message: &str) -> String {
        localized_backend_message(message, self.english_ui())
    }

    // ── lifecycle ──────────────────────────────────────────────────────────

    /// Python `load` — load persisted state and push initial snapshots.
    pub fn load(&mut self) {
        let load_ok = {
            let mut controller = self.controller.lock();
            controller.load()
        };
        if !load_ok {
            self.push_initial_snapshot();
            return;
        }
        let (settings, state) = {
            let controller = self.controller.lock();
            (controller.state.settings.clone(), controller.state.clone())
        };
        self.settings_cached = settings.clone();
        self.nodes_cached = state.nodes.clone();
        self.manual_groups_cached = state.manual_groups.clone();
        self.subscriptions_cached = state.subscriptions.clone();
        self.filter_group = settings.node_filter_group.trim().to_string();
        self.sort_key = if settings.node_sort_key.is_empty() {
            "manual".to_string()
        } else {
            settings.node_sort_key.clone()
        };
        self.sort_asc = settings.node_sort_ascending;
        self.dashboard_nodes.set_sort_key(&self.sort_key);
        self.push_initial_snapshot();
        // The subscription auto-refresh timer and the app/resource update
        // checks from Python `QTimer.singleShot` are handled by
        // `start_deferred_services` + `subscription_refresh_timer` in the
        // ported controller.
    }

    fn push_initial_snapshot(&mut self) {
        let (state, selected, connected, desired, locked) = {
            let controller = self.controller.lock();
            let selected = controller.selected_node();
            let connected = controller.connected;
            let desired = controller.runtime.desired_connected;
            (
                controller.state.clone(),
                selected,
                connected,
                desired,
                controller.locked,
            )
        };
        self.locked = locked;
        self.node_model.set_runtime_support(true);
        self.node_model.set_nodes(
            self.sorted_nodes(self.filtered_nodes(&state.nodes)),
            state.selected_node_id.as_deref(),
        );
        self.on_selection_changed_inner(selected);
        self.routing_cached = state.routing.clone();
        self.routing_mode = state.routing.mode.clone();
        self.changes.routing = true;
        self.apply_settings_snapshot(&state.settings);
        self.transition_target_connected = desired || connected;
        self.transition_connecting = self.transition_target_connected;
        self.on_connection_changed_inner(connected);
    }

    /// Python `startDeferred`.
    pub fn start_deferred(&mut self) {
        self.flush_pending_toasts();
        let profile_loaded = self.controller.lock().profile_loaded();
        if !profile_loaded || self.deferred_started {
            return;
        }
        self.deferred_started = true;
        {
            let mut controller = self.controller.lock();
            controller.start_deferred_services();
        }
        self.prefetch_component_versions();
        if self.settings_cached.check_updates || self.settings_cached.app_auto_update {
            self.start_app_update_check(true);
        }
        if self.settings_cached.resource_update_check {
            self.start_resource_update_check();
        }
        if self.settings_cached.resource_update_check || self.settings_cached.xray_auto_update {
            let controller = self.controller.clone();
            self.runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(4500)).await;
                let mut controller = controller.lock();
                let apply = controller.state.settings.xray_auto_update;
                if apply || controller.state.settings.resource_update_check {
                    controller.run_xray_core_update(apply, true);
                }
            });
        }
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            controller.lock().auto_connect_if_needed();
        });
    }

    /// Python `shutdown` — stop the loop, flush state and join the owner.
    pub fn shutdown(&mut self, deadline: Option<std::time::Instant>) {
        if self.shutdown_started {
            return;
        }
        self.shutdown_started = true;
        let deadline =
            deadline.unwrap_or_else(|| std::time::Instant::now() + Duration::from_secs(5));
        self.prepare_quit();
        if let Some(cancel) = self.warp_cancel.take() {
            let _ = cancel.send(());
        }
        if let Some(task) = self.update_check.take() {
            task.abort();
        }
        if let Some(worker) = self.update_download.take() {
            worker.cancel();
        }
        let controller = self.controller.clone();
        let runtime = self.runtime.clone();
        runtime.block_on(async move {
            let mut guard = controller.lock();
            guard.shutdown(Some(deadline)).await;
        });
        self.stop_controller_loop();
    }

    /// Python `prepareQuit`.
    pub fn prepare_quit(&mut self) {
        if !self.quitting {
            self.quitting = true;
            self.changes.quitting = true;
        }
    }

    /// Python `prepareSystemShutdown`.
    pub fn prepare_system_shutdown(&mut self) {
        self.system_shutdown = true;
        {
            use lumen_platform::power_reconnect::PowerReconnectController;
            let mut controller = self.controller.lock();
            controller.set_system_shutdown(true);
        }
        self.prepare_quit();
    }

    // ── controller event pump ──────────────────────────────────────────────

    /// Drain controller `AppEvent`s into the cached UI state. The shell calls
    /// this each frame.
    pub fn pump(&mut self) {
        self.pump_app_update_download();
        // While a transition holds the controller, events whose handlers read
        // it wait in `deferred_events`; status and transition events still
        // reach the UI so the status ring spins during a connect.
        let busy = self.controller.is_locked();
        if !busy {
            while let Some(event) = self.deferred_events.pop_front() {
                self.on_controller_event(event);
            }
        }
        loop {
            match self.event_rx.try_recv() {
                Ok(event) if busy && !Self::lock_free_event(&event) => {
                    self.deferred_events.push_back(event);
                }
                Ok(event) => self.on_controller_event(event),
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed)
                | Err(broadcast::error::TryRecvError::Empty) => {
                    break;
                }
            }
        }
    }

    fn lock_free_event(event: &AppEvent) -> bool {
        matches!(
            event,
            AppEvent::ConnectionStatus { .. }
                | AppEvent::TransitionStateChanged { .. }
                | AppEvent::ConnectionChanged(_)
                | AppEvent::LogLine(_)
                | AppEvent::SpeedUpdated { .. }
                | AppEvent::SpeedNodeProgress { .. }
        )
    }

    fn queue_ui_log(&mut self, line: &str) {
        self.pending_ui_logs.append(line.to_string());
        self.flush_ui_logs();
    }

    /// Python `_flush_ui_logs` — parse pending lines into the log model.
    pub fn flush_ui_logs(&mut self) {
        let english = self.english_ui();
        let pending = self.pending_ui_logs.drain(Some(96));
        if pending.is_empty() {
            return;
        }
        let localize = |line: String| -> String { localized_log_line(&line, english) };
        let translate = |line: &str| localized_backend_message(line, english);
        self.log_model
            .append_lines(pending.into_iter().map(localize), Some(&translate));
    }

    /// Python `clearLogs`.
    pub fn clear_logs(&mut self) {
        self.pending_ui_logs.clear();
        self.log_model.clear();
        let marker = tr("Журнал очищен; новые логи продолжают поступать", &[]);
        self.controller.lock().log(&format!("[app] {marker}"));
    }

    // ── controller → UI slots (Python `_on_*`) ─────────────────────────────

    fn on_controller_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::NodesChanged => self.on_nodes_changed(),
            AppEvent::SelectionChanged(node) => self.on_selection_changed_inner(node),
            AppEvent::ConnectionChanged(connected) => self.on_connection_changed_inner(connected),
            AppEvent::ConnectionStatus { phase, message } => {
                self.runtime_phase = phase;
                self.runtime_message = message;
                self.changes.runtime = true;
            }
            AppEvent::RoutingChanged => {
                let routing = self.controller.lock().state.routing.clone();
                self.routing_cached = routing.clone();
                self.routing_mode = routing.mode;
                self.changes.routing = true;
            }
            AppEvent::SettingsChanged(settings) => self.on_settings_changed_inner(settings),
            AppEvent::SubscriptionsChanged => {
                let subscriptions = self.controller.lock().state.subscriptions.clone();
                self.subscriptions_cached = subscriptions;
                self.changes.subscriptions = true;
                self.changes.onboarding = true;
            }
            AppEvent::LogLine(line) => self.queue_ui_log(&line),
            AppEvent::Status { level, message } => {
                self.on_status_message(level.to_owned(), message)
            }
            AppEvent::TransitionStateChanged { active, text: _ } => {
                self.on_transition(active);
            }
            AppEvent::PingUpdated { node_id, ms } => self.on_ping(&node_id, ms),
            AppEvent::SpeedUpdated {
                node_id,
                mbps,
                alive,
            } => {
                self.node_model.update_speed(&node_id, mbps);
                self.node_model.update_alive(&node_id, alive);
            }
            AppEvent::SpeedNodeProgress { node_id, percent } => {
                self.node_model
                    .update_speed_progress(&node_id, percent as i64);
            }
            AppEvent::SpeedTestCancelled { .. } => {}
            AppEvent::BulkTaskProgress {
                task,
                current,
                total,
                completed,
            } => {
                self.on_bulk_task_progress(&task, current, total, completed);
            }
            AppEvent::ConnectivityTestDone {
                ok,
                message,
                elapsed_ms,
            } => {
                self.outbox.push_back(UiEvent::ConnectivityResult {
                    ok,
                    message,
                    elapsed_ms,
                });
            }
            AppEvent::LiveMetricsUpdated(update) => match update {
                LiveMetricsEvent::Sample(sample) => self.on_live_metrics(&sample),
                LiveMetricsEvent::Reset => self.on_live_metrics_reset(),
            },
            AppEvent::XrayUpdateResult(result) => {
                self.changes.settings = true;
                self.outbox.push_back(UiEvent::XrayUpdateState(
                    self.xray_update_state_payload(&result),
                ));
            }
            AppEvent::XrayUpdateProgress(percent) => {
                self.outbox.push_back(UiEvent::XrayUpdateState(json!({
                    "phase": "updating",
                    "percent": percent,
                })));
            }
            AppEvent::ResourceUpdateResult(result) => {
                self.on_resource_update_result(result);
            }
            AppEvent::ResourceUpdateProgress { kind, percent } => {
                self.outbox.push_back(UiEvent::ResourceUpdateState(json!({
                    "kind": kind,
                    "phase": "updating",
                    "percent": percent,
                })));
            }
            AppEvent::LockStateChanged(locked) => {
                self.locked = locked;
                if locked {
                    self.notify("warning", "Приложение заблокировано");
                }
                self.changes.locked = true;
            }
            AppEvent::WarpGenerated(result) => self.on_warp_generated(result.ok()),
            AppEvent::AppUpdateChecked(result) => self.on_app_update_checked(result),
            AppEvent::SubscriptionImportStatus(status) => {
                self.sub_importing = status.is_some();
                self.sub_import_status = status.unwrap_or_default();
                self.changes.subscription_importing = true;
                self.changes.subscription_import_status = true;
            }
            AppEvent::SubscriptionImported(group) => {
                self.outbox.push_back(UiEvent::SubscriptionImported(group));
            }
            AppEvent::DiagnosticsFinished(result) => self.on_diagnostics_finished(result),
            AppEvent::PassphraseRequired => {
                let message = tr(
                    "Данные зашифрованы: укажите пароль шифрования в разделе «Данные», иначе серверы не будут загружены",
                    &[],
                );
                self.notify("error", &message);
            }
            AppEvent::AdminRelaunchRequested => self.on_admin_relaunch(),
            AppEvent::AutoSwitchTriggered(node_name) => {
                self.outbox
                    .push_back(UiEvent::AutoSwitch(node_name.clone()));
                self.outbox.push_back(UiEvent::TrayNotify {
                    title: tr("Автопереключение", &[]),
                    message: node_name,
                });
            }
            // Events without a Python bridge connection are dropped.
            _ => {}
        }
    }

    fn on_nodes_changed(&mut self) {
        {
            let controller = self.controller.lock();
            self.nodes_cached = controller.state.nodes.clone();
            self.manual_groups_cached = controller.state.manual_groups.clone();
            self.node_model.set_runtime_support(true);
            let nodes = self.sorted_nodes(self.filtered_nodes(&controller.state.nodes));
            let selected = controller.state.selected_node_id.clone();
            self.node_model.set_nodes(nodes, selected.as_deref());
        }
        self.changes.node_filters = true;
        self.changes.onboarding = true;
    }

    /// Python `_filtered_nodes`.
    fn filtered_nodes(&self, nodes: &[Node]) -> Vec<Node> {
        let group = &self.filter_group;
        let text = self.filter_text.trim().to_lowercase();
        if group.is_empty() && text.is_empty() {
            return nodes.to_vec();
        }
        nodes
            .iter()
            .filter(|node| {
                if !group.is_empty() && node.group_or_default() != group.as_str() {
                    return false;
                }
                if !text.is_empty()
                    && !node.name.to_lowercase().contains(&text)
                    && !node.server.to_lowercase().contains(&text)
                    && !node.group.to_lowercase().contains(&text)
                    && !lumen_core::node_transport::node_transport(node)
                        .to_lowercase()
                        .contains(&text)
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Python `_sorted_nodes`.
    fn sorted_nodes(&self, mut items: Vec<Node>) -> Vec<Node> {
        match self.sort_key.as_str() {
            "name" => items.sort_by(|a, b| {
                let key = |node: &Node| {
                    if node.name.is_empty() {
                        node.server.to_lowercase()
                    } else {
                        node.name.to_lowercase()
                    }
                };
                key(a).cmp(&key(b))
            }),
            "group" => items.sort_by(|a, b| a.group.to_lowercase().cmp(&b.group.to_lowercase())),
            "scheme" => items.sort_by(|a, b| a.scheme.to_lowercase().cmp(&b.scheme.to_lowercase())),
            "transport" => items.sort_by(|a, b| {
                lumen_core::node_transport::node_transport(a)
                    .to_lowercase()
                    .cmp(&lumen_core::node_transport::node_transport(b).to_lowercase())
            }),
            "ping" => items.sort_by(|a, b| {
                let key = |node: &Node| {
                    (
                        node.ping_ms.is_none(),
                        node.ping_ms.unwrap_or(1_000_000_000_000),
                    )
                };
                key(a).cmp(&key(b))
            }),
            "speed" => {
                // Higher speed first under ascending, unknown speeds last.
                items.sort_by(|a, b| {
                    let key = |node: &Node| {
                        (node.speed_mbps.is_none(), -(node.speed_mbps.unwrap_or(0.0)))
                    };
                    {
                        let a = key(a);
                        let b = key(b);
                        a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
                    }
                });
            }
            "last" => items.sort_by(|a, b| {
                let key = |node: &Node| {
                    (
                        node.last_used_at.is_none(),
                        node.last_used_at.clone().unwrap_or_default(),
                    )
                };
                key(a).cmp(&key(b))
            }),
            "address" => items.sort_by(|a, b| {
                (a.server.to_lowercase(), a.port).cmp(&(b.server.to_lowercase(), b.port))
            }),
            "status" => {
                // Reachable first, then untested, then failed.
                items.sort_by(|a, b| {
                    let key = |node: &Node| match node.is_alive {
                        Some(true) => 0u8,
                        Some(false) => 2u8,
                        None => 1u8,
                    };
                    key(a).cmp(&key(b))
                });
            }
            // "manual"
            _ => items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)),
        }
        if !self.sort_asc {
            items.reverse();
        }
        items
    }

    fn on_selection_changed_inner(&mut self, node: Option<Node>) {
        let prev_id = self.selected_id.clone();
        let new_id = node
            .as_ref()
            .map(|node| node.id.clone())
            .unwrap_or_default();
        let raw_name = node
            .as_ref()
            .map(|node| {
                if node.name.is_empty() {
                    node.server.clone()
                } else {
                    node.name.clone()
                }
            })
            .unwrap_or_default();
        let country = node
            .as_ref()
            .map(|node| {
                let code = node.country_code.to_uppercase();
                if code.is_empty() {
                    detect_country(&node.name, &node.server)
                } else {
                    code
                }
            })
            .unwrap_or_default();
        let display_name = server_display_name_without_country_prefix(&raw_name, &country);
        let selection_changed = !new_id.is_empty() && new_id != prev_id;
        // Python showed the in-app toast while a manual selection was in
        // progress so a click got immediate feedback.
        if selection_changed && self.manual_selection_in_progress {
            self.notify(
                "info",
                &format!("{}: {}", tr("Сервер изменён", &[]), display_name),
            );
        }
        if self.connected && !prev_id.is_empty() && selection_changed {
            self.outbox.push_back(UiEvent::TrayNotify {
                title: tr("Сервер изменён", &[]),
                message: raw_name,
            });
        }
        self.selected_id = new_id;
        self.selected_name = display_name;
        self.selected_flag = get_flag_emoji(&country);
        self.selected_flag_source = flag_svg_data_uri(&country);
        self.selected_flag_country = country;
        self.selected_latency = node
            .as_ref()
            .and_then(|node| node.ping_ms)
            .map(|ping| ping)
            .unwrap_or(-1);
        self.selected_protocol = node
            .as_ref()
            .map(lumen_core::node_transport::node_protocol_label)
            .unwrap_or_default();
        self.selected_source = {
            let controller = self.controller.lock();
            node_source_caption(&controller.state.subscriptions, node.as_ref())
        };
        self.node_model
            .set_selected(if self.selected_id.is_empty() {
                None
            } else {
                Some(&self.selected_id)
            });
        self.changes.selection = true;
    }

    fn on_connection_changed_inner(&mut self, connected: bool) {
        if connected == self.connected {
            return;
        }
        self.connected = connected;
        if !self.busy {
            self.transition_target_connected = connected;
        }
        if !connected {
            self.metrics = MetricsSnapshot {
                traffic_reason: "not connected".into(),
                health_status: "UNKNOWN".into(),
                health_reason: "not connected".into(),
                ..MetricsSnapshot::default()
            };
            self.session_upload_total = 0;
            self.session_download_total = 0;
            self.session_raw_upload_last = None;
            self.session_raw_download_last = None;
            self.process_model.set_stats(&[]);
            self.changes.metrics = true;
        }
        self.changes.connected = true;
    }

    /// Python `_transition_message_connecting`.
    fn transition_message_connecting(&self, message: &str) -> bool {
        let msg = message;
        if msg.contains("Отключ") || msg.contains("Disconnect") {
            return false;
        }
        if msg.contains("Подключ")
            || msg.contains("Запуск")
            || msg.contains("Переподключ")
            || msg.contains("Переключ")
        {
            return true;
        }
        if msg.contains("Connect")
            || msg.contains("Starting")
            || msg.contains("Reconnect")
            || msg.contains("Switch")
        {
            return true;
        }
        let desired = self
            .controller
            .try_lock()
            .map(|controller| controller.runtime.desired_connected)
            .unwrap_or(!self.connected);
        desired || self.connected
    }

    fn on_transition(&mut self, busy: bool) {
        // A starting transition usually flips the connection state; that
        // guess stands in while the controller is held.
        let desired = self
            .controller
            .try_lock()
            .map(|controller| controller.runtime.desired_connected)
            .unwrap_or(!self.connected);
        let target = desired || self.connected;
        let connecting = if busy {
            self.transition_message_connecting(&self.runtime_message)
        } else {
            self.connected
        };
        let changed = (busy != self.busy)
            || (target != self.transition_target_connected)
            || (connecting != self.transition_connecting);
        if !changed {
            return;
        }
        self.busy = busy;
        self.transition_target_connected = if busy { target } else { self.connected };
        self.transition_connecting = if busy { connecting } else { self.connected };
        self.changes.transition_direction = true;
        self.changes.transition_busy = true;
    }

    /// Python `_on_auto_switch` (the tray notification rides the outbox).
    fn on_status_message(&mut self, level: String, message: String) {
        let localized = self.localize(&message);
        if level == "error" || level == "warning" {
            if message.contains("Системный прокси изменён вне Lumen") {
                self.notify_action(
                    &level,
                    &localized,
                    "reset-system-proxy",
                    &tr("Сбросить", &[]),
                );
                return;
            }
            let entry = lumen_core::log_utils::parse_log_line(&localized, None);
            if entry.message != entry.details {
                let summary = self.localize(&entry.message);
                let details = self.localize(&entry.details);
                let hint = {
                    let head = details.split_once(". ").map(|(head, _)| head);
                    format!("{}. ", head.unwrap_or(&details))
                };
                if !entry.action_id.is_empty() {
                    self.notify_action(
                        &level,
                        &format!("{summary}\n{hint}"),
                        &entry.action_id,
                        &tr(&entry.action_label, &[]),
                    );
                    return;
                }
                self.notify(&level, &format!("{summary}\n{hint}"));
                return;
            }
            if !entry.action_id.is_empty() {
                let hint = entry
                    .details
                    .split_once(". ")
                    .map(|(head, _)| head)
                    .unwrap_or(&entry.details);
                self.notify_action(
                    &level,
                    &format!("{}\n{}.", entry.message, hint),
                    &entry.action_id,
                    &tr(&entry.action_label, &[]),
                );
                return;
            }
        }
        self.notify(&level, &localized);
    }

    /// Python `_on_settings_changed` (+ `_apply_node_model` side effects).
    fn on_settings_changed_inner(&mut self, settings: AppSettings) {
        let new_language = settings.language.clone();
        let language_changed = new_language != self.language;
        self.apply_settings_snapshot(&settings);
        if language_changed {
            self.changes.language = true;
            // Routing preset labels are translated in the bridge; refresh them
            // on a language change.
            self.changes.routing = true;
        }
        self.changes.settings = true;
        self.changes.onboarding = true;
        self.changes.node_sort = true;
        self.changes.node_filter = true;
        self.changes.node_table_layout = true;
        self.changes.selected_subscription = true;
    }

    /// The field-copying part of Python `_on_settings_changed`.
    fn apply_settings_snapshot(&mut self, settings: &AppSettings) {
        self.settings_revision = self.settings_revision.wrapping_add(1);
        self.tun_mode = settings.tun_mode;
        self.proxy_enabled = settings.enable_system_proxy;
        self.discord_proxy = settings.discord_proxy_enabled;
        self.filter_group = settings.node_filter_group.trim().to_string();
        self.sort_key = if settings.node_sort_key.is_empty() {
            "manual".to_string()
        } else {
            settings.node_sort_key.clone()
        };
        self.sort_asc = settings.node_sort_ascending;
        self.theme = settings.theme.clone();
        let new_language = settings.language.clone();
        self.language = new_language;
        set_language(&self.language);
        self.accent = if settings.accent_color.is_empty() {
            "#0078D4".to_string()
        } else {
            settings.accent_color.clone()
        };
        self.settings_cached = settings.clone();
        self.dashboard_nodes.set_sort_key(&self.sort_key);
        self.node_model.set_runtime_support(true);
        self.node_model
            .set_core_preference(&normalize_core_preference(&json!(settings.core_preference)));
    }

    fn on_ping(&mut self, node_id: &str, ping_ms: Option<i64>) {
        self.node_model.update_ping(node_id, ping_ms);
        if node_id == self.selected_id {
            self.selected_latency = ping_ms.unwrap_or(-1);
            self.changes.selection = true;
        }
    }

    /// Python `_on_bulk_task_progress`.
    fn on_bulk_task_progress(&mut self, task: &str, current: usize, total: usize, done: bool) {
        let was_running = !self.running_tests.is_empty();
        if task == "ping" || task == "speed" {
            if done {
                self.running_tests.remove(task);
            } else {
                self.running_tests.insert(task.to_string());
            }
        }
        if was_running != !self.running_tests.is_empty() {
            self.changes.tests_running = true;
        }
        if task == "ping" && done {
            self.node_model.clear_pinging();
        }
        self.outbox.push_back(UiEvent::BulkTaskProgress {
            task: task.to_string(),
            current,
            total,
            completed: done,
        });
    }

    /// Python `_on_live_metrics` (the controller forwards the worker sample).
    fn on_live_metrics(&mut self, sample: &lumen_app::live_metrics_worker::LiveMetricsSample) {
        self.metrics.down_bps = sample.down_bps;
        self.metrics.up_bps = sample.up_bps;
        if let Some(upload_total) = sample.upload_total {
            let raw_upload = upload_total.max(0) as i64;
            let delta = match self.session_raw_upload_last {
                None => raw_upload,
                Some(last) => {
                    if raw_upload >= last {
                        raw_upload - last
                    } else {
                        raw_upload
                    }
                }
            };
            self.session_upload_total += delta;
            self.session_raw_upload_last = Some(raw_upload);
        }
        if let Some(download_total) = sample.download_total {
            let raw_download = download_total.max(0) as i64;
            let delta = match self.session_raw_download_last {
                None => raw_download,
                Some(last) => {
                    if raw_download >= last {
                        raw_download - last
                    } else {
                        raw_download
                    }
                }
            };
            self.session_download_total += delta;
            self.session_raw_download_last = Some(raw_download);
        }
        self.metrics.latency_ms = sample.health.latency_ms.unwrap_or(-1);
        self.metrics.traffic_available = sample.traffic_available;
        self.metrics.traffic_reason = if sample.traffic_reason.is_empty() {
            "unavailable".to_string()
        } else {
            sample.traffic_reason.clone()
        };
        self.metrics.health_status = if sample.health.status.is_empty() {
            "UNKNOWN".to_string()
        } else {
            sample.health.status.clone()
        };
        self.metrics.health_reason = if sample.health.reason.is_empty() {
            "not sampled".to_string()
        } else {
            sample.health.reason.clone()
        };
        self.changes.metrics = true;
        if let Some(stats) = sample.process_stats.as_ref() {
            let rows: Vec<Value> = stats
                .iter()
                .map(|snapshot| {
                    json!({
                        "name": snapshot.exe,
                        "down": snapshot.down_speed,
                        "up": snapshot.up_speed,
                        "download": snapshot.download,
                        "upload": snapshot.upload,
                        "pid": 0,
                        "proxy_bytes": snapshot.proxy_bytes,
                        "direct_bytes": snapshot.direct_bytes,
                        "unknown_bytes": snapshot.unknown_bytes,
                        "connections": snapshot.connections,
                        "total_connections": snapshot.total_connections,
                        "top_host": snapshot.top_host,
                        "route": snapshot.route,
                    })
                })
                .collect();
            self.process_model.set_stats(&rows);
        }
    }

    fn on_live_metrics_reset(&mut self) {
        self.process_model.set_stats(&[]);
        self.changes.metrics = true;
    }

    /// Python `_on_admin_relaunch`.
    fn on_admin_relaunch(&mut self) {
        if is_process_elevated() {
            return;
        }
        self.outbox.push_back(UiEvent::AdminRelaunchRequested);
    }

    pub fn settings_changed_token(&self) -> u64 {
        self.settings_revision
    }

    pub fn save_state(&self) {
        self.controller.lock().save();
    }

    fn drive_power(
        &mut self,
        action: impl FnOnce(
            &mut lumen_platform::power_reconnect::WindowsPowerEventFilter<'_, AppController>,
        ),
    ) {
        use lumen_platform::power_reconnect::WindowsPowerEventFilter;
        let mut controller = self.controller.lock();
        let mut filter = WindowsPowerEventFilter::with_state(
            &mut *controller,
            std::mem::take(&mut self.power_state),
        );
        action(&mut filter);
        self.power_state = filter.into_state();
    }

    pub fn drive_power_message(&mut self, message: u32, wparam: usize) {
        self.drive_power(|filter| {
            filter.handle_message(message, wparam);
        });
    }

    pub fn tick_power_filter(&mut self) {
        if self.power_state.timer_is_active() && self.controller_free() {
            self.drive_power(|filter| {
                filter.fire_timer_if_due();
            });
        }
    }

    pub fn close_power_filter(&mut self) {
        self.drive_power(|filter| filter.close());
    }

    /// Python `_on_xray_update_result`.
    fn xray_update_state_payload(
        &self,
        result: &lumen_engines::xray::core_updater::XrayCoreUpdateResult,
    ) -> Value {
        use lumen_engines::xray::core_updater::XrayCoreUpdateStatus;
        let phase = match result.status {
            XrayCoreUpdateStatus::UpToDate => "uptodate",
            XrayCoreUpdateStatus::Available => "available",
            XrayCoreUpdateStatus::Updated => "updated",
            XrayCoreUpdateStatus::Error => "error",
        };
        let current_version = result.current_version.clone();
        let latest_version = result.latest_version.clone();
        let mut version = String::new();
        if phase == "updated" {
            version = latest_version.clone();
            if let Some(actual) = lumen_engines::xray::manager::get_xray_version(
                &self.controller.lock().state.settings.xray_path,
            ) {
                if !actual.is_empty() {
                    version = actual;
                }
            }
        }
        json!({
            "phase": phase,
            "version": version,
            "currentVersion": current_version,
            "latestVersion": latest_version,
            "message": self.localize(&result.message),
            "percent": if phase == "updated" { 100 } else { 0 },
        })
    }

    /// Python `_on_resource_update_result`.
    fn on_resource_update_result(
        &mut self,
        result: lumen_engines::core_resource_updater::ResourceUpdateResult,
    ) {
        if result.kind == "geodata" && !self.pending_regional_preset.is_empty() {
            self.pending_regional_preset = String::new();
            self.changes.regional_preset_busy = true;
            self.changes.settings = true;
            self.changes.routing = true;
        }
        use lumen_engines::core_resource_updater::ResourceUpdateStatus;
        let phase = match result.status {
            ResourceUpdateStatus::UpToDate => "uptodate",
            ResourceUpdateStatus::Available => "available",
            ResourceUpdateStatus::Updated => "updated",
            ResourceUpdateStatus::Error => "error",
        };
        let current_version = result.current_version.clone();
        let latest_version = result.latest_version.clone();
        let displayed_version = if phase == "updated" {
            latest_version.clone()
        } else {
            current_version.clone()
        };
        self.outbox.push_back(UiEvent::ResourceUpdateState(json!({
            "kind": result.kind,
            "phase": phase,
            "message": self.localize(&result.message),
            "version": displayed_version,
            "currentVersion": current_version,
            "latestVersion": latest_version,
            "percent": if phase == "updated" { 100 } else { 0 },
        })));
    }

    // ── QML-invokable commands ─────────────────────────────────────────────

    /// Python `toggleConnection`.
    pub fn toggle_connection(&mut self) {
        if self.controller.is_locked() {
            // A transition is running: queue the toggle on the controller
            // thread instead of waiting for the cores to finish starting.
            self.submit(Box::new(|controller: &mut AppController| controller.toggle_connection()));
            return;
        }
        let tun_mode = self.controller.lock().state.settings.tun_mode;
        if !self.connected && tun_mode && !is_process_elevated() {
            self.notify_action(
                "warning",
                &tr("VPN (TUN) требует права администратора", &[]),
                "restart-admin",
                &tr("Перезапустить от администратора", &[]),
            );
            return;
        }
        let target = self.controller.lock().runtime.desired_connected;
        self.controller.lock().toggle_connection();
        if target != self.transition_target_connected {
            self.transition_target_connected = target;
            self.changes.transition_busy = true;
        }
    }

    /// Python `selectNode`.
    pub fn select_node(&mut self, node_id: &str) {
        if node_id.is_empty() {
            return;
        }
        self.manual_selection_in_progress = true;
        self.controller.lock().set_selected_node(node_id);
        self.manual_selection_in_progress = false;
    }

    /// Python `switchNext`.
    pub fn switch_next(&mut self) {
        self.manual_selection_in_progress = true;
        self.controller.lock().switch_next_node();
        self.manual_selection_in_progress = false;
    }

    /// Python `switchPrev`.
    pub fn switch_prev(&mut self) {
        self.manual_selection_in_progress = true;
        self.controller.lock().switch_prev_node();
        self.manual_selection_in_progress = false;
    }

    /// Python `setRoutingMode`.
    pub fn set_routing_mode(&mut self, mode: &str) {
        let mut controller = self.controller.lock();
        let mut routing = controller.state.routing.clone();
        routing.mode = mode.to_string();
        routing.preset_id = "custom".into();
        controller.update_routing(routing, true);
    }

    /// Python `applyRoutingPreset`.
    pub fn apply_routing_preset(&mut self, preset_id: &str) {
        self.controller.lock().apply_routing_preset(preset_id, true);
    }

    /// Python `applyRoutingPresetOption`.
    pub fn apply_routing_preset_option(&mut self, preset_id: &str) {
        let preset_id = preset_id.trim();
        if BUILT_IN_ROUTING_PRESET_IDS.contains(&preset_id) {
            self.apply_routing_preset(preset_id);
        } else {
            self.apply_custom_routing_preset(preset_id);
        }
    }

    /// Python `saveRoutingPreset`.
    pub fn save_routing_preset(&mut self, name: &str) {
        let title = {
            let text = name.trim();
            if text.is_empty() {
                tr("Пресет", &[])
            } else {
                text.to_string()
            }
        };
        let preset = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "name": title,
            "routing": self.controller.lock().state.routing.to_dict(),
        });
        {
            let mut controller = self.controller.lock();
            controller.state.routing_presets.push(preset);
            controller.save();
        }
        self.changes.routing = true;
        self.notify(
            "success",
            &tr("Пресет сохранён: {name}", &[("name", &title)]),
        );
    }

    /// Python `applyCustomRoutingPreset`.
    pub fn apply_custom_routing_preset(&mut self, preset_id: &str) {
        let preset = {
            let controller = self.controller.lock();
            controller
                .state
                .routing_presets
                .iter()
                .find(|preset| preset.get("id").and_then(Value::as_str) == Some(preset_id))
                .cloned()
        };
        let Some(preset) = preset else {
            self.notify("warning", &tr("Пресет не найден", &[]));
            return;
        };
        let mut routing = RoutingSettings::from_dict(preset.get("routing").unwrap_or(&json!({})));
        routing.preset_id = preset_id.to_string();
        self.controller.lock().update_routing(routing, true);
        let name = preset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.notify(
            "success",
            &tr("Применён пресет: {name}", &[("name", &name)]),
        );
    }

    /// Python `deleteRoutingPreset`.
    pub fn delete_routing_preset(&mut self, preset_id: &str) {
        let mut controller = self.controller.lock();
        let before = controller.state.routing_presets.len();
        controller
            .state
            .routing_presets
            .retain(|preset| preset.get("id").and_then(Value::as_str) != Some(preset_id));
        if controller.state.routing_presets.len() != before {
            controller.save();
            drop(controller);
            self.changes.routing = true;
        }
    }

    /// Python `setTun`.
    pub fn set_tun(&mut self, enabled: bool) -> bool {
        if enabled && !is_process_elevated() {
            self.notify_action(
                "warning",
                &tr("VPN (TUN) требует права администратора", &[]),
                "restart-admin",
                &tr("Перезапустить от администратора", &[]),
            );
            self.changes.settings = true;
            return false;
        }
        if enabled && !self.request_network_mode("tun") {
            self.changes.settings = true;
            return false;
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.tun_mode = enabled;
        if enabled {
            settings.enable_system_proxy = false;
        }
        self.controller.lock().update_settings(settings);
        true
    }

    /// Python `setProxy`.
    pub fn set_proxy(&mut self, enabled: bool) -> bool {
        if enabled && !self.request_network_mode("proxy") {
            self.changes.settings = true;
            return false;
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.enable_system_proxy = enabled;
        if enabled && settings.tun_mode {
            settings.tun_mode = false;
        }
        self.controller.lock().update_settings(settings);
        true
    }

    /// Python `setDiscordProxy` (async on the controller).
    pub fn set_discord_proxy(&self, enabled: bool) {
        self.controller
            .lock()
            .request_discord_proxy_enabled(enabled);
    }

    /// Python `_network_conflicts`.
    fn network_conflicts(&self) -> Value {
        let controller = self.controller.lock();
        let settings = &controller.state.settings;
        let ports: HashSet<u16> = HashSet::from([
            if settings.local_socks_port == 0 {
                DEFAULT_SOCKS_PORT
            } else {
                settings.local_socks_port as u16
            },
            if settings.local_http_port == 0 {
                DEFAULT_HTTP_PORT
            } else {
                settings.local_http_port as u16
            },
            DEFAULT_DISCORD_SOCKS_PORT,
        ]);
        scan_network_conflicts(
            &ports,
            Some(&controller.owned_core_process_pids()),
            Some(&controller.owned_core_executable_paths()),
        )
    }

    /// Python `_request_network_mode`.
    fn request_network_mode(&mut self, mode: &str) -> bool {
        // The guard is opt-out.  A missing field in an old state file keeps
        // the historic safe behavior until the user explicitly disables it.
        if !self.controller.lock().state.settings.block_vpn_conflicts {
            return true;
        }
        let snapshot = self.network_conflicts();
        let apps: Vec<String> = snapshot
            .get("apps")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let unknown = snapshot
            .get("unknown_client")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if apps.is_empty() && !unknown {
            return true;
        }
        let names = if apps.is_empty() {
            tr("другой VPN/прокси-клиент", &[])
        } else {
            apps.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
        };
        self.pending_network_mode = mode.to_string();
        self.pending_conflict_processes = snapshot
            .get("processes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let message = tr(
            "Работе Lumen могут помешать запущенные процессы: {names}. Закройте окно или завершите конфликтующие процессы и продолжите.",
            &[("names", &names)],
        );
        let can_terminate = !self.pending_conflict_processes.is_empty();
        self.outbox.push_back(UiEvent::NetworkConflictRequested {
            mode: mode.to_string(),
            message,
            can_terminate,
        });
        false
    }

    /// Python `_apply_network_mode`.
    fn apply_network_mode(&mut self, mode: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        match mode {
            "tun" => {
                settings.tun_mode = true;
                settings.enable_system_proxy = false;
            }
            "proxy" => {
                settings.enable_system_proxy = true;
                settings.tun_mode = false;
            }
            _ => return,
        }
        self.controller.lock().update_settings(settings);
    }

    /// Python `cancelNetworkConflict`.
    pub fn cancel_network_conflict(&mut self) {
        self.pending_network_mode = String::new();
        self.pending_conflict_processes = Vec::new();
        self.changes.settings = true;
    }

    /// Python `closeNetworkConflictsAndContinue`.
    pub fn close_network_conflicts_and_continue(&mut self) -> bool {
        let mode = self.pending_network_mode.clone();
        let scanned = self.pending_conflict_processes.clone();
        if mode.is_empty() {
            self.cancel_network_conflict();
            return false;
        }
        if !self.controller.lock().state.settings.block_vpn_conflicts {
            self.pending_network_mode = String::new();
            self.pending_conflict_processes = Vec::new();
            self.apply_network_mode(&mode);
            return true;
        }
        if scanned.is_empty() {
            self.cancel_network_conflict();
            return false;
        }
        let (_closed, failed) = terminate_scanned_conflicts(&scanned);
        let snapshot = self.network_conflicts();
        let remaining: Vec<String> = snapshot
            .get("apps")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let unknown = snapshot
            .get("unknown_client")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !failed.is_empty() || !remaining.is_empty() || unknown {
            let mut names: Vec<String> = Vec::new();
            for item in failed.iter().chain(remaining.iter()) {
                if !names.contains(item) {
                    names.push(item.clone());
                }
            }
            let names = if names.is_empty() {
                tr("другой VPN/прокси-клиент", &[])
            } else {
                names.join(", ")
            };
            self.pending_conflict_processes = snapshot
                .get("processes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            self.notify(
                "error",
                &tr(
                    "Не удалось закрыть конфликтующие процессы: {names}",
                    &[("names", &names)],
                ),
            );
            return false;
        }
        self.pending_network_mode = String::new();
        self.pending_conflict_processes = Vec::new();
        self.apply_network_mode(&mode);
        true
    }

    /// Python `setCorePreference`.
    pub fn set_core_preference(&mut self, value: &str) {
        let preference = normalize_core_preference(&json!(value));
        if preference == self.core_preference() {
            return;
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.core_preference = preference;
        self.controller.lock().update_settings(settings);
    }

    /// Python `corePreference`.
    pub fn core_preference(&self) -> String {
        normalize_core_preference(&json!(self.settings_cached.core_preference))
    }

    /// Python `setTheme`.
    pub fn set_theme(&mut self, name: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.theme = name.to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `setLanguage`.
    pub fn set_language(&mut self, language: &str) {
        let mut value = language.trim().to_lowercase();
        if !available_languages().contains(&value) {
            value = "en".to_string();
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.language = value;
        self.controller.lock().update_settings(settings);
    }

    /// Python `setAccent`.
    pub fn set_accent(&mut self, hex_color: &str) {
        if hex_color.is_empty() {
            return;
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.accent_color = hex_color.to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `systemAccent` — Windows DWM accent as '#RRGGBB' ('' when
    /// unavailable).
    pub fn system_accent(&self) -> String {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::ERROR_SUCCESS;
            use windows::Win32::System::Registry::{
                HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
            };
            unsafe {
                let dwm: windows::core::PCWSTR =
                    windows::core::w!("Software\\Microsoft\\Windows\\DWM");
                let mut key = Default::default();
                if RegOpenKeyExW(HKEY_CURRENT_USER, dwm, None, KEY_READ, &mut key) == ERROR_SUCCESS
                {
                    let accent: windows::core::PCWSTR = windows::core::w!("AccentColor");
                    let mut value: u32 = 0;
                    let mut size = std::mem::size_of::<u32>() as u32;
                    let status = RegQueryValueExW(
                        key,
                        accent,
                        None,
                        None,
                        Some(&mut value as *mut u32 as *mut _),
                        Some(&mut size),
                    );
                    let _ = RegCloseKey(key);
                    if status == ERROR_SUCCESS {
                        let r = value & 0xFF;
                        let g = (value >> 8) & 0xFF;
                        let b = (value >> 16) & 0xFF;
                        return format!("#{r:02X}{g:02X}{b:02X}");
                    }
                }
            }
        }
        String::new()
    }

    /// Python `useSystemAccent`.
    pub fn use_system_accent(&mut self) {
        let hex_color = self.system_accent();
        if hex_color.is_empty() {
            self.notify("warning", &tr("Не удалось получить системный акцент", &[]));
            return;
        }
        self.set_accent(&hex_color);
    }

    /// Python `applySystemAccentQuietly`.
    pub fn apply_system_accent_quietly(&mut self) -> bool {
        let hex_color = self.system_accent();
        if hex_color.is_empty() {
            return false;
        }
        self.set_accent(&hex_color);
        true
    }

    // ── Onboarding (welcome flow) ──────────────────────────────────────────

    /// Python `completeOnboarding`.
    pub fn complete_onboarding(&mut self) {
        let mut settings = self.controller.lock().state.settings.clone();
        let mut changed = false;
        if settings.onboarding_revision < ONBOARDING_REVISION {
            settings.onboarding_revision = ONBOARDING_REVISION;
            changed = true;
        }
        let seen = settings.onboarding_version.clone();
        // Never move the marker backwards after a downgrade.
        if version_key(&seen) < version_key(APP_VERSION) || is_upgrade(&seen, APP_VERSION) {
            settings.onboarding_version = APP_VERSION.to_string();
            changed = true;
        }
        if changed {
            self.controller.lock().update_settings(settings);
        }
    }

    /// Python `showOnboardingAgain`.
    pub fn show_onboarding_again(&mut self) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.onboarding_revision = 0;
        self.controller.lock().update_settings(settings);
    }

    // ── WARP profiles in one click ─────────────────────────────────────────

    /// Python `generateWarpConfigs` — register free Cloudflare WARP profiles
    /// (AmneziaWG + MASQUE) in the background.
    pub fn generate_warp_configs(&mut self) {
        if self.quitting
            || self
                .warp_generating
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        if !self.controller.lock().profile_loaded() {
            self.notify(
                "warning",
                &tr("Сначала откройте зашифрованный профиль", &[]),
            );
            return;
        }
        let proxy_url = self.effective_http_proxy_url_for_background();
        self.warp_generating
            .store(true, std::sync::atomic::Ordering::Release);
        self.changes.warp_generation = true;
        let controller = self.controller.clone();
        let handle = self.runtime_handle();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        self.warp_cancel = Some(cancel);
        self.runtime.spawn_blocking(move || {
            let result = handle.block_on(async {
                let mut builder = reqwest::Client::builder();
                if let Some(proxy_url) = proxy_url.as_deref()
                    && let Ok(proxy) = reqwest::Proxy::all(proxy_url)
                {
                    builder = builder.proxy(proxy);
                }
                let client = builder.build().unwrap_or_default();
                tokio::select! {
                    _ = cancelled => None,
                    result = lumen_core::warp_generator::generate_warp_nodes(&client, None, None) => Some(result),
                }
            });
            if let Some(result) = result {
                controller.lock().report_warp_generation(result);
            }
        });
    }

    fn effective_http_proxy_url_for_background(&self) -> Option<String> {
        let mut controller = self.controller.lock();
        if !controller.connected {
            return None;
        }
        controller.get_effective_http_proxy_url()
    }

    /// Python `_on_warp_generated` — runs on the controller owner via
    /// [`Self::emit_forward_warp_result`].
    fn on_warp_generated(&mut self, result: Option<lumen_core::warp_generator::WarpResult>) {
        self.warp_cancel.take();
        self.warp_generating
            .store(false, std::sync::atomic::Ordering::Release);
        if self.quitting {
            return;
        }
        let Some(result) = result else {
            self.notify(
                "error",
                &tr(
                    "Не удалось получить профили WARP. Проверьте интернет и попробуйте ещё раз.",
                    &[],
                ),
            );
            self.changes.warp_generation = true;
            return;
        };
        let existing_ids: HashSet<String> = self
            .controller
            .lock()
            .state
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect();
        let (added, _errors) = {
            let mut controller = self.controller.lock();
            let select_imported = controller.state.selected_node_id.is_none();
            controller.import_parsed_nodes(
                result.nodes,
                Some(WARP_GROUP),
                Some(false),
                select_imported,
            )
        };
        if added > 0 {
            self.warp_added += added as i64;
            let imported_id = {
                let controller = self.controller.lock();
                controller
                    .state
                    .nodes
                    .iter()
                    .find(|node| !existing_ids.contains(&node.id))
                    .map(|node| node.id.clone())
                    .unwrap_or_default()
            };
            if !imported_id.is_empty() {
                self.outbox.push_back(UiEvent::NodeImported(imported_id));
            }
            self.notify(
                "success",
                &tr(
                    "Добавлено профилей WARP: {count}",
                    &[("count", &added.to_string())],
                ),
            );
        } else {
            self.notify("warning", &tr("Не удалось добавить профили WARP", &[]));
        }
        self.changes.warp_generation = true;
    }

    pub fn warp_generating(&self) -> bool {
        self.warp_generating
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn warp_generated_count(&self) -> i64 {
        self.warp_added
    }

    // ── Appearance Studio (settings mirrors) ───────────────────────────────

    /// Python `setInterfaceMode`.
    pub fn set_interface_mode(&mut self, mode: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.interface_mode = mode.to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiDensity`.
    pub fn set_ui_density(&mut self, value: &str) {
        let v = match value.trim().to_lowercase().as_str() {
            "compact" | "spacious" => value.trim().to_lowercase(),
            _ => "comfortable".to_string(),
        };
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_density = v;
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiCornerRadius`.
    pub fn set_ui_corner_radius(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_corner_radius = value.clamp(0, 20);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiFontFamily`.
    pub fn set_ui_font_family(&mut self, value: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_font_family = value.trim().to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiFontScale`.
    pub fn set_ui_font_scale(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_font_scale = value.clamp(80, 140);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiBackdrop`.
    pub fn set_ui_backdrop(&mut self, value: &str) {
        let mut v = value.trim().to_lowercase();
        if v == "acrylic" {
            v = "mica".into();
        }
        if v != "mica" && v != "solid" {
            v = "mica".into();
        }
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_backdrop = v;
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiTransparencyStrength`.
    pub fn set_ui_transparency_strength(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_transparency_strength = value.clamp(0, 100);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiThemePreset`.
    pub fn set_ui_theme_preset(&mut self, value: &str) {
        let preset = value.trim().to_lowercase();
        let accent = accent_for_theme_preset(&preset);
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_theme_preset = preset;
        settings.accent_color = accent.to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiBaseTint`.
    pub fn set_ui_base_tint(&mut self, value: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_base_tint = value.trim().to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiBaseTintSrc`.
    pub fn set_ui_base_tint_src(&mut self, value: &str) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_base_tint_src = value.trim().to_string();
        self.controller.lock().update_settings(settings);
    }

    /// Python `uiWallpaper` (missing files clear themselves).
    pub fn ui_wallpaper(&self) -> String {
        let value = self.settings_cached.ui_wallpaper.trim().to_string();
        if !value.is_empty() && !Path::new(&value).is_file() {
            return String::new();
        }
        value
    }

    /// Python `setUiWallpaper`.
    pub fn set_ui_wallpaper(&mut self, value: &str) {
        let v = value.trim().to_string();
        let new_image = !v.is_empty();
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_wallpaper = v;
        if new_image {
            settings.ui_wallpaper_opacity = 50;
            settings.ui_wallpaper_blur = 10;
            settings.ui_wallpaper_brightness = 50;
        }
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiWallpaperOpacity`.
    pub fn set_ui_wallpaper_opacity(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_wallpaper_opacity = value.clamp(0, 100);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiWallpaperBlur`.
    pub fn set_ui_wallpaper_blur(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_wallpaper_blur = value.clamp(0, 100);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiWallpaperBrightness`.
    pub fn set_ui_wallpaper_brightness(&mut self, value: i64) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_wallpaper_brightness = value.clamp(0, 100);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setUiAnimations`.
    pub fn set_ui_animations(&mut self, enabled: bool) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.ui_animations = enabled;
        self.controller.lock().update_settings(settings);
    }

    // ── Updates settings persistence ───────────────────────────────────────

    /// Python `setAppAutoUpdate`.
    pub fn set_app_auto_update(&mut self, enabled: bool) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.app_auto_update = enabled;
        if settings.app_auto_update {
            settings.check_updates = true;
        }
        self.controller.lock().update_settings(settings);
    }

    /// Python `setCheckUpdates`.
    pub fn set_check_updates(&mut self, enabled: bool) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.check_updates = enabled;
        if !settings.check_updates {
            settings.app_auto_update = false;
        }
        self.controller.lock().update_settings(settings);
    }

    /// Python `setAllowUpdates` (a no-op in the Python bridge too).
    pub fn set_allow_updates(&mut self, _enabled: bool) {}

    /// Python `setAlwaysRunAsAdmin`.
    pub fn set_always_run_as_admin(&mut self, enabled: bool) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.always_run_as_admin = enabled;
        self.controller.lock().update_settings(settings);
    }

    /// Python `saveWindowGeometry`.
    pub fn save_window_geometry(&mut self, width: i64, height: i64, x: i64, y: i64) {
        self.submit(Box::new(move |controller| {
            let settings = &mut controller.state.settings;
            (
                settings.window_width,
                settings.window_height,
                settings.window_x,
                settings.window_y,
            ) = (width, height, x, y);
            controller.schedule_save();
        }));
    }

    /// Python `fitWindowGeometry`.
    #[allow(clippy::too_many_arguments)]
    pub fn fit_window_geometry_for(
        &self,
        width: i64,
        height: i64,
        x: i64,
        y: i64,
        minimum_width: i64,
        minimum_height: i64,
        available: (i64, i64, i64, i64),
    ) -> Value {
        let position_saved = x != -1 && y != -1;
        fit_window_geometry(
            width,
            height,
            x,
            y,
            minimum_width,
            minimum_height,
            available.0,
            available.1,
            available.2,
            available.3,
            position_saved,
        )
    }

    /// Python `setReleaseChannel`.
    pub fn set_release_channel(&mut self, channel: &str) {
        if self.app_update_in_progress() {
            return;
        }
        let normalized = match channel.trim().to_lowercase().as_str() {
            "prerelease" | "pre-release" | "pre" | "beta" | "nightly" => "prerelease",
            _ => "stable",
        };
        {
            let mut controller = self.controller.lock();
            if !controller.profile_loaded() || controller.state.settings.release_channel == normalized {
                return;
            }
            let mut settings = controller.state.settings.clone();
            settings.release_channel = normalized.to_string();
            controller.update_settings(settings);
        }
        self.check_app_update();
    }

    /// Python `setXrayAutoUpdate`.
    pub fn set_xray_auto_update(&mut self, enabled: bool) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.xray_auto_update = enabled;
        self.controller.lock().update_settings(settings);
    }

    /// Python `setXrayReleaseChannel`.
    pub fn set_xray_release_channel(&mut self, channel: &str) {
        let mut normalized = channel.trim().to_lowercase();
        if normalized != "stable" && normalized != "beta" {
            normalized = "beta".into();
        }
        let mut settings = self.controller.lock().state.settings.clone();
        if settings.xray_release_channel == normalized {
            return;
        }
        settings.xray_release_channel = normalized;
        self.controller.lock().update_settings(settings);
        self.check_xray_update();
    }

    // ── Network settings ───────────────────────────────────────────────────

    fn update_settings_field(&mut self, mutate: impl FnOnce(&mut AppSettings)) {
        let mut settings = self.controller.lock().state.settings.clone();
        mutate(&mut settings);
        self.controller.lock().update_settings(settings);
    }

    /// Python `setProxyBypassLan`.
    pub fn set_proxy_bypass_lan(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.system_proxy_bypass_lan = enabled);
    }

    /// Python `setFirefoxProxyIntegration`.
    pub fn set_firefox_proxy_integration(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.firefox_proxy_integration = enabled);
    }

    /// Python `setReconnectOnNetworkChange`.
    pub fn set_reconnect_on_network_change(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.reconnect_on_network_change = enabled);
    }

    /// Python `setReconnectAfterSleep`.
    pub fn set_reconnect_after_sleep(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.reconnect_after_sleep = enabled);
    }

    /// Python `setRegionalPreset`.
    pub fn set_regional_preset(&mut self, value: &str) {
        let region = normalize_regional_preset(value);
        let outcome = self
            .controller
            .lock()
            .request_regional_preset_change(&region);
        match outcome {
            "downloading" => {
                self.pending_regional_preset = region.to_string();
                self.changes.regional_preset_busy = true;
                self.outbox.push_back(UiEvent::ResourceUpdateState(json!({
                    "kind": "geodata", "phase": "updating", "percent": 0
                })));
                self.notify("info", &tr("Обновление geoip/geosite...", &[]));
            }
            "applied" => {
                self.notify("success", &tr("Применено", &[]));
            }
            "pending" | "busy" => {
                self.changes.settings = true;
                self.changes.regional_preset_busy = true;
                self.notify("info", &tr("Обновление уже выполняется", &[]));
            }
            _ => {}
        }
    }

    /// Python `setPreferIpv6`.
    pub fn set_prefer_ipv6(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.prefer_ipv6 = enabled);
    }

    /// Python `setKillSwitch`.
    pub fn set_kill_switch(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.kill_switch = enabled);
    }

    /// Python `setResourceUpdateCheck`.
    pub fn set_resource_update_check(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.resource_update_check = enabled);
    }

    /// Python `setDiagnosticsUpload`.
    pub fn set_diagnostics_upload(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.diagnostics_upload_enabled = enabled);
    }

    /// Python `setProxyAllowLan`.
    pub fn set_proxy_allow_lan(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.proxy_allow_lan = enabled);
    }

    /// Python `setTunStrictRoute`.
    pub fn set_tun_strict_route(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.tun_strict_route = enabled);
    }

    /// Python `setTunStack`.
    pub fn set_tun_stack(&mut self, stack: &str) {
        self.update_settings_field(|settings| settings.tun_stack = stack.trim().to_lowercase());
    }

    /// Python `setTunMtu`.
    pub fn set_tun_mtu(&mut self, mtu: i64) {
        self.update_settings_field(|settings| settings.tun_mtu = mtu);
    }

    /// Python `_normalize_local_port` (models.py): keep inside 1024..=65535.
    fn normalize_local_port(port: i64) -> i64 {
        port.clamp(1024, 65535)
    }

    /// Python `setLocalSocksPort` (+ `__post_init__` normalization).
    pub fn set_local_socks_port(&mut self, port: i64) {
        self.update_settings_field(|settings| {
            settings.local_socks_port = Self::normalize_local_port(port);
            if settings.local_http_port == settings.local_socks_port {
                settings.local_http_port =
                    Self::normalize_local_port(settings.local_socks_port + 1);
            }
        });
    }

    /// Python `setLocalHttpPort` (+ `__post_init__` normalization).
    pub fn set_local_http_port(&mut self, port: i64) {
        self.update_settings_field(|settings| {
            settings.local_http_port = Self::normalize_local_port(port);
            if settings.local_http_port == settings.local_socks_port {
                settings.local_http_port =
                    Self::normalize_local_port(settings.local_socks_port + 1);
            }
        });
    }

    /// Python `setProxyAuthCredentials`.
    pub fn set_proxy_auth_credentials(&mut self, username: &str, password: &str) {
        let mut warned = false;
        self.update_settings_field(|settings| {
            settings.proxy_auth_username = username.trim().chars().take(256).collect();
            settings.proxy_auth_password = password.chars().take(256).collect();
            if settings.proxy_auth_enabled
                && (settings.proxy_auth_username.is_empty()
                    || settings.proxy_auth_password.is_empty())
            {
                settings.proxy_auth_enabled = false;
                warned = true;
            }
        });
        if warned {
            self.notify(
                "warning",
                &tr("Авторизация прокси отключена: укажите логин и пароль", &[]),
            );
        }
    }

    /// Python `setProxyAuthEnabled`.
    pub fn set_proxy_auth_enabled(&mut self, enabled: bool) -> bool {
        let settings = self.controller.lock().state.settings.clone();
        if enabled
            && (settings.proxy_auth_username.trim().is_empty()
                || settings.proxy_auth_password.is_empty())
        {
            self.notify("warning", &tr("Сначала укажите логин и пароль прокси", &[]));
            return false;
        }
        self.update_settings_field(|settings| settings.proxy_auth_enabled = enabled);
        true
    }

    /// Python `setTunEndpointIndependentNat`.
    pub fn set_tun_endpoint_independent_nat(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.tun_endpoint_independent_nat = enabled);
    }

    /// Python `setTunBlockQuic`.
    pub fn set_tun_block_quic(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.tun_block_quic = enabled);
    }

    /// Python `setSniffRouteOnly`.
    pub fn set_sniff_route_only(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.sniff_route_only = enabled);
    }

    /// Python `setXrayFragment`.
    pub fn set_xray_fragment(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.enable_xray_fragment = enabled);
    }

    /// Python `setFinalFragment`.
    pub fn set_final_fragment(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.enable_final_fragment = enabled);
    }

    /// Python `setFragmentation`.
    pub fn set_fragmentation(&mut self, enabled: bool) {
        self.update_settings_field(|settings| {
            settings.enable_xray_fragment = enabled;
            settings.enable_final_fragment = enabled;
        });
    }

    /// Python `setFragmentSettings`.
    pub fn set_fragment_settings(&mut self, packets: &str, length: &str, delay: &str) {
        self.update_settings_field(|settings| {
            let value = |text: &str, default: &str| {
                let text = text.trim();
                if text.is_empty() {
                    default.to_string()
                } else {
                    text.to_string()
                }
            };
            settings.fragment_packets = value(packets, "tlshello");
            settings.fragment_length = value(length, "50-100");
            settings.fragment_delay = value(delay, "10-20");
        });
    }

    /// Python `setTailFragment`.
    pub fn set_tail_fragment(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.tail_fragment_enabled = enabled);
    }

    /// Python `setMultiplexing`.
    pub fn set_multiplexing(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.multiplex_enabled = enabled);
    }

    /// Python `setMultiplexConcurrency`.
    pub fn set_multiplex_concurrency(&mut self, value: i64) {
        self.update_settings_field(|settings| {
            settings.multiplex_concurrency = value.clamp(1, 32);
        });
    }

    // ── Auto-switch ────────────────────────────────────────────────────────

    /// Python `setAutoSwitch`.
    pub fn set_auto_switch(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.auto_switch_enabled = enabled);
    }

    /// Python `setAutoSwitchThreshold`.
    pub fn set_auto_switch_threshold(&mut self, kbps: i64) {
        self.update_settings_field(|settings| settings.auto_switch_threshold_kbps = kbps.max(1));
    }

    /// Python `setAutoSwitchDelay`.
    pub fn set_auto_switch_delay(&mut self, seconds: i64) {
        self.update_settings_field(|settings| settings.auto_switch_delay_sec = seconds.max(1));
    }

    /// Python `setAutoSwitchCooldown`.
    pub fn set_auto_switch_cooldown(&mut self, seconds: i64) {
        self.update_settings_field(|settings| settings.auto_switch_cooldown_sec = seconds.max(1));
    }

    // ── Core paths ─────────────────────────────────────────────────────────

    /// Python `_normalize_core_path_setting`.
    fn normalize_core_path_setting(path: &str, default_path: &Path) -> String {
        let value = path.trim().trim_matches('"').to_string();
        if value.is_empty() {
            return String::new();
        }
        let normalized = value.replace('/', "\\");
        let default_relative = format!(
            "core\\{}",
            default_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        );
        if normalized.to_lowercase() == default_relative.to_lowercase() {
            return String::new();
        }
        if Path::new(&value) == default_path {
            return String::new();
        }
        value
    }

    /// Python `_display_core_path`.
    fn display_core_path(path: &str, default_path: &Path) -> String {
        let value = path.trim().to_string();
        let default_display = format!(
            "core\\{}",
            default_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        );
        if value.is_empty() {
            return default_display;
        }
        // Python compares `Path(value).resolve()`, so a relative path such as
        // `core/sing-box-extended/sing-box.exe` (resolved against the app
        // folder) still counts as the default.
        let absolute = lumen_core::constants::base_dir().join(&value);
        let key = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
        if key(&absolute) == key(default_path) {
            return default_display;
        }
        let normalized = value.replace('/', "\\");
        if normalized.to_lowercase() == default_display.to_lowercase() {
            return default_display;
        }
        value
    }

    /// Python `setXrayPath`.
    pub fn set_xray_path(&mut self, path: &str) {
        let normalized = Self::normalize_core_path_setting(path, &xray_path_default());
        self.update_settings_field(|settings| settings.xray_path = normalized);
    }

    /// Python `setSingboxPath`.
    pub fn set_singbox_path(&mut self, path: &str) {
        let normalized = Self::normalize_core_path_setting(path, &singbox_path_default());
        self.update_settings_field(|settings| settings.singbox_path = normalized);
    }

    /// Python `xrayPath` (display form).
    pub fn xray_path_display(&self) -> String {
        Self::display_core_path(self.settings_cached.xray_path.trim(), &xray_path_default())
    }

    /// Python `singboxPath` (display form).
    pub fn singbox_path_display(&self) -> String {
        Self::display_core_path(
            self.settings_cached.singbox_path.trim(),
            &singbox_path_default(),
        )
    }

    // ── Startup settings ───────────────────────────────────────────────────

    /// Python `setLaunchOnStartup`.
    pub fn set_launch_on_startup(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.launch_on_startup = enabled);
    }

    /// Python `setLaunchInTrayOnStartup`.
    pub fn set_launch_in_tray_on_startup(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.launch_in_tray_on_startup = enabled);
    }

    /// Python `setBlockVpnConflicts`.
    pub fn set_block_vpn_conflicts(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.block_vpn_conflicts = enabled);
    }

    /// Python `setZapretAutostart`.
    pub fn set_zapret_autostart(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.zapret_autostart = enabled);
    }

    // ── Security: master password / auto-lock ──────────────────────────────

    /// Python `setMasterPassword`.
    pub fn set_master_password(&mut self, password: &str) {
        if password.is_empty() {
            self.notify("warning", "Введите пароль");
            return;
        }
        self.controller.lock().set_master_password(password);
        self.changes.settings = true;
        self.notify("success", "Пароль установлен");
    }

    /// Python `disableMasterPassword`.
    pub fn disable_master_password(&mut self, password: &str) -> bool {
        if password.is_empty() {
            self.notify("warning", "Введите текущий пароль");
            return false;
        }
        if !self.controller.lock().unlock(password) {
            self.notify("error", "Неверный пароль");
            return false;
        }
        self.controller.lock().disable_master_password();
        self.changes.settings = true;
        self.notify("info", "Мастер-пароль отключён");
        true
    }

    /// Python `setAutoLockMinutes`.
    pub fn set_auto_lock_minutes(&mut self, minutes: i64) {
        let value = minutes.clamp(1, 120);
        let mut controller = self.controller.lock();
        controller.state.security.auto_lock_minutes = value;
        controller.save();
        drop(controller);
        self.changes.settings = true;
    }

    // ── Data: encryption + backup ──────────────────────────────────────────

    /// Python `setEncryptionPassword`.
    pub fn set_encryption_password(&mut self, password: &str) {
        if password.is_empty() {
            self.notify("warning", "Введите пароль шифрования");
            return;
        }
        if self.controller.lock().set_data_passphrase(password) {
            self.push_initial_snapshot();
            self.start_deferred();
        }
        self.changes.settings = true;
    }

    /// Python `disableEncryption`.
    pub fn disable_encryption(&mut self) {
        self.controller.lock().clear_data_passphrase();
        self.changes.settings = true;
    }

    /// Python `exportBackup` minus the Qt file dialog (the shell passes the
    /// destination path).
    pub fn export_backup(&mut self, path: &Path, passphrase: &str) {
        self.controller.lock().export_backup(path, passphrase);
    }

    /// Python `importBackup` minus the Qt file dialog.
    pub fn import_backup(&mut self, path: &Path, passphrase: &str) {
        self.controller.lock().import_backup(path, passphrase);
    }

    /// Python `resetSettingsToDefaults`.
    pub fn reset_settings_to_defaults(&mut self) {
        self.controller.lock().reset_settings_to_defaults();
        self.notify(
            "success",
            &tr(
                "Настройки сброшены по умолчанию. Серверы и подписки сохранены.",
                &[],
            ),
        );
    }

    // ── Node tests ─────────────────────────────────────────────────────────

    /// Python `pingNodes` (method = configured default).
    pub fn ping_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().ping_nodes(ids, None);
    }

    /// Python `tcpingNodes`.
    pub fn tcping_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().ping_nodes(ids, Some("tcping"));
    }

    /// Python `httpGetNodes`.
    pub fn http_get_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().ping_nodes(ids, Some("http"));
    }

    /// Python `realDelayNodes`.
    pub fn real_delay_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().ping_nodes(ids, Some("real"));
    }

    /// Python `icmpNodes`.
    pub fn icmp_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().ping_nodes(ids, Some("icmp"));
    }

    /// Python `speedTestNodes`.
    pub fn speed_test_nodes(&mut self, ids: Option<&HashSet<String>>) {
        self.controller.lock().speed_test_nodes(ids);
    }

    /// Python `cancelSpeedTest`.
    pub fn cancel_speed_test(&mut self) {
        self.controller.lock().cancel_speed_test();
    }

    /// Python `cancelTests`.
    pub fn cancel_tests(&mut self) {
        self.controller.lock().cancel_tests();
    }

    pub fn tests_running(&self) -> bool {
        !self.running_tests.is_empty()
    }

    /// Python `deleteNodes`.
    pub fn delete_nodes(&mut self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let set: HashSet<String> = ids.iter().cloned().collect();
        self.controller.lock().remove_nodes(&set);
    }

    /// Python `reorderNode`.
    pub fn reorder_node(&mut self, node_id: &str, direction: &str) {
        self.controller.lock().reorder_nodes(node_id, direction);
    }

    // ── node edit / bulk-edit / share-link ─────────────────────────────────

    /// Python `nodeEditFields` — flattened editable fields ({} when missing).
    pub fn node_edit_fields(&self, node_id: &str) -> Value {
        let node = self.get_node_by_id(node_id);
        match node {
            Some(node) => node_edit_helpers::load_node_edit_fields(&node),
            None => json!({}),
        }
    }

    /// Python `manualNodeProtocols`.
    pub fn manual_node_protocols(&self) -> Vec<&'static str> {
        node_edit_helpers::MANUAL_NODE_PROTOCOLS.to_vec()
    }

    /// Python `manualNodeFields`.
    pub fn manual_node_fields(&self, protocol: &str, group: &str) -> Value {
        node_edit_helpers::new_node_edit_fields(protocol, group)
    }

    /// Python `nodeEditOptions`.
    pub fn node_edit_options(&self) -> Value {
        json!({
            "fingerprints": node_edit_helpers::FINGERPRINTS,
            "networks": node_edit_helpers::NETWORKS,
            "security": node_edit_helpers::SECURITY,
            "flows": node_edit_helpers::FLOWS,
            "rawHeaders": node_edit_helpers::RAW_HEADERS,
        })
    }

    fn get_node_by_id(&self, node_id: &str) -> Option<Node> {
        if node_id.is_empty() {
            return None;
        }
        let controller = self.controller.lock();
        lumen_app::node_runtime_service::get_node_by_id(&controller.state.nodes, Some(node_id))
            .cloned()
    }

    /// Python `saveNodeEdit`.
    pub fn save_node_edit(&mut self, node_id: &str, fields: &Value) {
        let node = self.get_node_by_id(node_id);
        let Some(node) = node else {
            self.notify("warning", "Сервер не найден");
            return;
        };
        let fields_map = fields.as_object().cloned().unwrap_or_default();
        let updates = match node_edit_helpers::build_node_updates(&node, &fields_map) {
            Ok(updates) => updates,
            Err(error) => {
                self.notify(
                    "error",
                    &tr("Не удалось сохранить: {error}", &[("error", &error)]),
                );
                return;
            }
        };
        self.controller.lock().update_node(node_id, &updates);
        self.notify("success", "Сервер обновлён");
    }

    /// Python `createManualNode`.
    pub fn create_manual_node(&mut self, fields: &Value) -> String {
        let mut payload = fields.as_object().cloned().unwrap_or_default();
        let protocol = payload
            .remove("protocol")
            .map(|value| value.as_str().unwrap_or("vless").trim().to_lowercase())
            .unwrap_or_else(|| "vless".to_string());
        if !node_edit_helpers::MANUAL_NODE_PROTOCOLS.contains(&protocol.as_str()) {
            self.notify(
                "error",
                &tr(
                    "Неподдерживаемый протокол: {protocol}",
                    &[("protocol", &protocol)],
                ),
            );
            return String::new();
        }
        let group = payload
            .get("group")
            .and_then(Value::as_str)
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("Default")
            .to_string();
        let mut draft = Node::new();
        draft.scheme = protocol.clone();
        draft.group = group;
        draft.outbound = json!({"protocol": protocol});
        let updates = match node_edit_helpers::build_node_updates(&draft, &payload) {
            Ok(updates) => updates,
            Err(error) => {
                self.notify(
                    "error",
                    &tr("Не удалось добавить сервер: {error}", &[("error", &error)]),
                );
                return String::new();
            }
        };
        for key in [
            "name", "group", "scheme", "server", "port", "link", "outbound",
        ] {
            if let Some(value) = updates.get(key) {
                match key {
                    "name" => draft.name = value.as_str().unwrap_or_default().to_string(),
                    "group" => draft.group = value.as_str().unwrap_or_default().to_string(),
                    "scheme" => draft.scheme = value.as_str().unwrap_or_default().to_string(),
                    "server" => draft.server = value.as_str().unwrap_or_default().to_string(),
                    "port" => draft.port = value.as_i64().unwrap_or(draft.port),
                    "link" => draft.link = value.as_str().unwrap_or_default().to_string(),
                    "outbound" => draft.outbound = value.clone(),
                    _ => {}
                }
            }
        }
        let node_id = self.controller.lock().add_manual_node(draft);
        self.notify("success", &tr("Сервер добавлен", &[]));
        node_id
    }

    /// Python `bulkEditNodes` — apply a bulk group move.
    pub fn bulk_edit_nodes(&mut self, ids: &[String], operations: &Value) {
        let node_ids: HashSet<String> = ids.iter().filter(|id| !id.is_empty()).cloned().collect();
        if node_ids.is_empty() {
            self.notify("warning", "Не выбрано ни одного сервера");
            return;
        }
        let group = operations
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let payload = json!({"group": group});
        self.controller
            .lock()
            .bulk_update_nodes(&node_ids, &payload);
        self.notify(
            "success",
            &tr(
                "Обновлено серверов: {count}",
                &[("count", &node_ids.len().to_string())],
            ),
        );
    }

    /// Python `copyNodeLink` — the share link for a node (or the selection).
    pub fn node_export_link(&self, node_id: &str) -> String {
        let node = if node_id.is_empty() {
            self.controller.lock().selected_node()
        } else {
            self.get_node_by_id(node_id)
        };
        node_edit_helpers::normalized_node_export_link(node.as_ref())
    }

    /// Python `copyNodeLink` (clipboard handled by the shell).
    pub fn copy_node_link(&mut self, node_id: &str) -> Option<String> {
        let link = self.node_export_link(node_id);
        if link.is_empty() {
            self.notify("warning", "У сервера нет ссылки для копирования");
            return None;
        }
        self.notify("success", "Ссылка скопирована в буфер обмена");
        Some(link)
    }

    /// Python `_node_export_family`.
    fn node_family(&self, node: &Node) -> &'static str {
        node_edit_helpers::node_export_family(node)
    }

    /// Python `exportNodeLinks` — share links of the given nodes (or all),
    /// one per line. Returns the payload for the shell clipboard.
    pub fn export_node_links(&mut self, ids: &[String]) -> Option<String> {
        let wanted: HashSet<&String> = ids.iter().collect();
        let selected: Vec<Node> = {
            let controller = self.controller.lock();
            controller
                .state
                .nodes
                .iter()
                .filter(|node| wanted.is_empty() || wanted.contains(&node.id))
                .cloned()
                .collect()
        };
        // Запрет на экспорт несовместимых архитектур вместе (например awg + vless).
        if !wanted.is_empty() {
            let families: HashSet<&str> =
                selected.iter().map(|node| self.node_family(node)).collect();
            if families.len() > 1 {
                self.notify(
                    "warning",
                    &tr(
                        "Нельзя экспортировать вместе конфиги разных архитектур (например AWG и VLESS)",
                        &[],
                    ),
                );
                return None;
            }
        }
        let links: Vec<String> = selected
            .iter()
            .map(|node| node_edit_helpers::normalized_node_export_link(Some(node)))
            .filter(|link| !link.is_empty())
            .collect();
        if links.is_empty() {
            self.notify("warning", &tr("Нет ссылок для экспорта", &[]));
            return None;
        }
        let payload = links.join("\n");
        self.notify(
            "success",
            &tr(
                "Скопировано ссылок: {count}",
                &[("count", &links.len().to_string())],
            ),
        );
        Some(payload)
    }

    /// Python `showNodeQr` — the share link for the shell's QR renderer.
    pub fn show_node_qr(&mut self, node_id: &str) {
        let node = if node_id.is_empty() {
            self.controller.lock().selected_node()
        } else {
            self.get_node_by_id(node_id)
        };
        let link = node_edit_helpers::normalized_node_export_link(node.as_ref());
        if link.is_empty() {
            self.notify("warning", &tr("У сервера нет ссылки для QR-кода", &[]));
            return;
        }
        let name = node
            .as_ref()
            .map(|node| {
                if node.name.is_empty() {
                    node.server.clone()
                } else {
                    node.name.clone()
                }
            })
            .unwrap_or_default();
        self.outbox.push_back(UiEvent::NodeQrReady { link, name });
    }

    /// Python `historyData`.
    pub fn history_data(&self, days: i64) -> Value {
        let key = format!("history_data:{days}");
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| json!({}));
        }
        let value = self.history_data_now(days);
        self.remember_read(key, value.clone());
        value
    }

    fn history_data_now(&self, days: i64) -> Value {
        let controller = self.controller.lock();
        history_helpers::build_history_payload(
            Some(controller.traffic_history()),
            if days == 0 { 30 } else { days },
        )
    }

    /// Python `clearTrafficHistory`.
    pub fn clear_traffic_history(&mut self) {
        self.controller.lock().traffic_history().clear();
        self.notify("success", &tr("История очищена", &[]));
    }

    /// Python `updatesInitialState`.
    /// Python `updatesInitialState`, without blocking: the first call starts
    /// the version probes on a worker thread and returns `None` until they
    /// finish. Python ran them on the GUI thread, which froze the first
    /// visit of the Updates tab for about half a second.
    pub fn updates_initial_state(&mut self) -> Option<Value> {
        self.prefetch_component_versions();
        self.component_versions.lock().clone()
    }

    /// Starts the Updates tab version probes once (also from `start_deferred`,
    /// so the tab usually opens with them ready).
    pub fn prefetch_component_versions(&mut self) {
        if self.component_versions_started {
            return;
        }
        self.component_versions_started = true;
        let xray_path = self.settings_cached.xray_path.clone();
        let singbox_path = self.settings_cached.singbox_path.clone();
        let slot = self.component_versions.clone();
        let spawned = std::thread::Builder::new()
            .name("lumen-component-versions".into())
            .spawn(move || {
                let version =
                    lumen_engines::xray::manager::get_xray_version(&xray_path).unwrap_or_default();
                let singbox_version =
                    lumen_engines::singbox::manager::get_singbox_version(&singbox_path)
                        .unwrap_or_default();
                *slot.lock() = Some(json!({
                    "appVersion": APP_VERSION,
                    "xrayVersion": version,
                    "singboxVersion": singbox_version,
                    "drouteVersion": lumen_engines::discord_proxy_manager::get_droute_bundle_version(),
                }));
            });
        if spawned.is_err() {
            self.component_versions_started = false;
        }
    }

    // ── updates tab ────────────────────────────────────────────────────────

    /// Python `checkAppUpdate`.
    pub fn check_app_update(&mut self) {
        self.start_app_update_check(false);
    }

    pub fn app_update_in_progress(&self) -> bool {
        self.update_check.is_some() || self.update_download.is_some()
    }

    /// Python `_start_app_update_check`. The ported `app_update_check`
    /// performs release discovery; the shell receives progress via
    /// [`UiEvent::AppUpdateState`].
    pub fn start_app_update_check(&mut self, silent: bool) {
        if self.quitting || self.app_update_in_progress() {
            return;
        }
        let channel = self.release_channel();
        let proxy_url = self.effective_http_proxy_url_for_background();
        self.update_check_silent = silent;
        if !silent {
            self.outbox
                .push_back(UiEvent::AppUpdateState(json!({"phase": "checking"})));
        }
        let controller = self.controller.clone();
        self.update_check = Some(self.runtime.spawn(async move {
            let update = lumen_app::app_update_check::check_app_update(
                &channel,
                false,
                proxy_url.as_deref(),
            )
            .await
            .map_err(|error| error.to_string());
            controller.lock().report_app_update_check(update);
        }));
    }

    fn on_app_update_checked(
        &mut self,
        result: Result<Option<lumen_app::app_update_check::AppUpdate>, String>,
    ) {
        self.update_check.take();
        let silent = std::mem::take(&mut self.update_check_silent);
        if self.quitting {
            return;
        }
        match result {
            Ok(Some(update)) => {
                let notes = update.notes.trim();
                let notes = if let Some((cut, _)) = notes.char_indices().nth(1200) {
                    format!("{}…", notes[..cut].trim_end())
                } else {
                    notes.to_string()
                };
                let payload = json!({"phase": "available", "version": update.version, "notes": notes, "isDowngrade": update.is_downgrade});
                let automatic = lumen_app::app_update_check::should_auto_install(
                    Some(&update),
                    self.settings_cached.app_auto_update,
                );
                if silent {
                    let key = if update.is_downgrade {
                        "Доступен откат к v{version}"
                    } else {
                        "Доступно обновление v{version}"
                    };
                    self.notify("info", &tr(key, &[("version", &update.version)]));
                }
                self.app_update = Some(update);
                self.updates_available = true;
                self.changes.updates_available = true;
                self.outbox.push_back(UiEvent::AppUpdateState(payload));
                if automatic {
                    self.install_app_update();
                }
            }
            Ok(None) => {
                self.app_update = None;
                if !silent {
                    self.outbox
                        .push_back(UiEvent::AppUpdateState(json!({"phase": "uptodate"})));
                }
            }
            Err(error) if !silent => self.outbox.push_back(UiEvent::AppUpdateState(
                json!({"phase": "error", "message": error}),
            )),
            Err(_) => {}
        }
    }

    pub fn install_app_update(&mut self) -> bool {
        if self.quitting || self.update_download.is_some() {
            return false;
        }
        let Some(update) = self.app_update.clone() else {
            return false;
        };
        let proxy = self.effective_http_proxy_url_for_background();
        let handle = self.runtime_handle();
        let _runtime = handle.enter();
        self.update_download = Some(lumen_app::app_updater::AppUpdateDownloader::start(
            update, proxy,
        ));
        self.outbox.push_back(UiEvent::AppUpdateState(
            json!({"phase": "downloading", "percent": 0}),
        ));
        true
    }

    fn pump_app_update_download(&mut self) {
        use lumen_app::app_updater::AppUpdateDownloadEvent;
        let Some(worker) = self.update_download.as_mut() else {
            return;
        };
        while let Ok(event) = worker.events.try_recv() {
            let installed = matches!(&event, AppUpdateDownloadEvent::FinishedOk);
            let (payload, finished) = match event {
                AppUpdateDownloadEvent::Progress(percent) => {
                    (json!({"phase": "downloading", "percent": percent}), false)
                }
                AppUpdateDownloadEvent::Status(message) => {
                    (json!({"phase": "downloading", "message": message}), false)
                }
                AppUpdateDownloadEvent::Error(message) => {
                    (json!({"phase": "error", "message": message}), true)
                }
                AppUpdateDownloadEvent::FinishedOk => (json!({"phase": "updated"}), true),
            };
            self.outbox.push_back(UiEvent::AppUpdateState(payload));
            if finished {
                self.update_download.take();
                if installed {
                    self.prepare_quit();
                }
                break;
            }
        }
    }

    /// Python `releaseChannel`.
    pub fn release_channel(&self) -> String {
        let value = self
            .controller
            .lock()
            .state
            .settings
            .release_channel
            .trim()
            .to_lowercase();
        if matches!(
            value.as_str(),
            "prerelease" | "pre-release" | "pre" | "beta" | "nightly"
        ) {
            "prerelease".to_string()
        } else {
            "stable".to_string()
        }
    }

    /// Python `_start_resource_update_check`.
    pub fn start_resource_update_check(&mut self) {
        if self.quitting {
            return;
        }
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let mut guard = controller.lock();
            if !guard.state.settings.resource_update_check { return; }
            let settings = guard.state.settings.clone();
            let _ = guard.run_resource_update("singbox", false, None);
            let _ = guard.run_resource_update(
                "geodata",
                false,
                Some(settings.regional_preset.as_str()),
            );
        });
    }

    /// Python `checkXrayUpdate`.
    pub fn check_xray_update(&mut self) {
        if self.controller.lock().run_xray_core_update(false, false) {
            self.outbox.push_back(UiEvent::XrayUpdateState(
                json!({"phase": "checking", "percent": 0}),
            ));
        }
    }

    /// Python `updateXrayCore`.
    pub fn update_xray_core(&mut self) {
        if self.controller.lock().run_xray_core_update(true, false) {
            self.outbox.push_back(UiEvent::XrayUpdateState(
                json!({"phase": "updating", "percent": 0}),
            ));
        }
    }

    /// Python `checkSingboxUpdate` / `updateSingboxCore` /
    /// `checkGeodataUpdate` / `updateGeodataFiles` / `checkDrouteUpdate` /
    /// `updateDroute` share one implementation.
    pub fn resource_update(&mut self, kind: &str, apply_update: bool) {
        let phase = if apply_update { "updating" } else { "checking" };
        self.outbox.push_back(UiEvent::ResourceUpdateState(json!({
            "kind": kind, "phase": phase, "percent": 0
        })));
        let started = self
            .controller
            .lock()
            .run_resource_update(kind, apply_update, None);
        if !started {
            self.outbox.push_back(UiEvent::ResourceUpdateState(json!({
                "kind": kind, "phase": "idle", "percent": 0
            })));
        }
    }

    // ── Zapret tab ─────────────────────────────────────────────────────────

    /// Python `_zapret_manual_running`.
    fn zapret_manual_running(&self) -> bool {
        let controller = self.controller.lock();
        let zapret = &controller.zapret;
        if !zapret.running() {
            return false;
        }
        !zapret.current_preset().is_empty() || zapret.server_scope().is_none()
    }

    /// Python `_zapret_active_preset`.
    fn zapret_active_preset(&self) -> String {
        let controller = self.controller.lock();
        let zapret = &controller.zapret;
        if !zapret.current_preset().is_empty() {
            return zapret.current_preset().to_string();
        }
        // Only the server-limited part is running: no manual preset is active.
        if zapret.server_scope().is_some() {
            return String::new();
        }
        controller.state.settings.zapret_preset.clone()
    }

    /// Python `_zapret_server_preset`.
    fn zapret_server_preset(&self) -> String {
        let controller = self.controller.lock();
        controller
            .zapret
            .server_scope()
            .map(|scope| scope.preset.clone())
            .unwrap_or_default()
    }

    /// Python `zapretStatus`.
    pub fn zapret_status(&self) -> Value {
        let key = "zapret_status".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| json!({}));
        }
        let value = self.zapret_status_now();
        self.remember_read(key, value.clone());
        value
    }

    fn zapret_status_now(&self) -> Value {
        let running = self.zapret_manual_running();
        let preset = if running {
            self.zapret_active_preset()
        } else {
            String::new()
        };
        json!({
            "running": running,
            "preset": preset,
            "serverPreset": self.zapret_server_preset(),
            "error": "",
        })
    }

    /// Python `startZapret`.
    pub fn start_zapret(&mut self, preset_name: &str) {
        let name = preset_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        {
            let mut controller = self.controller.lock();
            controller.state.settings.zapret_preset = name.clone();
            controller.save();
        }
        if !is_process_elevated() {
            self.notify_action(
                "warning",
                &tr("Zapret требует права администратора", &[]),
                "restart-admin",
                &tr("Перезапустить от администратора", &[]),
            );
            return;
        }
        self.controller.lock().start_zapret_preset(&name);
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            controller.lock().sync_zapret_server_binding();
        });
    }

    /// Python `stopZapret`.
    pub fn stop_zapret(&mut self) {
        self.controller.lock().stop_zapret_manual();
    }

    /// Python `readPreset`.
    pub fn read_preset(&self, name: &str) -> String {
        lumen_engines::zapret_manager::read_preset(name)
    }

    /// Python `savePreset`.
    pub fn save_preset(&mut self, name: &str, description: &str, content: &str) -> bool {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.notify("warning", "Укажите имя пресета");
            return false;
        }
        if name.chars().any(|c| "\\/:*?\"<>|".contains(c)) {
            self.notify("warning", "Недопустимые символы в имени");
            return false;
        }
        let saved = lumen_engines::zapret_manager::save_preset(&name, content, description).is_ok();
        self.invalidate_zapret_presets();
        if saved {
            self.notify(
                "success",
                &tr("Пресет сохранён: {name}", &[("name", &name)]),
            );
            self.changes.settings = true;
            true
        } else {
            self.notify("error", "Не удалось сохранить пресет");
            false
        }
    }

    /// Python `saveRenamedPreset`.
    pub fn save_renamed_preset(
        &mut self,
        old_name: &str,
        name: &str,
        description: &str,
        content: &str,
    ) -> bool {
        let old_name = old_name.trim().to_string();
        let name = name.trim().to_string();
        if old_name.is_empty() || old_name == name {
            return self.save_preset(&name, description, content);
        }
        if name.is_empty() || name.chars().any(|c| "\\/:*?\"<>|".contains(c)) {
            self.notify("warning", &tr("Недопустимое имя пресета", &[]));
            return false;
        }
        match lumen_engines::zapret_manager::rename_preset(&old_name, &name) {
            Some(_) => {}
            None => {
                self.notify(
                    "error",
                    &tr("Пресет с именем «{name}» уже есть", &[("name", &name)]),
                );
                return false;
            }
        }
        let mut settings = self.controller.lock().state.settings.clone();
        if settings.zapret_preset == old_name {
            settings.zapret_preset = name.clone();
        }
        if let Some(bindings) = settings.zapret_server_presets.as_object_mut() {
            for (_node_id, preset) in bindings.iter_mut() {
                if preset.as_str() == Some(old_name.as_str()) {
                    *preset = json!(name);
                }
            }
        }
        self.controller.lock().update_settings(settings);
        self.changes.settings = true;
        self.save_preset(&name, description, content)
    }

    /// Python `deletePreset`.
    pub fn delete_preset(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let active = {
            let controller = self.controller.lock();
            let current = controller.zapret.current_preset();
            controller.zapret.running() && if current.is_empty() {
                controller.zapret.server_scope().is_none() && controller.state.settings.zapret_preset == name
            } else {
                current == name
            }
        };
        if active {
            self.controller.lock().zapret.stop_with(false, true, true);
        }
        let deleted = lumen_engines::zapret_manager::delete_preset(name);
        self.invalidate_zapret_presets();
        if deleted {
            self.notify("success", &tr("Пресет удалён: {name}", &[("name", name)]));
            self.changes.settings = true;
        } else {
            self.notify("error", "Не удалось удалить пресет");
        }
    }

    /// Python `importZapretPreset` (the shell passes the picked path).
    pub fn import_zapret_preset(&mut self, path: &Path) -> String {
        let imported = lumen_engines::zapret_manager::import_preset(path);
        self.invalidate_zapret_presets();
        if let Some(info) = imported {
            self.notify(
                "success",
                &tr("Импортирован пресет: {name}", &[("name", &info.name)]),
            );
            self.changes.settings = true;
            info.name
        } else {
            self.notify("warning", "Не удалось импортировать пресет");
            String::new()
        }
    }

    /// Python `zapretPresets`.
    pub fn zapret_presets(&self) -> Vec<Value> {
        // Edits made in Lumen clear the cache; the TTL picks up files changed
        // outside it.
        const TTL: Duration = Duration::from_secs(2);
        let mut cache = self.zapret_presets_cache.lock();
        if let Some((loaded, presets)) = cache.as_ref()
            && loaded.elapsed() < TTL
        {
            return presets.clone();
        }
        let presets = zapret_helpers::list_preset_maps();
        *cache = Some((std::time::Instant::now(), presets.clone()));
        presets
    }

    fn invalidate_zapret_presets(&self) {
        *self.zapret_presets_cache.lock() = None;
    }

    /// Python `_node_option_label`.
    fn node_option_label(node: Option<&Node>) -> String {
        let Some(node) = node else {
            return String::new();
        };
        let name = if node.name.trim().is_empty() {
            format!("{}:{}", node.server, node.port)
        } else {
            node.name.trim().to_string()
        };
        let group = node.group.trim().to_string();
        if group.is_empty() {
            name
        } else {
            format!("{name} · {group}")
        }
    }

    /// Python `zapretServerBindings`.
    pub fn zapret_server_bindings(&mut self) -> Vec<ZapretBindingRow> {
        let key = "zapret_server_bindings".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| Vec::new());
        }
        let value = self.zapret_server_bindings_now();
        self.remember_read(key, value.clone());
        value
    }

    fn zapret_server_bindings_now(&mut self) -> Vec<ZapretBindingRow> {
        let presets: Vec<(String, String)> = {
            let controller = self.controller.lock();
            let mut rows: Vec<(String, String)> = controller
                .state
                .settings
                .zapret_server_presets
                .as_object()
                .map(|map| {
                    map.iter()
                        .map(|(node_id, preset)| {
                            (
                                node_id.clone(),
                                preset.as_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            rows.sort();
            rows
        };
        let known: HashSet<String> = self
            .zapret_presets()
            .iter()
            .filter_map(|info| info.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        let connected_id = {
            let controller = self.controller.lock();
            if controller.connected {
                controller
                    .state
                    .selected_node_id
                    .clone()
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };
        let mut rows: Vec<ZapretBindingRow> = Vec::new();
        for (node_id, preset) in presets {
            let node = self.get_node_by_id(&node_id);
            let strategy_mode = strategies::is_server_strategy(&preset);
            let effective = if strategy_mode {
                String::new()
            } else if !preset.is_empty() {
                preset.clone()
            } else {
                self.settings_cached.zapret_preset.clone()
            };
            let choices = node
                .as_ref()
                .map(|node| self.zapret_binding_choices(node, &preset))
                .unwrap_or_default();
            let value = node
                .as_ref()
                .map(|node| self.zapret_binding_value(node, &preset))
                .unwrap_or_default();
            let values: Vec<String> = choices.iter().map(|(value, _)| value.clone()).collect();
            let kind = if strategy_mode {
                node.as_ref()
                    .map(|node| strategies::node_server_kind(node))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let name = node
                .as_ref()
                .map(|node| Self::node_option_label(Some(node)))
                .unwrap_or_else(|| tr("Сервер удалён", &[]));
            rows.push(ZapretBindingRow {
                choice_index: values
                    .iter()
                    .position(|candidate| *candidate == value)
                    .map(|index| index as i64)
                    .unwrap_or(-1),
                node_id: node_id.clone(),
                placeholder: if preset.is_empty() && !effective.is_empty() {
                    format!("{}: {effective}", tr("Пресет", &[]))
                } else {
                    String::new()
                },
                name,
                preset: preset.clone(),
                missing_node: node.is_none(),
                missing_preset: !effective.is_empty() && !known.contains(&effective),
                active: !connected_id.is_empty() && connected_id == node_id,
                strategy_mode,
                kind_title: strategies::kind_title(&kind).to_string(),
                choices,
            });
        }
        rows.sort_by(|a, b| {
            b.missing_node
                .cmp(&a.missing_node)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        rows
    }

    /// Python `_zapret_strategy_options` — default strategy first, then by
    /// reliability.
    fn zapret_strategy_options(&self, kind: &str) -> Vec<(String, String)> {
        let catalog = strategies::kind_catalog(kind, None);
        let default = strategies::default_strategy(kind);
        let mut entries: Vec<&strategies::Strategy> =
            catalog.iter().map(|(_, strategy)| strategy).collect();
        entries.sort_by(|a, b| {
            let a_default = a.strategy_id != default;
            let b_default = b.strategy_id != default;
            b_default.cmp(&a_default).then_with(|| {
                strategies::strategy_sort_key(a).cmp(&strategies::strategy_sort_key(b))
            })
        });
        entries
            .into_iter()
            .map(|entry| (entry.strategy_id.clone(), entry.name.clone()))
            .collect()
    }

    /// Python `_zapret_binding_value`.
    fn zapret_binding_value(&self, node: &Node, preset: &str) -> String {
        if !strategies::is_server_strategy(preset) {
            return preset.to_string();
        }
        let kind = strategies::node_server_kind(node);
        let current =
            strategies::strategy_for(&kind, &strategies::server_strategy_id(preset), None);
        let default = strategies::default_strategy(&kind);
        match current {
            Some(current) if current.strategy_id != default => {
                strategies::server_strategy_preset(&current.strategy_id)
            }
            _ => strategies::SERVER_STRATEGY_PRESET.to_string(),
        }
    }

    /// Python `_zapret_binding_choices` — automatic strategy, presets, then
    /// the other strategies of the server kind.
    fn zapret_binding_choices(&self, node: &Node, preset: &str) -> Vec<(String, String)> {
        let kind = strategies::node_server_kind(node);
        let mut choices: Vec<(String, String)> = Vec::new();
        let auto = strategies::strategy_for(&kind, "", None);
        if let Some(auto) = auto.as_ref() {
            choices.push((
                strategies::SERVER_STRATEGY_PRESET.to_string(),
                format!("{}: {}", tr("Авто", &[]), auto.name),
            ));
        }
        let mut names: Vec<String> = self
            .zapret_presets()
            .iter()
            .filter_map(|info| info.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        if !preset.is_empty()
            && !strategies::is_server_strategy(preset)
            && !names.contains(&preset.to_string())
        {
            // a vanished preset stays visible (marked "not found")
            names.push(preset.to_string());
        }
        for name in names {
            choices.push((name.clone(), format!("{}: {name}", tr("Пресет", &[]))));
        }
        let default = auto
            .map(|entry| entry.strategy_id.clone())
            .unwrap_or_default();
        for (value, label) in self.zapret_strategy_options(&kind) {
            if value != default {
                choices.push((strategies::server_strategy_preset(&value), label));
            }
        }
        choices
    }

    /// Python `zapretBindingChoices`.
    pub fn zapret_binding_choices_for(&self, node_id: &str) -> Vec<(String, String)> {
        let key = format!("zapret_binding_choices_for:{node_id}");
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| Vec::new());
        }
        let value = self.zapret_binding_choices_for_now(node_id);
        self.remember_read(key, value.clone());
        value
    }

    fn zapret_binding_choices_for_now(&self, node_id: &str) -> Vec<(String, String)> {
        match self.get_node_by_id(node_id.trim()) {
            Some(node) => self.zapret_binding_choices(&node, ""),
            None => Vec::new(),
        }
    }

    /// Python `zapretServerOptions`.
    pub fn zapret_server_options(&mut self) -> Vec<ZapretServerOption> {
        let bound: HashSet<String> = self
            .controller
            .lock()
            .state
            .settings
            .zapret_server_presets
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        let selected_id = self
            .controller
            .lock()
            .selected_node()
            .map(|node| node.id)
            .unwrap_or_default();
        let mut options: Vec<ZapretServerOption> = Vec::new();
        let mut selected_option: Option<ZapretServerOption> = None;
        {
            let controller = self.controller.lock();
            for node in &controller.state.nodes {
                if bound.contains(&node.id) {
                    continue;
                }
                let mut item = ZapretServerOption {
                    value: node.id.clone(),
                    label: Self::node_option_label(Some(node)),
                };
                if node.id == selected_id {
                    // The selected server is the one most likely to be bound:
                    // always first.
                    item.label = format!("{} ({})", item.label, tr("Текущий", &[]));
                    selected_option = Some(item);
                } else {
                    options.push(item);
                }
            }
        }
        if let Some(item) = selected_option {
            options.insert(0, item);
        }
        options
    }

    /// Python `_store_zapret_bindings`.
    fn store_zapret_bindings(&mut self, presets: Value) {
        let mut settings = self.controller.lock().state.settings.clone();
        settings.zapret_server_presets = presets;
        self.controller.lock().update_settings(settings);
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            controller.lock().sync_zapret_server_binding();
        });
    }

    /// Python `setZapretServerBinding`.
    pub fn set_zapret_server_binding(&mut self, node_id: &str, preset: &str) {
        let key = node_id.trim().to_string();
        if key.is_empty() || self.get_node_by_id(&key).is_none() {
            return;
        }
        let mut presets = self
            .controller
            .lock()
            .state
            .settings
            .zapret_server_presets
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut value = preset.trim().to_string();
        if strategies::is_server_strategy(&value) {
            let node = self.get_node_by_id(&key);
            let kind = node
                .as_ref()
                .map(strategies::node_server_kind)
                .unwrap_or_default();
            let sid = strategies::server_strategy_id(&value);
            if !sid.is_empty() && strategies::kind_catalog(&kind, None).get(&sid).is_none() {
                return;
            }
            let default = strategies::default_strategy(&kind);
            value = strategies::server_strategy_preset(if sid == default { "" } else { &sid });
        }
        if presets.get(&key).and_then(Value::as_str) == Some(value.as_str()) {
            return;
        }
        presets.insert(key, json!(value));
        self.store_zapret_bindings(Value::Object(presets));
    }

    /// Python `removeZapretServerBinding`.
    pub fn remove_zapret_server_binding(&mut self, node_id: &str) {
        let mut presets = self
            .controller
            .lock()
            .state
            .settings
            .zapret_server_presets
            .as_object()
            .cloned()
            .unwrap_or_default();
        if presets.remove(node_id).is_none() {
            return;
        }
        self.store_zapret_bindings(Value::Object(presets));
    }

    // ── Proxy chain ────────────────────────────────────────────────────────

    /// Python `proxyChainNodeOptions`.
    pub fn proxy_chain_node_options(&mut self) -> Vec<(String, String)> {
        let key = "proxy_chain_node_options".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| Vec::new());
        }
        let value = self.proxy_chain_node_options_now();
        self.remember_read(key, value.clone());
        value
    }

    fn proxy_chain_node_options_now(&mut self) -> Vec<(String, String)> {
        let mut options: Vec<(String, String)> = vec![(String::new(), tr("Не использовать", &[]))];
        let (front, exit) = {
            let controller = self.controller.lock();
            for node in &controller.state.nodes {
                if lumen_core::config::proxy_chain::is_full_config_node(node) {
                    continue;
                }
                options.push((node.id.clone(), Self::node_option_label(Some(node))));
            }
            (
                controller.state.settings.proxy_chain_front_node_id.clone(),
                controller.state.settings.proxy_chain_exit_node_id.clone(),
            )
        };
        for stale in [front, exit] {
            if !stale.is_empty() && options.iter().all(|(value, _)| *value != stale) {
                options.push((stale, tr("Сервер удалён", &[])));
            }
        }
        options
    }

    /// Python `proxyChainPreview` — hops for the currently selected server.
    pub fn proxy_chain_preview(&mut self) -> Value {
        let key = "proxy_chain_preview".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| json!({}));
        }
        let value = self.proxy_chain_preview_now();
        self.remember_read(key, value.clone());
        value
    }

    fn proxy_chain_preview_now(&mut self) -> Value {
        let controller = self.controller.lock();
        let mut settings = controller.state.settings.clone();
        settings.proxy_chain_enabled = true;
        let main = controller.selected_node();
        let chain = lumen_core::config::proxy_chain::resolve_proxy_chain(
            &settings.to_dict(),
            &controller.state.nodes,
            main.as_ref(),
        )
        .unwrap_or_default();
        let mut hops: Vec<Value> = Vec::new();
        if let Some(front) = chain.front.as_ref() {
            hops.push(json!({"role": "front", "label": Self::node_option_label(Some(front))}));
        }
        hops.push(json!({
            "role": "main",
            "label": main
                .as_ref()
                .map(|node| Self::node_option_label(Some(node)))
                .unwrap_or_else(|| tr("Выбранный сервер", &[])),
        }));
        if let Some(exit) = chain.exit.as_ref() {
            hops.push(json!({"role": "exit", "label": Self::node_option_label(Some(exit))}));
        }
        json!({
            "hops": hops,
            "notes": chain
                .notes
                .iter()
                .map(|note| self.localize(note))
                .collect::<Vec<_>>(),
        })
    }

    /// Python `setProxyChainEnabled`.
    pub fn set_proxy_chain_enabled(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.proxy_chain_enabled = enabled);
    }

    /// Python `setProxyChainFront`.
    pub fn set_proxy_chain_front(&mut self, node_id: &str) {
        self.update_settings_field(|settings| {
            settings.proxy_chain_front_node_id = node_id.trim().to_string()
        });
    }

    /// Python `setProxyChainExit`.
    pub fn set_proxy_chain_exit(&mut self, node_id: &str) {
        self.update_settings_field(|settings| {
            settings.proxy_chain_exit_node_id = node_id.trim().to_string()
        });
    }

    // ── Configs (sing-box / xray raw editors) ──────────────────────────────

    /// Python `openConfigDirectory` (the shell opens the returned path).
    pub fn config_directory(&mut self, core: &str) -> Option<PathBuf> {
        if core != "singbox" && core != "xray" {
            return None;
        }
        let mut controller = self.controller.lock();
        let path = if core == "singbox" {
            controller.get_singbox_config_dir()
        } else {
            controller.get_xray_config_dir()
        };
        let _ = std::fs::create_dir_all(&path);
        Some(path)
    }

    /// Python `loadConfig`.
    pub fn load_config(&mut self, core: &str) -> Value {
        let core = if core != "singbox" && core != "xray" {
            "singbox"
        } else {
            core
        };
        let load = |controller: &mut AppController| -> Result<(PathBuf, String), anyhow::Error> {
            if core == "singbox" {
                controller.load_active_singbox_config_text()
            } else {
                controller.load_active_xray_config_text()
            }
        };
        let mut controller = self.controller.lock();
        match load(&mut controller) {
            Ok((path, text)) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let message = tr("Открыта активная копия: {name}", &[("name", &name)]);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    &text,
                    Some(&path.to_string_lossy().replace('\\', "/")),
                    "info",
                    &message,
                )
            }
            Err(error) => configs_helpers::build_state(
                &mut controller,
                core,
                "",
                None,
                "error",
                &error.to_string(),
            ),
        }
    }

    /// Python `selectConfig`.
    pub fn select_config(&mut self, core: &str, relative_path: &str) -> Value {
        if (core != "singbox" && core != "xray") || relative_path.is_empty() {
            let controller = self.controller.clone();
            let mut controller = controller.lock();
            let core = if core != "singbox" && core != "xray" {
                "singbox"
            } else {
                core
            };
            return configs_helpers::build_state(&mut controller, core, "", None, "", "");
        }
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let load = |controller: &mut AppController| -> Result<(PathBuf, String), anyhow::Error> {
            if core == "singbox" {
                controller.load_singbox_config_text(Path::new(relative_path))
            } else {
                controller.load_xray_config_text(Path::new(relative_path))
            }
        };
        match load(&mut controller) {
            Ok((path, text)) => {
                configs_helpers::sync_template_for_config(&mut controller, core, &path);
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let message = tr("Открыт конфиг: {name}", &[("name", &name)]);
                self.notify("success", &message);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    &text,
                    Some(&path.to_string_lossy().replace('\\', "/")),
                    "info",
                    &message,
                )
            }
            Err(error) => {
                let first_line = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.notify("error", &first_line);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    "",
                    None,
                    "error",
                    &error.to_string(),
                )
            }
        }
    }

    /// Python `selectTemplate`.
    pub fn select_template(&mut self, core: &str, relative_path: &str) -> Value {
        self.load_config_template(core, Path::new(relative_path), false)
    }

    pub fn config_template_directory(&self, core: &str) -> PathBuf {
        let controller = self.controller.lock();
        if core == "xray" { controller.get_xray_template_dir() } else { controller.get_singbox_template_dir() }
    }

    /// Python `importTemplate` after the shell's file picker returns a path.
    pub fn import_config_template(&mut self, core: &str, path: &Path) -> Value {
        self.load_config_template(core, path, true)
    }

    fn load_config_template(&mut self, core: &str, selected: &Path, imported: bool) -> Value {
        if (core != "singbox" && core != "xray") || selected.as_os_str().is_empty() {
            let controller = self.controller.clone();
            let mut controller = controller.lock();
            let core = if core != "singbox" && core != "xray" {
                "singbox"
            } else {
                core
            };
            return configs_helpers::build_state(&mut controller, core, "", None, "", "");
        }
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let import = |controller: &mut AppController| -> Result<(PathBuf, String), anyhow::Error> {
            if core == "singbox" {
                controller.import_singbox_template(selected)
            } else {
                controller.import_xray_template(selected)
            }
        };
        match import(&mut controller) {
            Ok((path, text)) => {
                let name = selected
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let message = if imported {
                    self.notify("success", &tr("Импортирован template: {name}", &[("name", &name)]));
                    let active = path.file_name().unwrap_or_default().to_string_lossy();
                    tr("Импортирован template и обновлена активная копия: {name}", &[("name", &active)])
                } else {
                    let message = tr("Применён шаблон: {name}", &[("name", &name)]);
                    self.notify("success", &message);
                    message
                };
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    &text,
                    Some(&path.to_string_lossy().replace('\\', "/")),
                    "info",
                    &message,
                )
            }
            Err(error) => {
                let first_line = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.notify("error", &first_line);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    "",
                    None,
                    "error",
                    &error.to_string(),
                )
            }
        }
    }

    /// Python `resetConfig`.
    pub fn reset_config(&mut self, core: &str) -> Value {
        if core != "singbox" && core != "xray" {
            let controller = self.controller.clone();
            let mut controller = controller.lock();
            return configs_helpers::build_state(&mut controller, "singbox", "", None, "", "");
        }
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let result: Result<ProfileEditResult, anyhow::Error> = if core == "singbox" {
            controller.reset_active_singbox_config_to_template()
        } else {
            controller.reset_active_xray_config_to_template()
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let first_line = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.notify("error", &first_line);
                return configs_helpers::build_state(
                    &mut controller,
                    core,
                    "",
                    None,
                    "error",
                    &error.to_string(),
                );
            }
        };
        if result.path.is_none() {
            let message = if result.message.is_empty() {
                "Сброс не выполнен".to_string()
            } else {
                result.message.clone()
            };
            let first_line = message.lines().next().unwrap_or_default().to_string();
            self.notify("error", &first_line);
            return configs_helpers::build_state(
                &mut controller,
                core,
                "",
                None,
                "error",
                &message,
            );
        }
        let load = if core == "singbox" {
            controller.load_active_singbox_config_text()
        } else {
            controller.load_active_xray_config_text()
        };
        let Ok((loaded_path, text)) = load else {
            return configs_helpers::build_state(
                &mut controller,
                core,
                "",
                None,
                "error",
                "reset reload failed",
            );
        };
        self.notify("success", &result.message);
        configs_helpers::build_state(
            &mut controller,
            core,
            &text,
            Some(&loaded_path.to_string_lossy().replace('\\', "/")),
            "success",
            &result.message,
        )
    }

    /// Python `saveConfig`.
    pub fn save_config(&mut self, core: &str, text: &str, previous_text: &str) -> Value {
        if core != "singbox" && core != "xray" {
            let controller = self.controller.clone();
            let mut controller = controller.lock();
            return configs_helpers::build_state(&mut controller, "singbox", text, None, "", "");
        }
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let saved = if core == "singbox" {
            controller.save_singbox_config_text(
                text,
                None,
                if previous_text.is_empty() {
                    None
                } else {
                    Some(previous_text)
                },
            )
        } else {
            controller.save_xray_config_text(text, None)
        };
        match saved {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let message = tr("Сохранено: {name}", &[("name", &name)]);
                self.notify("success", &message);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    text,
                    Some(&path.to_string_lossy().replace('\\', "/")),
                    "success",
                    &message,
                )
            }
            Err(error) => {
                let first_line = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.notify("error", &first_line);
                configs_helpers::build_state(
                    &mut controller,
                    core,
                    text,
                    None,
                    "error",
                    &error.to_string(),
                )
            }
        }
    }

    /// Python `validateConfig`.
    pub fn validate_config(&mut self, core: &str, text: &str) -> Value {
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let (ok, message) = if core == "singbox" {
            controller.validate_singbox_json_text(text)
        } else if core == "xray" {
            controller.validate_xray_json_text(text)
        } else {
            return json!({"statusLevel": "error", "statusMessage": "Неизвестное ядро"});
        };
        if ok {
            self.notify("success", "JSON корректен");
        }
        json!({
            "statusLevel": if ok { "success" } else { "error" },
            "statusMessage": message,
        })
    }

    /// Python `applyConfig`.
    pub fn apply_config(&mut self, core: &str, text: &str, previous_text: &str) -> Value {
        if core != "singbox" && core != "xray" {
            return json!({
                "statusLevel": "error",
                "statusMessage": "Неизвестное ядро",
                "fileLabel": "",
            });
        }
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let applied = if core == "singbox" {
            controller.apply_singbox_config_text(
                text,
                if previous_text.is_empty() {
                    None
                } else {
                    Some(previous_text)
                },
            )
        } else {
            controller.apply_xray_config_text(text)
        };
        match applied {
            Ok(result) => {
                let level = if result.message.contains("Применяю") {
                    "info"
                } else {
                    "success"
                };
                let first_line = result
                    .message
                    .lines()
                    .next()
                    .unwrap_or("Применено")
                    .to_string();
                self.notify(level, &self.localize(&first_line));
                json!({
                    "statusLevel": level,
                    "statusMessage": self.localize(&result.message),
                    "fileLabel": result
                        .path
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default(),
                })
            }
            Err(error) => {
                let first_line = error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                self.notify("error", &first_line);
                json!({
                    "statusLevel": "error",
                    "statusMessage": error.to_string(),
                    "fileLabel": "",
                })
            }
        }
    }

    // ── clipboard imports (the shell passes the text/paths) ────────────────

    /// Python `importClipboard` minus the clipboard read: the shell hands the
    /// text (already split from dropped files).
    pub fn import_clipboard_text(&mut self, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            self.notify("warning", "Буфер обмена пуст");
            return;
        }
        if text.to_lowercase().starts_with("lumen:") {
            self.handle_deep_link(&text);
            return;
        }
        // HTTP proxy share links contain credentials or an explicit endpoint
        // port. Ordinary web URLs belong to the subscription fetch pipeline.
        let mut lines: Vec<String> = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() && !lines.contains(&trimmed) {
                lines.push(trimmed);
            }
        }
        let mut subscription_urls: Vec<String> = Vec::new();
        for line in &lines {
            if let Ok(parsed) = url::Url::parse(line) {
                let scheme = parsed.scheme();
                let is_subscription = matches!(scheme, "happ" | "incy")
                    || ((scheme == "http" || scheme == "https")
                        && parsed.username().is_empty()
                        && parsed.password().is_none()
                        && !(parsed.port().is_some()
                            && (parsed.path().is_empty() || parsed.path() == "/")
                            && parsed.query().unwrap_or_default().is_empty())
                        && !line.chars().any(char::is_whitespace));
                if is_subscription {
                    subscription_urls.push(line.clone());
                }
            }
        }
        let mut remaining_text = text.clone();
        if !subscription_urls.is_empty() {
            if subscription_urls.len() == 1 {
                self.import_subscription(&subscription_urls[0], "");
            } else {
                self.import_subscriptions(&subscription_urls.join("\n"));
            }
            remaining_text = lines
                .iter()
                .filter(|line| !subscription_urls.contains(line))
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if remaining_text.is_empty() {
                return;
            }
        }
        self.import_nodes_from_text(&remaining_text);
    }

    /// Python `importNodeFile` / `importNodeFiles` shared body.
    pub fn import_nodes_from_text(&mut self, text: &str) {
        let target_group = {
            let group = self.filter_group.trim().to_string();
            if group.is_empty() { None } else { Some(group) }
        };
        let existing_ids: HashSet<String> = self
            .controller
            .lock()
            .state
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect();
        let (added, errors) = self.controller.lock().import_nodes_from_text(
            text,
            target_group.as_deref(),
            None,
            false,
        );
        if added > 0 {
            let imported_id = {
                let controller = self.controller.lock();
                controller
                    .state
                    .nodes
                    .iter()
                    .find(|node| !existing_ids.contains(&node.id))
                    .map(|node| node.id.clone())
                    .unwrap_or_default()
            };
            if !imported_id.is_empty() {
                self.outbox.push_back(UiEvent::NodeImported(imported_id));
            }
            self.notify(
                "success",
                &tr(
                    "Импортировано серверов: {count}",
                    &[("count", &added.to_string())],
                ),
            );
        }
        if !errors.is_empty() {
            self.notify("warning", &join_messages(&errors, 3));
        }
        if added == 0 && errors.is_empty() {
            self.notify("warning", "Новых серверов не импортировано");
        }
    }

    /// Python `importNodeFiles`.
    pub fn import_node_files(&mut self, paths: &[PathBuf]) {
        const SUPPORTED: [&str; 6] = ["conf", "ovpn", "txt", "json", "yaml", "yml"];
        let mut valid: Vec<PathBuf> = Vec::new();
        let mut invalid_count = 0usize;
        for path in paths {
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase())
                .unwrap_or_default();
            if !SUPPORTED.contains(&extension.as_str()) || !path.is_file() {
                invalid_count += 1;
                continue;
            }
            if !valid.contains(path) {
                valid.push(path.clone());
            }
        }
        if valid.is_empty() {
            self.notify(
                "warning",
                &tr("Перетащите поддерживаемый файл конфигурации", &[]),
            );
            return;
        }
        let mut total_added = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for path in &valid {
            let (added, file_errors) = self.controller.lock().import_nodes_from_text(
                &path.to_string_lossy(),
                Some(self.filter_group.trim()).filter(|group| !group.is_empty()),
                None,
                false,
            );
            total_added += added;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            errors.extend(
                file_errors
                    .into_iter()
                    .map(|error| format!("{name}: {error}")),
            );
        }
        if total_added > 0 {
            let imported_id = {
                let controller = self.controller.lock();
                let existing: HashSet<String> = controller
                    .state
                    .nodes
                    .iter()
                    .map(|node| node.id.clone())
                    .collect();
                // Existing ids were captured before the import; the first
                // unseen id is the imported one.
                controller
                    .state
                    .nodes
                    .iter()
                    .find(|node| !existing.contains(&node.id))
                    .map(|node| node.id.clone())
                    .unwrap_or_default()
            };
            if !imported_id.is_empty() {
                self.outbox.push_back(UiEvent::NodeImported(imported_id));
            }
            self.notify(
                "success",
                &tr(
                    "Импортировано серверов: {count}",
                    &[("count", &total_added.to_string())],
                ),
            );
        }
        if invalid_count > 0 {
            errors.push(tr(
                "Пропущено неподдерживаемых файлов: {count}",
                &[("count", &invalid_count.to_string())],
            ));
        }
        if !errors.is_empty() {
            self.notify("warning", &join_messages(&errors, 3));
        }
        if total_added == 0 && errors.is_empty() {
            self.notify("warning", &tr("Новых серверов не импортировано", &[]));
        }
    }

    // ── Node table layout / filters ────────────────────────────────────────

    /// Python `setNodeSort`.
    pub fn set_node_sort(&mut self, key: &str, ascending: bool) {
        let old_key = self.sort_key.clone();
        let old_asc = self.sort_asc;
        self.sort_key = if key.is_empty() {
            "manual".to_string()
        } else {
            key.to_string()
        };
        self.sort_asc = ascending;
        {
            let mut controller = self.controller.lock();
            if controller.state.settings.node_sort_key != self.sort_key
                || controller.state.settings.node_sort_ascending != self.sort_asc
            {
                controller.state.settings.node_sort_key = self.sort_key.clone();
                controller.state.settings.node_sort_ascending = self.sort_asc;
                controller.schedule_save();
            }
        }
        if old_key != self.sort_key || old_asc != self.sort_asc {
            self.changes.node_sort = true;
        }
        self.apply_node_model();
        self.dashboard_nodes.set_sort_key(&self.sort_key);
    }

    /// Python `setNodeFilter`.
    pub fn set_node_filter(&mut self, group: &str, text: &str) {
        let old_group = self.filter_group.clone();
        self.filter_group = group.to_string();
        self.filter_text = text.to_string();
        {
            let mut controller = self.controller.lock();
            if controller.state.settings.node_filter_group != self.filter_group {
                controller.state.settings.node_filter_group = self.filter_group.clone();
                controller.schedule_save();
            }
        }
        if old_group != self.filter_group {
            self.changes.node_filter = true;
        }
        self.apply_node_model();
    }

    /// Python `_apply_node_model`.
    fn apply_node_model(&mut self) {
        let (nodes, selected) = {
            let controller = self.controller.lock();
            self.node_model.set_runtime_support(true);
            (
                self.sorted_nodes(self.filtered_nodes(&controller.state.nodes)),
                controller.state.selected_node_id.clone(),
            )
        };
        self.nodes_cached = nodes.clone();
        self.node_model.set_nodes(nodes, selected.as_deref());
    }

    /// Python `nodeSortKey`.
    pub fn node_sort_key(&self) -> String {
        self.sort_key.clone()
    }

    /// Python `nodeSortAscending`.
    pub fn node_sort_ascending(&self) -> bool {
        self.sort_asc
    }

    /// Python `nodeFilterGroup`.
    pub fn node_filter_group(&self) -> String {
        self.filter_group.clone()
    }

    /// Python `selectedSubscriptionId`.
    pub fn selected_subscription_id(&self) -> String {
        self.settings_cached.selected_subscription_id.clone()
    }

    /// Python `setSelectedSubscriptionId`.
    pub fn set_selected_subscription_id(&mut self, subscription_id: &str) {
        let value = subscription_id.trim().to_string();
        let mut controller = self.controller.lock();
        if controller.state.settings.selected_subscription_id == value {
            drop(controller);
            self.settings_cached.selected_subscription_id = value;
            return;
        }
        controller.state.settings.selected_subscription_id = value.clone();
        controller.schedule_save();
        drop(controller);
        // `selected_subscription_id()` reads the cached settings; without
        // this the source picker and inspector kept the old selection until
        // an unrelated settings refresh.
        self.settings_cached.selected_subscription_id = value;
        self.changes.selected_subscription = true;
    }

    /// Python `nodeTableLayout`.
    pub fn node_table_layout(&self) -> Value {
        self.settings_cached.node_table_layout.clone()
    }

    /// Python `setNodeTableLayout`.
    pub fn set_node_table_layout(&mut self, layout: &Value) {
        let Some(layout) = layout.as_object() else {
            return;
        };
        const ALLOWED: [&str; 11] = [
            "manual",
            "name",
            "type",
            "transport",
            "address",
            "port",
            "group",
            "ping",
            "speed",
            "status",
            "last",
        ];
        let mut normalized = serde_json::Map::new();
        for (key, value) in layout {
            if !ALLOWED.contains(&key.as_str()) {
                continue;
            }
            if key == "manual" {
                normalized.insert(key.clone(), json!(value.as_bool().unwrap_or(false)));
                continue;
            }
            if let Some(number) = value.as_f64() {
                normalized.insert(key.clone(), json!(number.clamp(48.0, 2000.0)));
            }
        }
        let mut controller = self.controller.lock();
        if controller.state.settings.node_table_layout == Value::Object(normalized.clone()) {
            return;
        }
        controller.state.settings.node_table_layout = Value::Object(normalized);
        controller.schedule_save();
        drop(controller);
        self.changes.node_table_layout = true;
    }

    /// Python `zapretTableLayout`.
    pub fn zapret_table_layout(&self) -> Value {
        self.settings_cached.zapret_table_layout.clone()
    }

    /// Python `setZapretTableLayout`.
    pub fn set_zapret_table_layout(&mut self, layout: &Value) {
        let Some(layout) = layout.as_object() else {
            return;
        };
        const ALLOWED: [&str; 5] = ["manual", "name", "description", "args", "date"];
        let mut normalized = serde_json::Map::new();
        for (key, value) in layout {
            if !ALLOWED.contains(&key.as_str()) {
                continue;
            }
            if key == "manual" {
                normalized.insert(key.clone(), json!(value.as_bool().unwrap_or(false)));
                continue;
            }
            if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
                normalized.insert(key.clone(), json!(number.clamp(48.0, 2000.0)));
            }
        }
        let mut controller = self.controller.lock();
        if controller.state.settings.zapret_table_layout == Value::Object(normalized.clone()) {
            return;
        }
        controller.state.settings.zapret_table_layout = Value::Object(normalized);
        controller.schedule_save();
        drop(controller);
        self.changes.zapret_table_layout = true;
    }

    // ── Groups ─────────────────────────────────────────────────────────────

    /// Python `createManualGroup`.
    pub fn create_manual_group(&mut self, name: &str) -> bool {
        let group = name.trim().to_string();
        if group.is_empty() {
            self.notify("warning", &tr("Введите имя группы", &[]));
            return false;
        }
        let mut existing: HashSet<String> = HashSet::from(["default".to_string()]);
        {
            let controller = self.controller.lock();
            for item in &controller.state.manual_groups {
                existing.insert(item.trim().to_lowercase());
            }
            for node in &controller.state.nodes {
                let group = if node.group.is_empty() {
                    "Default"
                } else {
                    node.group.as_str()
                };
                existing.insert(group.trim().to_lowercase());
            }
        }
        if existing.contains(&group.to_lowercase()) {
            self.notify("info", &tr("Такая группа уже есть", &[]));
            return false;
        }
        {
            let mut controller = self.controller.lock();
            controller.state.manual_groups.push(group.clone());
            controller.save();
        }
        self.changes.node_filters = true;
        self.notify(
            "success",
            &tr("Группа создана: {name}", &[("name", &group)]),
        );
        true
    }

    /// Python `deleteGroup`.
    pub fn delete_group(&mut self, name: &str) -> bool {
        let group = name.trim().to_string();
        if group.is_empty() || group.to_lowercase() == "default" {
            return false;
        }
        if !self.controller.lock().delete_group(&group) {
            return false;
        }
        self.changes.node_filters = true;
        self.notify(
            "success",
            &tr("Группа удалена: {name}", &[("name", &group)]),
        );
        true
    }

    /// Python `groupOptions`.
    pub fn group_options(&self) -> Vec<String> {
        let mut seen: Vec<String> = vec!["Default".to_string()];
        let mut seen_keys: HashSet<String> = HashSet::from(["default".to_string()]);
        for group in &self.manual_groups_cached {
            let group = group.trim().to_string();
            let key = group.to_lowercase();
            if !group.is_empty() && !seen_keys.contains(&key) {
                seen.push(group);
                seen_keys.insert(key);
            }
        }
        for node in &self.nodes_cached {
            let group = if node.group.trim().is_empty() {
                "Default".to_string()
            } else {
                node.group.trim().to_string()
            };
            let key = group.to_lowercase();
            if !seen_keys.contains(&key) {
                seen.push(group);
                seen_keys.insert(key);
            }
        }
        let mut rest = seen[1..].to_vec();
        rest.sort_by_key(|group| group.to_lowercase());
        let mut result = vec!["Default".to_string()];
        result.extend(rest);
        result
    }

    // ── Diagnostics ────────────────────────────────────────────────────────

    /// Python `exportDiagnostics` (all sections, no upload).
    pub fn export_diagnostics(&mut self) {
        self.start_diagnostics_export(None, false);
    }

    /// Python `runDiagnosticsExport`.
    pub fn run_diagnostics_export(&mut self, options: &Value) {
        let opts = options.as_object().cloned().unwrap_or_default();
        let flag = |key: &str, default: bool| -> bool {
            opts.get(key).and_then(Value::as_bool).unwrap_or(default)
        };
        let include = json!({
            "errors": flag("errors", true),
            "core": flag("core", true),
            "app": flag("app", true),
            "traffic": flag("traffic", true),
            "state": flag("state", true),
            "recent": flag("recent", true),
            "network": opts.get("network").and_then(Value::as_bool).unwrap_or(false),
        });
        let upload = flag("upload", false);
        let any = include
            .as_object()
            .unwrap()
            .values()
            .any(|value| value.as_bool() == Some(true));
        if !any {
            self.notify(
                "error",
                &tr("Выберите хотя бы один раздел для экспорта", &[]),
            );
            return;
        }
        self.start_diagnostics_export(Some(include), upload);
    }

    /// Python `_start_diagnostics_export`.
    fn start_diagnostics_export(&mut self, include: Option<Value>, upload: bool) {
        if self.diagnostics_exporting || self.quitting {
            return;
        }
        self.diagnostics_exporting = true;
        let controller = self.controller.clone();
        self.runtime.spawn(async move {
            let mut guard = controller.lock();
            guard.build_diagnostics(
                include,
                upload,
                None,
                Some(Box::new(move |controller, result| {
                    controller.report_diagnostics(result);
                })),
            );
        });
    }

    /// Python `_on_diagnostics_finished`.
    fn on_diagnostics_finished(&mut self, result: Result<PathBuf, String>) {
        self.diagnostics_exporting = false;
        if self.quitting {
            return;
        }
        let path = match result {
            Ok(path) => path,
            Err(error) => {
                self.notify(
                    "error",
                    &tr(
                        "Не удалось собрать диагностику: {error}",
                        &[("error", &error)],
                    ),
                );
                return;
            }
        };
        if let Some(directory) = path.parent() {
            self.outbox
                .push_back(UiEvent::OpenDirectory(directory.to_path_buf()));
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        self.notify(
            "success",
            &tr("Диагностика сохранена: {name}", &[("name", &name)]),
        );
    }

    // ── Node rows / details ────────────────────────────────────────────────

    /// Python `nodeIdAt`.
    pub fn node_id_at(&self, row: usize) -> String {
        self.node_model.node_id_at(row).unwrap_or("").to_string()
    }

    /// Python `nodeIndexById`.
    pub fn node_index_by_id(&self, node_id: &str) -> i64 {
        self.node_model.index_of_id(node_id)
    }

    /// Python `quickConnectNodeId`.
    pub fn quick_connect_node_id(&self) -> String {
        self.node_model.best_ping_node_id()
    }

    /// Python `nodeDetails` — everything the servers inspector shows about one
    /// node.
    pub fn node_details(&self, node_id: &str) -> Value {
        let key = format!("node_details:{node_id}");
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| json!({}));
        }
        let value = self.node_details_now(node_id);
        self.remember_read(key, value.clone());
        value
    }

    fn node_details_now(&self, node_id: &str) -> Value {
        let controller = self.controller.lock();
        let Some(node) = controller
            .state
            .nodes
            .iter()
            .find(|node| node.id == node_id)
        else {
            return json!({});
        };
        let outbound = if node.outbound.is_object() {
            node.outbound.as_object().unwrap().clone()
        } else {
            Default::default()
        };
        let stream = outbound
            .get("streamSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let native = outbound
            .get("singbox")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let native_tls = native
            .get("tls")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut security = stream
            .get("security")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if security.is_empty() && !native_tls.is_empty() {
            let reality = native_tls.get("reality");
            security =
                if reality.map(Value::is_object).unwrap_or(false) && reality != Some(&json!({})) {
                    "reality".to_string()
                } else if native_tls
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "tls".to_string()
                } else {
                    String::new()
                };
        }
        let sec = stream
            .get(&format!("{security}Settings"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let utls = native_tls
            .get("utls")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let sni = if sec
            .get("serverName")
            .is_some_and(|value| !value.as_str().unwrap_or_default().is_empty())
        {
            text_or_empty(sec.get("serverName"))
        } else {
            text_or_empty(native_tls.get("server_name"))
        };
        let fingerprint = if sec
            .get("fingerprint")
            .is_some_and(|value| !value.as_str().unwrap_or_default().is_empty())
        {
            text_or_empty(sec.get("fingerprint"))
        } else {
            text_or_empty(utls.get("fingerprint"))
        };
        let mut subscription = String::new();
        if !node.subscription_id.is_empty() {
            for sub in &controller.state.subscriptions {
                if sub.get("id").and_then(Value::as_str) == Some(node.subscription_id.as_str()) {
                    subscription = sub
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .or_else(|| sub.get("group").and_then(Value::as_str))
                        .unwrap_or("")
                        .to_string();
                    break;
                }
            }
        }
        let tail = |history: &[(String, Option<i64>)]| -> Vec<f64> {
            history
                .iter()
                .rev()
                .take(16)
                .rev()
                .map(|(_, value)| value.map(|value| value as f64).unwrap_or(-1.0))
                .collect()
        };
        json!({
            "id": node.id,
            "name": if node.name.is_empty() { node.server.clone() } else { node.name.clone() },
            "protocol": lumen_core::node_transport::node_protocol_label(node),
            "transport": lumen_core::node_transport::node_transport(node),
            "server": node.server,
            "port": node.port,
            "security": match security.as_str() {
                "reality" => "Reality".to_owned(),
                "tls" => "TLS".to_owned(),
                other => other.to_uppercase(),
            },
            "sni": sni,
            "fingerprint": fingerprint,
            "group": if node.group.is_empty() { "Default" } else { node.group.as_str() },
            "subscription": subscription,
            "ping": node.ping_ms.unwrap_or(-1),
            "pingKind": node.ping_kind,
            "speed": node.speed_mbps.unwrap_or(-1.0),
            "alive": match node.is_alive {
                None => "",
                Some(true) => "ok",
                Some(false) => "fail",
            },
            "lastUsed": node.last_used_at.clone().unwrap_or_default(),
            "description": node.description,
            "country": node.country_code.to_uppercase(),
            "pingHistory": tail(&node.ping_history),
            "speedHistory": node
                .speed_history
                .iter()
                .rev()
                .take(16)
                .rev()
                .map(|(_, value)| value.unwrap_or(-1.0))
                .collect::<Vec<_>>(),
        })
    }

    // ── Lock / connectivity ────────────────────────────────────────────────

    /// Python `lockNow`.
    pub fn lock_now(&mut self) {
        self.controller.lock().lock();
    }

    /// Python `unlock`.
    pub fn unlock(&self, password: &str) -> bool {
        self.controller.lock().unlock(password)
    }

    /// Python `testConnectivity`.
    pub fn test_connectivity(&mut self, url: &str) {
        let url = if url.is_empty() { None } else { Some(url) };
        self.controller.lock().test_connectivity(url);
    }

    /// Python `runToastAction`.
    pub fn run_toast_action(&mut self, action_id: &str) {
        if action_id == "restart-admin" {
            self.on_admin_relaunch();
            return;
        }
        if action_id == "reset-system-proxy" {
            let result = self.controller.lock().proxy.reset_conflicted_state();
            match result {
                Ok(()) => {
                    self.notify(
                        "success",
                        &tr("Состояние прокси сброшено. Повторите подключение.", &[]),
                    );
                }
                Err(error) => {
                    self.notify(
                        "error",
                        &tr(
                            "Не удалось сбросить прокси: {error}",
                            &[("error", &error.to_string())],
                        ),
                    );
                }
            }
            return;
        }
        let Some(occupied_text) = action_id.strip_prefix("change-port:") else {
            return;
        };
        let Ok(occupied) = occupied_text.parse::<i64>() else {
            return;
        };
        self.change_local_port(occupied);
    }

    /// Python `runToastAction` change-port branch.
    fn change_local_port(&mut self, occupied: i64) {
        let controller = self.controller.clone();
        let mut controller = controller.lock();
        let result = (|| -> Result<i64, String> {
            let (path, text) = controller
                .load_active_xray_config_text()
                .map_err(|error| error.to_string())?;
            let mut payload: Value =
                serde_json::from_str(&text).map_err(|error| error.to_string())?;
            let inbounds = payload
                .get_mut("inbounds")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| {
                    "В активном Xray-конфиге нет локальных входящих портов".to_string()
                })?;
            let mut used: HashSet<i64> = inbounds
                .iter()
                .filter_map(|item| item.get("port").and_then(Value::as_i64))
                .collect();
            used.insert(occupied);
            let new_port = find_free_local_port(occupied + 1, &used)
                .ok_or_else(|| "Не удалось найти свободный локальный порт".to_string())?;
            let mut changed = false;
            for inbound in inbounds.iter_mut() {
                if inbound.get("port").and_then(Value::as_i64) == Some(occupied) {
                    inbound["port"] = json!(new_port);
                    changed = true;
                }
            }
            if !changed {
                return Err(format!("Порт {occupied} не найден в активном Xray-конфиге"));
            }
            controller
                .save_xray_config_text(
                    &lumen_core::pyjson::dumps(&payload, false, Some(2)),
                    Some(&path),
                )
                .map_err(|error| error.to_string())?;
            Ok(new_port)
        })();
        match result {
            Ok(new_port) => {
                if controller.connected || controller.runtime.desired_connected {
                    controller.request_transition("local proxy port changed");
                }
                self.notify(
                    "success",
                    &tr(
                        "Порт изменён: {old} → {new}",
                        &[
                            ("old", &occupied.to_string()),
                            ("new", &new_port.to_string()),
                        ],
                    ),
                );
            }
            Err(error) => {
                self.notify(
                    "error",
                    &tr("Не удалось сменить порт: {error}", &[("error", &error)]),
                );
            }
        }
    }

    // ── clipboard / links helpers ──────────────────────────────────────────

    /// Python `copyText` — returns the payload for the shell clipboard.
    pub fn copy_text(&mut self, text: &str) -> Option<String> {
        let payload = text.trim().to_string();
        if payload.is_empty() {
            return None;
        }
        self.notify("success", "Скопировано в буфер обмена");
        Some(payload)
    }

    /// Python `_copy_or_warn`.
    pub fn copy_or_warn(&mut self, payload: Option<String>) -> Option<String> {
        match payload {
            Some(payload) if !payload.is_empty() => {
                self.notify("success", "JSON скопирован в буфер обмена");
                Some(payload)
            }
            _ => {
                self.notify("warning", "Выберите сервер для экспорта");
                None
            }
        }
    }

    /// Python `copyOutboundJson`.
    pub fn node_json_for_export(&mut self, node_id: &str, runtime: bool) -> Option<(String, &'static str)> {
        let node = (!node_id.is_empty()).then_some(node_id);
        let (payload, name) = {
            let mut controller = self.controller.lock();
            if runtime {
                let settings = &controller.state.settings;
                let selected = node.and_then(|id| controller.state.nodes.iter().find(|node| node.id == id));
                let singbox = controller.is_singbox_editor_mode() || (!settings.tun_mode && lumen_app::node_runtime_service::proxy_core_for_node(selected, &settings.core_preference) == "singbox");
                (controller.export_runtime_config_json(node), if singbox { "singbox_config.json" } else { "xray_config.json" })
            } else { (controller.export_node_outbound_json(node), "outbound.json") }
        };
        if payload.is_none() { self.notify("warning", &tr("Выберите сервер для экспорта", &[])); }
        payload.map(|payload| (payload, name))
    }

    pub fn finish_json_export(&mut self, path: anyhow::Result<Option<PathBuf>>, payload: &str) -> bool {
        match path {
            Ok(None) => {
                self.notify("info", &tr("Экспорт отменён, JSON скопирован в буфер обмена", &[]));
                true
            }
            Ok(Some(path)) => {
                match std::fs::write(&path, payload) {
                    Ok(()) => self.notify("success", &tr("JSON экспортирован: {path}", &[("path", &path.to_string_lossy())])),
                    Err(error) => self.notify("error", &tr("Не удалось сохранить файл: {error}", &[("error", &error.to_string())])),
                }
                false
            }
            Err(error) => {
                self.notify("error", &tr("Не удалось сохранить файл: {error}", &[("error", &error.to_string())]));
                false
            }
        }
    }

    /// Python `copyOutboundJson`.
    pub fn copy_outbound_json(&mut self, node_id: &str) -> Option<String> {
        let payload = self
            .controller
            .lock()
            .export_node_outbound_json(if node_id.is_empty() {
                None
            } else {
                Some(node_id)
            });
        self.copy_or_warn(payload)
    }

    /// Python `copyRuntimeJson`.
    pub fn copy_runtime_json(&mut self, node_id: &str) -> Option<String> {
        let payload = self
            .controller
            .lock()
            .export_runtime_config_json(if node_id.is_empty() {
                None
            } else {
                Some(node_id)
            });
        self.copy_or_warn(payload)
    }

    /// Python `countryFlagSource`.
    pub fn country_flag_source(&self, code: &str) -> String {
        flag_svg_data_uri(code)
    }

    // ── Subscriptions ──────────────────────────────────────────────────────

    /// Python `cancelSubscriptionImport`.
    pub fn cancel_subscription_import(&mut self) {
        self.controller.lock().cancel_subscription_import();
    }

    /// Python `importSubscription`.
    pub fn import_subscription(&mut self, url: &str, name: &str) {
        let target = url.trim().to_string();
        if target.is_empty() {
            self.notify("warning", &tr("Введите ссылку на подписку", &[]));
            return;
        }
        let name = name.trim().to_string();
        let _result = self
            .controller
            .lock()
            .import_subscription(&target, if name.is_empty() { None } else { Some(&name) });
    }

    /// Python `handleDeepLink`.
    pub fn handle_deep_link(&mut self, value: &str) -> bool {
        match lumen_core::deeplinks::parse_lumen_deep_link(value) {
            Err(_error) => {
                self.notify("warning", &tr("Некорректная ссылка lumen", &[]));
                false
            }
            Ok(None) => false,
            Ok(Some(request)) => {
                self.import_subscription(&request.url, &request.name);
                true
            }
        }
    }

    /// Python `importSubscriptions`.
    pub fn import_subscriptions(&mut self, text: &str) {
        self.controller.lock().import_subscriptions(text);
    }

    /// Python `updateSubscription`.
    pub fn update_subscription(&mut self, url: &str) {
        let target = url.trim().to_string();
        if target.is_empty() {
            return;
        }
        // The dispatcher reports the "updating" status itself; a second
        // notification here showed the toast twice.
        let _ = self.controller.lock().update_subscription(&target);
    }

    /// Python `updateAllSubscriptions`.
    pub fn update_all_subscriptions(&mut self) {
        // The dispatcher reports the "updating" status itself.
        let _ = self.controller.lock().update_all_subscriptions();
    }

    /// Python `removeSubscription`.
    pub fn remove_subscription(&mut self, url: &str, delete_nodes: bool) {
        let target = url.trim().to_string();
        if target.is_empty() {
            return;
        }
        self.controller
            .lock()
            .remove_subscription(&target, delete_nodes);
        self.notify("info", "Подписка удалена");
    }

    /// Python `subscriptions` — presentation-decorated subscription rows.
    pub fn subscriptions(&self) -> Vec<Value> {
        let mut rows: Vec<Value> = Vec::new();
        for item in &self.subscriptions_cached {
            if !item.is_object() {
                continue;
            }
            let mut row = item.clone();
            row["presentation"] =
                lumen_core::subscription::presentation::present_subscription(item, None);
            rows.push(row);
        }
        rows.sort_by_key(|row| !row["presentation"]["pinned"].as_bool().unwrap_or(false));
        rows
    }

    /// Python `subscriptionUrlsText`.
    pub fn subscription_urls_text(&self) -> String {
        self.subscriptions_cached
            .iter()
            .filter_map(|item| item.get("url").and_then(Value::as_str))
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ── subscription settings mirrors ──────────────────────────────────────

    /// Python `setSubscriptionAutoUpdateMinutes`.
    pub fn set_subscription_auto_update_minutes(&mut self, value: i64) {
        let minutes = value.clamp(0, 1440);
        self.update_settings_field(|settings| settings.subscription_auto_update_minutes = minutes);
    }

    /// Python `setSubscriptionIncludeRegex`.
    pub fn set_subscription_include_regex(&mut self, value: &str) {
        self.update_settings_field(|settings| {
            settings.subscription_include_regex = value.trim().to_string()
        });
    }

    /// Python `setSubscriptionExcludeRegex`.
    pub fn set_subscription_exclude_regex(&mut self, value: &str) {
        self.update_settings_field(|settings| {
            settings.subscription_exclude_regex = value.trim().to_string()
        });
    }

    /// Python `setSubscriptionUserAgent`.
    pub fn set_subscription_user_agent(&mut self, value: &str) {
        self.update_settings_field(|settings| {
            settings.subscription_user_agent = value.trim().to_string()
        });
    }

    /// Python `setSubscriptionUseRealHwid`.
    pub fn set_subscription_use_real_hwid(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.subscription_use_real_hwid = enabled);
    }

    /// Python `setSubscriptionUseProxyTun`.
    pub fn set_subscription_use_proxy_tun(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.subscription_use_proxy_tun = enabled);
    }

    /// Python `setSubscriptionHwid`.
    pub fn set_subscription_hwid(&mut self, value: &str) {
        self.update_settings_field(|settings| {
            settings.subscription_hwid = value.trim().to_string()
        });
    }

    /// Python `setSubscriptionConverterEnabled`.
    pub fn set_subscription_converter_enabled(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.subscription_converter_enabled = enabled);
    }

    /// Python `setSubscriptionConverterUrl`.
    pub fn set_subscription_converter_url(&mut self, value: &str) {
        self.update_settings_field(|settings| {
            settings.subscription_converter_url = value.trim().to_string()
        });
    }

    // ── Auto-connect / server-test settings ────────────────────────────────

    /// Python `setAutoConnectLast`.
    pub fn set_auto_connect_last(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.auto_connect_last = enabled);
    }

    /// Python `setAutoConnectOnImport`.
    pub fn set_auto_connect_on_import(&mut self, enabled: bool) {
        self.update_settings_field(|settings| settings.auto_connect_on_import = enabled);
    }

    /// Python `setPingMethod`.
    pub fn set_ping_method(&mut self, method: &str) {
        let value = match method.trim().to_lowercase().as_str() {
            "icmp" | "http" | "real" => method.trim().to_lowercase(),
            _ => "tcping".to_string(),
        };
        self.update_settings_field(|settings| settings.ping_method = value);
    }

    /// Python `setSpeedTestUrl`.
    pub fn set_speed_test_url(&mut self, url: &str) {
        self.update_settings_field(|settings| settings.speed_test_url = url.trim().to_string());
    }

    /// Python `setSpeedTestConcurrency`.
    pub fn set_speed_test_concurrency(&mut self, value: i64) {
        let count = value.clamp(0, SPEED_TEST_MAX_CONCURRENCY as i64);
        self.update_settings_field(|settings| settings.speed_test_concurrency = count);
    }

    // ── Routing detail: commands ───────────────────────────────────────────

    fn mutate_routing(&mut self, mutate: impl FnOnce(&mut RoutingSettings)) {
        let mut routing = self.controller.lock().state.routing.clone();
        mutate(&mut routing);
        self.controller.lock().update_routing(routing, true);
    }

    /// Python `setBypassLan`.
    pub fn set_bypass_lan(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.bypass_lan = enabled);
    }

    /// Python `setTunDefaultOutbound`.
    pub fn set_tun_default_outbound(&mut self, outbound: &str) {
        let value = outbound.trim().to_lowercase();
        if value != "proxy" && value != "direct" {
            return;
        }
        self.mutate_routing(|routing| {
            routing.tun_default_outbound = value;
            routing.tun_default_outbound_user_selected = true;
            routing.mode = "rule".into();
            // Keep preset_id: regional domain/rule-set membership is derived
            // from it, while this value only changes the unmatched fallback.
        });
    }

    /// Python `setDnsMode`.
    pub fn set_dns_mode(&mut self, mode: &str) {
        self.mutate_routing(|routing| {
            routing.dns_mode = if mode.is_empty() {
                "system".into()
            } else {
                mode.into()
            };
        });
    }

    /// Python `setBootstrapDns`.
    pub fn set_bootstrap_dns(&mut self, server: &str, dns_type: &str) {
        self.mutate_routing(|routing| {
            if !server.is_empty() {
                routing.dns_bootstrap_server = server.trim().to_string();
                let mut servers = vec![routing.dns_bootstrap_server.clone()];
                servers.extend(routing.dns_bootstrap_servers.iter().skip(1).cloned());
                routing.dns_bootstrap_servers = servers;
            }
            if !dns_type.is_empty() {
                routing.dns_bootstrap_type = dns_type.to_string();
            }
        });
    }

    /// Python `setProxyDns`.
    pub fn set_proxy_dns(&mut self, server: &str, dns_type: &str) {
        self.mutate_routing(|routing| {
            if !server.is_empty() {
                routing.dns_proxy_server = server.trim().to_string();
                let mut servers = vec![routing.dns_proxy_server.clone()];
                servers.extend(routing.dns_proxy_servers.iter().skip(1).cloned());
                routing.dns_proxy_servers = servers;
            }
            if !dns_type.is_empty() {
                routing.dns_proxy_type = dns_type.to_string();
            }
        });
    }

    /// Python `setDnsBootstrapStrategy`.
    pub fn set_dns_bootstrap_strategy(&mut self, value: &str) {
        let value = if value.is_empty() { "ipv4_only" } else { value };
        self.mutate_routing(|routing| routing.dns_bootstrap_strategy = value.to_string());
    }

    /// Python `setDnsProxyStrategy`.
    pub fn set_dns_proxy_strategy(&mut self, value: &str) {
        let value = if value.is_empty() { "ipv4_only" } else { value };
        self.mutate_routing(|routing| routing.dns_proxy_strategy = value.to_string());
    }

    /// Python `setDnsFakeEnabled`.
    pub fn set_dns_fake_enabled(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.dns_fake_enabled = enabled);
    }

    /// Python `setDnsHijackEnabled`.
    pub fn set_dns_hijack_enabled(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.dns_hijack_enabled = enabled);
    }

    /// Python `setTunRouteExcludeAddress`.
    pub fn set_tun_route_exclude_address(&mut self, text: &str) {
        self.mutate_routing(|routing| {
            let mut items: Vec<String> = Vec::new();
            for chunk in text.replace(',', "\n").replace(';', "\n").lines() {
                let value = chunk.trim().to_string();
                if !value.is_empty() {
                    items.push(value);
                }
            }
            routing.tun_route_exclude_address = items;
        });
    }

    /// Python `setServiceRoute`.
    pub fn set_service_route(&mut self, service_id: &str, action: &str) {
        self.mutate_routing(|routing| {
            if action.is_empty() || action == "off" {
                routing.service_routes.remove(service_id);
            } else if action == "proxy" || action == "direct" {
                routing
                    .service_routes
                    .insert(service_id.to_string(), action.to_string());
            } else {
                routing.service_routes.remove(service_id);
            }
        });
    }

    /// Python `setProcessPresetRoute`.
    pub fn set_process_preset_route(&mut self, preset_id: &str, action: &str) {
        self.mutate_routing(|routing| {
            if action.is_empty() || action == "off" {
                routing.process_preset_routes.remove(preset_id);
            } else {
                routing
                    .process_preset_routes
                    .insert(preset_id.to_string(), action.to_string());
            }
        });
    }

    /// Python `addProcessRule`.
    pub fn add_process_rule(&mut self, process: &str, action: &str) {
        let process = process.trim().to_string();
        if process.is_empty() {
            return;
        }
        self.mutate_routing(|routing| {
            let mut rules: Vec<Value> = routing
                .process_rules
                .iter()
                .filter(|rule| rule.get("process").and_then(Value::as_str) != Some(process.as_str()))
                .cloned()
                .collect();
            rules.push(json!({"process": process, "action": if action.is_empty() { "proxy" } else { action }}));
            routing.process_rules = rules;
        });
    }

    /// Python `removeProcessRule`.
    pub fn remove_process_rule(&mut self, process: &str) {
        self.mutate_routing(|routing| {
            routing.process_rules = routing
                .process_rules
                .iter()
                .filter(|rule| rule.get("process").and_then(Value::as_str) != Some(process))
                .cloned()
                .collect();
        });
    }

    /// Python `runningProcesses`.
    pub fn running_processes(&self) -> Vec<String> {
        list_running_executables()
    }

    /// Python `_normalized_domain_input` — (values, invalid, changed, internal).
    fn normalized_domain_input(&self, text: &str) -> (Vec<String>, Vec<String>, bool, bool) {
        let mut values: Vec<String> = Vec::new();
        let mut invalid: Vec<String> = Vec::new();
        let mut changed = false;
        let mut internal = false;
        for item in split_routing_addresses(text) {
            match normalize_routing_address(&item) {
                None => invalid.push(item),
                Some(value) => {
                    if is_internal_preset_rule(&value) {
                        internal = true;
                        continue;
                    }
                    changed = changed || value != item;
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
            }
        }
        (values, invalid, changed, internal)
    }

    /// Python `normalizeDomainRuleInput`.
    pub fn normalize_domain_rule_input(&self, text: &str) -> Value {
        let (values, invalid, changed, internal) = self.normalized_domain_input(text);
        json!({
            "values": values,
            "invalid": invalid,
            "changed": changed || internal,
        })
    }

    /// Python `addDomainRule`.
    pub fn add_domain_rule(&mut self, addr: &str, action: &str) {
        let (values, invalid, changed, internal) = self.normalized_domain_input(addr);
        if values.is_empty() {
            if let Some(first) = invalid.first() {
                self.notify(
                    "warning",
                    &tr(
                        "Не похоже на домен, IP или CIDR: {value}",
                        &[("value", first)],
                    ),
                );
            } else if internal {
                self.notify(
                    "warning",
                    &tr("Добавляйте конкретный домен, IP или CIDR", &[]),
                );
            }
            return;
        }
        self.mutate_routing(|routing| {
            let direct: Vec<String> = routing
                .direct_domains
                .iter()
                .filter(|item| !values.contains(item))
                .cloned()
                .collect();
            let proxy: Vec<String> = routing
                .proxy_domains
                .iter()
                .filter(|item| !values.contains(item))
                .cloned()
                .collect();
            let block: Vec<String> = routing
                .block_domains
                .iter()
                .filter(|item| !values.contains(item))
                .cloned()
                .collect();
            let mut target = match action {
                "direct" => direct,
                "block" => block,
                _ => proxy,
            };
            target.extend(values.iter().cloned());
            routing.direct_domains = match action {
                "direct" => target.clone(),
                _ => routing
                    .direct_domains
                    .iter()
                    .filter(|item| !values.contains(item))
                    .cloned()
                    .collect(),
            };
            routing.proxy_domains = match action {
                "" | "proxy" => target.clone(),
                _ => routing
                    .proxy_domains
                    .iter()
                    .filter(|item| !values.contains(item))
                    .cloned()
                    .collect(),
            };
            routing.block_domains = match action {
                "block" => target,
                _ => routing
                    .block_domains
                    .iter()
                    .filter(|item| !values.contains(item))
                    .cloned()
                    .collect(),
            };
        });
        if !invalid.is_empty() {
            self.notify(
                "warning",
                &tr(
                    "Пропущено некорректных адресов: {count}",
                    &[("count", &invalid.len().to_string())],
                ),
            );
        } else if changed {
            let mut shown = values
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if values.len() > 3 {
                shown.push_str(" …");
            }
            self.notify("info", &tr("Добавлено: {value}", &[("value", &shown)]));
        }
    }

    /// Python `removeDomainRule`.
    pub fn remove_domain_rule(&mut self, addr: &str) {
        self.mutate_routing(|routing| {
            routing.direct_domains.retain(|item| item != addr);
            routing.proxy_domains.retain(|item| item != addr);
            routing.block_domains.retain(|item| item != addr);
        });
    }

    /// Python `importDomainRules`.
    pub fn import_domain_rules(&mut self, text: &str) {
        let accepted_lines = match parse_routing_rules(text, "") {
            Ok(lines) => lines,
            Err(error) => {
                self.notify("error", &error.to_string());
                return;
            }
        };
        if accepted_lines.is_empty() {
            self.notify("warning", "Нет строк для импорта");
            return;
        }
        let accepted_lines: Vec<(String, String)> = accepted_lines
            .into_iter()
            .map(|(addr, action)| (normalize_routing_address(&addr).unwrap_or(addr), action))
            .filter(|(addr, _)| !is_internal_preset_rule(addr))
            .collect();
        if accepted_lines.is_empty() {
            self.notify(
                "warning",
                &tr("Добавляйте конкретный домен, IP или CIDR", &[]),
            );
            return;
        }
        let imported_count = accepted_lines.len();
        self.mutate_routing(|routing| {
            let mut direct = routing.direct_domains.clone();
            let mut proxy = routing.proxy_domains.clone();
            let mut block = routing.block_domains.clone();
            for (addr, act) in &accepted_lines {
                for list in [&mut direct, &mut proxy, &mut block] {
                    list.retain(|item| item != addr);
                }
                match act.as_str() {
                    "direct" => direct.push(addr.clone()),
                    "block" => block.push(addr.clone()),
                    _ => proxy.push(addr.clone()),
                }
            }
            routing.direct_domains = direct;
            routing.proxy_domains = proxy;
            routing.block_domains = block;
        });
        self.notify(
            "success",
            &tr(
                "Импортировано правил: {count}",
                &[("count", &imported_count.to_string())],
            ),
        );
    }

    /// Python `_dns_server_lines`.
    fn dns_server_lines(text: &str, fallback: &str) -> Vec<String> {
        let mut values: Vec<String> = Vec::new();
        for raw in text.replace(';', "\n").lines() {
            let value = raw.trim().to_string();
            if !value.is_empty() && !values.contains(&value) {
                values.push(value);
            }
        }
        if values.is_empty() && !fallback.is_empty() {
            values.push(fallback.to_string());
        }
        values
    }

    /// Python `setDnsBootstrapServers`.
    pub fn set_dns_bootstrap_servers(&mut self, text: &str) {
        self.mutate_routing(|routing| {
            routing.dns_bootstrap_servers = Self::dns_server_lines(text, "");
            routing.dns_bootstrap_server = routing
                .dns_bootstrap_servers
                .first()
                .cloned()
                .unwrap_or_default();
        });
    }

    /// Python `setDnsProxyServers`.
    pub fn set_dns_proxy_servers(&mut self, text: &str) {
        self.mutate_routing(|routing| {
            routing.dns_proxy_servers = Self::dns_server_lines(text, "cloudflare-dns.com");
            routing.dns_proxy_server = routing
                .dns_proxy_servers
                .first()
                .cloned()
                .unwrap_or_default();
        });
    }

    /// Python `setDnsParallelQuery`.
    pub fn set_dns_parallel_query(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.dns_parallel_query = enabled);
    }

    /// Python `setDnsOptimisticCache`.
    pub fn set_dns_optimistic_cache(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.dns_optimistic_cache = enabled);
    }

    /// Python `setDnsGeoCheck`.
    pub fn set_dns_geo_check(&mut self, enabled: bool) {
        self.mutate_routing(|routing| routing.dns_geo_check = enabled);
    }

    /// Python `_parse_dns_hosts_text`.
    fn parse_dns_hosts_text(
        text: &str,
    ) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
        let mut hosts = std::collections::BTreeMap::new();
        for (line_number, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !line.contains('=') {
                return Err(tr(
                    "Hosts, строка {line}: ожидается domain=address",
                    &[("line", &(line_number + 1).to_string())],
                ));
            }
            let (domain, raw_addresses) = line.split_once('=').unwrap();
            let domain = domain.trim().to_lowercase();
            let domain = domain.strip_suffix('.').unwrap_or(&domain).to_string();
            let addresses: Vec<String> = raw_addresses
                .replace(';', ",")
                .split(',')
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if domain.is_empty() || addresses.is_empty() {
                return Err(tr(
                    "Hosts, строка {line}: пустой домен или адрес",
                    &[("line", &(line_number + 1).to_string())],
                ));
            }
            hosts.insert(domain, addresses);
        }
        Ok(hosts)
    }

    /// Python `setDnsHosts`.
    pub fn set_dns_hosts(&mut self, text: &str) -> bool {
        match Self::parse_dns_hosts_text(text) {
            Ok(hosts) => {
                self.mutate_routing(move |routing| routing.dns_hosts = hosts);
                true
            }
            Err(error) => {
                self.notify("warning", &error);
                false
            }
        }
    }

    /// Python `applyDnsSettings`.
    pub fn apply_dns_settings(
        &mut self,
        bootstrap_text: &str,
        proxy_text: &str,
        hosts_text: &str,
    ) -> bool {
        let hosts = match Self::parse_dns_hosts_text(hosts_text) {
            Ok(hosts) => hosts,
            Err(error) => {
                self.notify("warning", &error);
                return false;
            }
        };
        self.mutate_routing(move |routing| {
            routing.dns_bootstrap_servers = Self::dns_server_lines(bootstrap_text, "");
            routing.dns_bootstrap_server = routing
                .dns_bootstrap_servers
                .first()
                .cloned()
                .unwrap_or_default();
            routing.dns_proxy_servers = Self::dns_server_lines(proxy_text, "cloudflare-dns.com");
            routing.dns_proxy_server = routing
                .dns_proxy_servers
                .first()
                .cloned()
                .unwrap_or_default();
            routing.dns_hosts = hosts;
        });
        self.notify("success", &tr("Настройки DNS применены", &[]));
        true
    }

    /// Python `importDomainRulesFile` (the shell passes the payload).
    pub fn import_domain_rules_file(&mut self, file: &Path) {
        match std::fs::read_to_string(file) {
            Ok(text) => self.import_domain_rules_file_payload(
                text.strip_prefix('\u{feff}').unwrap_or(&text),
                file.extension().and_then(|ext| ext.to_str()).unwrap_or_default(),
            ),
            Err(error) => self.notify_rule_import_error(&error.to_string()),
        }
    }

    pub fn notify_rule_import_error(&mut self, error: &str) {
        self.notify("error", &tr("Не удалось импортировать правила: {error}", &[("error", error)]));
    }

    pub fn import_domain_rules_file_payload(&mut self, payload: &str, suffix: &str) {
        let accepted_lines = match parse_routing_rules(payload, suffix) {
            Ok(lines) => lines,
            Err(error) => {
                self.notify(
                    "error",
                    &tr(
                        "Не удалось импортировать правила: {error}",
                        &[("error", &error.to_string())],
                    ),
                );
                return;
            }
        };
        if accepted_lines.is_empty() {
            self.notify(
                "warning",
                &tr("Файл не содержит поддерживаемых правил", &[]),
            );
            return;
        }
        let text = accepted_lines
            .iter()
            .map(|(address, action)| format!("{address}|{action}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.import_domain_rules(&text);
    }

    /// Python `exportDomainRules` — rule payload for the shell clipboard.
    pub fn export_domain_rules(&mut self) -> Option<String> {
        let routing = &self.routing_cached;
        let mut lines: Vec<String> = Vec::new();
        for addr in &routing.direct_domains {
            if !is_internal_preset_rule(addr) {
                lines.push(format!("{addr}|direct"));
            }
        }
        for addr in &routing.proxy_domains {
            if !is_internal_preset_rule(addr) {
                lines.push(format!("{addr}|proxy"));
            }
        }
        for addr in &routing.block_domains {
            if !is_internal_preset_rule(addr) {
                lines.push(format!("{addr}|block"));
            }
        }
        let payload = lines.join("\n");
        if payload.is_empty() {
            self.notify("warning", "Нет правил для экспорта");
            return None;
        }
        self.notify("success", "Правила скопированы в буфер обмена");
        Some(payload)
    }

    // ── Routing property mirrors ───────────────────────────────────────────

    /// Python `domainRules`.
    pub fn domain_rules(&self) -> Vec<Value> {
        let routing = &self.routing_cached;
        let mut out: Vec<Value> = Vec::new();
        for addr in &routing.direct_domains {
            if !is_internal_preset_rule(addr) {
                out.push(json!({"addr": addr, "action": "direct"}));
            }
        }
        for addr in &routing.proxy_domains {
            if !is_internal_preset_rule(addr) {
                out.push(json!({"addr": addr, "action": "proxy"}));
            }
        }
        for addr in &routing.block_domains {
            if !is_internal_preset_rule(addr) {
                out.push(json!({"addr": addr, "action": "block"}));
            }
        }
        out
    }

    /// Python `serviceList`.
    pub fn service_list(&self) -> Vec<Value> {
        let routing = &self.routing_cached;
        SERVICE_PRESETS
            .iter()
            .map(|service| {
                json!({
                    "id": service.id,
                    "name": service.name,
                    "description": service.description,
                    "defaultAction": service.default_action,
                    // Display the route currently taken by this service. An
                    // inherited route remains inherited until the user edits it.
                    "action": lumen_core::routing_runtime::effective_service_action(&routing.to_dict(), service.id),
                })
            })
            .collect()
    }

    /// Python `processPresetList`.
    pub fn process_preset_list(&self) -> Vec<Value> {
        let routes = &self.routing_cached.process_preset_routes;
        PROCESS_PRESETS
            .iter()
            .map(|preset| {
                json!({
                    "id": preset.id,
                    "name": preset.name,
                    "description": preset.description,
                    "defaultAction": preset.default_action,
                    "action": routes.get(preset.id).cloned().unwrap_or_else(|| "off".to_string()),
                })
            })
            .collect()
    }

    /// Python `customRoutingPresets`.
    pub fn custom_routing_presets(&self) -> Vec<Value> {
        self.routing_cached
            .to_dict()
            .get("preset_id")
            .cloned()
            .map(|_| ())
            .unwrap_or(());
        self.controller
            .try_lock()
            .map(|controller| {
                controller
                    .state
                    .routing_presets
                    .iter()
                    .map(|preset| {
                        json!({
                            "id": preset.get("id").cloned().unwrap_or(json!("")),
                            "name": preset.get("name").cloned().unwrap_or(json!("")),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Python `regionalRoutingPresets`.
    pub fn regional_routing_presets(&self) -> Vec<Value> {
        let labels: std::collections::HashMap<&str, String> = std::collections::HashMap::from([
            ("global", tr("Всё через VPN", &[])),
            ("blocked", tr("Только заблокированное", &[])),
            ("blocked_cn", tr("Только заблокированное", &[])),
            ("except_ru", tr("Всё кроме РФ", &[])),
            ("except_cn", tr("Всё кроме Китая", &[])),
            ("except_ir", tr("Всё кроме Ирана", &[])),
        ]);
        regional_routing_preset_ids(&self.regional_preset())
            .into_iter()
            .map(|preset_id| {
                json!({
                    "id": preset_id,
                    "name": labels.get(preset_id).cloned().unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Python `routingPresetOptions`.
    pub fn routing_preset_options(&mut self) -> Vec<Value> {
        let key = "routing_preset_options".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| Vec::new());
        }
        let value = self.routing_preset_options_now();
        self.remember_read(key, value.clone());
        value
    }

    fn routing_preset_options_now(&mut self) -> Vec<Value> {
        let mut items = self.regional_routing_presets();
        let custom = {
            let controller = self.controller.lock();
            controller
                .state
                .routing_presets
                .iter()
                .filter_map(|preset| {
                    let id = preset.get("id").and_then(Value::as_str)?;
                    Some(json!({
                        "id": id,
                        "name": preset.get("name").cloned().unwrap_or(json!("")),
                    }))
                })
                .collect::<Vec<_>>()
        };
        items.extend(custom);
        items
    }

    /// Python `regionalPreset`.
    pub fn regional_preset(&self) -> String {
        let value = self.settings_cached.regional_preset.trim().to_string();
        if value.is_empty() {
            "russia".to_string()
        } else {
            value
        }
    }

    /// Python `regionalPresetBusy`.
    pub fn regional_preset_busy(&self) -> bool {
        !self.pending_regional_preset.is_empty()
    }

    // ── Property mirrors (the cached QML properties) ───────────────────────

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn transition_busy(&self) -> bool {
        self.busy
    }

    pub fn transition_target_connected(&self) -> bool {
        self.transition_target_connected
    }

    pub fn transition_connecting(&self) -> bool {
        self.transition_connecting
    }

    pub fn runtime_phase(&self) -> &str {
        &self.runtime_phase
    }

    pub fn runtime_message(&self) -> &str {
        &self.runtime_message
    }

    pub fn metrics(&self) -> &MetricsSnapshot {
        &self.metrics
    }

    pub fn session_upload_total(&self) -> i64 {
        self.session_upload_total
    }

    pub fn session_download_total(&self) -> i64 {
        self.session_download_total
    }

    pub fn session_traffic_total(&self) -> i64 {
        self.session_upload_total + self.session_download_total
    }

    pub fn selected_node_id(&self) -> &str {
        &self.selected_id
    }

    pub fn selected_node_name(&self) -> &str {
        &self.selected_name
    }

    pub fn selected_node_flag(&self) -> &str {
        &self.selected_flag
    }

    pub fn selected_node_flag_source(&self) -> &str {
        &self.selected_flag_source
    }

    pub(crate) fn selected_node_flag_country(&self) -> &str {
        &self.selected_flag_country
    }

    pub fn selected_latency(&self) -> i64 {
        self.selected_latency
    }

    pub fn selected_node_protocol(&self) -> &str {
        &self.selected_protocol
    }

    pub fn selected_node_source(&self) -> &str {
        &self.selected_source
    }

    pub fn routing_mode(&self) -> &str {
        &self.routing_mode
    }

    pub fn active_routing_preset_id(&self) -> String {
        let value = self.routing_cached.preset_id.trim().to_string();
        if value.is_empty() {
            "custom".to_string()
        } else {
            value
        }
    }

    pub fn tun_mode(&self) -> bool {
        self.tun_mode
    }

    pub fn limited_mode(&self) -> bool {
        !is_process_elevated()
    }

    pub fn proxy_enabled(&self) -> bool {
        self.proxy_enabled
    }

    pub fn discord_proxy(&self) -> bool {
        self.discord_proxy
    }

    pub fn theme_name(&self) -> &str {
        &self.theme
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn effective_language(&self) -> String {
        let languages = available_languages();
        if languages.contains(&self.language.to_string()) {
            self.language.clone()
        } else {
            "en".to_string()
        }
    }

    pub fn language_labels(&self) -> Vec<String> {
        available_languages()
            .iter()
            .map(|code| language_name(code))
            .collect()
    }

    pub fn accent_color(&self) -> &str {
        &self.accent
    }

    pub fn compact_mode(&self) -> bool {
        self.settings_cached.interface_mode == "compact"
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings_cached
    }

    pub fn profile_loaded(&self) -> bool {
        let key = "profile_loaded".to_string();
        if !self.controller_free() {
            return self.cached_read(&key).unwrap_or_else(|| true);
        }
        let value = self.profile_loaded_now();
        self.remember_read(key, value.clone());
        value
    }

    fn profile_loaded_now(&self) -> bool {
        self.controller.lock().profile_loaded()
    }

    pub fn ui_backdrop_available(&self) -> bool {
        is_mica_supported()
    }

    pub fn updates_available(&self) -> bool {
        self.updates_available
    }

    /// Python `markUpdatesSeen`.
    pub fn mark_updates_seen(&mut self) {
        if !self.updates_available {
            return;
        }
        self.updates_available = false;
        self.changes.updates_available = true;
    }

    pub fn quitting(&self) -> bool {
        self.quitting
    }

    pub fn set_tray_available(&mut self, value: bool) {
        if value != self.tray_available {
            self.tray_available = value;
            self.changes.tray_available = true;
        }
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available
    }

    /// Python `notifyHiddenToTray`.
    pub fn notify_hidden_to_tray(&mut self) {
        if self.quitting {
            return;
        }
        self.outbox.push_back(UiEvent::TrayMessageRequested);
    }

    // Onboarding mirrors.
    pub fn onboarding_setup_required(&self) -> bool {
        self.settings_cached.onboarding_revision < ONBOARDING_REVISION
    }

    pub fn onboarding_show_whats_new(&self) -> bool {
        is_upgrade(&self.settings_cached.onboarding_version, APP_VERSION)
    }

    pub fn onboarding_required(&self) -> bool {
        if !self.profile_loaded() {
            return false;
        }
        self.onboarding_setup_required() || self.onboarding_show_whats_new()
    }

    /// Python `onboardingWhatsNew`.
    pub fn onboarding_whats_new(&self) -> Vec<Value> {
        notes_between(&self.settings_cached.onboarding_version, APP_VERSION, 5)
    }

    /// Python `onboardingPreviousVersion`.
    pub fn onboarding_previous_version(&self) -> String {
        let seen = &self.settings_cached.onboarding_version;
        if seen.is_empty() || seen == LEGACY_PROFILE_VERSION {
            String::new()
        } else {
            seen.clone()
        }
    }

    pub fn onboarding_returning_user(&self) -> bool {
        if !self.settings_cached.onboarding_version.is_empty() {
            return true;
        }
        !self.nodes_cached.is_empty() || !self.subscriptions_cached.is_empty()
    }

    pub fn onboarding_server_count(&self) -> usize {
        self.nodes_cached.len()
    }

    pub fn recommended_vpn_url(&self) -> &'static str {
        RECOMMENDED_VPN_URL
    }

    pub fn app_name(&self) -> &'static str {
        lumen_core::constants::APP_NAME
    }

    pub fn app_version(&self) -> &'static str {
        APP_VERSION
    }

    pub fn translations(&self) -> std::collections::BTreeMap<String, String> {
        active_map()
    }

    pub fn available_languages(&self) -> Vec<String> {
        available_languages()
    }

    // Settings read mirrors (straight from the cached snapshot).
    pub fn always_run_as_admin(&self) -> bool {
        self.settings_cached.always_run_as_admin
    }

    pub fn is_admin(&self) -> bool {
        is_process_elevated()
    }

    pub fn window_geometry(&self) -> (i64, i64, i64, i64) {
        let settings = &self.settings_cached;
        (
            if settings.window_width == 0 {
                1280
            } else {
                settings.window_width
            },
            if settings.window_height == 0 {
                720
            } else {
                settings.window_height
            },
            settings.window_x,
            settings.window_y,
        )
    }

    pub fn check_updates(&self) -> bool {
        self.settings_cached.check_updates
    }

    pub fn app_auto_update(&self) -> bool {
        self.settings_cached.app_auto_update
    }

    pub fn xray_auto_update(&self) -> bool {
        self.settings_cached.xray_auto_update
    }

    pub fn xray_release_channel(&self) -> String {
        let value = self
            .settings_cached
            .xray_release_channel
            .trim()
            .to_lowercase();
        if value == "stable" {
            value
        } else {
            "beta".to_string()
        }
    }

    pub fn proxy_bypass_lan(&self) -> bool {
        self.settings_cached.system_proxy_bypass_lan
    }

    pub fn firefox_proxy_integration(&self) -> bool {
        self.settings_cached.firefox_proxy_integration
    }

    pub fn reconnect_on_network_change(&self) -> bool {
        self.settings_cached.reconnect_on_network_change
    }

    pub fn reconnect_after_sleep(&self) -> bool {
        self.settings_cached.reconnect_after_sleep
    }

    pub fn prefer_ipv6(&self) -> bool {
        self.settings_cached.prefer_ipv6
    }

    pub fn kill_switch(&self) -> bool {
        self.settings_cached.kill_switch
    }

    pub fn proxy_allow_lan(&self) -> bool {
        self.settings_cached.proxy_allow_lan
    }

    pub fn tun_strict_route(&self) -> bool {
        self.settings_cached.tun_strict_route
    }

    pub fn tun_stack(&self) -> String {
        if self.settings_cached.tun_stack.is_empty() {
            "mixed".to_string()
        } else {
            self.settings_cached.tun_stack.clone()
        }
    }

    pub fn tun_mtu(&self) -> i64 {
        self.settings_cached.tun_mtu
    }

    pub fn local_socks_port(&self) -> i64 {
        self.settings_cached.local_socks_port
    }

    pub fn local_http_port(&self) -> i64 {
        self.settings_cached.local_http_port
    }

    pub fn proxy_auth_enabled(&self) -> bool {
        self.settings_cached.proxy_auth_enabled
    }

    pub fn proxy_auth_username(&self) -> String {
        self.settings_cached.proxy_auth_username.clone()
    }

    pub fn proxy_auth_password(&self) -> String {
        self.settings_cached.proxy_auth_password.clone()
    }

    pub fn tun_block_quic(&self) -> bool {
        self.settings_cached.tun_block_quic
    }

    pub fn tun_endpoint_independent_nat(&self) -> bool {
        self.settings_cached.tun_endpoint_independent_nat
    }

    pub fn sniff_route_only(&self) -> bool {
        self.settings_cached.sniff_route_only
    }

    pub fn resource_update_check(&self) -> bool {
        self.settings_cached.resource_update_check
    }

    pub fn diagnostics_upload_enabled(&self) -> bool {
        self.settings_cached.diagnostics_upload_enabled
    }

    pub fn xray_fragment_enabled(&self) -> bool {
        self.settings_cached.enable_xray_fragment
    }

    pub fn final_fragment_enabled(&self) -> bool {
        self.settings_cached.enable_final_fragment
    }

    pub fn fragmentation_enabled(&self) -> bool {
        self.settings_cached.enable_xray_fragment || self.settings_cached.enable_final_fragment
    }

    pub fn fragment_packets(&self) -> String {
        if self.settings_cached.fragment_packets.is_empty() {
            "tlshello".to_string()
        } else {
            self.settings_cached.fragment_packets.clone()
        }
    }

    pub fn fragment_length(&self) -> String {
        if self.settings_cached.fragment_length.is_empty() {
            "50-100".to_string()
        } else {
            self.settings_cached.fragment_length.clone()
        }
    }

    pub fn fragment_delay(&self) -> String {
        if self.settings_cached.fragment_delay.is_empty() {
            "10-20".to_string()
        } else {
            self.settings_cached.fragment_delay.clone()
        }
    }

    pub fn tail_fragment_enabled(&self) -> bool {
        self.settings_cached.tail_fragment_enabled
    }

    pub fn multiplexing_enabled(&self) -> bool {
        self.settings_cached.multiplex_enabled
    }

    pub fn multiplex_concurrency(&self) -> i64 {
        self.settings_cached.multiplex_concurrency
    }

    pub fn auto_switch_enabled(&self) -> bool {
        self.settings_cached.auto_switch_enabled
    }

    pub fn auto_switch_threshold(&self) -> i64 {
        self.settings_cached.auto_switch_threshold_kbps
    }

    pub fn auto_switch_delay(&self) -> i64 {
        self.settings_cached.auto_switch_delay_sec
    }

    pub fn auto_switch_cooldown(&self) -> i64 {
        self.settings_cached.auto_switch_cooldown_sec
    }

    pub fn launch_on_startup(&self) -> bool {
        self.settings_cached.launch_on_startup
    }

    pub fn launch_in_tray_on_startup(&self) -> bool {
        self.settings_cached.launch_in_tray_on_startup
    }

    pub fn block_vpn_conflicts(&self) -> bool {
        self.settings_cached.block_vpn_conflicts
    }

    pub fn zapret_autostart(&self) -> bool {
        self.settings_cached.zapret_autostart
    }

    pub fn auto_connect_last(&self) -> bool {
        self.settings_cached.auto_connect_last
    }

    pub fn auto_connect_on_import(&self) -> bool {
        self.settings_cached.auto_connect_on_import
    }

    pub fn ping_method(&self) -> String {
        if self.settings_cached.ping_method.is_empty() {
            "tcping".to_string()
        } else {
            self.settings_cached.ping_method.clone()
        }
    }

    pub fn speed_test_url(&self) -> String {
        self.settings_cached.speed_test_url.clone()
    }

    pub fn speed_test_concurrency(&self) -> i64 {
        self.settings_cached.speed_test_concurrency
    }

    pub fn subscription_auto_update_minutes(&self) -> i64 {
        self.settings_cached.subscription_auto_update_minutes
    }

    pub fn subscription_include_regex(&self) -> String {
        self.settings_cached.subscription_include_regex.clone()
    }

    pub fn subscription_exclude_regex(&self) -> String {
        self.settings_cached.subscription_exclude_regex.clone()
    }

    pub fn subscription_user_agent(&self) -> String {
        self.settings_cached.subscription_user_agent.clone()
    }

    pub fn subscription_use_real_hwid(&self) -> bool {
        self.settings_cached.subscription_use_real_hwid
    }

    pub fn subscription_use_proxy_tun(&self) -> bool {
        self.settings_cached.subscription_use_proxy_tun
    }

    pub fn subscription_hwid(&self) -> String {
        self.settings_cached.subscription_hwid.clone()
    }

    pub fn subscription_converter_enabled(&self) -> bool {
        self.settings_cached.subscription_converter_enabled
    }

    pub fn subscription_converter_url(&self) -> String {
        self.settings_cached.subscription_converter_url.clone()
    }

    pub fn subscription_importing(&self) -> bool {
        self.sub_importing
    }

    pub fn subscription_import_status(&self) -> &str {
        &self.sub_import_status
    }

    pub fn auto_lock_minutes(&self) -> i64 {
        self.controller.lock().state.security.auto_lock_minutes
    }

    pub fn master_password_enabled(&self) -> bool {
        self.controller.lock().state.security.enabled
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn encryption_active(&self) -> bool {
        self.controller.lock().is_data_encrypted()
    }

    // Routing mirrors.
    pub fn bypass_lan(&self) -> bool {
        self.routing_cached.bypass_lan
    }

    pub fn tun_default_outbound(&self) -> String {
        let value = self
            .routing_cached
            .tun_default_outbound
            .trim()
            .to_lowercase();
        if value == "proxy" || value == "direct" {
            value
        } else {
            "proxy".to_string()
        }
    }

    pub fn dns_mode(&self) -> String {
        self.routing_cached.dns_mode.clone()
    }

    pub fn dns_bootstrap_server(&self) -> String {
        self.routing_cached.dns_bootstrap_server.clone()
    }

    pub fn dns_bootstrap_servers_text(&self) -> String {
        self.routing_cached.dns_bootstrap_servers.join("\n")
    }

    pub fn dns_bootstrap_type(&self) -> String {
        self.routing_cached.dns_bootstrap_type.clone()
    }

    pub fn dns_proxy_server(&self) -> String {
        self.routing_cached.dns_proxy_server.clone()
    }

    pub fn dns_proxy_servers_text(&self) -> String {
        self.routing_cached.dns_proxy_servers.join("\n")
    }

    pub fn dns_proxy_type(&self) -> String {
        self.routing_cached.dns_proxy_type.clone()
    }

    pub fn dns_bootstrap_strategy(&self) -> String {
        if self.routing_cached.dns_bootstrap_strategy.is_empty() {
            "ipv4_only".to_string()
        } else {
            self.routing_cached.dns_bootstrap_strategy.clone()
        }
    }

    pub fn dns_proxy_strategy(&self) -> String {
        if self.routing_cached.dns_proxy_strategy.is_empty() {
            "ipv4_only".to_string()
        } else {
            self.routing_cached.dns_proxy_strategy.clone()
        }
    }

    pub fn dns_fake_enabled(&self) -> bool {
        self.routing_cached.dns_fake_enabled
    }

    pub fn dns_hijack_enabled(&self) -> bool {
        self.routing_cached.dns_hijack_enabled
    }

    pub fn dns_parallel_query(&self) -> bool {
        self.routing_cached.dns_parallel_query
    }

    pub fn dns_optimistic_cache(&self) -> bool {
        self.routing_cached.dns_optimistic_cache
    }

    pub fn dns_geo_check(&self) -> bool {
        self.routing_cached.dns_geo_check
    }

    pub fn dns_hosts_text(&self) -> String {
        self.routing_cached
            .dns_hosts
            .iter()
            .map(|(domain, addresses)| format!("{domain}={}", addresses.join(",")))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn tun_route_exclude_address(&self) -> String {
        self.routing_cached.tun_route_exclude_address.join("\n")
    }

    pub fn process_rules(&self) -> Vec<Value> {
        self.routing_cached.process_rules.clone()
    }

    // ── Dashboard models ───────────────────────────────────────────────────

    /// Python `setDashboardNodeFilter`.
    pub fn set_dashboard_node_filter(&mut self, text: &str) {
        self.dashboard_nodes.set_filter_text(text);
    }

    /// Python `dashboardNodeIds`.
    pub fn dashboard_node_ids(&self) -> Vec<String> {
        self.dashboard_nodes.node_ids(&self.node_model)
    }

    /// Python `setProcessTrafficSort`.
    pub fn set_process_traffic_sort(&mut self, key: &str, ascending: bool) {
        self.process_model.set_sort(key, ascending);
    }
}

pub fn find_free_local_port(start: i64, excluded: &HashSet<i64>) -> Option<i64> {
    for port in start.max(1024)..65535 {
        if excluded.contains(&port) {
            continue;
        }
        if std::net::TcpListener::bind(("127.0.0.1", port as u16)).is_ok() {
            return Some(port);
        }
    }
    None
}

// `Node::group_or_default` helper used by the filter.
trait GroupOrDefault {
    fn group_or_default(&self) -> &str;
}

impl GroupOrDefault for Node {
    fn group_or_default(&self) -> &str {
        if self.group.is_empty() {
            "Default"
        } else {
            &self.group
        }
    }
}

fn text_or_empty(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_string()
}

#[cfg(test)]
mod core_path_tests {
    use super::AppBridge;
    use lumen_core::constants::base_dir;

    #[test]
    fn relative_default_core_path_displays_short_name() {
        let default = base_dir().join("core").join("sing-box-extended").join("sing-box.exe");
        for stored in ["", "core/sing-box-extended/sing-box.exe", r"core\sing-box-extended\sing-box.exe"] {
            assert_eq!(AppBridge::display_core_path(stored, &default), r"core\sing-box.exe", "{stored:?}");
        }
        let absolute = default.to_string_lossy().into_owned();
        assert_eq!(AppBridge::display_core_path(&absolute, &default), r"core\sing-box.exe");
        assert_eq!(AppBridge::display_core_path(r"D:\tools\sb.exe", &default), r"D:\tools\sb.exe");
    }
}
