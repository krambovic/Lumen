//! Connection runtime cleanup, metrics and bounded shutdown.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lumen_core::constants::{DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};
use lumen_core::traffic_history::ProcessStatsUpdate;
use lumen_engines::singbox::manager::SingBoxManager;
use lumen_engines::xray::operations::OperationsController;
use lumen_platform::process_traffic_collector::reset_connection_tracking;
use lumen_platform::subprocess::is_windows_shutting_down;
use lumen_platform::win_proc_monitor::clear_pid_cache;

use crate::connection_service::{DisconnectOptions, disconnect_current};
use crate::live_metrics_worker::{
    LiveMetricsOptions, LiveMetricsSample, LiveMetricsWorker, MetricsMode, MetricsProfile,
};

#[derive(Default)]
pub struct ConnectionRuntime {
    pub connecting: bool,
    pub disconnecting: bool,
    pub reconnecting: bool,
    pub switching: bool,
    pub desired_connected: bool,
    pub auto_switch_transitioning: bool,
    pub kill_switch_engaged: bool,
    pub shutting_down: bool,
    pub system_shutdown: bool,
    cleaning_connection_state: bool,
    traffic_save_counter: u8,
    metrics: Option<LiveMetricsWorker>,
    retired_metrics: Vec<LiveMetricsWorker>,
}

impl ConnectionRuntime {
    fn should_engage_kill_switch(&self, enabled: bool) -> bool {
        enabled && self.desired_connected && !self.reconnecting && !self.auto_switch_transitioning
    }

    pub fn metrics_running(&self) -> bool {
        self.metrics.is_some()
    }

    /// Guard the controller's select branch with `metrics_running`.
    pub async fn next_metrics(&mut self) -> Option<LiveMetricsSample> {
        let sample = self.metrics.as_mut()?.samples.recv().await;
        if sample.is_none() {
            self.stop_metrics();
        }
        sample
    }

    fn stop_metrics(&mut self) {
        if let Some(worker) = self.metrics.take() {
            worker.stop();
            self.retired_metrics.push(worker);
        }
    }

    /// Join only threads that have already finished.
    pub fn reap_metrics(&mut self) -> Vec<std::io::Error> {
        let mut errors = Vec::new();
        let mut index = 0;
        while index < self.retired_metrics.len() {
            if self.retired_metrics[index].is_finished() {
                if let Err(error) = self.retired_metrics.swap_remove(index).join() {
                    errors.push(error);
                }
            } else {
                index += 1;
            }
        }
        errors
    }
}

pub enum MetricsUpdate<'a> {
    Sample(&'a LiveMetricsSample),
    Reset,
}

