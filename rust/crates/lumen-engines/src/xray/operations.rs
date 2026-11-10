//! Xray connection operations (`engines/xray/operations.py`).
//!
//! The Python module orchestrates `AppController`; the controller itself is
//! ported into `lumen-app`, which depends on this crate, so the exact
//! controller surface used by the module is abstracted as the
//! [`OperationsController`] trait. The `lumen-app` controller implements it
//! (the same trait also serves `singbox/operations.py`).

use std::path::Path;

use serde_json::Value;

use lumen_core::models::{AppSettings, Node, RoutingSettings, utc_now_iso};
use lumen_platform::network_route_context::get_windows_default_route_context;
use lumen_platform::windows_network::wait_for_adapters_gone;

use crate::singbox::runtime_planner::{SingboxRuntimePlan, json_str_or};
use crate::xray::tun::{apply_xray_tun, xray_wintun_dll};

/// Read surface of the Python `application.session_state.XrayRuntimeConfig`
/// dataclass produced by `controller._build_runtime_xray_config`.
pub trait XrayRuntimeConfigView {
    fn config(&self) -> &Value;
    fn config_mut(&mut self) -> &mut Value;
    fn source_path(&self) -> &Path;
    fn used_selected_node(&self) -> bool;
    fn socks_port(&self) -> i64;
    fn http_port(&self) -> i64;
    fn api_port(&self) -> i64;
    fn inbound_tags(&self) -> &[String];
    fn ping_host(&self) -> &str;
    fn ping_port(&self) -> i64;
    fn health_proxy_url(&self) -> &str;
}

/// Keyword arguments of Python `controller._capture_active_session`.
/// `None` mirrors the Python `None` defaults (resolved from settings
/// downstream, e.g. `socks_port=int(settings.local_socks_port)`).
#[derive(Debug, Clone, Default)]
pub struct SessionCapture {
    pub tun: bool,
    pub core: String,
    pub api_port: i64,
    pub clash_api_secret: String,
    pub clash_api_selector: String,
    pub clash_api_node_signatures: Vec<(String, String)>,
    pub hybrid: bool,
    pub socks_port: Option<i64>,
    pub http_port: Option<i64>,
    pub xray_inbound_tags: Option<Vec<String>>,
    pub sidecar_relay_port: i64,
    pub protect_ss_port: i64,
    pub protect_ss_password: String,
    pub ping_host: String,
    pub ping_port: i64,
    pub health_proxy_url: String,
}

/// The fields of `controller._active_session` read by the operations modules
/// (Python `ActiveSessionSnapshot`).
#[derive(Debug, Clone, Default)]
pub struct ActiveSessionView {
    pub node_id: Option<String>,
    pub active_core: String,
    pub tun_mode: bool,
    pub hybrid: bool,
    pub api_port: i64,
    pub socks_port: i64,
    pub http_port: i64,
    pub clash_api_selector: String,
    pub clash_api_secret: String,
    pub clash_api_node_signatures: Vec<(String, String)>,
    pub xray_inbound_tags: Vec<String>,
    pub health_proxy_url: String,
}

/// The `AppController` surface used by `engines/xray/operations.py` and
/// `engines/singbox/operations.py`. Python `ValueError`/`RuntimeError`
/// escalations are modeled as `Err(message)`.
pub trait OperationsController {
    /// Runtime config returned by `_build_runtime_xray_config`.
    type Runtime: XrayRuntimeConfigView;

    // ── controller state (Python attributes) ────────────────────────────────
    fn set_active_core(&mut self, core: &str);
    fn set_switching(&mut self, switching: bool);
    fn xray_api_port(&self) -> i64;
    fn set_xray_api_port(&mut self, port: i64);
    fn protect_ss_port(&self) -> i64;
    fn set_protect_ss_port(&mut self, port: i64);
    fn protect_ss_password(&self) -> String;
    fn set_protect_ss_password(&mut self, password: &str);
    fn selected_node(&self) -> Option<Node>;
    fn selected_node_mut(&mut self) -> Option<&mut Node>;
    fn settings(&self) -> &AppSettings;
    /// Python `controller.state.routing`.
    fn routing(&self) -> RoutingSettings;
    /// Python `controller._active_session`.
    fn active_session(&self) -> Option<ActiveSessionView>;

