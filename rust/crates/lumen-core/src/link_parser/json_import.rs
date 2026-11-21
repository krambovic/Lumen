//! JSON subscription import: node graphs, SIP008, native sing-box/xray
//! configs and the sing-box outbound wrapper (`_parse_json_*` helpers).

use std::collections::BTreeMap;

use base64::Engine;
use serde_json::{json, Map, Value};

use super::clash::{is_clash_proxy_payload, parse_clash_proxy_payload};
use super::helpers::{
    py_int_or, py_int_value, py_str_or, py_str, py_truthy, PyUrlSplit,
};
use super::schemes::{json_name, parse_naive_link};
use super::{parse_links_text, parse_single, LinkParseError, Node, MAX_IMPORT_NODES};
use crate::wireguard_normalization::{normalize_singbox_wireguard_endpoints, normalize_wireguard_endpoint};

// ── shared lookups ──

/// Python `_first_mapping(value)` — first dict item of a list.
pub(super) fn first_mapping(value: Option<&Value>) -> Map<String, Value> {
    let Some(Value::Array(items)) = value else {
        return Map::new();
    };
    for item in items {
        if let Some(map) = item.as_object() {
            return map.clone();
        }
    }
    Map::new()
}

/// Python `_is_sip008_server(payload)`.
fn is_sip008_server(payload: &Value) -> bool {
    let Some(map) = payload.as_object() else {
        return false;
    };
    ["server", "server_port", "method", "password"]
        .iter()
        .all(|key| map.contains_key(*key))
        && !py_truthy(map.get("type").unwrap_or(&Value::Null))
        && !py_truthy(map.get("protocol").unwrap_or(&Value::Null))
}

// ── entry points ──

/// Python `_parse_json_nodes_text(text)`.
pub(super) fn parse_json_nodes_text(text: &str) -> Result<(Vec<Node>, Vec<String>), LinkParseError> {
    let payload: Value =
        crate::pyjson::loads(text).map_err(LinkParseError::new)?;
    parse_json_nodes_payload(&payload, 0)
}

/// Python `_try_parse_json_lines(text)` — one complete JSON node per line.
pub(super) fn try_parse_json_lines(text: &str) -> Option<(Vec<Node>, Vec<String>)> {
    let lines: Vec<(usize, &str)> = super::helpers::py_splitlines(text)
        .into_iter()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    if lines.len() <= 1 || lines.len() > super::MAX_IMPORT_LINES {
        return None;
    }

    let mut payloads: Vec<(usize, Value)> = Vec::new();
    for (line_number, line) in lines {
        match serde_json::from_str::<Value>(line) {
            Ok(payload) => payloads.push((line_number, payload)),
            Err(_) => return None,
        }
    }

    let mut nodes: Vec<Node> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for (line_number, payload) in payloads {
        match parse_json_nodes_payload(&payload, 0) {
            Ok((parsed_nodes, parsed_errors)) => {
                nodes.extend(parsed_nodes);
                errors.extend(
                    parsed_errors
                        .into_iter()
                        .map(|error| format!("JSON line {line_number}: {error}")),
                );
            }
            Err(exc) => errors.push(format!("JSON line {line_number}: {exc}")),
        }
        if nodes.len() > MAX_IMPORT_NODES {
            return Some((
                Vec::new(),
                vec![format!("JSON contains more than {MAX_IMPORT_NODES} nodes")],
            ));
        }
    }
    Some((nodes, errors))
}

