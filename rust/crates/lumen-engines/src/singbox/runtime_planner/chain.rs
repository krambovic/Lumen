//! Proxy chain wiring, the hot-switch selector and the imported-config proxy
//! alias (`_apply_singbox_proxy_chain`, `_ensure_singbox_hot_switch_selector`,
//! `_ensure_full_config_proxy_alias`).

use std::collections::HashSet;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::bootstrap::{
    ensure_awg3_windows_bind_workaround, ensure_endpoint_server_bootstrap_contract,
    ensure_proxy_server_bootstrap_contract, prefetch_payload_endpoints, strip_legacy_awg15_fields,
};
use super::node_runtime::{is_xray_exclusive_node, node_uses_xray_sidecar};
use super::planner::node_is_full_singbox_config;
use super::{
    dial_value_mut, ensure_list, is_falsy_json, is_singbox_endpoint, is_domain_name, json_str_field,
    lower_trim, replace_or_append_tagged, DialPath,
};
use lumen_core::config::proxy_chain::{CHAIN_FRONT_TAG, CHAIN_MAIN_TAG, ProxyChain};
use lumen_core::constants::SINGBOX_CLASH_API_SELECTOR;
use lumen_core::models::Node;

/// Result of `_apply_singbox_proxy_chain`: the inserted front hop (the only one
/// dialed directly, needing the server bootstrap contract) and whether an exit
/// hop re-tagged the main dial.
#[derive(Debug, Clone, Default)]
pub struct ChainApplication {
    pub front: Option<(Node, DialPath, bool)>,
    pub exit_applied: bool,
}

/// Python `_find_singbox_dial_by_tag`.
pub fn find_singbox_dial_by_tag(payload: &Value, tag: &str) -> Option<(&'static str, usize)> {
    for section in ["outbounds", "endpoints"] {
        if let Some(items) = payload.get(section).and_then(Value::as_array) {
            for (index, item) in items.iter().enumerate() {
                if item.is_object() && json_str_field(item, "tag") == tag {
                    return Some((section, index));
                }
            }
        }
    }
    None
}

fn build_singbox_chain_hop(
    chain: &mut ProxyChain,
    node: &Node,
    role: &str,
    tag: &str,
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
) -> Option<Value> {
    let label = if !node.name.is_empty() {
        node.name.clone()
    } else if !node.server.is_empty() {
        node.server.clone()
    } else {
        "?".to_string()
    };
    if node_uses_xray_sidecar(Some(node), "auto") {
        chain
            .notes
            .push(format!("{role} прокси «{label}» требует ядро Xray и не может быть звеном sing-box — пропущено."));
        return None;
    }
    match crate::singbox::config_builder::build_singbox_outbound(node, tag, multiplex_enabled, multiplex_concurrency) {
        Ok(mut payload) => {
            payload["tag"] = json!(tag);
            if is_singbox_endpoint(&payload) {
                strip_legacy_awg15_fields(&mut payload);
            }
            Some(payload)
        }
        Err(exc) => {
            chain
                .notes
                .push(format!("{role} прокси «{label}» не поддерживается sing-box ({exc}) — пропущено."));
            None
        }
    }
}

/// Python `_insert_singbox_dial`.
pub fn insert_singbox_dial(payload: &mut Value, dial: Value, index: Option<usize>) {
    let section = if is_singbox_endpoint(&dial) { "endpoints" } else { "outbounds" };
    let tag = json_str_field(&dial, "tag");
    let items = ensure_list(payload, section);
    let position = replace_or_append_tagged(items, &tag, &dial);
    if let Some(index) = index {
        let length = items.len();
        // Python: `section[-1] is dial and 0 <= index < len(section) - 1`.
        if length > 0 && position == length - 1 && index + 1 < length {
            let moved = items.pop().unwrap();
            items.insert(index, moved);
        }
    }
}

