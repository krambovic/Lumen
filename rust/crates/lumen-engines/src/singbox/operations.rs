//! Sing-box connection operations (`engines/singbox/operations.py`).
//!
//! Controller hooks are abstracted by
//! [`crate::xray::operations::OperationsController`]; see the module docs of
//! `xray::operations` for the mapping of Python `AppController` attributes.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Value, json};

use lumen_core::models::Node;
use lumen_core::models::utc_now_iso;
use lumen_platform::windows_network::wait_for_adapters_gone;

use crate::singbox::clash_api::{ClashApiError, SelectorSwitchResult, SingboxClashApiClient};
use crate::singbox::runtime_planner::{
    SingboxRuntimePlan, json_str_or, py_str_list_item, singbox_node_source_signature,
    singbox_node_tag,
};
use crate::xray::operations::{OperationsController, SessionCapture, file_name_of};

const HOT_SWITCH_PRUNE_ATTEMPTS: usize = 5;
static CONFIG_ITEM_ERROR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(outbounds|endpoints)\[(\d+)\]").expect("valid regex"));
const SELECTOR_ENFORCE_ATTEMPTS: usize = 4;
const SELECTOR_ENFORCE_DELAY: Duration = Duration::from_millis(250);

/// Python `SingboxStartResult` dataclass.
pub struct SingboxStartResult {
    pub plan: SingboxRuntimePlan,
    pub session_label: String,
}

pub fn start_runtime<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    prev_active_core: &str,
    tun_mode: bool,
) -> Option<SingboxStartResult> {
    controller.set_active_core("singbox");
    let plan_started = Instant::now();
    let plan = match controller.plan_runtime_singbox(node, tun_mode, true) {
        Ok(plan) => plan,
        Err(exc) => {
            controller.set_active_core(prev_active_core);
            controller.set_connection_status("error", &exc, "error");
            return None;
        }
    };

    let log_domain = if tun_mode { "tun" } else { "proxy" };
    controller.log(&format!(
        "[{log_domain}] runtime plan ready in {} ms",
        plan_started.elapsed().as_millis()
    ));

    let node_name = node.map(|node| node.name.clone()).unwrap_or_default();
    let source_name = file_name_of(&plan.source_path);
    let mut session_label = source_name.clone();
    if plan.used_selected_node && node.is_some() {
        session_label = format!("{source_name} / {node_name}");
    }
    let start_message = if plan.is_hybrid() {
        format!("Запуск VPN: {session_label} (sing-box + xray sidecar)...")
    } else if tun_mode {
        format!("Запуск VPN: {session_label}...")
    } else {
        format!("Запуск прокси: {session_label}...")
    };
    controller.set_connection_status("starting", &start_message, "info");
    controller.log(&format!(
        "[{log_domain}] sing-box planner outcome: {} from {}",
        plan.outcome,
        plan.source_path.display()
    ));
    let route_final = config_final(&plan.singbox_config, "route");
    let dns_final = config_final(&plan.singbox_config, "dns");
    controller.log(&format!(
        "[{log_domain}] routing final={route_final} dns_final={dns_final}"
    ));
    for line in runtime_summary_lines(&plan.singbox_config) {
        controller.log(&format!("[{log_domain}] {line}"));
    }
    if plan.used_selected_node && node.is_some() {
        if plan.is_hybrid() {
            controller.log(&format!(
                "[{log_domain}] outbound tag 'proxy' replaced with local xray relay for unsupported node: {node_name}"
            ));
        } else {
            controller.log(&format!(
                "[{log_domain}] outbound tag 'proxy' replaced from selected node: {node_name}"
            ));
        }
    }

    let validation_started = Instant::now();
    let (plan, valid, validation_output) = validate_runtime_plan(controller, plan, node, tun_mode);
    controller.log(&format!(
        "[{log_domain}] core config check completed in {} ms",
        validation_started.elapsed().as_millis()
    ));
    if !valid {
        controller.set_active_core(prev_active_core);
        controller.set_connection_status(
            "error",
            &format!("sing-box не принимает новый конфиг: {validation_output}"),
            "error",
        );
        return None;
    }
    if let Some(sidecar) = plan.xray_sidecar.as_ref() {
        let xray_path = controller.settings().xray_path.clone();
        let (valid, validation_output) =
            controller.xray_validate_config(&xray_path, &sidecar.config);
        if !valid {
            controller.set_active_core(prev_active_core);
            controller.set_connection_status(
                "error",
                &format!("Xray sidecar не принимает новый конфиг: {validation_output}"),
                "error",
            );
            return None;
        }
    }

    if !controller.start_singbox_runtime_plan(&plan) {
        let message = if plan.is_hybrid() {
            "Не удалось запустить sing-box hybrid runtime. Смотрите причину в последних строках лога sing-box."
        } else if tun_mode {
            "Не удалось запустить sing-box TUN runtime. Смотрите причину в последних строках лога sing-box."
        } else {
            "Не удалось запустить sing-box proxy runtime. Смотрите причину в последних строках лога sing-box."
        };
        controller.set_connection_status("error", message, "error");
        controller.set_active_core(prev_active_core);
        return None;
    }

    if !tun_mode {
        let settings = controller.settings().clone();
        let applied = if settings.enable_system_proxy {
            let bypass_lan = controller.system_proxy_bypass_lan();
            controller.proxy_enable(
                settings.local_http_port,
                settings.local_socks_port,
                bypass_lan,
                settings.firefox_proxy_integration,
            )
        } else {
            controller.proxy_disable(true);
            Ok(())
        };
        if let Err(exc) = applied {
            controller.singbox_stop(true, false);
            if controller.xray_is_running() {
                controller.xray_stop(true, false);
            }
            controller.set_active_core(prev_active_core);
            controller.set_connection_status(
                "error",
                &format!("Не удалось применить системный прокси для sing-box: {exc}"),
                "error",
            );
            return None;
        }
    }

    Some(SingboxStartResult {
        plan,
        session_label,
    })
}