/// Python `_parse_json_nodes_payload(payload, _depth=0)`.
pub(super) fn parse_json_nodes_payload(
    payload: &Value,
    depth: usize,
) -> Result<(Vec<Node>, Vec<String>), LinkParseError> {
    if depth > 16 {
        return Err(LinkParseError::new(
            "JSON subscription nesting exceeds 16 levels",
        ));
    }
    if let Value::String(text) = payload {
        return parse_links_text(text, false);
    }
    if let Some(map) = payload.as_object() {
        if map.contains_key("proxy")
            && !["type", "protocol", "outbounds", "endpoints", "proxies"]
                .iter()
                .any(|key| map.contains_key(*key))
        {
            return parse_naiveproxy_config_payload(payload);
        }
    }

    let mut nodes: Vec<Node> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let items: Vec<Value>;
    let mut label = String::new();
    match payload {
        Value::Array(list) => items = list.clone(),
        Value::Object(map) => {
            // Keep whole native graphs where flattening would destroy
            // dependencies or AUTO.
            let native_graph = is_singbox_wireguard_config_payload(payload)
                || is_singbox_openvpn_config_payload(payload)
                || is_xray_auto_config_payload(payload)
                || (map.get("providers").map(|v| v.is_array()).unwrap_or(false)
                    && map.get("outbounds").map(|v| v.is_array()).unwrap_or(false));
            if native_graph
                || map.contains_key("protocol")
                || map.contains_key("type")
                || is_sip008_server(payload)
            {
                return Ok((vec![parse_json_outbound_payload(payload)?], Vec::new()));
            }
            let mut collected: Vec<Value> = Vec::new();
            if let Some(outbounds) = map.get("outbounds").filter(|v| v.is_array()) {
                collected.extend(json_proxy_outbounds(outbounds.as_array().unwrap()));
            }
            if let Some(endpoints) = map.get("endpoints").filter(|v| v.is_array()) {
                collected.extend(endpoints.as_array().unwrap().clone());
            }
            // These are independent containers, not an if/elif choice that
            // loses siblings.
            for key in ["proxies", "servers", "nodes", "configs", "links", "subs", "items"] {
                match map.get(key) {
                    Some(Value::Array(list)) => collected.extend(list.clone()),
                    Some(value @ (Value::Object(_) | Value::String(_))) => collected.push(value.clone()),
                    _ => {}
                }
            }
            for key in ["remarks", "name"] {
                if let Some(value) = map.get(key) {
                    if py_truthy(value) {
                        label = py_str(value);
                        break;
                    }
                }
            }
            if collected.is_empty() {
                if let Some(outbound) = map.get("outbound").filter(|v| v.is_object()) {
                    collected = vec![outbound.clone()];
                } else {
                    let mut found: Option<Value> = None;
                    for key in ["link", "uri", "url", "config", "data", "content"] {
                        if let Some(value) = map.get(key) {
                            if value.is_object() || value.is_array() || value.is_string() {
                                found = Some(value.clone());
                                break;
                            }
                        }
                    }
                    if let Some(value) = found {
                        collected = vec![value];
                    } else if ["add", "id", "port"]
                        .iter()
                        .all(|key| map.contains_key(*key))
                    {
                        let compact = base64::engine::general_purpose::STANDARD
                            .encode(crate::pyjson::dumps(payload, false, None));
                        return parse_single(&format!("vmess://{compact}")).map(|node| (vec![node], Vec::new()));
                    } else {
                        return Err(LinkParseError::new(
                            "JSON contains no supported node or subscription container",
                        ));
                    }
                }
            }
            items = collected;
        }
        _ => {
            return Err(LinkParseError::new(
                "JSON subscription must be an object, array or encoded string",
            ))
        }
    }
    if items.len() > MAX_IMPORT_NODES {
        return Err(LinkParseError::new(format!(
            "JSON contains more than {MAX_IMPORT_NODES} nodes"
        )));
    }
    for (index, item) in items.iter().enumerate() {
        let index = index + 1;
        match parse_json_nodes_payload(item, depth + 1) {
            Ok((inner, inner_errors)) => {
                nodes.extend(inner);
                errors.extend(
                    inner_errors
                        .into_iter()
                        .map(|error| format!("JSON item {index}: {error}")),
                );
            }
            Err(exc) => errors.push(format!("JSON item {index}: {exc}")),
        }
        if nodes.len() > MAX_IMPORT_NODES {
            return Ok((
                Vec::new(),
                vec![format!("JSON contains more than {MAX_IMPORT_NODES} nodes")],
            ));
        }
    }
    if !label.is_empty() {
        let single = nodes.len() == 1;
        for node in nodes.iter_mut() {
            if single {
                node.name = label.clone();
            } else if node.name != label {
                node.name = format!("{label} \u{b7} {}", node.name);
            }
        }
    }
    Ok((nodes, errors))
}

/// Python `_json_proxy_outbounds(outbounds)`.
fn json_proxy_outbounds(outbounds: &[Value]) -> Vec<Value> {
    let supported = [
        "vless", "vmess", "trojan", "shadowsocks", "socks", "http", "warp", "wireguard", "awg",
        "hysteria", "hysteria2", "hy", "hy2", "tuic", "mieru", "masque", "openvpn",
        "openvpn-client", "naive",
    ];
    let auto_kinds = ["selector", "urltest", "url-test"];
    let ignored = ["freedom", "blackhole", "dns", "direct", "block", "selector", "urltest", "url-test"];
    let mut result: Vec<Value> = Vec::new();
    let candidates: Vec<Value> = outbounds.iter().filter(|item| item.is_object()).cloned().collect();
    // Python dict: insertion order, a repeated tag keeps its first position.
    let mut tag_map: Map<String, Value> = Map::new();
    for item in &candidates {
        let tag = py_str_or(item.get("tag"), "").trim().to_string();
        if !tag.is_empty() {
            tag_map.insert(tag, item.clone());
        }
    }

    for item in &candidates {
        let kind = py_str_or(
            item.get("protocol").or_else(|| item.get("type")),
            "",
        )
        .trim()
        .to_lowercase();
        if !auto_kinds.contains(&kind.as_str()) {
            continue;
        }
        if let Some(resolved) = resolve_auto_selector_payload(item, &tag_map) {
            result.push(resolved);
        }
    }

    for item in outbounds {
        if !item.is_object() {
            continue;
        }
        let kind = py_str_or(item.get("protocol").or_else(|| item.get("type")), "")
            .trim()
            .to_lowercase();
        if supported.contains(&kind.as_str()) {
            result.push(item.clone());
        } else if !kind.is_empty() && !ignored.contains(&kind.as_str()) {
            result.push(item.clone());
        }
    }
    result
}

/// Python `_resolve_auto_selector_payload(selector, tag_map)`.
fn resolve_auto_selector_payload(
    selector: &Value,
    tag_map: &Map<String, Value>,
) -> Option<Value> {
    let refs = selector.get("outbounds").and_then(Value::as_array)?;
    for reference in refs {
        let target = tag_map.get(&py_str_or(Some(reference), "").trim().to_string());
        let Some(target) = target else { continue };
        let target_kind = py_str_or(target.get("protocol").or_else(|| target.get("type")), "")
            .trim()
            .to_lowercase();
        if matches!(
            target_kind.as_str(),
            "freedom" | "blackhole" | "dns" | "direct" | "block" | "selector" | "urltest" | "url-test"
        ) {
            continue;
        }
        let final_tag = {
            let tag = py_str_or(selector.get("tag"), "");
            if tag.is_empty() { "proxy".to_string() } else { tag }
        };
        let config = json!({
            "outbounds": tag_map.values().cloned().collect::<Vec<_>>(),
            "route": { "final": final_tag },
        });
        let name = {
            let a = py_str_or(selector.get("name"), "");
            let a = if !a.is_empty() { a } else { py_str_or(selector.get("remarks"), "") };
            let a = if !a.is_empty() { a } else { py_str_or(selector.get("tag"), "") };
            if a.is_empty() { "Автовыбор сервера".to_string() } else { a }
        }
        .trim()
        .to_string();
        let target_server = if target_kind == "openvpn" || target_kind == "openvpn-client" {
            first_mapping(target.get("servers"))
        } else {
            Map::new()
        };
        let server = {
            let a = py_str_or(target.get("server"), "");
            let a = if !a.is_empty() { a } else { py_str_or(target.get("address"), "") };
            if !a.is_empty() {
                a
            } else {
                py_str_or(target_server.get("server"), "")
            }
        };
        let port = {
            let a = py_int_or(target.get("server_port"), 0).unwrap_or(0);
            let a = if a != 0 { a } else { py_int_or(target.get("port"), 0).unwrap_or(0) };
            if a != 0 {
                a
            } else {
                py_int_or(target_server.get("server_port"), 0).unwrap_or(0)
            }
        };
        let lumen_scheme = if target_kind == "hy2" {
            "hysteria2"
        } else if target_kind == "hy" {
            "hysteria"
        } else if target_kind == "openvpn-client" {
            "openvpn"
        } else {
            target_kind.as_str()
        };
        return Some(json!({
            "protocol": "singbox_config",
            "tag": name,
            "remarks": name,
            "singbox_config": config,
            "__lumen_scheme": lumen_scheme,
            "__lumen_server": server,
            "__lumen_port": port,
        }));
    }
    None
}

