//! Profile loading, migrations and the queued state-save pipeline
//! (`AppController.load/save/...` plus the module-level migration helpers).

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use lumen_core::constants::{APP_VERSION, DIAGNOSTICS_UPLOAD_URL};
use lumen_core::link_parser::repair_node_outbound_from_link;
use lumen_core::models::{AppSettings, AppState, RoutingSettings};
use lumen_core::routing_presets::{
    ROUTING_PRESET_BLOCKED, build_routing_preset, normalize_regional_preset,
    normalize_routing_preset_for_region, repair_builtin_preset_service_routes,
};
use lumen_core::secret_scrubber::scrub_text_depth;
use lumen_core::storage::{StateLoadError, StateStorage};

use crate::app_controller::{AppController, AppEvent, DelayedAction};

/// Python `_load_state` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadState {
    #[default]
    Unloaded,
    Loaded,
    Locked,
    Error,
}

const DNS_DEFAULTS_MIGRATION_KEY: &str = "recommended_dns_defaults_2026_07";
const MASQUE_DIRECT_MIGRATION_KEY: &str = "masque_direct_outbound_v1";

/// Apply the new DNS profile once without touching nodes or other user data.
pub fn apply_dns_defaults_update_once(state: &mut AppState) -> bool {
    if state
        .applied_migrations
        .get(DNS_DEFAULTS_MIGRATION_KEY)
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
    let defaults = RoutingSettings::default();
    state.routing.dns_mode = defaults.dns_mode.clone();
    state.routing.dns_bootstrap_server = defaults.dns_bootstrap_server.clone();
    state.routing.dns_bootstrap_servers = defaults.dns_bootstrap_servers.clone();
    state.routing.dns_bootstrap_type = defaults.dns_bootstrap_type.clone();
    state.routing.dns_bootstrap_strategy = defaults.dns_bootstrap_strategy.clone();
    state.routing.dns_proxy_server = defaults.dns_proxy_server.clone();
    state.routing.dns_proxy_servers = defaults.dns_proxy_servers.clone();
    state.routing.dns_proxy_type = defaults.dns_proxy_type.clone();
    state.routing.dns_proxy_strategy = defaults.dns_proxy_strategy.clone();
    state.routing.dns_fake_enabled = defaults.dns_fake_enabled;
    state.routing.dns_hijack_enabled = defaults.dns_hijack_enabled;
    state.routing.dns_parallel_query = defaults.dns_parallel_query;
    state.routing.dns_optimistic_cache = defaults.dns_optimistic_cache;
    state.routing.dns_geo_check = defaults.dns_geo_check;
    state.routing.dns_hosts = defaults.dns_hosts.clone();
    state
        .applied_migrations
        .insert(DNS_DEFAULTS_MIGRATION_KEY.into(), true);
    true
}