pub fn start_tun<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    prev_active_core: &str,
) -> Option<SingboxStartResult> {
    // After a core switch Xray's tunnel may still hold the default route.
    if !wait_for_adapters_gone(&["lumen_xray"], 3.0) {
        controller.log("[tun] Xray TUN adapter is still up; starting sing-box TUN anyway");
    }
    start_runtime(controller, node, prev_active_core, true)
}

pub fn start_proxy<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    prev_active_core: &str,
) -> Option<SingboxStartResult> {
    start_runtime(controller, node, prev_active_core, false)
}

/// Switch a native sing-box selector without restarting its runtime.
pub fn try_hot_switch_selector<C: OperationsController>(
    controller: &mut C,
    reason: &str,
) -> bool {
    let Some(session) = controller.active_session() else {
        return false;
    };
    let Some(node) = controller.selected_node() else {
        return false;
    };
    if session.active_core != "singbox"
        || session.node_id.as_deref() == Some(node.id.as_str())
        || session.hybrid
        || session.clash_api_selector.is_empty()
        || session.clash_api_secret.is_empty()
    {
        return false;
    }

    let settings = controller.settings().clone();
    if settings.tun_mode != session.tun_mode {
        return false;
    }

    if session.clash_api_node_signatures.is_empty() {
        return false;
    }
    let node_signature = singbox_node_source_signature(&node);
    let known = session
        .clash_api_node_signatures
        .iter()
        .any(|(node_id, signature)| *node_id == node.id && *signature == node_signature);
    if !known {
        controller.log("[clash-api] selected node is not present in the active runtime mapping");
        return false;
    }

    let target = singbox_node_tag(&node.id);
    let client = SingboxClashApiClient::new(&session.clash_api_secret);
    let result = match client.switch_selector(&session.clash_api_selector, &target) {
        Ok(result) => result,
        Err(exc) => {
            controller.log(&format!(
                "[clash-api] selector switch unavailable; using restart fallback: {exc}"
            ));
            return false;
        }
    };

    controller.log(&format!(
        "[clash-api] switched selector={} {} -> {}; reason={}",
        result.selector, result.previous, result.current, reason
    ));
    controller.emit_metrics_request(false);
    if let Some(stored) = controller.selected_node_mut() {
        stored.last_used_at = Some(utc_now_iso());
    }
    let (ping_host, ping_port) = controller.infer_singbox_ping_target(
        &json!({"outbounds": [{"tag": "proxy", "type": "selector"}]}),
        Some(&node),
    );
    controller.capture_active_session(
        Some(&node),
        SessionCapture {
            tun: session.tun_mode,
            core: session.active_core.clone(),
            api_port: session.api_port,
            clash_api_secret: session.clash_api_secret.clone(),
            clash_api_selector: session.clash_api_selector.clone(),
            clash_api_node_signatures: session.clash_api_node_signatures.clone(),
            health_proxy_url: session.health_proxy_url.clone(),
            hybrid: false,
            socks_port: Some(session.socks_port),
            http_port: Some(session.http_port),
            xray_inbound_tags: Some(session.xray_inbound_tags.clone()),
            sidecar_relay_port: 0,
            protect_ss_port: 0,
            protect_ss_password: String::new(),
            ping_host,
            ping_port,
        },
    );
    let label = if node.name.is_empty() {
        node.server.clone()
    } else {
        node.name.clone()
    };
    let mode_label = if session.tun_mode { "TUN" } else { "прокси" };
    controller.set_connection_status(
        "running",
        &format!("Переключено: {label} ({mode_label})"),
        "success",
    );
    controller.save();
    controller.emit_metrics_request(true);
    true
}