/// Python `_apply_singbox_proxy_chain`: wire the proxy chain into a native
/// sing-box config via dial `detour`.
pub fn apply_singbox_proxy_chain(
    runtime_config: &mut Value,
    chain: Option<&mut ProxyChain>,
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
) -> ChainApplication {
    let mut result = ChainApplication::default();
    let Some(chain) = chain else {
        return result;
    };
    if !chain.active() {
        return result;
    }
    let Some((section, main_index)) = find_singbox_dial_by_tag(runtime_config, "proxy") else {
        chain
            .notes
            .push("В конфиге нет outbound `proxy` — цепочка прокси не применена.".to_string());
        return result;
    };
    let main_path = DialPath::from_section(section, main_index);
    let inherited = {
        let main = dial_value_mut(runtime_config, main_path);
        json_str_field(main, "detour").trim().to_string()
    };
    let front_node = chain.front.clone();
    let exit_node = chain.exit.clone();
    let front = front_node.as_ref().and_then(|node| {
        build_singbox_chain_hop(chain, node, "Предыдущий", CHAIN_FRONT_TAG, multiplex_enabled, multiplex_concurrency)
    });
    let exit_hop = exit_node
        .as_ref()
        .and_then(|node| build_singbox_chain_hop(chain, node, "Выходной", "proxy", multiplex_enabled, multiplex_concurrency));

    if let Some(mut front_payload) = front {
        let front_is_endpoint = is_singbox_endpoint(&front_payload);
        {
            let main = dial_value_mut(runtime_config, main_path);
            if !inherited.is_empty() {
                front_payload["detour"] = json!(inherited);
            } else if let Some(map) = front_payload.as_object_mut() {
                map.remove("detour");
            }
            main["detour"] = json!(CHAIN_FRONT_TAG);
        }
        if front_is_endpoint {
            ensure_awg3_windows_bind_workaround(runtime_config, &mut front_payload);
        }
        insert_singbox_dial(runtime_config, front_payload, None);
        if let Some(node) = front_node.clone() {
            if let Some((front_section, front_index)) = find_singbox_dial_by_tag(runtime_config, CHAIN_FRONT_TAG) {
                result.front = Some((node, DialPath::from_section(front_section, front_index), front_is_endpoint));
            }
        }
    }

    if let Some(mut exit_payload) = exit_hop {
        result.exit_applied = true;
        {
            let main = dial_value_mut(runtime_config, main_path);
            main["tag"] = json!(CHAIN_MAIN_TAG);
        }
        exit_payload["detour"] = json!(CHAIN_MAIN_TAG);
        let insert_index = if section == "outbounds" && !is_singbox_endpoint(&exit_payload) {
            Some(main_index)
        } else {
            None
        };
        insert_singbox_dial(runtime_config, exit_payload, insert_index);
    }

    // Hops after the first one are dialed through a detour; their domain names
    // are still resolved locally and must not recurse into proxy-dns.
    if result.exit_applied {
        set_bootstrap_resolver_on_tag(runtime_config, CHAIN_MAIN_TAG);
        set_bootstrap_resolver_on_tag(runtime_config, "proxy"); // the exit hop
    } else {
        set_bootstrap_resolver_on_tag(runtime_config, "proxy"); // the main dial
    }
    result
}

fn set_bootstrap_resolver_on_tag(runtime_config: &mut Value, tag: &str) {
    let Some((section, index)) = find_singbox_dial_by_tag(runtime_config, tag) else {
        return;
    };
    let path = DialPath::from_section(section, index);
    let is_domain = {
        let dial = dial_value_mut(runtime_config, path);
        is_domain_name(&json_str_field(dial, "server"))
    };
    if is_domain {
        dial_value_mut(runtime_config, path)["domain_resolver"] = json!("bootstrap-dns");
    }
}

/// Python `singbox_node_tag`: stable, opaque outbound tag for a stored node.
pub fn singbox_node_tag(node_id: &str) -> String {
    let digest = Sha256::digest(node_id.as_bytes());
    let hex = format!("{digest:x}");
    format!("lumen-node-{}", &hex[..20])
}

