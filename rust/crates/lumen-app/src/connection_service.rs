//! Connect, disconnect and reconnect through the selected engine.

use std::collections::HashSet;
use std::path::PathBuf;

use lumen_core::constants::{DEFAULT_DISCORD_SOCKS_PORT, DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};
use lumen_core::models::{AppSettings, Node, bool_field, str_list, utc_now_iso};
use lumen_core::routing_presets::{normalize_regional_preset, preset_requires_regional_geodata};
use lumen_engines::core_resource_updater::regional_geodata_installed;
use lumen_engines::singbox::operations as singbox;
use lumen_engines::xray::operations::{self as xray, SessionCapture, XrayRuntimeConfigView};
use lumen_platform::process_conflicts::scan_network_conflicts;
use lumen_platform::startup::is_process_elevated;
use serde_json::Value;

use crate::node_runtime_service::{
    core_preference_of, node_uses_xray_sidecar, proxy_core_for_node,
};
use crate::runtime_services::{
    RuntimeController, cleanup_connection_runtime_state, stop_active_connection_processes,
};
use crate::server_preflight::prepare_connection_node;

pub use crate::runtime_introspection::find_free_api_port;

/// Controller inputs needed before calling the existing engine operations.
pub trait ConnectionController: RuntimeController {
    fn locked(&self) -> bool;
    fn owned_core_process_pids(&self) -> HashSet<u32>;
    fn owned_core_executable_paths(&self) -> HashSet<PathBuf>;
    fn is_singbox_editor_mode(&self) -> bool;
    fn uses_xray_raw_config(&self) -> bool;
    fn can_connect_without_selected_node(&self) -> bool;
    fn active_singbox_config_name(&self) -> String;
    fn active_xray_config_name(&self) -> String;
    fn schedule_save(&mut self);
    fn reset_tun_log_count(&mut self);
    fn disable_discord_proxy(&mut self) -> String;

    fn scan_conflicts(&self, ports: &HashSet<u16>) -> Value {
        scan_network_conflicts(
            ports,
            Some(&self.owned_core_process_pids()),
            Some(&self.owned_core_executable_paths()),
        )
    }

    fn has_regional_geodata(&self, region: &str) -> bool {
        regional_geodata_installed(region, None, None)
    }

    fn is_elevated(&self) -> bool {
        is_process_elevated()
    }
}

pub fn should_block_vpn_conflicts(settings: Option<&AppSettings>) -> bool {
    settings.is_none_or(|settings| settings.block_vpn_conflicts)
}

pub fn connect_selected(
    controller: &mut impl ConnectionController,
    allow_during_reconnect: bool,
) -> bool {
    if controller.runtime().connecting {
        controller.set_connection_status("starting", "Подключение уже выполняется...", "info");
        return false;
    }
    controller.runtime_mut().connecting = true;
    controller.runtime_mut().kill_switch_engaged = false;
    let connected = connect_inner(controller, allow_during_reconnect);
    controller.runtime_mut().connecting = false;
    connected
}