    // ── status, logging, signals ─────────────────────────────────────────────
    fn log(&mut self, line: &str);
    fn set_connection_status(&mut self, phase: &str, message: &str, level: &str);
    fn emit_metrics_request(&mut self, connected: bool);
    fn emit_connection_changed(&mut self, connected: bool);
    fn save(&mut self);
    /// Python `_refresh_connected_state` returning `(previous, connected)`.
    fn refresh_connected_state(&mut self) -> (bool, bool);
    fn handle_unexpected_disconnect(&mut self);
    fn system_proxy_bypass_lan(&self) -> bool;

    // ── engine managers (Python controller.xray / controller.singbox) ────────
    fn xray_is_running(&self) -> bool;
    fn xray_validate_config(&mut self, xray_path: &str, config: &Value) -> (bool, String);
    fn xray_start(&mut self, xray_path: &str, config: &Value, prevalidated: bool) -> bool;
    fn xray_stop(&mut self, expected: bool, fast: bool) -> bool;
    fn singbox_is_running(&self) -> bool;
    fn singbox_validate_config(&mut self, singbox_path: &str, config: &Value) -> (bool, String);
    fn singbox_stop(&mut self, expected: bool, fast: bool) -> bool;
    /// Python `controller.proxy.enable(...)`; the error carries the
    /// exception text used in user-facing messages.
    fn proxy_enable(
        &mut self,
        http_port: i64,
        socks_port: i64,
        bypass_lan: bool,
        configure_firefox: bool,
    ) -> Result<(), String>;
    fn proxy_disable(&mut self, restore_previous: bool);

    // ── runtime planning and session capture ─────────────────────────────────
    fn build_runtime_xray_config(&mut self, node: Option<&Node>) -> Result<Self::Runtime, String>;
    fn plan_runtime_singbox(
        &mut self,
        node: Option<&Node>,
        tun_mode: bool,
        hot_switch: bool,
    ) -> Result<SingboxRuntimePlan, String>;
    fn start_singbox_runtime_plan(&mut self, plan: &SingboxRuntimePlan) -> bool;
    fn infer_singbox_ping_target(&self, payload: &Value, node: Option<&Node>) -> (String, i64);
    fn capture_active_session(&mut self, node: Option<&Node>, capture: SessionCapture);
}

/// Python `XrayStartResult` dataclass.
pub struct XrayStartResult<R> {
    pub runtime: R,
    pub session_label: String,
}

/// Python `Path.name` (empty for paths without a file name).
pub(crate) fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn start_proxy<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    prev_active_core: &str,
) -> Option<XrayStartResult<C::Runtime>> {
    controller.set_active_core("xray");
    let runtime = match controller.build_runtime_xray_config(node) {
        Ok(runtime) => runtime,
        Err(exc) => {
            controller.set_active_core(prev_active_core);
            controller.set_connection_status("error", &exc, "error");
            return None;
        }
    };

    let source_name = file_name_of(runtime.source_path());
    let mut session_label = source_name.clone();
    if runtime.used_selected_node() && node.is_some() {
        let node_name = node.map(|node| node.name.clone()).unwrap_or_default();
        session_label = format!("{source_name} / {node_name}");
    }
    controller.set_connection_status(
        "starting",
        &format!("Запуск прокси: {session_label}..."),
        "info",
    );
    if runtime.used_selected_node() && node.is_some() {
        let node = node.expect("checked above");
        let mut protocol = json_str_or(node.outbound.get("protocol"), "");
        if protocol.is_empty() {
            protocol = node.scheme.clone();
        }
        let protocol = protocol.trim().to_lowercase();
        if protocol == "xray_config" {
            controller.log(&format!(
                "[xray] full AUTO profile selected with its observatory and balancer: {}",
                node.name
            ));
        } else {
            controller.log(&format!(
                "[xray] outbound tag 'proxy' replaced from selected node: {}",
                node.name
            ));
        }
    }

    controller.set_xray_api_port(runtime.api_port());
    let xray_path = controller.settings().xray_path.clone();
    if !controller.xray_start(&xray_path, runtime.config(), false) {
        controller.set_active_core(prev_active_core);
        return None;
    }

    if controller.settings().enable_system_proxy {
        if runtime.http_port() <= 0 || runtime.socks_port() <= 0 {
            controller.xray_stop(true, false);
            controller.set_connection_status(
                "error",
                "В raw xray config нет HTTP/SOCKS inbound портов для включения системного прокси.",
                "error",
            );
            controller.set_active_core(prev_active_core);
            return None;
        }
        let bypass_lan = controller.system_proxy_bypass_lan();
        let configure_firefox = controller.settings().firefox_proxy_integration;
        if let Err(exc) = controller.proxy_enable(
            runtime.http_port(),
            runtime.socks_port(),
            bypass_lan,
            configure_firefox,
        ) {
            controller.xray_stop(true, false);
            controller.set_connection_status(
                "error",
                &format!("Не удалось включить системный прокси: {exc}"),
                "error",
            );
            controller.set_active_core(prev_active_core);
            return None;
        }
    } else {
        controller.proxy_disable(true);
    }

    Some(XrayStartResult {
        runtime,
        session_label,
    })
}