/// Repair saved Clash MASQUE nodes once, without reimporting subscriptions.
pub fn apply_masque_direct_update_once(state: &mut AppState) -> bool {
    if state
        .applied_migrations
        .get(MASQUE_DIRECT_MIGRATION_KEY)
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
    for node in &mut state.nodes {
        let outbound = node.outbound.as_object();
        let native = outbound
            .and_then(|outbound| outbound.get("singbox"))
            .filter(|value| value.is_object());
        let native_type = native
            .and_then(|native| native.get("type"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .map(str::to_lowercase)
            .unwrap_or_default();
        let protocol = outbound
            .and_then(|outbound| outbound.get("protocol"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .map(str::to_lowercase)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                (!node.scheme.is_empty()).then(|| node.scheme.trim().to_lowercase())
            })
            .or_else(|| {
                (!native_type.is_empty()).then_some(native_type.clone())
            })
            .unwrap_or_default();
        if protocol == "masque" || native_type == "masque" {
            repair_node_outbound_from_link(node);
        }
    }
    state
        .applied_migrations
        .insert(MASQUE_DIRECT_MIGRATION_KEY.into(), true);
    true
}

/// One in-flight state write plus at most one pending snapshot
/// (`ThreadPoolExecutor(max_workers=1)` + `_pending_save_snapshot`).
#[derive(Default)]
struct QueueState {
    pending: Option<(Arc<StateStorage>, AppState)>,
    worker_running: bool,
    shutdown: bool,
    completed_generation: u64,
    queued_generation: u64,
}

/// The Python `_save_executor`/`_save_futures` pipeline.
#[derive(Clone, Default)]
pub(crate) struct SaveQueue {
    state: Arc<(Mutex<QueueState>, Condvar)>,
}

impl SaveQueue {
    /// `_enqueue_state_save`: hand a snapshot of the state to the single
    /// background write worker.
    pub fn enqueue(
        &self,
        events: &tokio::sync::broadcast::Sender<AppEvent>,
        storage: &Arc<StateStorage>,
        snapshot: AppState,
    ) {
        let (lock, signal) = &*self.state;
        let mut state = lock.lock();
        if state.shutdown {
            return;
        }
        state.pending = Some((Arc::clone(storage), snapshot));
        state.queued_generation += 1;
        if state.worker_running {
            return;
        }
        state.worker_running = true;
        let queue = Arc::clone(&self.state);
        let events = events.clone();
        let _ = std::thread::Builder::new()
            .name("state-save".into())
            .spawn(move || write_pending_states(queue, events));
        signal.notify_all();
    }

    /// `_flush_state_saves`: wait (bounded) until every queued snapshot is
    /// written. `false` means the deadline passed with work outstanding.
    pub fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (lock, signal) = &*self.state;
        let mut state = lock.lock();
        loop {
            let target = state.queued_generation;
            if state.completed_generation >= target && state.pending.is_none() {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let _ = signal.wait_for(&mut state, remaining.min(Duration::from_millis(250)));
        }
    }

    pub fn has_pending(&self) -> bool {
        let state = self.state.0.lock();
        state.pending.is_some() || state.worker_running
    }

    /// Python `_save_executor.shutdown(wait=False, cancel_futures=False)`.
    pub fn shutdown(&self) {
        let (lock, signal) = &*self.state;
        let mut state = lock.lock();
        state.shutdown = true;
        signal.notify_all();
    }
}

fn write_pending_states(
    queue: Arc<(Mutex<QueueState>, Condvar)>,
    events: tokio::sync::broadcast::Sender<AppEvent>,
) {
    let (lock, signal) = &*queue;
    loop {
        let job = {
            let mut state = lock.lock();
            let Some(job) = state.pending.take() else {
                state.worker_running = false;
                state.completed_generation = state.queued_generation;
                signal.notify_all();
                return;
            };
            job
        };
        let (storage, snapshot) = job;
        if let Err(error) = storage.save(&snapshot) {
            let message = scrub_text_depth(&error.to_string(), 0);
            let _ = events.send(AppEvent::SaveFailed(message.clone()));
            let _ = events.send(AppEvent::Status {
                level: "error",
                message: format!("Профиль не сохранён: {message}"),
            });
        }
    }
}

/// Python `StateStorage.default_state()`: a fresh default profile with
/// immediately usable core paths and the blocked preset.
pub(crate) fn default_state() -> AppState {
    let mut state = AppState::default();
    state.routing = build_routing_preset(&state.routing, ROUTING_PRESET_BLOCKED);
    state.settings.xray_path = lumen_core::path_utils::normalize_configured_path(
        &state.settings.xray_path,
        Some(&lumen_core::constants::xray_path_default()),
        true,
        true,
    );
    state.settings.singbox_path = lumen_core::path_utils::normalize_configured_path(
        &state.settings.singbox_path,
        Some(&lumen_core::constants::singbox_path_default()),
        true,
        true,
    );
    state
}

impl AppController {
    /// Python `load`.
    pub fn load(&mut self) -> bool {
        self.load_state = LoadState::Unloaded;
        match self.storage.load() {
            Err(StateLoadError::PassphraseRequired) => {
                self.load_state = LoadState::Locked;
                self.emit(AppEvent::PassphraseRequired);
                return false;
            }
            Err(error) => {
                self.load_state = LoadState::Error;
                self.emit_status("error", &format!("Не удалось открыть профиль: {error}"));
                return false;
            }
            Ok(state) => self.state = state,
        }

        self.load_state = LoadState::Loaded;
        self.migrate_sort_order();
        let region = normalize_regional_preset(&self.state.settings.regional_preset);
        let current_preset = self.state.routing.preset_id.trim().to_lowercase();
        let normalized_preset = normalize_routing_preset_for_region(&current_preset, &region);
        let mut migrated = normalized_preset != current_preset;
        if migrated {
            self.state.routing = build_routing_preset(&self.state.routing, &normalized_preset);
            self.log(&format!(
                "[routing] normalized saved preset for {region}: {current_preset} -> {normalized_preset}"
            ));
        }
        let repaired_routing = repair_builtin_preset_service_routes(&self.state.routing);
        if repaired_routing.to_dict() != self.state.routing.to_dict() {
            self.state.routing = repaired_routing;
            migrated = true;
            self.log(
                "[routing] migrated the unmatched-traffic fallback for the saved blocked preset",
            );
        }
        migrated = apply_masque_direct_update_once(&mut self.state) || migrated;
        migrated = apply_dns_defaults_update_once(&mut self.state) || migrated;
        if !lumen_platform::startup::is_process_elevated() && self.state.settings.tun_mode {
            // Preserve the usable proxy-only application instead of letting a
            // saved TUN preference make non-admin startup fail completely.
            self.state.settings.tun_mode = false;
            migrated = true;
        }
        if migrated {
            self.save();
        }
        self.emit(AppEvent::NodesChanged);
        self.emit(AppEvent::SelectionChanged(self.selected_node()));
        self.emit(AppEvent::RoutingChanged);
        self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        self.configure_diagnostics_upload();
        self.emit(AppEvent::SubscriptionsChanged);
        true
    }

    /// Python `_migrate_sort_order`.
    pub(crate) fn migrate_sort_order(&mut self) {
        if !self.state.nodes.is_empty()
            && self.state.nodes.iter().all(|node| node.sort_order == 0)
        {
            for (index, node) in self.state.nodes.iter_mut().enumerate() {
                node.sort_order = index as i64 + 1;
            }
            self.save();
        }
    }

    /// Python `_configure_diagnostics_upload`.
    pub(crate) fn configure_diagnostics_upload(&self) {
        let upload_url = if self.state.settings.diagnostics_upload_enabled {
            DIAGNOSTICS_UPLOAD_URL
        } else {
            ""
        };
        lumen_core::diagnostics_uploader::configure_diagnostics_upload(upload_url, APP_VERSION);
    }

    /// Python `set_data_passphrase`.
    pub fn set_data_passphrase(&mut self, passphrase: &str) -> bool {
        let previous = self.storage.passphrase();
        self.storage.set_passphrase(passphrase);
        if !self.profile_loaded() {
            if !self.load() {
                self.storage.set_passphrase(&previous);
                self.emit_status("error", "Не удалось открыть профиль: проверьте пароль");
                return false;
            }
            self.emit_status("success", "Зашифрованный профиль открыт");
            return true;
        }
        self.save();
        self.emit_status("info", "Сохранение профиля с паролем запрошено");
        true
    }

    /// Python `clear_data_passphrase`.
    pub fn clear_data_passphrase(&mut self) -> bool {
        if !self.profile_loaded() {
            self.emit_status("error", "Сначала откройте зашифрованный профиль");
            return false;
        }
        self.storage.set_passphrase("");
        self.save();
        self.emit_status("info", "Сохранение профиля без пароля запрошено");
        true
    }

    /// Python `is_data_encrypted`.
    pub fn is_data_encrypted(&self) -> bool {
        self.storage.is_encrypted()
    }

    /// Python `save` (owner-task entry; immediate enqueue).
    pub fn save(&mut self) {
        if !self.profile_loaded() {
            return;
        }
        self.timers.save_pending = false;
        self.timers.save_deadline = None;
        self.enqueue_state_save();
    }

    /// Python `schedule_save` (single-shot 250 ms debounce).
    pub fn schedule_save(&mut self) {
        if !self.profile_loaded() {
            return;
        }
        self.timers.save_pending = true;
        self.timers.save_deadline = Some(Instant::now() + Duration::from_millis(250));
    }

    /// Python `_flush_scheduled_save`.
    pub(crate) fn flush_scheduled_save(&mut self) {
        if self.timers.save_pending {
            self.timers.save_pending = false;
            self.enqueue_state_save();
        }
    }

    /// Python `_enqueue_state_save`.
    fn enqueue_state_save(&mut self) {
        if !self.profile_loaded() {
            return;
        }
        // The cloned state is the snapshot: writes never observe later edits.
        let snapshot = self.state.clone();
        self.save_queue.enqueue(&self.events, &self.storage, snapshot);
    }

    /// Python `_flush_state_saves`.
    pub(crate) fn flush_state_saves(&mut self, timeout: Duration) {
        if self.timers.save_deadline.take().is_some() {
            self.enqueue_state_save();
        }
        if !self.save_queue.flush(timeout) {
            self.log("[state] Timed out waiting for pending state saves");
        }
    }

    /// Python `reset_settings_to_defaults`.
    pub fn reset_settings_to_defaults(&mut self) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let defaults = default_state();
        self.update_settings(defaults.settings);
        self.update_routing(defaults.routing, true);
    }

    /// Python `update_settings`.
    pub fn update_settings(&mut self, settings: AppSettings) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let old = self.state.settings.clone();
        // Reject delayed Windows reconciliation of superseded user intent.
        if old.launch_on_startup != settings.launch_on_startup
            || old.launch_in_tray_on_startup != settings.launch_in_tray_on_startup
            || old.always_run_as_admin != settings.always_run_as_admin
        {
            self.startup_generation
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let old_launch = old.launch_on_startup;
        let old_launch_in_tray = old.launch_in_tray_on_startup;
        let old_admin = old.always_run_as_admin;
        let old_tun = old.tun_mode;
        let old_proxy = old.enable_system_proxy;
        let old_firefox_proxy = old.firefox_proxy_integration;
        let old_diagnostics_upload = old.diagnostics_upload_enabled;
        self.state.settings = settings.clone();
        if old_diagnostics_upload != settings.diagnostics_upload_enabled {
            self.configure_diagnostics_upload();
        }
        self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        self.schedule_save();

        if settings.tun_mode && !old_tun && settings.discord_proxy_enabled {
            // Strip droute from Discord as soon as TUN takes over.
            self.schedule_delayed(Duration::ZERO, DelayedAction::DiscordProxy);
        }

        if old_proxy && !settings.enable_system_proxy {
            let _ = self.proxy.disable_necko_overrides();
        } else if old_firefox_proxy && !settings.firefox_proxy_integration {
            let _ = self.proxy.disable_necko_overrides();
        }

        let startup_changed = old_launch != settings.launch_on_startup
            || (settings.launch_on_startup
                && old_launch_in_tray != settings.launch_in_tray_on_startup);
        let admin_changed = old_admin != settings.always_run_as_admin;
        if startup_changed || admin_changed {
            self.queue_windows_preferences(startup_changed, admin_changed);
        }

        if self.connected || self.runtime.desired_connected {
            if old_tun != settings.tun_mode {
                self.runtime.desired_connected = true;
                self.request_transition("TUN mode toggled");
                return;
            }
            self.request_transition("settings changed");
        }
    }

    /// Python `export_backup`.
    pub fn export_backup(&mut self, path: &Path, passphrase: &str) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        match self.storage.export_backup(path, passphrase) {
            Ok(_) => self.emit_status("success", "Резервная копия сохранена"),
            Err(error) => self.emit_status(
                "error",
                &format!("Не удалось создать резервную копию: {error}"),
            ),
        }
    }

    /// Python `import_backup`.
    pub fn import_backup(&mut self, path: &Path, passphrase: &str) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        match self.storage.import_backup(path, passphrase) {
            Ok(state) => {
                self.state = state;
                self.parsed_singbox_document = None;
                self.save();
                self.emit(AppEvent::NodesChanged);
                self.emit(AppEvent::SelectionChanged(self.selected_node()));
                self.emit(AppEvent::RoutingChanged);
                self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
                self.emit_status("success", "Резервная копия импортирована");
            }
            Err(error) => {
                self.emit_status(
                    "error",
                    &format!("Не удалось импортировать резервную копию: {error}"),
                );
            }
        }
    }
}

use std::path::Path;