/// Python `_parse_naiveproxy_config_payload(payload)`.
fn parse_naiveproxy_config_payload(
    payload: &Value,
) -> Result<(Vec<Node>, Vec<String>), LinkParseError> {
    let raw_proxies = payload.get("proxy").cloned().unwrap_or(Value::Null);
    let proxy_uris: Vec<Value> = match raw_proxies {
        Value::Array(items) => items,
        other => vec![other],
    };
    let mut nodes: Vec<Node> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for (index, value) in proxy_uris.iter().enumerate() {
        let index = index + 1;
        let uri = py_str_or(Some(value), "").trim().to_string();
        if uri.is_empty() {
            continue;
        }
        let scheme = PyUrlSplit::parse(&uri)
            .map(|parsed| parsed.scheme)
            .unwrap_or_default();
        if scheme != "https" && scheme != "quic" {
            errors.push(format!(
                "NaiveProxy proxy {index}: unsupported transport `{}`",
                if scheme.is_empty() { "unknown" } else { &scheme }
            ));
            continue;
        }
        match parse_naive_link(&uri, &scheme) {
            Ok(node) => nodes.push(node),
            Err(exc) => errors.push(format!("NaiveProxy proxy {index}: {exc}")),
        }
    }
    Ok((nodes, errors))
}

// ── single outbound JSON ──

/// Python `_parse_json_outbound(text)`.
pub(super) fn parse_json_outbound_text(text: &str) -> Result<Node, LinkParseError> {
    let payload: Value =
        crate::pyjson::loads(text).map_err(LinkParseError::new)?;
    if !payload.is_object() {
        return Err(LinkParseError::new("JSON node must be an object"));
    }
    parse_json_outbound_payload(&payload)
}