/// Validate a plan; never let another stored node break the selected one.
///
/// The hot-switch selector embeds every stored node, so one malformed import
/// would make sing-box reject the whole runtime.  Drop the rejected member
/// and retry, then fall back to a plan with only the selected node.
pub fn validate_runtime_plan<C: OperationsController>(
    controller: &mut C,
    plan: SingboxRuntimePlan,
    node: Option<&Node>,
    tun_mode: bool,
) -> (SingboxRuntimePlan, bool, String) {
    let singbox_path = controller.settings().singbox_path.clone();
    let (mut valid, mut output) =
        controller.singbox_validate_config(&singbox_path, &plan.singbox_config);
    if valid || plan.clash_api_selector.is_empty() {
        return (plan, valid, output);
    }
    let mut plan = plan;
    for _ in 0..HOT_SWITCH_PRUNE_ATTEMPTS {
        let pruned = prune_rejected_selector_member(&mut plan, &output);
        if pruned.is_empty() {
            break;
        }
        controller.log(&format!(
            "[hot-switch] sing-box rejected server {pruned}; it is excluded from fast switching"
        ));
        let (next_valid, next_output) =
            controller.singbox_validate_config(&singbox_path, &plan.singbox_config);
        valid = next_valid;
        output = next_output;
        if valid {
            return (plan, valid, output);
        }
    }
    controller.log("[hot-switch] falling back to a runtime with only the selected server");
    match controller.plan_runtime_singbox(node, tun_mode, false) {
        Ok(fallback) => {
            let (fallback_valid, fallback_output) =
                controller.singbox_validate_config(&singbox_path, &fallback.singbox_config);
            (fallback, fallback_valid, fallback_output)
        }
        // Python: except (ValueError, RuntimeError): return plan, valid, output
        Err(_) => (plan, valid, output),
    }
}