/// The app-owned state and notifications shared by runtime and engine operations.
/// The engine's `set_switching` must update `runtime().switching`.
pub trait RuntimeController: OperationsController {
    fn runtime(&self) -> &ConnectionRuntime;
    fn runtime_mut(&mut self) -> &mut ConnectionRuntime;
    fn active_core(&self) -> &str;
    fn clear_active_session(&mut self);
    fn reset_auto_switch(&mut self, reset_cooldown: bool, reset_cycle: bool);
    fn emit_status(&mut self, level: &str, message: &str);
    fn emit_live_metrics(&mut self, update: MetricsUpdate<'_>);
    fn check_auto_switch(&mut self, sample: &LiveMetricsSample);
    fn start_traffic_session(&mut self, label: &str, core: &str);
    fn end_traffic_session(&mut self);
    fn update_traffic_session(&mut self, stats: &BTreeMap<String, ProcessStatsUpdate>);
    fn save_traffic_periodic(&mut self);
    fn effective_proxy_ports(&self) -> (i64, i64);
}

pub fn start_metrics_worker(controller: &mut impl RuntimeController) {
    if controller.runtime().shutting_down {
        return;
    }
    let Some(session) = controller.active_session() else {
        return;
    };
    let core = controller.active_core().to_owned();
    controller.log(&format!("[metrics] starting worker, active_core={core}"));
    stop_metrics_worker(controller);
    let (socks, http) = controller.effective_proxy_ports();
    let options = LiveMetricsOptions {
        mode: if core == "singbox" {
            MetricsMode::Singbox
        } else {
            MetricsMode::Xray
        },
        xray_api_port: controller.xray_api_port(),
        xray_inbound_tags: session.xray_inbound_tags,
        clash_api_secret: session.clash_api_secret,
        socks_port: u16::try_from(socks).unwrap_or(DEFAULT_SOCKS_PORT),
        http_port: u16::try_from(http).unwrap_or(DEFAULT_HTTP_PORT),
        profile: MetricsProfile {
            id: session.node_id.unwrap_or_default(),
            outbound_tag: if session.clash_api_selector.is_empty() {
                "proxy".into()
            } else {
                session.clash_api_selector
            },
            health_proxy_url: session.health_proxy_url,
        },
        ..Default::default()
    };
    match LiveMetricsWorker::start(options) {
        Ok(worker) => controller.runtime_mut().metrics = Some(worker),
        Err(error) => controller.log(&format!("[metrics] failed to start worker: {error}")),
    }
}

pub fn stop_metrics_worker(controller: &mut impl RuntimeController) {
    controller.runtime_mut().stop_metrics();
    reap_metrics(controller);
}

fn reap_metrics(controller: &mut impl RuntimeController) {
    for error in controller.runtime_mut().reap_metrics() {
        controller.log(&format!("[metrics] worker failed: {error}"));
    }
}

pub fn cleanup_connection_runtime_state(
    controller: &mut impl RuntimeController,
    end_traffic_session: bool,
    reset_auto_switch_cycle: bool,
    reset_auto_switch_cooldown: bool,
) {
    controller.set_xray_api_port(0);
    controller.set_protect_ss_port(0);
    controller.set_protect_ss_password("");
    controller.runtime_mut().traffic_save_counter = 0;
    controller.reset_auto_switch(reset_auto_switch_cooldown, reset_auto_switch_cycle);
    if end_traffic_session {
        controller.end_traffic_session();
    }
    reset_connection_tracking();
    clear_pid_cache();
}

pub fn stop_active_connection_processes(
    controller: &mut impl RuntimeController,
    disable_proxy: bool,
    fast: bool,
) -> bool {
    let order = if controller.active_core() == "singbox" {
        ["singbox", "xray"]
    } else {
        ["xray", "singbox"]
    };
    let mut stopped = true;
    for core in order {
        let result = match core {
            "xray" if controller.xray_is_running() => controller.xray_stop(true, fast),
            "singbox" if controller.singbox_is_running() => controller.singbox_stop(true, fast),
            _ => true,
        };
        stopped &= result;
    }
    if disable_proxy && controller.settings().enable_system_proxy {
        controller.proxy_disable(true);
    }
    stopped
}

pub fn handle_unexpected_disconnect(controller: &mut impl RuntimeController) {
    let runtime = controller.runtime();
    if runtime.cleaning_connection_state || runtime.shutting_down || runtime.system_shutdown {
        return;
    }
    let reconnecting = runtime.reconnecting;
    let reset_cycle = !runtime.auto_switch_transitioning;
    let kill_switch = runtime.should_engage_kill_switch(controller.settings().kill_switch);
    let runtime = controller.runtime_mut();
    runtime.cleaning_connection_state = true;
    runtime.kill_switch_engaged = kill_switch;
    cleanup_connection_runtime_state(controller, true, reset_cycle, true);
    stop_active_connection_processes(controller, !reconnecting && !kill_switch, false);
    controller.set_active_core("xray");
    controller.clear_active_session();
    if !reconnecting {
        controller.runtime_mut().desired_connected = false;
    }
    if kill_switch {
        controller.emit_status("error", "Kill-switch: соединение разорвано — трафик заблокирован (прокси не сброшен). Переподключитесь.");
        controller
            .log("[kill-switch] unexpected disconnect — system proxy kept enabled (fail-closed)");
    }
    let runtime = controller.runtime_mut();
    runtime.auto_switch_transitioning = false;
    runtime.cleaning_connection_state = false;
}

pub fn on_core_state_changed(controller: &mut impl RuntimeController) {
    if controller.runtime().shutting_down {
        return;
    }
    let (was_connected, is_connected) = controller.refresh_connected_state();
    let switching = controller.runtime().switching;
    if !switching && was_connected != is_connected {
        controller.emit_connection_changed(is_connected);
    }
    if is_connected {
        if !was_connected
            && !switching
            && !controller.runtime().metrics_running()
            && controller.active_session().is_some()
        {
            start_metrics_worker(controller);
        }
    } else {
        stop_metrics_worker(controller);
        if was_connected && !switching {
            controller.emit_live_metrics(MetricsUpdate::Reset);
            if !controller.runtime().disconnecting && !controller.runtime().system_shutdown {
                handle_unexpected_disconnect(controller);
            }
        }
        let runtime = controller.runtime();
        if controller.active_core() == "xray"
            && controller.settings().enable_system_proxy
            && !runtime.reconnecting
            && !runtime.kill_switch_engaged
        {
            controller.proxy_disable(true);
        }
    }
}

pub fn on_live_metrics(controller: &mut impl RuntimeController, sample: &LiveMetricsSample) {
    if controller.runtime().shutting_down {
        return;
    }
    controller.emit_live_metrics(MetricsUpdate::Sample(sample));
    for event in &sample.connection_events {
        controller.log(event);
    }
    controller.check_auto_switch(sample);
    if let Some(stats) = sample
        .process_stats
        .as_ref()
        .filter(|stats| !stats.is_empty())
    {
        let stats = stats
            .iter()
            .map(|process| {
                (
                    process.exe.clone(),
                    ProcessStatsUpdate {
                        upload: process.upload,
                        download: process.download,
                        route: process.route.clone(),
                        proxy_bytes: process.proxy_bytes,
                        direct_bytes: process.direct_bytes,
                        unknown_bytes: process.unknown_bytes,
                    },
                )
            })
            .collect();
        controller.update_traffic_session(&stats);
        let counter = &mut controller.runtime_mut().traffic_save_counter;
        *counter += 1;
        if *counter >= 15 {
            *counter = 0;
            controller.save_traffic_periodic();
        }
    }
}

/// Shutdown hooks cover controller-owned monitors and workers beyond the connection.
pub trait ShutdownController: RuntimeController {
    fn stop_network_monitor(&mut self);
    fn stop_lock_timer(&mut self);
    fn cancel_background_workers(&mut self);
    /// Poll thread/task completion without waiting for controller event delivery.
    fn pending_background_workers(&mut self) -> Vec<&'static str>;
    fn zapret_is_running(&self) -> bool;
    fn stop_zapret(&mut self, fast: bool);
}

pub async fn shutdown(controller: &mut impl ShutdownController, deadline: Option<Instant>) {
    let deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(5));
    controller.runtime_mut().shutting_down = true;
    controller.stop_network_monitor();
    controller.stop_lock_timer();
    stop_metrics_worker(controller);
    controller.cancel_background_workers();
    loop {
        reap_metrics(controller);
        let mut pending = controller.pending_background_workers();
        if !controller.runtime().retired_metrics.is_empty() {
            pending.push("retired metrics");
        }
        if pending.is_empty() {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            for label in pending {
                controller.log(&format!(
                    "[shutdown] {label} did not stop before the deadline"
                ));
            }
            break;
        }
        tokio::time::sleep(Duration::from_millis(10).min(remaining)).await;
    }
    disconnect_current(
        controller,
        DisconnectOptions {
            fast: true,
            ..Default::default()
        },
    );
    if controller.singbox_is_running() {
        controller.singbox_stop(true, true);
    }
    if controller.xray_is_running() {
        controller.xray_stop(true, true);
    }
    if controller.zapret_is_running() {
        controller.stop_zapret(true);
    }
    controller.proxy_disable(true);
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() && !controller.runtime().system_shutdown && !is_windows_shutting_down()
    {
        SingBoxManager::cleanup_orphaned_tun_adapters(remaining.as_secs_f64().min(1.0), "");
    }
    controller.save();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_requires_an_unexpected_loss_of_a_desired_connection() {
        let mut runtime = ConnectionRuntime {
            desired_connected: true,
            ..Default::default()
        };
        assert!(runtime.should_engage_kill_switch(true));
        runtime.reconnecting = true;
        assert!(!runtime.should_engage_kill_switch(true));
        runtime.reconnecting = false;
        runtime.auto_switch_transitioning = true;
        assert!(!runtime.should_engage_kill_switch(true));
    }
}