/// Python `_parse_json_outbound_payload(payload)`.
pub(super) fn parse_json_outbound_payload(payload: &Value) -> Result<Node, LinkParseError> {
    let original_payload = payload.clone();
    let outbound: Value;
    let explicit_protocol = py_str_or(payload.get("protocol"), "").trim().to_lowercase();

    if is_sip008_server(payload) {
        let map = payload.as_object().unwrap();
        let raw_server_port = map.get("server_port").cloned().unwrap_or(Value::Null);
        if raw_server_port.is_boolean() {
            return Err(LinkParseError::new(
                "SIP008 Shadowsocks server has an invalid server_port",
            ));
        }
        let server_port = py_int_value(&raw_server_port).map_err(|_| {
            LinkParseError::new("SIP008 Shadowsocks server has an invalid server_port")
        })?;
        let server = py_str_or(map.get("server"), "").trim().to_string();
        let method = py_str_or(map.get("method"), "").trim().to_string();
        let password = py_str_or(map.get("password"), "");
        if server.is_empty()
            || !(0 < server_port && server_port <= 65535)
            || method.is_empty()
            || (password.is_empty() && !method.eq_ignore_ascii_case("none"))
        {
            return Err(LinkParseError::new(
                "SIP008 Shadowsocks server has invalid required fields",
            ));
        }
        let mut native = Map::new();
        native.insert("type".to_string(), Value::String("shadowsocks".into()));
        native.insert("server".to_string(), Value::String(server));
        native.insert("server_port".to_string(), json!(server_port));
        native.insert("method".to_string(), Value::String(method));
        native.insert("password".to_string(), Value::String(password));
        let plugin = super::schemes::normalize_shadowsocks_plugin_name_pub(map.get("plugin"));
        if !plugin.is_empty() {
            native.insert("plugin".to_string(), Value::String(plugin.clone()));
            let mut plugin_opts = py_str_or(map.get("plugin_opts"), "");
            if plugin == "obfs-local" {
                plugin_opts = super::schemes::normalize_obfs_plugin_options(&plugin_opts);
            } else if plugin == "v2ray-plugin" {
                plugin_opts = super::schemes::normalize_v2ray_plugin_options(&plugin_opts);
            }
            if !plugin_opts.is_empty() {
                native.insert("plugin_opts".to_string(), Value::String(plugin_opts));
            }
        }
        outbound = native_singbox_outbound(&Value::Object(native))?;
    } else if explicit_protocol == "singbox_config"
        && payload.get("singbox_config").map(|v| v.is_object()).unwrap_or(false)
    {
        let inferred = native_singbox_config(payload.get("singbox_config").unwrap())?;
        let mut merged = Map::new();
        if let Some(inferred_obj) = inferred.as_object() {
            for (key, value) in inferred_obj {
                merged.insert(key.clone(), value.clone());
            }
        }
        if let Some(payload_obj) = payload.as_object() {
            for (key, value) in payload_obj {
                merged.insert(key.clone(), value.clone());
            }
        }
        if let Some(singbox_config) = inferred.get("singbox_config") {
            merged.insert("singbox_config".to_string(), singbox_config.clone());
        }
        for metadata_key in ["__lumen_scheme", "__lumen_server", "__lumen_port"] {
            let payload_value = payload.get(metadata_key).cloned().unwrap_or(Value::Null);
            let inferred_value = inferred.get(metadata_key).cloned().unwrap_or(Value::Null);
            let payload_empty = !py_truthy(&payload_value);
            let inferred_present = py_truthy(&inferred_value);
            if payload_empty && inferred_present {
                merged.insert(metadata_key.to_string(), inferred_value);
            }
        }
        outbound = Value::Object(merged);
    } else if explicit_protocol == "xray_config"
        && payload.get("xray_config").map(|v| v.is_object()).unwrap_or(false)
    {
        outbound = payload.clone();
    } else if is_xray_auto_config_payload(payload) {
        outbound = native_xray_config(payload);
    } else if is_singbox_wireguard_config_payload(payload) || is_singbox_openvpn_config_payload(payload) {
        outbound = native_singbox_config(payload)?;
    } else if is_clash_proxy_payload(payload) {
        return parse_clash_proxy_payload(payload);
    } else if payload.as_object().map(|m| m.contains_key("type")).unwrap_or(false) {
        outbound = native_singbox_outbound(payload)?;
    } else if payload.as_object().map(|m| m.contains_key("protocol")).unwrap_or(false) {
        outbound = payload.clone();
    } else if payload.get("providers").map(|v| v.is_array()).unwrap_or(false)
        && payload.get("outbounds").map(|v| v.is_array()).unwrap_or(false)
    {
        outbound = native_singbox_config(payload)?;
    } else if payload
        .get("endpoints")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
    {
        let first = payload
            .get("endpoints")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(Value::Null);
        outbound = native_singbox_outbound(&first)?;
    } else if payload
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
    {
        let outbounds = payload.get("outbounds").and_then(Value::as_array).unwrap().clone();
        let first_outbound = pick_json_proxy_outbound(&outbounds)?;
        outbound = if first_outbound.as_object().map(|m| m.contains_key("type")).unwrap_or(false) {
            native_singbox_outbound(&first_outbound)?
        } else {
            first_outbound
        };
    } else {
        return Err(LinkParseError::new("JSON must contain `protocol` or `outbounds`"));
    }

    let outbound_obj = outbound.as_object().cloned().unwrap_or_else(Map::new);
    let protocol = py_str_or(outbound_obj.get("protocol"), "custom");
    let tag = {
        let t = py_str_or(outbound_obj.get("tag"), "");
        if t.is_empty() { protocol.clone() } else { t }
    };
    let mut server = String::new();
    let mut port: i64 = 0;

    let native = outbound_obj
        .get("singbox")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let native_obj = native.as_object().cloned().unwrap_or_default();
    let settings = outbound_obj
        .get("settings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let settings_obj = settings.as_object().cloned().unwrap_or_default();

    if protocol == "vless" || protocol == "vmess" {
        if !native_obj.is_empty() {
            server = py_str_or(native_obj.get("server"), "");
            port = py_int_or(native_obj.get("server_port"), 0)?;
        } else {
            let vnext = match settings_obj.get("vnext") {
                Some(Value::Array(items)) if !items.is_empty() => items[0].clone(),
                _ => json!({}),
            };
            let vnext_obj = vnext.as_object().cloned().unwrap_or_default();
            server = py_str_or(vnext_obj.get("address"), "");
            port = py_int_or(vnext_obj.get("port"), 0)?;
        }
    } else if matches!(
        protocol.as_str(),
        "trojan" | "shadowsocks" | "socks" | "http"
    ) {
        if !native_obj.is_empty() {
            server = py_str_or(native_obj.get("server"), "");
            port = py_int_or(native_obj.get("server_port"), 0)?;
        } else {
            let servers = match settings_obj.get("servers") {
                Some(Value::Array(items)) if !items.is_empty() => items[0].clone(),
                _ => json!({}),
            };
            let server_obj = servers.as_object().cloned().unwrap_or_default();
            server = py_str_or(server_obj.get("address"), "");
            port = py_int_or(server_obj.get("port"), 0)?;
        }
    } else if matches!(protocol.as_str(), "warp" | "wireguard" | "awg") && !native_obj.is_empty() {
        let peer = match native_obj.get("peers").and_then(Value::as_array) {
            Some(items) if !items.is_empty() && items[0].is_object() => {
                items[0].as_object().cloned().unwrap_or_default()
            }
            _ => Map::new(),
        };
        server = py_str_or(peer.get("address"), "");
        if server.is_empty() {
            server = py_str_or(native_obj.get("server"), "");
        }
        port = py_int_or(peer.get("port"), 0)?;
        if port == 0 {
            port = py_int_or(native_obj.get("server_port"), 0)?;
        }
    } else if matches!(
        protocol.as_str(),
        "hysteria" | "hysteria2" | "tuic" | "mieru" | "naive" | "anytls" | "snell"
    ) {
        if !native_obj.is_empty() {
            server = py_str_or(native_obj.get("server"), "");
            port = py_int_or(native_obj.get("server_port"), 0)?;
        } else {
            server = {
                let a = py_str_or(settings_obj.get("address"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(settings_obj.get("server"), "")
                }
            };
            port = {
                let a = py_int_or(settings_obj.get("port"), 0)?;
                if a != 0 {
                    a
                } else {
                    py_int_or(settings_obj.get("server_port"), 0)?
                }
            };
        }
    } else if protocol == "masque" && !native_obj.is_empty() {
        let profile = native_obj
            .get("profile")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        server = py_str_or(profile.get("id"), "");
        if server.is_empty() {
            server = py_str_or(native_obj.get("server"), "");
        }
        port = py_int_or(native_obj.get("server_port"), 0)?;
    } else if (protocol == "openvpn" || protocol == "openvpn-client") && !native_obj.is_empty() {
        let openvpn_server = first_mapping(native_obj.get("servers"));
        server = py_str_or(openvpn_server.get("server"), "");
        port = py_int_or(openvpn_server.get("server_port"), 0)?;
    } else if protocol == "singbox_config" {
        let full_config = outbound_obj
            .get("singbox_config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let endpoint = first_wireguard_endpoint(&Value::Object(full_config.clone()));
        if endpoint.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
            let peer = first_mapping(endpoint.get("peers"));
            let endpoint_type = py_str_or(endpoint.get("type"), "");
            server = py_str_or(peer.get("address"), "");
            if server.is_empty() && endpoint_type == "warp" {
                server = "engage.cloudflareclient.com".to_string();
            }
            port = py_int_or(peer.get("port"), 0)?;
            if port == 0 && endpoint_type == "warp" {
                port = 2408;
            }
        } else {
            let outbounds = full_config.get("outbounds").and_then(Value::as_array).cloned();
            let proxy = match &outbounds {
                Some(items) => pick_json_proxy_outbound(items).unwrap_or(Value::Object(Map::new())),
                None => Value::Object(Map::new()),
            };
            let native_proxy = proxy.as_object().cloned().unwrap_or_default();
            let openvpn_server = first_mapping(native_proxy.get("servers"));
            server = py_str_or(native_proxy.get("server"), "");
            if server.is_empty() {
                server = py_str_or(openvpn_server.get("server"), "");
            }
            port = py_int_or(native_proxy.get("server_port"), 0)?;
            if port == 0 {
                port = py_int_or(openvpn_server.get("server_port"), 0)?;
            }
        }
    } else if protocol == "xray_config" {
        let full_config = outbound_obj
            .get("xray_config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(outbounds) = full_config.get("outbounds").and_then(Value::as_array) {
            let proxy = pick_json_proxy_outbound(&outbounds.as_slice().to_vec())
                .unwrap_or(Value::Null);
            if proxy.is_object() {
                let proxy_node = parse_json_outbound_payload(&proxy)?;
                server = proxy_node.server;
                port = proxy_node.port;
            }
        }
    }

    let mut display_scheme = {
        let a = py_str_or(original_payload.get("__lumen_scheme"), "");
        if !a.is_empty() {
            a
        } else {
            let b = py_str_or(outbound_obj.get("__lumen_scheme"), "");
            if !b.is_empty() {
                b
            } else if protocol == "xray_config" {
                "auto".to_string()
            } else {
                protocol.clone()
            }
        }
    };
    display_scheme = display_scheme.trim().to_string();
    let lumen_server = {
        let a = original_payload.get("__lumen_server").cloned();
        if a.as_ref().map(py_truthy).unwrap_or(false) {
            a
        } else {
            outbound_obj.get("__lumen_server").cloned()
        }
    };
    if let Some(value) = lumen_server {
        if py_truthy(&value) {
            server = py_str(&value);
        }
    }
    let lumen_port = {
        let a = original_payload.get("__lumen_port").cloned();
        if a.as_ref().map(py_truthy).unwrap_or(false) {
            a
        } else {
            outbound_obj.get("__lumen_port").cloned()
        }
    };
    if let Some(value) = lumen_port {
        if py_truthy(&value) {
            port = py_int_value(&value)?;
        }
    }

    let meta = original_payload
        .get("meta")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut description = {
        let a = py_str_or(original_payload.get("serverDescription"), "");
        if !a.is_empty() {
            a
        } else {
            py_str_or(meta.get("serverDescription"), "")
        }
    }
    .trim()
    .to_string();
    description = description.chars().take(30).collect();

    let fallback_name = if protocol == "singbox_config" {
        let tag_value = py_str_or(outbound_obj.get("tag"), "");
        if !tag_value.is_empty() {
            tag_value
        } else if display_scheme == "auto" {
            "AUTO".to_string()
        } else {
            display_scheme.to_uppercase()
        }
    } else {
        format!("json-{tag}")
    };
    if display_scheme == "auto" && description.is_empty() {
        description = "Автовыбор лучших серверов".to_string();
    }

    let mut node = Node::new();
    node.name = json_name(&original_payload, fallback_name);
    node.scheme = display_scheme;
    node.server = server;
    node.port = port;
    node.link = serde_json::to_string(&original_payload).unwrap_or_default();
    node.outbound = outbound;
    node.description = description;
    Ok(node)
}

/// Python `_pick_json_proxy_outbound(outbounds)`.
pub(super) fn pick_json_proxy_outbound(outbounds: &[Value]) -> Result<Value, LinkParseError> {
    let candidates: Vec<Value> = outbounds.iter().filter(|item| item.is_object()).cloned().collect();
    if candidates.is_empty() {
        return Err(LinkParseError::new("JSON outbounds list is empty"));
    }
    let kind = |item: &Value| {
        py_str_or(item.get("protocol").or_else(|| item.get("type")), "")
            .trim()
            .to_lowercase()
    };
    let ignored = ["freedom", "blackhole", "dns", "direct", "block", "selector", "urltest", "url-test"];
    let supported = [
        "vless", "vmess", "trojan", "shadowsocks", "socks", "http", "warp", "wireguard", "awg",
        "hysteria", "hysteria2", "hy", "hy2", "tuic", "mieru", "masque", "openvpn",
        "openvpn-client", "naive",
    ];
    for item in &candidates {
        let tag = py_str_or(item.get("tag"), "").trim().to_lowercase();
        if tag == "proxy" && !ignored.contains(&kind(item).as_str()) {
            return Ok(item.clone());
        }
    }
    for item in &candidates {
        if supported.contains(&kind(item).as_str()) {
            return Ok(item.clone());
        }
    }
    for item in &candidates {
        if !ignored.contains(&kind(item).as_str()) {
            return Ok(item.clone());
        }
    }
    Ok(candidates[0].clone())
}

// ── native wrappers ──

/// Python `_native_singbox_outbound(payload)`.
pub(super) fn native_singbox_outbound(payload: &Value) -> Result<Value, LinkParseError> {
    let mut native = payload.clone();
    let mut protocol = py_str_or(native.get("type"), "custom").to_lowercase();
    if protocol == "hy" {
        protocol = "hysteria".to_string();
        if let Some(obj) = native.as_object_mut() {
            obj.insert("type".to_string(), Value::String("hysteria".into()));
        }
    } else if protocol == "hy2" {
        protocol = "hysteria2".to_string();
        if let Some(obj) = native.as_object_mut() {
            obj.insert("type".to_string(), Value::String("hysteria2".into()));
        }
    } else if protocol == "openvpn" || protocol == "openvpn-client" {
        if let Some(obj) = native.as_object_mut() {
            obj.insert("system".to_string(), Value::Bool(false));
            let name = py_str_or(obj.get("name"), "");
            obj.insert(
                "name".to_string(),
                Value::String(if name.is_empty() { "openvpn0".into() } else { name }),
            );
        }
        protocol = "openvpn".to_string();
    } else if protocol == "wireguard" || protocol == "awg" || protocol == "warp" {
        native = normalize_wireguard_endpoint(&native)
            .map_err(|error| LinkParseError::new(error.to_string()))?;
        protocol = py_str_or(native.get("type"), "custom").to_lowercase();
    }
    let final_protocol = if protocol == "wireguard"
        && native.get("amnezia").map(|v| v.is_object()).unwrap_or(false)
    {
        "awg"
    } else {
        protocol.as_str()
    };
    Ok(json!({
        "protocol": final_protocol,
        "singbox": native,
    }))
}

/// Python `_native_singbox_config(payload)`.
pub(super) fn native_singbox_config(payload: &Value) -> Result<Value, LinkParseError> {
    let mut config = payload.clone();
    if let Some(obj) = config.as_object_mut() {
        for metadata_key in [
            "remarks",
            "remark",
            "ps",
            "name",
            "meta",
            "serverDescription",
            "__lumen_scheme",
            "__lumen_server",
            "__lumen_port",
        ] {
            obj.remove(metadata_key);
        }
    }
    normalize_singbox_wireguard_endpoints(&mut config)
        .map_err(|error| LinkParseError::new(error.to_string()))?;

    let mut used_openvpn_names: Vec<String> = Vec::new();
    let mut openvpn_index: usize = 0;
    let mut plan: Vec<(bool, usize)> = Vec::new();
    if let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) {
        for (index, item) in outbounds.iter().enumerate() {
            if item.is_object()
                && matches!(
                    py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
                    "openvpn" | "openvpn-client"
                )
            {
                plan.push((true, index));
            }
        }
    }
    if let Some(endpoints) = config.get("endpoints").and_then(Value::as_array) {
        for (index, item) in endpoints.iter().enumerate() {
            if item.is_object()
                && matches!(
                    py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
                    "openvpn" | "openvpn-client"
                )
            {
                plan.push((false, index));
            }
        }
    }
    for (is_outbound, index) in plan {
        let container = if is_outbound {
            config.get_mut("outbounds")
        } else {
            config.get_mut("endpoints")
        };
        let Some(Value::Array(items)) = container else { continue };
        let Some(item_obj) = items[index].as_object_mut() else { continue };
        item_obj.insert("system".to_string(), Value::Bool(false));
        let requested_name = {
            let name = py_str_or(item_obj.get("name"), "");
            if name.is_empty() {
                format!("openvpn{openvpn_index}")
            } else {
                name
            }
        }
        .trim()
        .to_string();
        let mut name_candidate = requested_name.clone();
        let mut suffix = 1;
        while used_openvpn_names.contains(&name_candidate) {
            name_candidate = format!("{requested_name}-{suffix}");
            suffix += 1;
        }
        item_obj.insert("name".to_string(), Value::String(name_candidate.clone()));
        used_openvpn_names.push(name_candidate);
        openvpn_index += 1;
    }

    let name = json_name(payload, String::new());
    let endpoint = first_wireguard_endpoint(&config);
    let openvpn_outbound = first_openvpn_outbound(&config);
    let wireguard_auto_group = wireguard_auto_group(&config);
    let openvpn_auto_group = openvpn_auto_group(&config);
    let has_auto_group = py_truthy(&wireguard_auto_group) || py_truthy(&openvpn_auto_group);

    let outbounds_missing = !config.get("outbounds").map(|v| v.is_array()).unwrap_or(false);
    if outbounds_missing {
        if let Some(obj) = config.as_object_mut() {
            obj.insert("outbounds".to_string(), Value::Array(Vec::new()));
        }
    }
    let has_direct = config
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .any(|item| item.is_object() && py_str_or(item.get("tag"), "") == "direct")
        })
        .unwrap_or(false);
    if !has_direct {
        if let Some(items) = config
            .get_mut("outbounds")
            .and_then(Value::as_array_mut)
        {
            items.insert(0, json!({ "type": "direct", "tag": "direct" }));
        }
    }

    let tag = if !name.is_empty() {
        name
    } else if py_truthy(&openvpn_auto_group) {
        "AUTO OpenVPN".to_string()
    } else if py_truthy(&wireguard_auto_group) {
        "AUTO WG".to_string()
    } else if py_truthy(&openvpn_outbound) {
        "OpenVPN".to_string()
    } else {
        "sing-box providers".to_string()
    };
    let mut wrapper = Map::new();
    wrapper.insert("protocol".to_string(), Value::String("singbox_config".into()));
    wrapper.insert("tag".to_string(), Value::String(tag));
    wrapper.insert("singbox_config".to_string(), config.clone());

    if endpoint.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
        let peer = first_mapping(endpoint.get("peers"));
        let endpoint_type = py_str_or(endpoint.get("type"), "");
        let server = py_str_or(peer.get("address"), "");
        wrapper.insert(
            "__lumen_server".to_string(),
            Value::String(if !server.is_empty() {
                server
            } else if endpoint_type == "warp" {
                "engage.cloudflareclient.com".to_string()
            } else {
                String::new()
            }),
        );
        let port = py_int_or(peer.get("port"), 0)?;
        wrapper.insert(
            "__lumen_port".to_string(),
            json!(if port != 0 {
                port
            } else if endpoint_type == "warp" {
                2408
            } else {
                0
            }),
        );
        let scheme = if has_auto_group {
            "auto"
        } else if endpoint_type == "warp" {
            "warp"
        } else if endpoint.get("amnezia").map(|v| v.is_object()).unwrap_or(false) {
            "awg"
        } else {
            "wireguard"
        };
        wrapper.insert("__lumen_scheme".to_string(), Value::String(scheme.into()));
    } else if py_truthy(&openvpn_outbound) {
        let openvpn_server = first_mapping(openvpn_outbound.get("servers"));
        wrapper.insert(
            "__lumen_server".to_string(),
            Value::String(py_str_or(openvpn_server.get("server"), "")),
        );
        wrapper.insert(
            "__lumen_port".to_string(),
            json!(py_int_or(openvpn_server.get("server_port"), 0)?),
        );
        let scheme = if py_truthy(&openvpn_auto_group) {
            "auto"
        } else {
            "openvpn"
        };
        wrapper.insert("__lumen_scheme".to_string(), Value::String(scheme.into()));
    }
    Ok(Value::Object(wrapper))
}