pub fn start_tun<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    prev_active_core: &str,
) -> Option<XrayStartResult<C::Runtime>> {
    // TUN on Xray alone (main core: Xray).
    controller.set_active_core("xray");
    let settings = controller.settings().clone();
    if xray_wintun_dll(&settings.xray_path).is_none() {
        controller.set_active_core(prev_active_core);
        controller.set_connection_status(
            "error",
            "Для TUN на ядре Xray нужен wintun.dll рядом с xray.exe. Обновите Xray в разделе «Обновления» или выберите Auto в «Настройки → Сеть → Основное ядро».",
            "error",
        );
        return None;
    }

    // After a core switch the sing-box tunnel may still be going away.
    if !wait_for_adapters_gone(&["singbox_tun", "lumen_xray"], 3.0) {
        controller.log("[tun] previous TUN adapter is still up; starting Xray TUN anyway");
    }
    // Read the physical route before the tunnel takes the default route.
    let route_context = get_windows_default_route_context(true);
    if let Some(context) = route_context.as_ref() {
        let alias = if context.interface_alias.is_empty() {
            "auto"
        } else {
            context.interface_alias.as_str()
        };
        controller.log(&format!("[tun] Xray outbound interface: {alias}"));
    }
    let routing = controller.routing();
    let mut runtime = match controller.build_runtime_xray_config(node) {
        Ok(runtime) => runtime,
        Err(exc) => {
            controller.set_active_core(prev_active_core);
            controller.set_connection_status("error", &exc, "error");
            return None;
        }
    };
    if let Err(exc) =
        apply_xray_tun(runtime.config_mut(), &settings, &routing, route_context.as_ref())
    {
        controller.set_active_core(prev_active_core);
        controller.set_connection_status("error", &exc.to_string(), "error");
        return None;
    }

    let source_name = file_name_of(runtime.source_path());
    let mut session_label = source_name.clone();
    if runtime.used_selected_node() && node.is_some() {
        let node_name = node.map(|node| node.name.clone()).unwrap_or_default();
        session_label = format!("{source_name} / {node_name}");
        controller.log(&format!(
            "[xray] outbound tag 'proxy' replaced from selected node: {node_name}"
        ));
    }
    controller.set_connection_status(
        "starting",
        &format!("Запуск VPN (Xray TUN): {session_label}..."),
        "info",
    );
    controller.set_xray_api_port(runtime.api_port());
    if !controller.xray_start(&settings.xray_path, runtime.config(), false) {
        controller.set_active_core(prev_active_core);
        return None;
    }
    controller.log(
        "[tun] Xray TUN started: interface lumen_xray, auto routes and outbound interface",
    );
    Some(XrayStartResult {
        runtime,
        session_label,
    })
}

pub fn restart_proxy_core<C: OperationsController>(controller: &mut C, reason: &str) -> bool {
    let node = controller.selected_node();
    controller.set_switching(true);
    let result = restart_proxy_core_inner(controller, node.as_ref(), reason);
    // Python `finally:` block.
    controller.set_switching(false);
    let (_, connected) = controller.refresh_connected_state();
    controller.emit_connection_changed(connected);
    controller.emit_metrics_request(connected);
    result
}

