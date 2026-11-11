//! Measurement workers, core/resource updates, metrics worker, country
//! detection and pending update-disconnect bookkeeping.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use lumen_core::models::Node;
use tokio::sync::oneshot;

use crate::app_controller::{AppController, AppEvent, DelayedAction, ResourceUpdateEntry};
use crate::update_service::{XrayUpdateConnection, XrayUpdateEvent, XrayUpdateRequest};
use crate::worker_service::TestConnection;

impl AppController {
    /// Build the live connection view and run a worker call against it. The
    /// proxy URL is owned by this scope, matching Python's
    /// `get_effective_http_proxy_url()` snapshot.
    fn with_workers<T>(
        &mut self,
        f: impl FnOnce(
            &mut crate::worker_service::WorkerService,
            &mut lumen_core::models::AppState,
            TestConnection<'_>,
        ) -> T,
    ) -> T {
        let proxy_url = if self.connected {
            self.get_effective_http_proxy_url()
        } else {
            None
        };
        let http_port = self
            .get_effective_http_proxy_port()
            .and_then(|port| u16::try_from(port).ok());
        let connection = TestConnection {
            connected: self.connected,
            active_session: self.active_session.as_ref(),
            effective_http_port: http_port,
            effective_proxy_url: proxy_url.as_deref(),
        };
        f(&mut self.workers, &mut self.state, connection)
    }

    /// Python `ping_nodes`.
    pub fn ping_nodes(&mut self, node_ids: Option<&HashSet<String>>, method: Option<&str>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let mut events = Vec::new();
        self.with_workers(|workers, state, connection| {
            workers.ping_nodes(state, node_ids, method, connection, |event| {
                events.push(event)
            })
        });
        for event in events {
            self.handle_test_event(&event);
        }
    }