// ── payload shape detection ──

/// Python `_is_singbox_wireguard_config_payload(payload)`.
pub(super) fn is_singbox_wireguard_config_payload(payload: &Value) -> bool {
    let Some(map) = payload.as_object() else {
        return false;
    };
    let endpoint_items: Vec<Value> = map
        .get("endpoints")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let outbound_items: Vec<Value> = map
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let is_wg = |item: &Value, allow_warp: bool| {
        let kind = py_str_or(item.get("type"), "").trim().to_lowercase();
        if allow_warp {
            kind == "wireguard" || kind == "awg" || kind == "warp"
        } else {
            kind == "wireguard" || kind == "awg"
        }
    };
    let wireguard_endpoints: Vec<&Value> = endpoint_items
        .iter()
        .filter(|item| is_wg(item, true))
        .collect();
    let legacy_wireguard: Vec<&Value> = outbound_items.iter().filter(|item| is_wg(item, false)).collect();
    if wireguard_endpoints.is_empty() && legacy_wireguard.is_empty() {
        return false;
    }

    let mut endpoint_tags: Vec<String> = Vec::new();
    for item in wireguard_endpoints.iter().chain(legacy_wireguard.iter()) {
        let tag = py_str_or(item.get("tag"), "").trim().to_string();
        if !tag.is_empty() && !endpoint_tags.contains(&tag) {
            endpoint_tags.push(tag);
        }
    }
    let has_wg_group = outbound_items.iter().any(|item| {
        matches!(
            py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
            "urltest" | "url-test" | "selector"
        ) && item
            .get("outbounds")
            .and_then(Value::as_array)
            .map(|refs| {
                refs.iter().any(|r| endpoint_tags.contains(&py_str_or(Some(r), "").trim().to_string()))
            })
            .unwrap_or(false)
    });
    has_wg_group
        || wireguard_endpoints.len() + legacy_wireguard.len() > 1
        || map.get("route").map(|v| v.is_object()).unwrap_or(false)
        || map.get("inbounds").map(|v| v.is_array()).unwrap_or(false)
        || map.get("dns").map(|v| v.is_object()).unwrap_or(false)
}