fn connect_inner(controller: &mut impl ConnectionController, allow_during_reconnect: bool) -> bool {
    if controller.runtime().reconnecting && !allow_during_reconnect {
        controller.set_connection_status("starting", "Переподключение...", "info");
        return false;
    }
    if controller.locked() {
        controller.set_connection_status(
            "error",
            "Приложение заблокировано. Разблокируйте для подключения.",
            "warning",
        );
        return false;
    }
    let settings = controller.settings().clone();
    if should_block_vpn_conflicts(Some(&settings)) && !controller.runtime().reconnecting {
        let ports = [
            conflict_port(settings.local_socks_port, DEFAULT_SOCKS_PORT),
            conflict_port(settings.local_http_port, DEFAULT_HTTP_PORT),
            DEFAULT_DISCORD_SOCKS_PORT,
        ]
        .into();
        if let Some(message) = conflict_message(&controller.scan_conflicts(&ports)) {
            controller.set_connection_status("error", &message, "error");
            return false;
        }
    }
    if preset_requires_regional_geodata(&controller.routing().preset_id)
        && !controller.has_regional_geodata(&normalize_regional_preset(&settings.regional_preset))
    {
        controller.set_connection_status("error", "Файлы маршрутизации выбранного региона отсутствуют или повреждены. Обновите geoip/geosite и повторите подключение.", "error");
        return false;
    }
    let singbox_editor = controller.is_singbox_editor_mode();
    let xray_raw = controller.uses_xray_raw_config();
    let has_node = controller.selected_node().is_some();
    if !has_node && !controller.can_connect_without_selected_node() {
        let message = if singbox_editor || xray_raw {
            "В конфиге есть outbound tag `proxy`. Сначала выберите сервер."
        } else {
            "Сначала выберите сервер."
        };
        controller.set_connection_status("error", message, "warning");
        return false;
    }
    let reset = !controller.runtime().auto_switch_transitioning;
    controller.reset_auto_switch(reset, reset);
    let previous_core = controller.active_core().to_owned();
    controller.set_xray_api_port(0);
    let mut repaired = false;
    let prepared = prepare_connection_node(controller.selected_node_mut(), &settings, |_| {
        repaired = true
    });
    if repaired {
        controller.schedule_save();
    }
    if let Err(problem) = prepared {
        controller.set_connection_status("error", &problem, "error");
        return false;
    }
    let node = controller.selected_node();
    let preference = core_preference_of(Some(&settings));
    let tun = settings.tun_mode;
    let core = connection_core(node.as_ref(), preference, tun);
    log_core_choice(controller, node.as_ref(), preference, core, tun);
    if tun {
        let label = match &node {
            Some(node) => node.name.clone(),
            None if singbox_editor => controller.active_singbox_config_name(),
            None if xray_raw => controller.active_xray_config_name(),
            None => "unknown".into(),
        };
        let admin = controller.is_elevated();
        controller.log(&format!(
            "[tun] attempting TUN connect, admin={}",
            if admin { "True" } else { "False" }
        ));
        controller.set_connection_status("starting", &format!("Запуск VPN: {label}..."), "info");
        if !admin {
            controller.log("[tun] NOT admin — aborting");
            controller.set_connection_status("error", "Режим TUN требует прав Администратора. Запустите приложение от имени Администратора.", "error");
            return false;
        }
        controller.proxy_disable(true);
        if settings.discord_proxy_enabled {
            let message = controller.disable_discord_proxy();
            controller.log(&format!("[discord-proxy] disabled for TUN: {message}"));
        }
        controller.reset_tun_log_count();
    }
    let (session_node, label, suffix, capture) = if core == "singbox" {
        let result = if tun {
            singbox::start_tun(controller, node.as_ref(), &previous_core)
        } else {
            singbox::start_proxy(controller, node.as_ref(), &previous_core)
        };
        let Some(result) = result else {
            return false;
        };
        let plan = result.plan;
        let session_node = node
            .as_ref()
            .filter(|_| !singbox_editor || plan.used_selected_node);
        let (ping_host, ping_port) =
            controller.infer_singbox_ping_target(&plan.singbox_config, session_node);
        let hybrid = plan.is_hybrid();
        let capture = SessionCapture {
            hybrid,
            ping_host,
            ping_port,
            clash_api_secret: plan.clash_api_secret,
            clash_api_selector: plan.clash_api_selector,
            clash_api_node_signatures: plan.clash_api_node_signatures,
            sidecar_relay_port: plan
                .xray_sidecar
                .as_ref()
                .map_or(0, |sidecar| sidecar.relay_port),
            health_proxy_url: plan.health_proxy_url,
            ..capture_base(controller, tun)
        };
        let suffix = if !tun {
            " (sing-box)"
        } else if hybrid {
            " (TUN, xray sidecar)"
        } else {
            " (TUN)"
        };
        (session_node, result.session_label, suffix, capture)
    } else {
        let result = if tun {
            xray::start_tun(controller, node.as_ref(), &previous_core)
        } else {
            xray::start_proxy(controller, node.as_ref(), &previous_core)
        };
        let Some(result) = result else {
            return false;
        };
        let runtime = result.runtime;
        let session_node = node
            .as_ref()
            .filter(|_| !xray_raw || runtime.used_selected_node());
        let capture = SessionCapture {
            socks_port: Some(runtime.socks_port()),
            http_port: Some(runtime.http_port()),
            xray_inbound_tags: Some(runtime.inbound_tags().to_vec()),
            ping_host: runtime.ping_host().into(),
            ping_port: runtime.ping_port(),
            health_proxy_url: runtime.health_proxy_url().into(),
            ..capture_base(controller, tun)
        };
        let suffix = if tun { " (TUN, Xray)" } else { "" };
        (session_node, result.session_label, suffix, capture)
    };
    if session_node.is_some()
        && let Some(selected) = controller.selected_node_mut()
    {
        selected.last_used_at = Some(utc_now_iso());
    }
    controller.set_connection_status(
        "running",
        &format!("Подключено: {label}{suffix}"),
        "success",
    );
    let core = capture.core.clone();
    controller.capture_active_session(session_node, capture);
    controller.save();
    controller.start_traffic_session(&label, &core);
    true
}