/// Python `singbox_node_source_signature`: the source data used to build a
/// node's native outbound.
pub fn singbox_node_source_signature(node: &Node) -> String {
    let payload = json!({
        "scheme": node.scheme,
        "server": node.server,
        "port": node.port,
        "link": node.link,
        "outbound": if node.outbound.is_object() { node.outbound.clone() } else { json!({}) },
    });
    let text = sorted_dumps(&payload);
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Python `json.dumps(value, ensure_ascii=True, sort_keys=True,
/// separators=(",", ":"))`.
fn sorted_dumps(value: &Value) -> String {
    let mut out = String::new();
    write_sorted_value(&mut out, value);
    out
}

fn write_sorted_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                out.push_str(&number.to_string());
            } else {
                out.push_str(&lumen_core::pyjson::float_repr(number.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(text) => lumen_core::pyjson::write_string(out, text, true),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_sorted_value(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                lumen_core::pyjson::write_string(out, key, true);
                out.push(':');
                write_sorted_value(out, &map[*key]);
            }
            out.push('}');
        }
    }
}

/// Python `_node_has_eager_singbox_runtime`: whether merely declaring the node
/// can start background traffic.
pub fn node_has_eager_singbox_runtime(node: &Node) -> bool {
    let outbound = &node.outbound;
    let native = outbound.get("singbox").filter(|value| value.is_object());
    let outbound_type = if native
        .and_then(|value| value.get("type"))
        .map(|value| !is_falsy_json(value))
        .unwrap_or(false)
    {
        super::json_str(native.and_then(|value| value.get("type")))
    } else if outbound.get("protocol").map(|value| !is_falsy_json(value)).unwrap_or(false) {
        super::json_str(outbound.get("protocol"))
    } else if !node.scheme.is_empty() {
        node.scheme.clone()
    } else {
        String::new()
    };
    let outbound_type = lower_trim(&outbound_type);
    matches!(
        outbound_type.as_str(),
        "masque" | "wireguard" | "awg" | "warp" | "openvpn" | "openvpn-client"
    )
}