/// Python `_is_singbox_openvpn_config_payload(payload)`.
pub(super) fn is_singbox_openvpn_config_payload(payload: &Value) -> bool {
    let Some(map) = payload.as_object() else {
        return false;
    };
    let items: Vec<Value> = map
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let endpoint_items: Vec<Value> = map
        .get("endpoints")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let is_openvpn = |item: &Value| {
        matches!(
            py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
            "openvpn" | "openvpn-client"
        )
    };
    let openvpn_items: Vec<Value> = items
        .iter()
        .chain(endpoint_items.iter())
        .filter(|item| is_openvpn(item))
        .cloned()
        .collect();
    if openvpn_items.is_empty() {
        return false;
    }
    let mut tags: Vec<String> = Vec::new();
    for item in &openvpn_items {
        let tag = py_str_or(item.get("tag"), "").trim().to_string();
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let has_group = items.iter().any(|item| {
        matches!(
            py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
            "urltest" | "url-test" | "selector"
        ) && item
            .get("outbounds")
            .and_then(Value::as_array)
            .map(|refs| refs.iter().any(|r| tags.contains(&py_str_or(Some(r), "").trim().to_string())))
            .unwrap_or(false)
    });
    has_group
        || openvpn_items.len() > 1
        || map.get("route").map(|v| v.is_object()).unwrap_or(false)
        || map.get("inbounds").map(|v| v.is_array()).unwrap_or(false)
        || map.get("dns").map(|v| v.is_object()).unwrap_or(false)
}

/// Python `_first_wireguard_endpoint(config)`.
pub(super) fn first_wireguard_endpoint(config: &Value) -> Value {
    let Some(endpoints) = config.get("endpoints").and_then(Value::as_array) else {
        return json!({});
    };
    for endpoint in endpoints {
        if endpoint.is_object()
            && matches!(
                py_str_or(endpoint.get("type"), "").trim().to_lowercase().as_str(),
                "wireguard" | "warp"
            )
        {
            return endpoint.clone();
        }
    }
    json!({})
}

/// Python `_first_openvpn_outbound(config)`.
pub(super) fn first_openvpn_outbound(config: &Value) -> Value {
    let outbounds = config.get("outbounds").and_then(Value::as_array).cloned();
    let endpoints = config.get("endpoints").and_then(Value::as_array).cloned();
    for outbound in outbounds.into_iter().flatten().chain(endpoints.into_iter().flatten()) {
        if outbound.is_object()
            && matches!(
                py_str_or(outbound.get("type"), "").trim().to_lowercase().as_str(),
                "openvpn" | "openvpn-client"
            )
        {
            return outbound;
        }
    }
    json!({})
}

/// Python `_wireguard_auto_group(config)`.
fn wireguard_auto_group(config: &Value) -> Value {
    let mut endpoint_tags: Vec<String> = Vec::new();
    if let Some(endpoints) = config.get("endpoints").and_then(Value::as_array) {
        for item in endpoints {
            if item.is_object()
                && matches!(
                    py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
                    "wireguard" | "warp"
                )
            {
                let tag = py_str_or(item.get("tag"), "").trim().to_string();
                if !tag.is_empty() && !endpoint_tags.contains(&tag) {
                    endpoint_tags.push(tag);
                }
            }
        }
    }
    if endpoint_tags.is_empty() {
        return json!({});
    }
    let mut groups: Vec<Value> = config
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let route = config
        .get("route")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let final_tag = py_str_or(route.get("final"), "").trim().to_string();
    groups.sort_by_key(|item| py_str_or(item.get("tag"), "") != final_tag);
    for group in &groups {
        if !matches!(
            py_str_or(group.get("type"), "").trim().to_lowercase().as_str(),
            "urltest" | "url-test" | "selector"
        ) {
            continue;
        }
        if let Some(refs) = group.get("outbounds").and_then(Value::as_array) {
            let intersects = refs
                .iter()
                .any(|r| endpoint_tags.contains(&py_str_or(Some(r), "").trim().to_string()));
            if intersects {
                return group.clone();
            }
        }
    }
    json!({})
}

/// Python `_openvpn_auto_group(config)`.
fn openvpn_auto_group(config: &Value) -> Value {
    let mut outbound_tags: Vec<String> = Vec::new();
    let outbounds = config.get("outbounds").and_then(Value::as_array).cloned();
    let endpoints = config.get("endpoints").and_then(Value::as_array).cloned();
    for item in outbounds.into_iter().flatten().chain(endpoints.into_iter().flatten()) {
        if item.is_object()
            && matches!(
                py_str_or(item.get("type"), "").trim().to_lowercase().as_str(),
                "openvpn" | "openvpn-client"
            )
        {
            let tag = py_str_or(item.get("tag"), "").trim().to_string();
            if !tag.is_empty() && !outbound_tags.contains(&tag) {
                outbound_tags.push(tag);
            }
        }
    }
    if outbound_tags.is_empty() {
        return json!({});
    }
    let mut groups: Vec<Value> = config
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter(|item| item.is_object()).cloned().collect())
        .unwrap_or_default();
    let route = config
        .get("route")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let final_tag = py_str_or(route.get("final"), "").trim().to_string();
    groups.sort_by_key(|item| py_str_or(item.get("tag"), "") != final_tag);
    for group in &groups {
        if !matches!(
            py_str_or(group.get("type"), "").trim().to_lowercase().as_str(),
            "urltest" | "url-test" | "selector"
        ) {
            continue;
        }
        if let Some(refs) = group.get("outbounds").and_then(Value::as_array) {
            let intersects = refs
                .iter()
                .any(|r| outbound_tags.contains(&py_str_or(Some(r), "").trim().to_string()));
            if intersects {
                return group.clone();
            }
        }
    }
    json!({})
}