/// Python `_prune_rejected_selector_member`: drop the outbound the core
/// rejected from the hot-switch selector. Returns the removed tag.
fn prune_rejected_selector_member(plan: &mut SingboxRuntimePlan, output: &str) -> String {
    let Some(captures) = CONFIG_ITEM_ERROR.captures(output) else {
        return String::new();
    };
    let section = captures.get(1).expect("regex group").as_str().to_string();
    let Ok(index) = captures.get(2).expect("regex group").as_str().parse::<usize>() else {
        return String::new();
    };
    let Some(config) = plan.singbox_config.as_object_mut() else {
        return String::new();
    };
    let selector_tag = plan.clash_api_selector.clone();
    let Some(items) = config.get(&section).and_then(Value::as_array) else {
        return String::new();
    };
    if index >= items.len() || !items[index].is_object() {
        return String::new();
    }
    let tag = json_str_or(items[index].get("tag"), "");
    let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
        return String::new();
    };
    let Some(selector_index) = outbounds.iter().position(|item| {
        item.get("type").and_then(Value::as_str) == Some("selector")
            && item.get("tag").and_then(Value::as_str) == Some(selector_tag.as_str())
    }) else {
        return String::new();
    };
    // Python: `tag == selector.get("default")` — a missing default never matches.
    if outbounds[selector_index]
        .get("default")
        .is_some_and(|default| default == &Value::String(tag.clone()))
    {
        return String::new();
    }
    let original_len = outbounds[selector_index]
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|existing| existing.len())
        .unwrap_or(0);
    let members: Vec<Value> = outbounds[selector_index]
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|existing| {
            existing
                .iter()
                .filter(|member| *member != &Value::String(tag.clone()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if members.len() == original_len {
        return String::new();
    }
    // Only the definition and the selector membership may mention the tag.
    let needle = serde_json::to_string(&Value::String(tag.clone())).unwrap_or_default();
    let haystack = serde_json::to_string(&*config).unwrap_or_default();
    if haystack.matches(&needle).count() > 2 {
        return String::new();
    }
    if let Some(outbounds_mut) = config.get_mut("outbounds").and_then(Value::as_array_mut) {
        if let Some(selector) = outbounds_mut.get_mut(selector_index) {
            selector["outbounds"] = Value::Array(members);
        }
    }
    if let Some(items_mut) = config.get_mut(&section).and_then(Value::as_array_mut) {
        if index < items_mut.len() {
            items_mut.remove(index);
        }
    }
    plan.clash_api_node_signatures
        .retain(|(node_id, _)| singbox_node_tag(node_id) != tag);
    tag
}

/// Point Lumen's hot-switch selector at the planned node after start.
///
/// sing-box restores the last selector choice from cache_file and ignores
/// `default` when that tag still exists, so a restart or a fresh connect
/// would silently keep the previous server.
pub fn enforce_selector_default<C: OperationsController>(
    controller: &mut C,
    plan: &SingboxRuntimePlan,
) -> bool {
    let secret = plan.clash_api_secret.clone();
    enforce_selector_default_with(controller, plan, |selector, target| {
        SingboxClashApiClient::new(&secret).switch_selector(selector, target)
    })
}

/// Testable variant of [`enforce_selector_default`] (the Python test
/// monkeypatches `operations.SingboxClashApiClient` for the same purpose).
pub fn enforce_selector_default_with<C, F>(
    controller: &mut C,
    plan: &SingboxRuntimePlan,
    mut switch: F,
) -> bool
where
    C: OperationsController,
    F: FnMut(&str, &str) -> Result<SelectorSwitchResult, ClashApiError>,
{
    let selector = plan.clash_api_selector.as_str();
    if selector.is_empty() || plan.clash_api_secret.is_empty() {
        return true;
    }
    let target = plan
        .singbox_config
        .get("outbounds")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                let is_selector = item.get("type").and_then(Value::as_str) == Some("selector")
                    && item.get("tag").and_then(Value::as_str) == Some(selector);
                is_selector.then(|| json_str_or(item.get("default"), ""))
            })
        })
        .unwrap_or_default();
    if target.is_empty() {
        return true;
    }
    let mut error: Option<ClashApiError> = None;
    for attempt in 0..SELECTOR_ENFORCE_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(SELECTOR_ENFORCE_DELAY);
        }
        match switch(selector, &target) {
            Err(exc) => {
                error = Some(exc);
                continue;
            }
            Ok(result) => {
                if result.changed {
                    controller.log(&format!(
                        "[clash-api] restored selector={selector} {} -> {} (cached choice ignored)",
                        result.previous, result.current
                    ));
                }
                return true;
            }
        }
    }
    controller.log(&format!(
        "[clash-api] cannot confirm selected server in sing-box selector: {}",
        error.map(|err| err.0).unwrap_or_default()
    ));
    false
}

pub fn restart_runtime<C: OperationsController>(controller: &mut C, reason: &str) -> bool {
    let node = controller.selected_node();
    let tun_mode = controller.settings().tun_mode;
    controller.set_switching(true);
    let result = restart_runtime_inner(controller, node.as_ref(), tun_mode, reason);
    // Python `finally:` block.
    controller.set_switching(false);
    let (_, connected) = controller.refresh_connected_state();
    controller.emit_connection_changed(connected);
    controller.emit_metrics_request(connected);
    result
}