/// Python `_ensure_singbox_hot_switch_selector`: build a selector containing
/// every node usable by native sing-box.
///
/// The regular runtime still exposes the selected node as `proxy`.  This
/// helper replaces that single outbound only when at least two stored nodes
/// can be represented natively, leaving unsupported/hybrid nodes to the
/// existing restart fallback.
pub fn ensure_singbox_hot_switch_selector(
    config: &mut Value,
    selected_node: &Node,
    candidate_nodes: &[Node],
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
    core_preference: &str,
) -> (String, Vec<(String, String)>) {
    if !config.get("outbounds").is_some_and(Value::is_array) {
        return (String::new(), Vec::new());
    }

    let mut candidates: Vec<&Node> = vec![selected_node];
    let mut seen_ids: HashSet<String> = HashSet::from([selected_node.id.clone()]);
    for node in candidate_nodes {
        if seen_ids.insert(node.id.clone()) {
            candidates.push(node);
        }
    }

    let mut native_entries: Vec<(String, Node, Value)> = Vec::new();
    for &node in &candidates {
        if node_is_full_singbox_config(Some(node))
            || is_xray_exclusive_node(Some(node))
            || node_uses_xray_sidecar(Some(node), core_preference)
            || node_has_eager_singbox_runtime(node)
        {
            continue;
        }
        let tag = singbox_node_tag(&node.id);
        let native = match crate::singbox::config_builder::build_singbox_outbound(
            node,
            &tag,
            multiplex_enabled,
            multiplex_concurrency,
        ) {
            Ok(native) => native,
            Err(_) => continue,
        };
        native_entries.push((tag, node.clone(), native));
    }

    let selected_tag = singbox_node_tag(&selected_node.id);
    if native_entries.len() < 2 || !native_entries.iter().any(|(tag, _, _)| tag == &selected_tag) {
        return (String::new(), Vec::new());
    }

    {
        let outbounds = ensure_list(config, "outbounds");
        outbounds.retain(|item| {
            !(item.is_object() && json_str_field(item, "tag") == SINGBOX_CLASH_API_SELECTOR)
        });
    }
    {
        let endpoints = ensure_list(config, "endpoints");
        endpoints.retain(|item| {
            !(item.is_object() && {
                let tag = json_str_field(item, "tag");
                tag == SINGBOX_CLASH_API_SELECTOR || tag.starts_with("lumen-node-")
            })
        });
    }

    {
        let mut extra: Vec<Value> = Vec::new();
        for (_, _, native) in &native_entries {
            extra.push(native.clone());
        }
        for (_, node, _) in &native_entries {
            extra.push(json!({"server": node.server}));
        }
        prefetch_payload_endpoints(config, &extra);
    }
    let mut member_tags: Vec<String> = Vec::new();
    for (tag, node, native) in &native_entries {
        member_tags.push(tag.clone());
        if is_singbox_endpoint(native) {
            let mut native = native.clone();
            strip_legacy_awg15_fields(&mut native);
            ensure_awg3_windows_bind_workaround(config, &mut native);
            let endpoints = ensure_list(config, "endpoints");
            let position = replace_or_append_tagged(endpoints, tag, &native);
            ensure_endpoint_server_bootstrap_contract(config, DialPath::Endpoint(position));
        } else {
            let outbounds = ensure_list(config, "outbounds");
            outbounds.push(native.clone());
            let position = outbounds.len() - 1;
            ensure_proxy_server_bootstrap_contract(config, DialPath::Outbound(position), &node.server);
        }
    }

    {
        let outbounds = ensure_list(config, "outbounds");
        outbounds.push(json!({
            "type": "selector",
            "tag": SINGBOX_CLASH_API_SELECTOR,
            "outbounds": member_tags,
            "default": selected_tag,
            "interrupt_exist_connections": true,
        }));
    }
    (
        SINGBOX_CLASH_API_SELECTOR.to_string(),
        native_entries
            .iter()
            .map(|(_, node, _)| (node.id.clone(), singbox_node_source_signature(node)))
            .collect(),
    )
}

/// Python `_ensure_full_config_proxy_alias`.
pub fn ensure_full_config_proxy_alias(config: &mut Value) {
    if !config.get("outbounds").is_some_and(Value::is_array) {
        return;
    }
    let mut tags: HashSet<String> = HashSet::new();
    for item in config.get("outbounds").and_then(Value::as_array).into_iter().flatten() {
        if item.is_object() {
            tags.insert(json_str_field(item, "tag"));
        }
    }
    for item in config.get("endpoints").and_then(Value::as_array).into_iter().flatten() {
        if item.is_object() {
            tags.insert(json_str_field(item, "tag"));
        }
    }
    if tags.contains("proxy") {
        return;
    }
    let route_final = config
        .get("route")
        .filter(|value| value.is_object())
        .and_then(|route| route.get("final"))
        .map(|value| super::json_str(Some(value)))
        .unwrap_or_default();
    let ignored = ["", "direct", "block", "dns"];
    let mut preferred = route_final.trim().to_string();
    if preferred.is_empty() || ignored.contains(&preferred.as_str()) || !tags.contains(&preferred) {
        'search: for section in ["outbounds", "endpoints"] {
            for item in config.get(section).and_then(Value::as_array).into_iter().flatten() {
                if !item.is_object() {
                    continue;
                }
                let tag = json_str_field(item, "tag").trim().to_string();
                let outbound_type = lower_trim(&json_str_field(item, "type"));
                if !tag.is_empty() && !ignored.contains(&tag.as_str()) && !ignored.contains(&outbound_type.as_str()) {
                    preferred = tag;
                    break 'search;
                }
            }
        }
    }
    if preferred.is_empty() || ignored.contains(&preferred.as_str()) {
        return;
    }
    let outbounds = ensure_list(config, "outbounds");
    outbounds.push(json!({
        "type": "selector",
        "tag": "proxy",
        "outbounds": [preferred],
        "default": preferred,
        "interrupt_exist_connections": true,
    }));
}