fn capture_base(controller: &impl RuntimeController, tun: bool) -> SessionCapture {
    SessionCapture {
        tun,
        core: controller.active_core().to_owned(),
        api_port: controller.xray_api_port(),
        protect_ss_port: controller.protect_ss_port(),
        protect_ss_password: controller.protect_ss_password(),
        ..Default::default()
    }
}

fn connection_core(node: Option<&Node>, preference: &str, tun: bool) -> &'static str {
    if tun && preference != "xray" {
        "singbox"
    } else {
        proxy_core_for_node(node, preference)
    }
}

fn log_core_choice(
    controller: &mut impl ConnectionController,
    node: Option<&Node>,
    preference: &str,
    core: &str,
    tun: bool,
) {
    let Some(node) = node else {
        return;
    };
    let label = [&*node.name, &*node.server]
        .into_iter()
        .find(|label| !label.is_empty())
        .unwrap_or("?");
    let engine = if core == "xray" {
        "Xray"
    } else if (preference == "singbox" || tun) && node_uses_xray_sidecar(Some(node), preference) {
        "sing-box extended + Xray sidecar"
    } else {
        "sing-box extended"
    };
    controller.log(&format!(
        "[core] {label}: {engine} (основное ядро: {preference})"
    ));
}

fn conflict_port(value: i64, default: u16) -> u16 {
    u16::try_from(value)
        .ok()
        .filter(|port| *port != 0)
        .unwrap_or(default)
}

fn conflict_message(conflicts: &Value) -> Option<String> {
    let apps = str_list(conflicts, "apps");
    if !apps.is_empty() {
        let names = apps.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
        Some(format!(
            "Нельзя запустить Lumen: одновременно работает другой VPN/прокси-клиент — {names}. Отключите или закройте {names} и повторите запуск."
        ))
    } else if bool_field(conflicts, "unknown_client", false) {
        Some("Нельзя запустить Lumen: уже работает другой VPN/прокси-клиент. Отключите или закройте его и повторите запуск.".into())
    } else {
        None
    }
}

pub struct DisconnectOptions {
    pub disable_proxy: bool,
    pub emit_status: bool,
    pub fast: bool,
}

impl Default for DisconnectOptions {
    fn default() -> Self {
        Self {
            disable_proxy: true,
            emit_status: true,
            fast: false,
        }
    }
}

pub fn disconnect_current(
    controller: &mut impl RuntimeController,
    options: DisconnectOptions,
) -> bool {
    let runtime = controller.runtime_mut();
    runtime.disconnecting = true;
    runtime.kill_switch_engaged = false;
    let reset_cycle = !runtime.auto_switch_transitioning;
    let reset_cooldown = reset_cycle && !runtime.reconnecting;
    cleanup_connection_runtime_state(controller, true, reset_cycle, reset_cooldown);
    let tun = controller.active_session().map_or_else(
        || controller.settings().tun_mode,
        |session| session.tun_mode,
    );
    if options.emit_status && tun {
        controller.emit_status("info", "Остановка VPN...");
    }
    let stopped = stop_active_connection_processes(controller, options.disable_proxy, options.fast);
    if stopped {
        controller.set_active_core("xray");
        controller.clear_active_session();
    }
    if options.emit_status {
        if stopped {
            controller.set_connection_status("idle", "Отключено", "info");
        } else {
            controller.set_connection_status(
                "error",
                "Не удалось корректно остановить подключение",
                "error",
            );
        }
    }
    controller.runtime_mut().disconnecting = false;
    stopped
}