/// Python `_is_xray_auto_config_payload(payload)`.
pub(super) fn is_xray_auto_config_payload(payload: &Value) -> bool {
    let Some(map) = payload.as_object() else {
        return false;
    };
    let outbounds = map.get("outbounds").and_then(Value::as_array);
    let Some(outbounds) = outbounds else {
        return false;
    };
    let has_protocol = outbounds
        .iter()
        .any(|item| item.is_object() && !py_str_or(item.get("protocol"), "").trim().is_empty());
    if !has_protocol {
        return false;
    }
    if map.get("observatory").map(|v| v.is_object()).unwrap_or(false) {
        return true;
    }
    let routing = map.get("routing").and_then(Value::as_object);
    let Some(routing) = routing else {
        return false;
    };
    if routing
        .get("balancers")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    if let Some(rules) = routing.get("rules").and_then(Value::as_array) {
        return rules.iter().any(|rule| {
            rule.is_object() && !py_str_or(rule.get("balancerTag"), "").trim().is_empty()
        });
    }
    false
}

/// Python `_native_xray_config(payload)`.
fn native_xray_config(payload: &Value) -> Value {
    let name = json_name(payload, "AUTO".to_string());
    json!({
        "protocol": "xray_config",
        "tag": name,
        "xray_config": payload,
    })
}