fn restart_proxy_core_inner<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    reason: &str,
) -> bool {
    controller.log(&format!("[proxy-hot-swap] {reason}"));
    let runtime = match controller.build_runtime_xray_config(node) {
        Ok(runtime) => runtime,
        Err(exc) => {
            controller.set_connection_status("error", &exc, "error");
            return false;
        }
    };

    let xray_path = controller.settings().xray_path.clone();
    let (valid, validation_output) = controller.xray_validate_config(&xray_path, runtime.config());
    if !valid {
        controller.set_connection_status(
            "error",
            &format!("Xray не принимает новый конфиг: {validation_output}"),
            "error",
        );
        return false;
    }

    let source_name = file_name_of(runtime.source_path());
    let mut session_label = source_name.clone();
    if runtime.used_selected_node() && node.is_some() {
        let node_name = node.map(|node| node.name.clone()).unwrap_or_default();
        session_label = format!("{source_name} / {node_name}");
    }
    controller.set_connection_status(
        "starting",
        &format!("Переключение на {session_label}..."),
        "info",
    );
    controller.emit_metrics_request(false);
    if controller.xray_is_running() && !controller.xray_stop(true, false) {
        controller.set_connection_status(
            "error",
            "Не удалось остановить предыдущий процесс Xray",
            "error",
        );
        return false;
    }

    controller.set_xray_api_port(runtime.api_port());
    if !controller.xray_start(&xray_path, runtime.config(), true) {
        controller.handle_unexpected_disconnect();
        return false;
    }

    if controller.settings().enable_system_proxy {
        if runtime.http_port() <= 0 || runtime.socks_port() <= 0 {
            controller.xray_stop(true, false);
            controller.set_connection_status(
                "error",
                "В raw xray config нет HTTP/SOCKS inbound портов для включения системного прокси.",
                "error",
            );
            controller.handle_unexpected_disconnect();
            return false;
        }
        let bypass_lan = controller.system_proxy_bypass_lan();
        let configure_firefox = controller.settings().firefox_proxy_integration;
        if let Err(exc) = controller.proxy_enable(
            runtime.http_port(),
            runtime.socks_port(),
            bypass_lan,
            configure_firefox,
        ) {
            controller.xray_stop(true, false);
            controller.set_connection_status(
                "error",
                &format!("Не удалось включить системный прокси: {exc}"),
                "error",
            );
            controller.handle_unexpected_disconnect();
            return false;
        }
    } else {
        controller.proxy_disable(true);
    }

    let session_node = if runtime.used_selected_node() {
        node
    } else {
        None
    };
    if session_node.is_some() {
        if let Some(stored) = controller.selected_node_mut() {
            stored.last_used_at = Some(utc_now_iso());
        }
    }
    controller.capture_active_session(
        session_node,
        SessionCapture {
            tun: false,
            core: "xray".to_string(),
            api_port: controller.xray_api_port(),
            socks_port: Some(runtime.socks_port()),
            http_port: Some(runtime.http_port()),
            xray_inbound_tags: Some(runtime.inbound_tags().to_vec()),
            health_proxy_url: runtime.health_proxy_url().to_string(),
            ping_host: runtime.ping_host().to_string(),
            ping_port: runtime.ping_port(),
            ..SessionCapture::default()
        },
    );
    controller.set_connection_status(
        "running",
        &format!("Переключено: {session_label}"),
        "success",
    );
    controller.save();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Clone, Default)]
    struct TestXrayRuntime {
        config: Value,
        source_path: std::path::PathBuf,
        used_selected_node: bool,
        socks_port: i64,
        http_port: i64,
        api_port: i64,
        inbound_tags: Vec<String>,
        ping_host: String,
        ping_port: i64,
        health_proxy_url: String,
    }

    impl XrayRuntimeConfigView for TestXrayRuntime {
        fn config(&self) -> &Value {
            &self.config
        }
        fn config_mut(&mut self) -> &mut Value {
            &mut self.config
        }
        fn source_path(&self) -> &Path {
            &self.source_path
        }
        fn used_selected_node(&self) -> bool {
            self.used_selected_node
        }
        fn socks_port(&self) -> i64 {
            self.socks_port
        }
        fn http_port(&self) -> i64 {
            self.http_port
        }
        fn api_port(&self) -> i64 {
            self.api_port
        }
        fn inbound_tags(&self) -> &[String] {
            &self.inbound_tags
        }
        fn ping_host(&self) -> &str {
            &self.ping_host
        }
        fn ping_port(&self) -> i64 {
            self.ping_port
        }
        fn health_proxy_url(&self) -> &str {
            &self.health_proxy_url
        }
    }

    struct FakeController {
        core: String,
        switching: bool,
        xray_api_port: i64,
        node: Option<Node>,
        settings: AppSettings,
        statuses: Vec<(String, String)>,
        logs: Vec<String>,
        metrics: Vec<bool>,
        proxy_enabled: Option<(i64, i64, bool, bool)>,
        proxy_disabled: usize,
        started: Option<(String, bool)>,
        stopped: usize,
        validate_result: (bool, String),
        runtime: Option<TestXrayRuntime>,
        start_ok: bool,
    }

    impl FakeController {
        fn new(runtime: TestXrayRuntime) -> Self {
            Self {
                core: "singbox".to_string(),
                switching: false,
                xray_api_port: 0,
                node: None,
                settings: AppSettings::default(),
                statuses: Vec::new(),
                logs: Vec::new(),
                metrics: Vec::new(),
                proxy_enabled: None,
                proxy_disabled: 0,
                started: None,
                stopped: 0,
                validate_result: (true, String::new()),
                runtime: Some(runtime),
                start_ok: true,
            }
        }
    }

    impl OperationsController for FakeController {
        type Runtime = TestXrayRuntime;

        fn set_active_core(&mut self, core: &str) {
            self.core = core.to_string();
        }
        fn set_switching(&mut self, switching: bool) {
            self.switching = switching;
        }
        fn xray_api_port(&self) -> i64 {
            self.xray_api_port
        }
        fn set_xray_api_port(&mut self, port: i64) {
            self.xray_api_port = port;
        }
        fn protect_ss_port(&self) -> i64 {
            0
        }
        fn set_protect_ss_port(&mut self, _port: i64) {}
        fn protect_ss_password(&self) -> String {
            String::new()
        }
        fn set_protect_ss_password(&mut self, _password: &str) {}
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
            None
        }
        fn log(&mut self, line: &str) {
            self.logs.push(line.to_string());
        }
        fn set_connection_status(&mut self, phase: &str, message: &str, _level: &str) {
            self.statuses.push((phase.to_string(), message.to_string()));
        }
        fn emit_metrics_request(&mut self, connected: bool) {
            self.metrics.push(connected);
        }
        fn emit_connection_changed(&mut self, _connected: bool) {}
        fn save(&mut self) {}
        fn refresh_connected_state(&mut self) -> (bool, bool) {
            (false, false)
        }
        fn handle_unexpected_disconnect(&mut self) {}
        fn system_proxy_bypass_lan(&self) -> bool {
            false
        }
        fn xray_is_running(&self) -> bool {
            false
        }
        fn xray_validate_config(&mut self, _path: &str, _config: &Value) -> (bool, String) {
            self.validate_result.clone()
        }
        fn xray_start(&mut self, path: &str, _config: &Value, prevalidated: bool) -> bool {
            self.started = Some((path.to_string(), prevalidated));
            self.start_ok
        }
        fn xray_stop(&mut self, _expected: bool, _fast: bool) -> bool {
            self.stopped += 1;
            true
        }
        fn singbox_is_running(&self) -> bool {
            false
        }
        fn singbox_validate_config(&mut self, _path: &str, _config: &Value) -> (bool, String) {
            (true, String::new())
        }
        fn singbox_stop(&mut self, _expected: bool, _fast: bool) -> bool {
            true
        }
        fn proxy_enable(
            &mut self,
            http_port: i64,
            socks_port: i64,
            bypass_lan: bool,
            configure_firefox: bool,
        ) -> Result<(), String> {
            self.proxy_enabled = Some((http_port, socks_port, bypass_lan, configure_firefox));
            Ok(())
        }
        fn proxy_disable(&mut self, _restore_previous: bool) {
            self.proxy_disabled += 1;
        }
        fn build_runtime_xray_config(&mut self, _node: Option<&Node>) -> Result<TestXrayRuntime, String> {
            self.runtime
                .clone()
                .ok_or_else(|| "no runtime fixture".to_string())
        }
        fn plan_runtime_singbox(
            &mut self,
            _node: Option<&Node>,
            _tun_mode: bool,
            _hot_switch: bool,
        ) -> Result<crate::singbox::runtime_planner::SingboxRuntimePlan, String> {
            Err("not used by the xray tests".to_string())
        }
        fn start_singbox_runtime_plan(
            &mut self,
            _plan: &crate::singbox::runtime_planner::SingboxRuntimePlan,
        ) -> bool {
            false
        }
        fn infer_singbox_ping_target(&self, _payload: &Value, _node: Option<&Node>) -> (String, i64) {
            (String::new(), 0)
        }
        fn capture_active_session(&mut self, _node: Option<&Node>, _capture: SessionCapture) {}
    }

    fn runtime_fixture() -> TestXrayRuntime {
        TestXrayRuntime {
            config: json!({"inbounds": [], "outbounds": []}),
            source_path: std::path::PathBuf::from("default.json"),
            used_selected_node: true,
            socks_port: 10808,
            http_port: 10809,
            api_port: 15490,
            inbound_tags: vec!["socks-in".to_string()],
            ..TestXrayRuntime::default()
        }
    }

    #[test]
    fn start_proxy_starts_core_and_enables_system_proxy() {
        let mut node = Node::new();
        node.name = "Test node".to_string();
        node.scheme = "vless".to_string();
        node.outbound = json!({"protocol": "vless"});
        let mut controller = FakeController::new(runtime_fixture());
        controller.node = Some(node);
        controller.settings.xray_path = "xray.exe".to_string();

        let node = controller.node.clone();
        let result = start_proxy(&mut controller, node.as_ref(), "singbox");

        let result = result.expect("started");
        assert_eq!(result.session_label, "default.json / Test node");
        assert_eq!(controller.core, "xray");
        assert_eq!(controller.xray_api_port, 15490);
        assert_eq!(
            controller.statuses,
            vec![(
                "starting".to_string(),
                "Запуск прокси: default.json / Test node...".to_string()
            )]
        );
        assert!(controller
            .logs
            .iter()
            .any(|line| line
                == "[xray] outbound tag 'proxy' replaced from selected node: Test node"));
        assert_eq!(controller.started, Some(("xray.exe".to_string(), false)));
        assert_eq!(controller.proxy_enabled, Some((10809, 10808, false, false)));
        assert_eq!(controller.stopped, 0);
    }

    #[test]
    fn start_proxy_reports_missing_inbound_ports_for_system_proxy() {
        let mut controller = FakeController::new(TestXrayRuntime {
            http_port: 0,
            socks_port: 0,
            ..runtime_fixture()
        });
        controller.settings.xray_path = "xray.exe".to_string();

        let result = start_proxy(&mut controller, None, "singbox");

        assert!(result.is_none());
        assert_eq!(controller.core, "singbox");
        assert_eq!(
            controller.statuses,
            vec![
                (
                    "starting".to_string(),
                    "Запуск прокси: default.json...".to_string()
                ),
                (
                    "error".to_string(),
                    "В raw xray config нет HTTP/SOCKS inbound портов для включения системного прокси."
                        .to_string()
                ),
            ]
        );
        assert_eq!(controller.stopped, 1);
    }

    #[test]
    fn restart_proxy_core_reports_rejected_config_and_resets_switching() {
        let mut controller = FakeController::new(runtime_fixture());
        controller.validate_result = (false, "boom".to_string());

        let ok = restart_proxy_core(&mut controller, "settings changed");

        assert!(!ok);
        assert!(!controller.switching);
        assert_eq!(
            controller.statuses,
            vec![(
                "error".to_string(),
                "Xray не принимает новый конфиг: boom".to_string()
            )]
        );
        assert_eq!(controller.metrics, vec![false]);
    }

    #[test]
    fn restart_proxy_core_switches_and_captures_session() {
        let mut node = Node::new();
        node.name = "Test node".to_string();
        let mut controller = FakeController::new(runtime_fixture());
        controller.node = Some(node);
        controller.settings.xray_path = "xray.exe".to_string();

        let ok = restart_proxy_core(&mut controller, "settings changed");

        assert!(ok);
        assert_eq!(controller.started, Some(("xray.exe".to_string(), true)));
        assert_eq!(
            controller.statuses,
            vec![
                (
                    "starting".to_string(),
                    "Переключение на default.json / Test node...".to_string()
                ),
                (
                    "running".to_string(),
                    "Переключено: default.json / Test node".to_string()
                ),
            ]
        );
        assert_eq!(controller.metrics, vec![false, false]);
    }
}