pub fn reconnect(controller: &mut impl ConnectionController, reason: &str) -> bool {
    if controller.runtime().reconnecting {
        controller.set_connection_status("starting", "Переподключение уже выполняется...", "info");
        return false;
    }
    controller.runtime_mut().reconnecting = true;
    controller.set_switching(true);
    controller.log(&format!("[reconnect] {reason}"));
    controller.set_connection_status("starting", "Переподключение...", "info");
    let stopped = disconnect_current(
        controller,
        DisconnectOptions {
            disable_proxy: false,
            emit_status: false,
            fast: true,
        },
    );
    if !stopped {
        controller.set_connection_status(
            "error",
            "Не удалось остановить предыдущий процесс Xray",
            "error",
        );
    }
    let connected = stopped && connect_selected(controller, true);
    if !connected && controller.settings().enable_system_proxy {
        controller.proxy_disable(true);
    }
    controller.runtime_mut().reconnecting = false;
    controller.set_switching(false);
    controller.runtime_mut().auto_switch_transitioning = false;
    let (_, running) = controller.refresh_connected_state();
    controller.emit_connection_changed(running);
    controller.emit_metrics_request(running);
    connected
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use lumen_core::link_parser::parse_single;
    use lumen_core::models::RoutingSettings;
    use lumen_core::traffic_history::ProcessStatsUpdate;
    use lumen_engines::singbox::runtime_planner::SingboxRuntimePlan;
    use lumen_engines::xray::operations::{ActiveSessionView, OperationsController};
    use lumen_platform::process_traffic_collector::ProcessTrafficSnapshot;
    use serde_json::json;

    use super::*;
    use crate::live_metrics_worker::LiveMetricsSample;
    use crate::runtime_services::{
        ConnectionRuntime, MetricsUpdate, ShutdownController, handle_unexpected_disconnect,
        on_core_state_changed, on_live_metrics, shutdown,
    };
    use crate::xray_runtime_service::XrayRuntimeConfig;

    struct Controller {
        runtime: ConnectionRuntime,
        settings: AppSettings,
        node: Option<Node>,
        core: String,
        session: Option<ActiveSessionView>,
        capture: Option<SessionCapture>,
        xray: bool,
        singbox: bool,
        connected: bool,
        api: i64,
        stop_ok: bool,
        conflicts: bool,
        geodata: bool,
        admin: bool,
        pending_worker: bool,
        events: Vec<String>,
    }

    impl Controller {
        fn new() -> Self {
            let mut settings = AppSettings::default();
            settings.core_preference = "xray".into();
            settings.enable_system_proxy = true;
            Self {
                settings,
                runtime: ConnectionRuntime::default(),
                node: Some(
                    parse_single(
                        "vless://00000000-0000-0000-0000-000000000001@node.example:443#Server",
                    )
                    .unwrap(),
                ),
                core: "xray".into(),
                session: None,
                capture: None,
                xray: false,
                singbox: false,
                connected: false,
                api: 0,
                stop_ok: true,
                conflicts: false,
                geodata: true,
                admin: true,
                pending_worker: false,
                events: Vec::new(),
            }
        }

        fn event(&mut self, event: impl Into<String>) {
            self.events.push(event.into());
        }

        fn position(&self, event: &str) -> usize {
            self.events.iter().position(|item| item == event).unwrap()
        }
    }

    impl OperationsController for Controller {
        type Runtime = XrayRuntimeConfig;

        fn set_active_core(&mut self, core: &str) {
            self.core = core.into();
        }
        fn set_switching(&mut self, switching: bool) {
            self.runtime.switching = switching;
        }
        fn xray_api_port(&self) -> i64 {
            self.api
        }
        fn set_xray_api_port(&mut self, port: i64) {
            self.api = port;
        }
        fn protect_ss_port(&self) -> i64 {
            0
        }
        fn set_protect_ss_port(&mut self, _: i64) {}
        fn protect_ss_password(&self) -> String {
            String::new()
        }
        fn set_protect_ss_password(&mut self, _: &str) {}
        fn selected_node(&self) -> Option<Node> {
            self.node.clone()
        }
        fn selected_node_mut(&mut self) -> Option<&mut Node> {
            self.node.as_mut()
        }
        fn settings(&self) -> &AppSettings {
            &self.settings
        }
        fn routing(&self) -> RoutingSettings {
            RoutingSettings::default()
        }
        fn active_session(&self) -> Option<ActiveSessionView> {
            self.session.clone()
        }
        fn log(&mut self, line: &str) {
            self.event(line);
        }
        fn set_connection_status(&mut self, phase: &str, message: &str, _: &str) {
            self.event(format!("{phase}: {message}"));
        }
        fn emit_metrics_request(&mut self, running: bool) {
            self.event(format!("metrics:{running}"));
        }
        fn emit_connection_changed(&mut self, running: bool) {
            self.event(format!("connection:{running}"));
        }
        fn save(&mut self) {
            self.event("save");
        }
        fn refresh_connected_state(&mut self) -> (bool, bool) {
            let connected = if self.core == "singbox" {
                self.singbox
                    && (self.session.as_ref().is_none_or(|session| !session.hybrid) || self.xray)
            } else {
                self.xray
            };
            (std::mem::replace(&mut self.connected, connected), connected)
        }
        fn handle_unexpected_disconnect(&mut self) {
            handle_unexpected_disconnect(self);
        }
        fn system_proxy_bypass_lan(&self) -> bool {
            true
        }
        fn xray_is_running(&self) -> bool {
            self.xray
        }
        fn xray_validate_config(&mut self, _: &str, _: &Value) -> (bool, String) {
            unreachable!()
        }
        fn xray_start(&mut self, _: &str, _: &Value, _: bool) -> bool {
            self.event("xray start");
            self.xray = true;
            true
        }
        fn xray_stop(&mut self, expected: bool, fast: bool) -> bool {
            assert!(expected);
            self.event(if fast { "xray fast stop" } else { "xray stop" });
            if self.stop_ok {
                self.xray = false;
            }
            self.stop_ok
        }
        fn singbox_is_running(&self) -> bool {
            self.singbox
        }
        fn singbox_validate_config(&mut self, _: &str, _: &Value) -> (bool, String) {
            unreachable!()
        }
        fn singbox_stop(&mut self, expected: bool, fast: bool) -> bool {
            assert!(expected);
            self.event(if fast {
                "singbox fast stop"
            } else {
                "singbox stop"
            });
            if self.stop_ok {
                self.singbox = false;
            }
            self.stop_ok
        }
        fn proxy_enable(&mut self, _: i64, _: i64, _: bool, _: bool) -> Result<(), String> {
            self.event("proxy enable");
            Ok(())
        }
        fn proxy_disable(&mut self, restore_previous: bool) {
            assert!(restore_previous);
            self.event("proxy restore");
        }
        fn build_runtime_xray_config(
            &mut self,
            node: Option<&Node>,
        ) -> Result<Self::Runtime, String> {
            Ok(XrayRuntimeConfig {
                config: json!({}),
                source_path: "test.json".into(),
                has_proxy_outbound: true,
                used_selected_node: node.is_some(),
                socks_port: 10808,
                http_port: 10809,
                api_port: 10085,
                inbound_tags: vec!["http".into()],
                ping_host: "node.example".into(),
                ping_port: 443,
                health_proxy_url: "http://127.0.0.1:10809".into(),
            })
        }
        fn plan_runtime_singbox(
            &mut self,
            _: Option<&Node>,
            _: bool,
            _: bool,
        ) -> Result<SingboxRuntimePlan, String> {
            unreachable!()
        }
        fn start_singbox_runtime_plan(&mut self, _: &SingboxRuntimePlan) -> bool {
            unreachable!()
        }
        fn infer_singbox_ping_target(&self, _: &Value, _: Option<&Node>) -> (String, i64) {
            unreachable!()
        }
        fn capture_active_session(&mut self, node: Option<&Node>, capture: SessionCapture) {
            self.event("capture");
            self.session = Some(ActiveSessionView {
                node_id: node.map(|node| node.id.clone()),
                ..Default::default()
            });
            self.capture = Some(capture);
        }
    }

    impl RuntimeController for Controller {
        fn runtime(&self) -> &ConnectionRuntime {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut ConnectionRuntime {
            &mut self.runtime
        }
        fn active_core(&self) -> &str {
            &self.core
        }
        fn clear_active_session(&mut self) {
            self.session = None;
        }
        fn reset_auto_switch(&mut self, _: bool, _: bool) {
            self.event("reset auto-switch");
        }
        fn emit_status(&mut self, _: &str, message: &str) {
            self.event(message);
        }
        fn emit_live_metrics(&mut self, update: MetricsUpdate<'_>) {
            self.event(match update {
                MetricsUpdate::Sample(sample) => {
                    assert_eq!(sample.down_bps, 20.0);
                    "sample"
                }
                MetricsUpdate::Reset => "metrics reset",
            });
        }
        fn check_auto_switch(&mut self, _: &LiveMetricsSample) {
            self.event("check auto-switch");
        }
        fn start_traffic_session(&mut self, _: &str, _: &str) {
            self.event("traffic start");
        }
        fn end_traffic_session(&mut self) {
            self.event("traffic end");
        }
        fn update_traffic_session(&mut self, stats: &BTreeMap<String, ProcessStatsUpdate>) {
            assert_eq!(stats["probe.exe"].proxy_bytes, 10);
            self.event("traffic update");
        }
        fn save_traffic_periodic(&mut self) {
            self.event("traffic save");
        }
        fn effective_proxy_ports(&self) -> (i64, i64) {
            (10808, 10809)
        }
    }

    impl ConnectionController for Controller {
        fn locked(&self) -> bool {
            false
        }
        fn owned_core_process_pids(&self) -> HashSet<u32> {
            HashSet::new()
        }
        fn owned_core_executable_paths(&self) -> HashSet<PathBuf> {
            HashSet::new()
        }
        fn is_singbox_editor_mode(&self) -> bool {
            false
        }
        fn uses_xray_raw_config(&self) -> bool {
            false
        }
        fn can_connect_without_selected_node(&self) -> bool {
            false
        }
        fn active_singbox_config_name(&self) -> String {
            "singbox.json".into()
        }
        fn active_xray_config_name(&self) -> String {
            "test.json".into()
        }
        fn schedule_save(&mut self) {
            self.event("schedule save");
        }
        fn reset_tun_log_count(&mut self) {
            self.event("reset tun log");
        }
        fn disable_discord_proxy(&mut self) -> String {
            unreachable!()
        }
        fn scan_conflicts(&self, ports: &HashSet<u16>) -> Value {
            assert!(ports.contains(&DEFAULT_DISCORD_SOCKS_PORT));
            if self.conflicts {
                json!({"apps":["Other VPN"]})
            } else {
                json!({})
            }
        }
        fn is_elevated(&self) -> bool {
            self.admin
        }

        fn has_regional_geodata(&self, region: &str) -> bool {
            assert_eq!(region, "russia");
            self.geodata
        }
    }

    impl ShutdownController for Controller {
        fn stop_network_monitor(&mut self) {
            self.event("network stop");
        }
        fn stop_lock_timer(&mut self) {
            self.event("timer stop");
        }
        fn cancel_background_workers(&mut self) {
            self.event("cancel workers");
        }
        fn pending_background_workers(&mut self) -> Vec<&'static str> {
            self.event("poll workers");
            if self.pending_worker {
                vec!["pending worker"]
            } else {
                Vec::new()
            }
        }
        fn zapret_is_running(&self) -> bool {
            true
        }
        fn stop_zapret(&mut self, fast: bool) {
            assert!(fast);
            self.event("zapret stop");
        }
    }

    #[test]
    fn tun_and_proxy_choose_the_python_core_order() {
        assert_eq!(connection_core(None, "auto", true), "singbox");
        assert_eq!(connection_core(None, "xray", true), "xray");
        assert_eq!(connection_core(None, "auto", false), "xray");
    }

    #[test]
    fn initial_conflict_blocks_start_but_reconnect_captures_the_replacement_session() {
        let mut controller = Controller::new();
        controller.conflicts = true;
        assert!(!connect_selected(&mut controller, false));
        assert!(
            controller
                .events
                .iter()
                .any(|event| event.contains("Other VPN"))
        );
        assert!(!controller.runtime.connecting);
        controller.xray = true;
        controller.runtime.auto_switch_transitioning = true;
        assert!(reconnect(&mut controller, "manual"));
        assert!(controller.position("xray fast stop") < controller.position("xray start"));
        assert!(controller.position("proxy enable") < controller.position("capture"));
        assert!(controller.position("capture") < controller.position("metrics:true"));
        let capture = controller.capture.unwrap();
        assert_eq!(capture.xray_inbound_tags.unwrap(), ["http"]);
        assert_eq!(capture.api_port, 10085);
        assert_eq!(capture.ping_host, "node.example");
        assert!(controller.node.unwrap().last_used_at.is_some());
        assert!(
            !controller.runtime.reconnecting
                && !controller.runtime.switching
                && !controller.runtime.auto_switch_transitioning
        );
    }

    #[test]
    fn failed_reconnect_restores_proxy_and_preserves_the_running_session() {
        let mut controller = Controller::new();
        controller.xray = true;
        controller.stop_ok = false;
        controller.session = Some(ActiveSessionView::default());
        assert!(!reconnect(&mut controller, "manual"));
        assert!(controller.session.is_some());
        assert!(controller.position("xray fast stop") < controller.position("proxy restore"));
        assert!(!controller.events.contains(&"xray start".into()));
        assert!(!controller.runtime.reconnecting);
    }

    #[test]
    fn tun_without_elevation_leaves_proxy_and_engines_untouched() {
        let mut controller = Controller::new();
        controller.settings.tun_mode = true;
        controller.admin = false;
        assert!(!connect_selected(&mut controller, false));
        assert!(
            controller
                .events
                .iter()
                .any(|event| event.contains("требует прав Администратора"))
        );
        assert!(!controller.events.contains(&"proxy restore".into()));
        assert!(!controller.events.contains(&"xray start".into()));
        assert!(!controller.runtime.connecting);
    }

    #[test]
    fn regional_preset_requires_installed_geodata_before_start() {
        let mut controller = Controller::new();
        controller.geodata = false;
        assert!(!connect_selected(&mut controller, false));
        assert!(
            controller
                .events
                .iter()
                .any(|event| event.contains("geoip/geosite"))
        );
        assert!(!controller.xray);
        assert!(!controller.runtime.connecting);
    }

    #[test]
    fn core_loss_with_kill_switch_keeps_proxy_and_stops_the_owned_sidecar() {
        let mut controller = Controller::new();
        controller.settings.kill_switch = true;
        controller.runtime.desired_connected = true;
        controller.core = "singbox".into();
        controller.connected = true;
        controller.xray = true;
        controller.session = Some(ActiveSessionView {
            hybrid: true,
            ..Default::default()
        });
        on_core_state_changed(&mut controller);
        assert!(controller.runtime.kill_switch_engaged);
        assert!(!controller.runtime.desired_connected);
        assert!(controller.session.is_none());
        assert!(controller.position("metrics reset") < controller.position("xray stop"));
        assert!(!controller.events.contains(&"proxy restore".into()));
    }

    #[test]
    fn live_metrics_update_history_and_save_every_fifteen_samples() {
        let mut controller = Controller::new();
        let sample = LiveMetricsSample {
            up_bps: 10.0,
            down_bps: 20.0,
            process_stats: Some(vec![ProcessTrafficSnapshot {
                exe: "probe.exe".into(),
                proxy_bytes: 10,
                ..Default::default()
            }]),
            connection_events: vec!["network event".into()],
            traffic_available: true,
            traffic_reason: String::new(),
            traffic_checked_at: 1.0,
            upload_total: None,
            download_total: None,
            health: Default::default(),
        };
        for _ in 0..14 {
            on_live_metrics(&mut controller, &sample);
        }
        assert!(!controller.events.contains(&"traffic save".into()));
        on_live_metrics(&mut controller, &sample);
        assert_eq!(controller.events.last().unwrap(), "traffic save");
        assert_eq!(controller.position("sample"), 0);
        assert_eq!(controller.position("network event"), 1);
        assert_eq!(controller.position("check auto-switch"), 2);
    }

    #[tokio::test]
    async fn shutdown_cancels_before_waiting_and_stops_both_engines_at_deadline() {
        let mut controller = Controller::new();
        controller.core = "singbox".into();
        controller.singbox = true;
        controller.xray = true;
        controller.pending_worker = true;
        shutdown(&mut controller, Some(Instant::now())).await;
        assert!(controller.runtime.shutting_down);
        assert!(controller.position("cancel workers") < controller.position("poll workers"));
        assert!(controller.position("singbox fast stop") < controller.position("xray fast stop"));
        assert!(controller.position("xray fast stop") < controller.position("zapret stop"));
        assert!(
            controller
                .events
                .iter()
                .any(|event| event.contains("pending worker did not stop"))
        );
        assert_eq!(controller.events.last().unwrap(), "save");
    }
}