    /// Python `speed_test_nodes`.
    pub fn speed_test_nodes(&mut self, node_ids: Option<&HashSet<String>>) -> bool {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return false;
        }
        let mut events = Vec::new();
        let started = self.with_workers(|workers, state, connection| {
            workers.speed_test_nodes(state, node_ids, connection, |event| events.push(event))
        });
        for event in events {
            self.handle_test_event(&event);
        }
        started
    }

    /// Python `cancel_speed_test`.
    pub fn cancel_speed_test(&mut self) -> bool {
        let mut events = Vec::new();
        let cancelled = self.workers.cancel_speed_test(|event| events.push(event));
        for event in events {
            self.handle_test_event(&event);
        }
        cancelled
    }

    /// Python `cancel_tests`.
    pub fn cancel_tests(&mut self) -> bool {
        let mut events = Vec::new();
        let cancelled = self.workers.cancel_tests(|event| events.push(event));
        for event in events {
            self.handle_test_event(&event);
        }
        cancelled
    }

    /// Python `get_fastest_alive_node`.
    pub fn get_fastest_alive_node(&self) -> Option<Node> {
        let selected = self.selected_node();
        crate::node_runtime_service::get_fastest_alive_node(&self.state.nodes, selected.as_ref())
            .cloned()
    }

    /// Python `test_connectivity`.
    pub fn test_connectivity(&mut self, url: Option<&str>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let mut events = Vec::new();
        self.with_workers(|workers, state, connection| {
            workers.test_connectivity(state, url, connection, |event| events.push(event))
        });
        for event in events {
            self.handle_test_event(&event);
        }
    }

    /// Python `_start_metrics_worker` / `_stop_metrics_worker`.
    pub(crate) fn start_metrics_worker(&mut self) {
        crate::runtime_services::start_metrics_worker(self);
    }

    pub(crate) fn stop_metrics_worker(&mut self) {
        crate::runtime_services::stop_metrics_worker(self);
    }

    // ── Xray core updates ──────────────────────────────────────────────────

    /// Python `run_xray_core_update`.
    pub fn run_xray_core_update(&mut self, apply_update: bool, silent: bool) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let effective_proxy_url = if self.connected {
            self.get_effective_http_proxy_url()
        } else {
            None
        };
        let connection = self.xray_update_connection();
        let mut events = Vec::new();
        self.updates.run_xray_core_update(
            &self.state.settings,
            connection,
            XrayUpdateRequest {
                apply_update,
                silent,
            },
            effective_proxy_url,
            |event| events.push(event),
        );
        for event in events {
            self.dispatch_xray_update_event(event);
        }
    }

    pub(crate) fn xray_update_connection(&self) -> XrayUpdateConnection {
        XrayUpdateConnection {
            connected: self.connected,
            transition_in_progress: self.transitions.pending
                || self.transitions.scheduled
                || self.transitions.active,
            shutting_down: self.shutting_down(),
        }
    }

    /// Event-loop hook for Xray update worker completions.
    pub(crate) fn handle_xray_update_event(
        &mut self,
        event: lumen_engines::xray::core_updater::XrayCoreUpdateEvent,
    ) {
        let settings = self.state.settings.clone();
        let connection = self.xray_update_connection();
        let mut events = Vec::new();
        self.updates
            .handle_worker_event(event, &settings, connection, |event| events.push(event));
        for event in events {
            self.dispatch_xray_update_event(event);
        }
    }

    pub(crate) fn dispatch_xray_update_event(&mut self, event: XrayUpdateEvent) {
        match event {
            XrayUpdateEvent::Progress(percent) => {
                self.emit(AppEvent::XrayUpdateProgress(percent));
            }
            XrayUpdateEvent::Result(result) => {
                self.emit(AppEvent::XrayUpdateResult(result));
            }
            XrayUpdateEvent::Status { level, message } => self.emit_status(level, &message),
            XrayUpdateEvent::Log(line) => self.log(&line),
            XrayUpdateEvent::TransitionRequested {
                desired_connected,
                reason,
            } => {
                self.runtime.desired_connected = desired_connected;
                self.request_transition(reason);
            }
        }
    }

    // ── resource updates (geodata / singbox / droute) ──────────────────────

    /// Python `run_resource_update`.
    pub fn run_resource_update(
        &mut self,
        kind: &str,
        apply_update: bool,
        region: Option<&str>,
    ) -> bool {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return false;
        }
        if self.shutting_down() {
            return false;
        }
        let kind = kind.trim().to_lowercase();
        if !matches!(kind.as_str(), "singbox" | "geodata" | "droute") {
            self.emit_status("error", &format!("Неизвестный тип обновления: {kind}"));
            return false;
        }
        if self.resource_updates.iter().any(|entry| entry.kind == kind) {
            self.emit_status(
                "info",
                &format!("Проверка или обновление {kind} уже выполняется"),
            );
            return false;
        }
        let proxy_url = if self.connected {
            self.get_effective_http_proxy_url()
        } else {
            None
        };
        let region = lumen_core::routing_presets::normalize_regional_preset(
            region.unwrap_or(self.state.settings.regional_preset.as_str()),
        );
        let options = lumen_engines::core_resource_updater::ResourceUpdateOptions {
            kind: kind.clone(),
            singbox_path: self.state.settings.singbox_path.clone(),
            apply_update,
            proxy_url,
            region: region.clone(),
        };
        let worker = lumen_engines::core_resource_updater::ResourceUpdateWorker::start(options);
        self.resource_updates.push(ResourceUpdateEntry {
            kind,
            region,
            apply_update,
            worker,
        });
        true
    }

    /// Python `_on_update_disconnect_request` (resource workers).
    pub(crate) fn on_resource_update_disconnect_request(&mut self, confirm: oneshot::Sender<bool>) {
        if self.shutting_down() {
            self.reconnect_after_resource_updates = false;
            let _ = confirm.send(false);
            return;
        }
        if !self.connected {
            let _ = confirm.send(true);
            return;
        }
        self.log("[updater] Connection active. Disconnecting before replacing files...");
        self.emit_status("info", "Остановка ядра для установки обновлений...");
        self.reconnect_after_resource_updates = true;
        // The teardown itself must go through the transition state machine so
        // it never races an in-flight connect worker and never blocks.
        self.pending_resource_disconnects.push(confirm);
        self.runtime.desired_connected = false;
        self.request_transition("updater disconnect");
    }

    /// Python `_resolve_pending_update_disconnects`.
    pub(crate) fn resolve_pending_update_disconnects(&mut self) {
        // Xray update worker first: its service tracks its own pending ack.
        let connection = self.xray_update_connection();
        let mut events = Vec::new();
        self.updates
            .resolve_pending_disconnect(connection, |event| events.push(event));
        for event in events {
            self.dispatch_xray_update_event(event);
        }

        if self.pending_resource_disconnects.is_empty() {
            return;
        }
        let stopped = !self.connected;
        if !stopped
            && (self.transitions.pending || self.transitions.scheduled || self.transitions.active)
        {
            return;
        }
        let confirms = std::mem::take(&mut self.pending_resource_disconnects);
        if stopped {
            self.log("[updater] Disconnected successfully.");
        } else {
            self.reconnect_after_resource_updates = false;
            self.log("[updater] Failed to disconnect.");
        }
        for confirm in confirms {
            let _ = confirm.send(stopped);
        }
    }

    /// Python `_on_resource_update_done` plus
    /// `_on_resource_update_worker_finished`.
    pub(crate) fn on_resource_update_done(
        &mut self,
        entry: ResourceUpdateEntry,
        result: lumen_engines::core_resource_updater::ResourceUpdateResult,
    ) {
        let ResourceUpdateEntry {
            kind,
            region,
            apply_update: _,
            worker: _,
        } = entry;
        let status = status_label(&result.status);
        let message = result.message.clone();
        let pending_region = if kind == "geodata" {
            self.pending_regional_preset.clone()
        } else {
            String::new()
        };
        if !pending_region.is_empty() && region == pending_region {
            if matches!(status, "updated" | "up_to_date") {
                // An actual install has already stopped the active core and
                // the finished hook reconnects it. If nothing had to be
                // installed, request one normal runtime transition here so
                // the active config still adopts the region.
                let restart_runtime = !self.reconnect_after_resource_updates;
                self.commit_regional_preset(&pending_region, restart_runtime);
            }
            self.pending_regional_preset = String::new();
        }
        self.emit(AppEvent::ResourceUpdateResult(result));
        if status == "error" {
            self.emit_status("error", &message);
            self.log(&format!("[updater] Failed to update {kind}: {message}"));
        } else if matches!(status, "updated" | "up_to_date") {
            self.emit_status(
                if status == "updated" { "success" } else { "info" },
                &message,
            );
            self.log(&format!(
                "[updater] Resource update ({kind}) finished: status={status}, message={message}"
            ));
        } else {
            self.emit_status("warning", &message);
            self.log(&format!(
                "[updater] Resource update ({kind}) warning: status={status}, message={message}"
            ));
        }

        if kind == "droute" && status == "updated" && !self.shutting_down() {
            self.schedule_discord_proxy_apply();
        }

        // Python `_on_resource_update_worker_finished`.
        if !self.reconnect_after_resource_updates || self.shutting_down() {
            return;
        }
        let another_install_running = self
            .resource_updates
            .iter()
            .any(|candidate| candidate.apply_update);
        if another_install_running {
            return;
        }
        self.reconnect_after_resource_updates = false;
        self.runtime.desired_connected = true;
        self.request_transition("resource update reconnect");
    }

    // ── country detection ──────────────────────────────────────────────────

    /// Python `_detect_countries_sync`.
    pub fn detect_countries_sync(&mut self) {
        if crate::node_runtime_service::detect_countries_sync(&mut self.state.nodes) {
            self.schedule_save();
            self.emit(AppEvent::NodesChanged);
        }
    }

    /// Python `_process_country_detection_batch` (short slices of the saved
    /// profile; the Python version holds live node references, so the Rust
    /// batch looks nodes up by the captured ID snapshot).
    pub(crate) fn process_country_detection_batch(&mut self) {
        if self.shutting_down() {
            self.timers.country_detect.node_ids.clear();
            return;
        }
        {
            let end = self
                .timers
                .country_detect
                .node_ids
                .len()
                .min(self.timers.country_detect.cursor + 32);
            while self.timers.country_detect.cursor < end {
                let index = self.timers.country_detect.cursor;
                self.timers.country_detect.cursor += 1;
                let node_id = self.timers.country_detect.node_ids[index].clone();
                let Some(node) = self
                    .state
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == node_id)
                else {
                    continue;
                };
                if node.country_code.is_empty() {
                    let code = lumen_core::country_flags::detect_country(&node.name, &node.server);
                    if !code.is_empty() {
                        node.country_code = code;
                        self.timers.country_detect.changed = true;
                    }
                }
            }
        }
        if self.timers.country_detect.cursor < self.timers.country_detect.node_ids.len() {
            self.schedule_delayed(Duration::from_millis(10), DelayedAction::CountryDetectBatch);
            return;
        }
        self.timers.country_detect.node_ids.clear();
        if self.timers.country_detect.changed {
            self.schedule_save();
            self.emit(AppEvent::NodesChanged);
        }
        self.schedule_delayed(
            Duration::from_millis(500),
            DelayedAction::StartCountryIpResolution,
        );
    }

    /// Python `_start_country_ip_resolution`.
    pub(crate) fn start_country_ip_resolution(&mut self) {
        let nodes = self.state.nodes.clone();
        let sender = self.internal.0.clone();
        self.country_resolver
            .start_country_ip_resolution(&nodes, move |results| {
                let _ =
                    sender.send(crate::app_controller::InternalEvent::CountriesResolved(results));
            });
    }

    /// Python `_on_countries_resolved`.
    pub(crate) fn on_countries_resolved(&mut self, results: &BTreeMap<String, String>) {
        if crate::node_runtime_service::on_countries_resolved(&mut self.state.nodes, results) {
            self.save();
            self.emit(AppEvent::NodesChanged);
        }
    }

    /// Python `_get_next_node_for_auto_switch`.
    pub fn get_next_node_for_auto_switch(&self) -> Option<Node> {
        crate::auto_switch_service::get_next_node_for_auto_switch(&self.state).cloned()
    }
}

/// `ResourceUpdateStatus` value label used in the Python comparisons.
fn status_label(status: &lumen_engines::core_resource_updater::ResourceUpdateStatus) -> &'static str {
    use lumen_engines::core_resource_updater::ResourceUpdateStatus as S;
    match status {
        S::Updated => "updated",
        S::UpToDate => "up_to_date",
        S::Error => "error",
        _ => "unknown",
    }
}