fn restart_runtime_inner<C: OperationsController>(
    controller: &mut C,
    node: Option<&Node>,
    tun_mode: bool,
    reason: &str,
) -> bool {
    let log_prefix = if tun_mode {
        "tun-hot-swap"
    } else {
        "proxy-hot-swap"
    };
    controller.log(&format!("[{log_prefix}] {reason}"));
    let plan = match controller.plan_runtime_singbox(node, tun_mode, true) {
        Ok(plan) => plan,
        Err(exc) => {
            controller.set_connection_status("error", &exc, "error");
            return false;
        }
    };

    let (plan, valid, validation_output) = validate_runtime_plan(controller, plan, node, tun_mode);
    if !valid {
        controller.set_connection_status(
            "error",
            &format!("sing-box не принимает новый конфиг: {validation_output}"),
            "error",
        );
        return false;
    }
    if let Some(sidecar) = plan.xray_sidecar.as_ref() {
        let xray_path = controller.settings().xray_path.clone();
        let (valid, validation_output) =
            controller.xray_validate_config(&xray_path, &sidecar.config);
        if !valid {
            controller.set_connection_status(
                "error",
                &format!("Xray sidecar не принимает новый конфиг: {validation_output}"),
                "error",
            );
            return false;
        }
    }

    let node_name = node.map(|node| node.name.clone()).unwrap_or_default();
    let source_name = file_name_of(&plan.source_path);
    let mut session_label = source_name.clone();
    if plan.used_selected_node && node.is_some() {
        session_label = format!("{source_name} / {node_name}");
    }
    let start_message = if plan.is_hybrid() {
        format!("Переключение на {session_label} (sing-box + xray sidecar)...")
    } else {
        format!("Переключение на {session_label}...")
    };
    controller.set_connection_status("starting", &start_message, "info");
    let route_final = config_final(&plan.singbox_config, "route");
    let dns_final = config_final(&plan.singbox_config, "dns");
    controller.log(&format!(
        "[{log_prefix}] routing final={route_final} dns_final={dns_final}"
    ));
    for line in runtime_summary_lines(&plan.singbox_config) {
        controller.log(&format!("[{log_prefix}] {line}"));
    }
    controller.emit_metrics_request(false);

    if controller.singbox_is_running() && !controller.singbox_stop(true, true) {
        controller.set_connection_status(
            "error",
            "Не удалось остановить предыдущий процесс sing-box",
            "error",
        );
        return false;
    }
    if controller.xray_is_running() && !controller.xray_stop(true, true) {
        controller.set_connection_status(
            "error",
            "Не удалось остановить предыдущий процесс Xray sidecar",
            "error",
        );
        return false;
    }

    controller.set_xray_api_port(0);
    controller.set_protect_ss_port(0);
    controller.set_protect_ss_password("");
    if !controller.start_singbox_runtime_plan(&plan) {
        let message = if plan.is_hybrid() {
            "Не удалось перезапустить sing-box hybrid runtime. Смотрите причину в последних строках лога sing-box."
        } else {
            "Не удалось перезапустить sing-box runtime. Смотрите причину в последних строках лога sing-box."
        };
        controller.set_connection_status("error", message, "error");
        controller.handle_unexpected_disconnect();
        return false;
    }

    if !tun_mode {
        let settings = controller.settings().clone();
        let applied = if settings.enable_system_proxy {
            let bypass_lan = controller.system_proxy_bypass_lan();
            controller.proxy_enable(
                settings.local_http_port,
                settings.local_socks_port,
                bypass_lan,
                settings.firefox_proxy_integration,
            )
        } else {
            controller.proxy_disable(true);
            Ok(())
        };
        if let Err(exc) = applied {
            controller.set_connection_status(
                "error",
                &format!("Не удалось применить системный прокси: {exc}"),
                "error",
            );
            controller.handle_unexpected_disconnect();
            return false;
        }
    }

    let used_selected_node = plan.used_selected_node;
    if used_selected_node && node.is_some() {
        if let Some(stored) = controller.selected_node_mut() {
            stored.last_used_at = Some(utc_now_iso());
        }
    }
    let session_node = if used_selected_node { node } else { None };

    let (ping_host, ping_port) = controller.infer_singbox_ping_target(&plan.singbox_config, session_node);
    let protect_ss_port = controller.protect_ss_port();
    let protect_ss_password = controller.protect_ss_password();
    controller.capture_active_session(
        session_node,
        SessionCapture {
            tun: tun_mode,
            core: "singbox".to_string(),
            api_port: 0,
            clash_api_secret: plan.clash_api_secret.clone(),
            clash_api_selector: plan.clash_api_selector.clone(),
            clash_api_node_signatures: plan.clash_api_node_signatures.clone(),
            health_proxy_url: plan.health_proxy_url.clone(),
            hybrid: plan.is_hybrid(),
            socks_port: None,
            http_port: None,
            xray_inbound_tags: Some(Vec::new()),
            sidecar_relay_port: plan
                .xray_sidecar
                .as_ref()
                .map(|sidecar| sidecar.relay_port)
                .unwrap_or(0),
            protect_ss_port,
            protect_ss_password,
            ping_host,
            ping_port,
        },
    );
    let status_suffix = if plan.is_hybrid() && tun_mode {
        " (TUN, xray sidecar)"
    } else if tun_mode {
        " (TUN)"
    } else {
        " (прокси)"
    };
    controller.set_connection_status(
        "running",
        &format!("Переключено: {session_label}{status_suffix}"),
        "success",
    );
    controller.save();
    true
}

/// Python `route_final or '--'` / `dns_final or '--'` log values.
fn config_final(config: &Value, section: &str) -> String {
    json_str_or(config.get(section).and_then(|item| item.get("final")), "--")
}

/// Python `_runtime_summary_lines`.
fn runtime_summary_lines(config: &Value) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let Some(config) = config.as_object() else {
        return lines;
    };

    if let Some(dns) = config.get("dns").filter(|dns| dns.is_object()) {
        let servers: &[Value] = dns
            .get("servers")
            .and_then(Value::as_array)
            .map(|value| value.as_slice())
            .unwrap_or(&[]);
        let server_tags: Vec<String> = servers
            .iter()
            .filter(|server| server.is_object())
            .map(|server| json_str_or(server.get("tag"), ""))
            .filter(|tag| !tag.is_empty())
            .collect();
        let fake_enabled = servers.iter().any(|server| {
            server.is_object()
                && json_str_or(server.get("tag"), "") == "fake-dns"
                && json_str_or(server.get("type"), "") == "fakeip"
        });
        lines.push(format!(
            "dns servers={} final={} fake={}",
            server_tags.join(","),
            json_str_or(dns.get("final"), "--"),
            if fake_enabled { "on" } else { "off" }
        ));
        let rules: &[Value] = dns
            .get("rules")
            .and_then(Value::as_array)
            .map(|value| value.as_slice())
            .unwrap_or(&[]);
        let fake_rules = rules
            .iter()
            .filter(|rule| rule.is_object() && json_str_or(rule.get("server"), "") == "fake-dns")
            .count();
        if fake_enabled {
            lines.push(format!(
                "dns fake rules={fake_rules} total_dns_rules={}",
                rules.len()
            ));
        }
    }

    let proxy = find_tagged(config.get("outbounds"), "proxy")
        .or_else(|| find_tagged(config.get("endpoints"), "proxy"));
    if let Some(proxy) = proxy.filter(|proxy| proxy.is_object()) {
        let mut server = json_str_or(proxy.get("server"), "");
        let proxy_type = json_str_or(proxy.get("type"), "");
        if server.is_empty() && (proxy_type == "wireguard" || proxy_type == "warp") {
            let peers: &[Value] = proxy
                .get("peers")
                .and_then(Value::as_array)
                .map(|value| value.as_slice())
                .unwrap_or(&[]);
            if let Some(peer) = peers.iter().find(|peer| peer.is_object()) {
                server = json_str_or(peer.get("address"), "");
            }
        }
        if server.is_empty() && (proxy_type == "openvpn" || proxy_type == "openvpn-client") {
            let servers: &[Value] = proxy
                .get("servers")
                .and_then(Value::as_array)
                .map(|value| value.as_slice())
                .unwrap_or(&[]);
            if let Some(first) = servers.iter().find(|item| item.is_object()) {
                server = json_str_or(first.get("server"), "");
            }
        }
        let server_name = json_str_or(
            proxy
                .get("tls")
                .filter(|tls| tls.is_object())
                .and_then(|tls| tls.get("server_name")),
            "",
        );
        let resolver_text = proxy
            .get("domain_resolver")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut line = format!(
            "proxy type={} server={}",
            json_str_or(proxy.get("type"), "--"),
            if server.is_empty() { "--" } else { &server }
        );
        if !server_name.is_empty() {
            line.push_str(&format!(" sni={server_name}"));
        }
        if !resolver_text.is_empty() {
            line.push_str(&format!(" resolver={resolver_text}"));
        }
        lines.push(line);
    }

    let tun = config
        .get("inbounds")
        .and_then(Value::as_array)
        .and_then(|inbounds| {
            inbounds.iter().find(|inbound| {
                inbound.is_object() && json_str_or(inbound.get("type"), "") == "tun"
            })
        });
    if let Some(tun) = tun {
        let excludes_list: Vec<String> = tun
            .get("route_exclude_address")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(py_str_list_item).collect())
            .unwrap_or_default();
        let mut preview = excludes_list
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        if excludes_list.len() > 4 {
            preview.push_str(",...");
        }
        let mut line = format!(
            "tun interface={} stack={} route_exclude={}",
            json_str_or(tun.get("interface_name"), "--"),
            json_str_or(tun.get("stack"), "--"),
            excludes_list.len()
        );
        if !preview.is_empty() {
            line.push_str(&format!(" [{preview}]"));
        }
        lines.push(line);
    }
    lines
}

/// Python `_find_tagged`.
fn find_tagged<'a>(items: Option<&'a Value>, tag: &str) -> Option<&'a Value> {
    items?
        .as_array()?
        .iter()
        .find(|item| item.is_object() && json_str_or(item.get("tag"), "") == tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};

    use lumen_core::models::{AppSettings, RoutingSettings};

    use crate::xray::operations::{ActiveSessionView, SessionCapture, XrayRuntimeConfigView};

    #[derive(Debug, Clone, Default)]
    struct TestXrayRuntime {
        config: Value,
        source_path: PathBuf,
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
        settings: AppSettings,
        node: Option<Node>,
        statuses: Vec<(String, String)>,
        logs: Vec<String>,
        metrics: Vec<bool>,
        validate_queue: VecDeque<(bool, String)>,
        validate_calls: usize,
        plan_error: Option<String>,
        fallback_plan: Option<SingboxRuntimePlan>,
    }

    impl FakeController {
        fn new() -> Self {
            Self {
                core: String::new(),
                settings: AppSettings::default(),
                node: None,
                statuses: Vec::new(),
                logs: Vec::new(),
                metrics: Vec::new(),
                validate_queue: VecDeque::new(),
                validate_calls: 0,
                plan_error: None,
                fallback_plan: None,
            }
        }
    }

    impl OperationsController for FakeController {
        type Runtime = TestXrayRuntime;

        fn set_active_core(&mut self, core: &str) {
            self.core = core.to_string();
        }
        fn set_switching(&mut self, _switching: bool) {}
        fn xray_api_port(&self) -> i64 {
            0
        }
        fn set_xray_api_port(&mut self, _port: i64) {}
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
            (true, String::new())
        }
        fn xray_start(&mut self, _path: &str, _config: &Value, _prevalidated: bool) -> bool {
            true
        }
        fn xray_stop(&mut self, _expected: bool, _fast: bool) -> bool {
            true
        }
        fn singbox_is_running(&self) -> bool {
            false
        }
        fn singbox_validate_config(&mut self, _path: &str, _config: &Value) -> (bool, String) {
            self.validate_calls += 1;
            self.validate_queue
                .pop_front()
                .expect("unexpected sing-box validate call")
        }
        fn singbox_stop(&mut self, _expected: bool, _fast: bool) -> bool {
            true
        }
        fn proxy_enable(
            &mut self,
            _http_port: i64,
            _socks_port: i64,
            _bypass_lan: bool,
            _configure_firefox: bool,
        ) -> Result<(), String> {
            Ok(())
        }
        fn proxy_disable(&mut self, _restore_previous: bool) {}
        fn build_runtime_xray_config(&mut self, _node: Option<&Node>) -> Result<TestXrayRuntime, String> {
            Ok(TestXrayRuntime::default())
        }
        fn plan_runtime_singbox(
            &mut self,
            _node: Option<&Node>,
            _tun_mode: bool,
            hot_switch: bool,
        ) -> Result<SingboxRuntimePlan, String> {
            if let Some(exc) = self.plan_error.clone() {
                return Err(exc);
            }
            if !hot_switch {
                return self
                    .fallback_plan
                    .clone()
                    .ok_or_else(|| "no fallback expected".to_string());
            }
            Err("no plan expected".to_string())
        }
        fn start_singbox_runtime_plan(&mut self, _plan: &SingboxRuntimePlan) -> bool {
            true
        }
        fn infer_singbox_ping_target(&self, _payload: &Value, _node: Option<&Node>) -> (String, i64) {
            (String::new(), 0)
        }
        fn capture_active_session(&mut self, _node: Option<&Node>, _capture: SessionCapture) {}
    }

    fn hot_switch_plan() -> SingboxRuntimePlan {
        let good = singbox_node_tag("good");
        let bad = singbox_node_tag("bad");
        let other = singbox_node_tag("other");
        SingboxRuntimePlan {
            outcome: "native_singbox".to_string(),
            source_path: PathBuf::new(),
            text_hash: String::new(),
            singbox_config: json!({
                "outbounds": [
                    {"type": "vless", "tag": good},
                    {"type": "vless", "tag": bad},
                    {"type": "vless", "tag": other},
                    {"type": "selector", "tag": "proxy", "outbounds": [good, bad, other], "default": good},
                ]
            }),
            has_proxy_outbound: true,
            used_selected_node: true,
            clash_api_secret: "secret".to_string(),
            xray_sidecar: None,
            clash_api_selector: "proxy".to_string(),
            clash_api_node_signatures: vec![
                ("good".to_string(), "g".to_string()),
                ("bad".to_string(), "b".to_string()),
                ("other".to_string(), "o".to_string()),
            ],
            health_proxy_url: String::new(),
        }
    }

    #[test]
    fn rejected_hot_switch_member_is_pruned_instead_of_failing() {
        let mut controller = FakeController::new();
        controller.validate_queue = VecDeque::from(vec![
            (
                false,
                "FATAL decode config: outbounds[1].transport: x_padding_bytes cannot be disabled"
                    .to_string(),
            ),
            (true, String::new()),
        ]);
        let plan = hot_switch_plan();

        let (plan, valid, _output) = validate_runtime_plan(&mut controller, plan, None, false);

        assert!(valid);
        let tags: Vec<String> = plan.singbox_config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("tag").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert!(!tags.contains(&singbox_node_tag("bad")));
        let selector = plan.singbox_config["outbounds"].as_array().unwrap().last().unwrap();
        assert_eq!(
            selector["outbounds"],
            json!([singbox_node_tag("good"), singbox_node_tag("other")])
        );
        assert_eq!(
            plan.clash_api_node_signatures,
            vec![
                ("good".to_string(), "g".to_string()),
                ("other".to_string(), "o".to_string())
            ]
        );
        assert_eq!(controller.validate_calls, 2);
    }

    #[test]
    fn rejected_selected_node_falls_back_to_single_node_plan() {
        let mut single = hot_switch_plan();
        single.clash_api_selector = String::new();
        let mut controller = FakeController::new();
        controller.validate_queue = VecDeque::from(vec![
            (false, "FATAL decode config: outbounds[0].transport: broken".to_string()),
            (true, String::new()),
        ]);
        controller.fallback_plan = Some(single.clone());

        let (plan, valid, _output) =
            validate_runtime_plan(&mut controller, hot_switch_plan(), None, true);

        assert!(valid);
        assert_eq!(plan.clash_api_selector, "");
        assert_eq!(plan.singbox_config, single.singbox_config);
    }

    #[test]
    fn selector_default_overrides_cached_choice() {
        let mut controller = FakeController::new();
        let mut switched: Vec<(String, String)> = Vec::new();

        let ok = enforce_selector_default_with(&mut controller, &hot_switch_plan(), |selector, target| {
            switched.push((selector.to_string(), target.to_string()));
            Ok(SelectorSwitchResult {
                selector: selector.to_string(),
                previous: singbox_node_tag("old"),
                current: target.to_string(),
                changed: true,
            })
        });

        assert!(ok);
        assert_eq!(
            switched,
            vec![("proxy".to_string(), singbox_node_tag("good"))]
        );
        assert!(controller
            .logs
            .iter()
            .any(|line| line.contains("cached choice ignored")));
    }

    // Ported from windows/tests/test_singbox_runtime_planner.py (the test that
    // needs operations.start_runtime).
    #[test]
    fn planner_runtime_error_is_reported_as_a_connection_error() {
        let mut controller = FakeController::new();
        controller.core = "xray".to_string();
        controller.plan_error =
            Some("Не найден региональный rule-set geosite:ru-blocked.".to_string());

        let result = start_runtime(&mut controller, None, "xray", true);

        assert!(result.is_none());
        assert_eq!(controller.core, "xray");
        assert_eq!(
            controller.statuses,
            vec![(
                "error".to_string(),
                "Не найден региональный rule-set geosite:ru-blocked.".to_string()
            )]
        );
    }

    #[test]
    fn runtime_summary_lines_reports_dns_proxy_and_tun() {
        let config = json!({
            "dns": {
                "servers": [
                    {"tag": "local", "type": "udp"},
                    {"tag": "fake-dns", "type": "fakeip"}
                ],
                "final": "local",
                "rules": [
                    {"server": "fake-dns"},
                    {"server": "local"}
                ]
            },
            "outbounds": [
                {"type": "vless", "tag": "proxy", "server": "example.com",
                 "tls": {"server_name": "example.com"}, "domain_resolver": "local"}
            ],
            "inbounds": [
                {"type": "mixed", "tag": "socks-in"},
                {"type": "tun", "tag": "tun-in", "interface_name": "singbox_tun",
                 "stack": "mixed",
                 "route_exclude_address": ["10.0.0.0/8", "192.168.0.0/16",
                                           "172.16.0.0/12", "127.0.0.0/8",
                                           "169.254.0.0/16"]}
            ]
        });

        let lines = runtime_summary_lines(&config);

        assert_eq!(
            lines,
            vec![
                "dns servers=local,fake-dns final=local fake=on".to_string(),
                "dns fake rules=1 total_dns_rules=2".to_string(),
                "proxy type=vless server=example.com sni=example.com resolver=local".to_string(),
                "tun interface=singbox_tun stack=mixed route_exclude=5 [10.0.0.0/8,192.168.0.0/16,172.16.0.0/12,127.0.0.0/8,...]".to_string(),
            ]
        );
    }
}
