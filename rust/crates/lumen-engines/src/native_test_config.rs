//! Isolated sing-box test listener; never creates TUN or a system proxy.
//!
//! Port of `windows/xray_fluent/native_test_config.py`.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};

use lumen_core::models::Node;
use lumen_core::probe_capabilities::{GROUP_TYPES, NON_PROXY_TYPES, node_protocol};

use crate::singbox::config_builder::build_singbox_outbound;
use crate::singbox::runtime_planner::{ensure_awg3_windows_bind_workaround, normalize_openvpn_outbounds};

/// Python `str(value)`: converts a JSON scalar the way Python renders it.
/// Only scalar shapes occur in the fields this module stringifies (tags and
/// member lists), but keep the mapping complete for parity.
fn py_str(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// `str(value or "")`: Python falsy shapes render as "".
pub(crate) fn py_str_or(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        Some(Value::Number(number)) if number.as_f64().map(|n| n != 0.0).unwrap_or(true) => {
            number.to_string()
        }
        Some(Value::Bool(true)) => "True".into(),
        Some(value @ (Value::Object(_) | Value::Array(_)))
            if !value.as_object().map(|o| o.is_empty()).unwrap_or(false) =>
        {
            py_str(value)
        }
        _ => String::new(),
    }
}

pub fn build_native_test_config(node: &Node, port: i64) -> Result<Value> {
    let mut config: Value;
    let mut final_tag: String;
    if node_protocol(node) == "singbox_config" {
        let source = node
            .outbound
            .get("singbox_config")
            .cloned()
            .unwrap_or(Value::Null);
        if !source.is_object() {
            bail!("Full sing-box profile is missing");
        }
        config = source;
        let outbounds = config
            .get("outbounds")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let endpoints = config
            .get("endpoints")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let mut all_items: Vec<Value> = Vec::new();
        if let Value::Array(list) = outbounds {
            all_items.extend(list.into_iter().filter(|item| item.is_object()));
        }
        if let Value::Array(list) = endpoints {
            all_items.extend(list.into_iter().filter(|item| item.is_object()));
        }
        final_tag = py_str_or(
            config
                .get("route")
                .and_then(|route| route.as_object())
                .and_then(|route| route.get("final")),
        );
        if final_tag.is_empty() {
            if all_items.is_empty() {
                bail!("No profile outbounds");
            }
            final_tag = py_str_or(all_items[0].get("tag"));
        }
        exclude_direct_group_members(&mut config, &final_tag)?;
        validate_proxy_group(&config, &final_tag)?;
    } else {
        // Python defaults: tag="speed-proxy", multiplex disabled, concurrency 8.
        let outbound = build_singbox_outbound(node, "speed-proxy", false, 8)?;
        final_tag = "speed-proxy".into();
        let endpoint = matches!(
            outbound.get("type").and_then(Value::as_str),
            Some("wireguard" | "warp" | "openvpn-client")
        );
        let mut object = Map::new();
        if endpoint {
            object.insert("endpoints".into(), json!([outbound]));
        } else {
            object.insert("outbounds".into(), json!([outbound]));
        }
        object.insert(
            "dns".into(),
            json!({"servers": [{"type": "local", "tag": "test-dns"}], "final": "test-dns"}),
        );
        object.insert(
            "route".into(),
            json!({"default_domain_resolver": "test-dns"}),
        );
        config = Value::Object(object);
    }
    // Full imported documents need the same OpenVPN endpoint migration as normal
    // connections; otherwise only their ping/speed checks retain obsolete outbounds.
    normalize_openvpn_outbounds(&mut config)?;
    // No provider listeners, shared cache, services or OS interfaces in tests.
    if let Some(object) = config.as_object_mut() {
        object.insert(
            "inbounds".into(),
            json!([{"type": "mixed", "tag": "speed-http", "listen": "127.0.0.1", "listen_port": port}]),
        );
        object.insert("log".into(), json!({"disabled": true}));
        object.remove("experimental");
        object.remove("services");
    }
    // Header-protected AWG must use ClientBind for probes too; the default
    // Windows bind corrupts the protected handshake. The workaround may add a
    // direct outbound to the document, so the endpoints are taken out first.
    let mut endpoints = config
        .get_mut("endpoints")
        .and_then(Value::as_array_mut)
        .map(std::mem::take);
    if let Some(endpoints) = endpoints.as_mut() {
        for item in endpoints.iter_mut() {
            if let Some(object) = item.as_object_mut() {
                object.insert("system".into(), Value::Bool(false));
                ensure_awg3_windows_bind_workaround(&mut config, item);
            }
        }
    }
    if let (Some(endpoints), Some(object)) = (endpoints, config.as_object_mut()) {
        object.insert("endpoints".into(), Value::Array(endpoints));
    }
    if let Some(object) = config.as_object_mut() {
        // `config.setdefault("route", {})`: an existing non-object route makes
        // the Python item assignment raise, so fail closed the same way.
        if let Some(other) = object.get("route") {
            if !other.is_object() {
                bail!("route must be a JSON object for item assignment");
            }
        }
        let route = object
            .entry("route")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("route object");
        route.insert(
            "rules".into(),
            json!([{"inbound": ["speed-http"], "action": "route", "outbound": final_tag}]),
        );
        route.insert("final".into(), Value::String(final_tag.clone()));
        route.remove("auto_detect_interface");
    }
    Ok(config)
}

fn validate_proxy_group(config: &Value, root: &str) -> Result<()> {
    let items: Vec<&Value> = config
        .get("outbounds")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|item| item.is_object())
                .collect::<Vec<&Value>>()
        })
        .unwrap_or_default()
        .into_iter()
        .chain(
            config
                .get("endpoints")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter(|item| item.is_object())
                        .collect::<Vec<&Value>>()
                })
                .unwrap_or_default(),
        )
        .collect();
    let by_tag: HashMap<String, &Value> = items
        .iter()
        .map(|item| (py_str_or(item.get("tag")), *item))
        .collect();
    let mut visiting: Vec<String> = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    fn visit(
        tag: &str,
        by_tag: &HashMap<String, &Value>,
        visiting: &mut Vec<String>,
        visited: &mut Vec<String>,
    ) -> Result<()> {
        if visiting.iter().any(|seen| seen == tag) {
            bail!("Cyclic profile selector");
        }
        if visited.iter().any(|seen| seen == tag) {
            return Ok(());
        }
        let item = by_tag
            .get(tag)
            .ok_or_else(|| anyhow!("A proxy test cannot use a missing/direct group member"))?;
        let kind = py_str_or(item.get("type"));
        if NON_PROXY_TYPES.contains(&kind.as_str()) {
            bail!("A proxy test cannot use a missing/direct group member");
        }
        visiting.push(tag.to_string());
        if GROUP_TYPES.contains(&kind.as_str()) {
            let members = item
                .get("outbounds")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("Empty proxy group"))?;
            if members.is_empty() {
                bail!("Empty proxy group");
            }
            for member in members {
                visit(&py_str(member), by_tag, visiting, visited)?;
            }
        }
        visiting.pop();
        visited.push(tag.to_string());
        Ok(())
    }
    visit(root, &by_tag, &mut visiting, &mut visited)
}

/// An isolated proxy probe must not succeed through an AUTO direct fallback.
fn exclude_direct_group_members(config: &mut Value, root: &str) -> Result<()> {
    // Python mutates the shared outbound/endpoints dicts in place; take the
    // arrays out of the document, mutate them by tag and write them back.
    let had_outbounds = config
        .get("outbounds")
        .map(Value::is_array)
        .unwrap_or(false);
    let had_endpoints = config
        .get("endpoints")
        .map(Value::is_array)
        .unwrap_or(false);
    let mut outbounds: Vec<Value> = config
        .get_mut("outbounds")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    let mut endpoints: Vec<Value> = config
        .get_mut("endpoints")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();

    let mut items: Vec<&mut Value> = outbounds
        .iter_mut()
        .filter(|item| item.is_object())
        .chain(endpoints.iter_mut().filter(|item| item.is_object()))
        .collect();
    let by_tag: HashMap<String, usize> = items
        .iter()
        .enumerate()
        .map(|(index, item)| (py_str_or(item.get("tag")), index))
        .collect();
    let mut visiting: Vec<String> = Vec::new();

    fn visit(
        tag: &str,
        items: &mut [&mut Value],
        by_tag: &HashMap<String, usize>,
        visiting: &mut Vec<String>,
    ) -> bool {
        let Some(&index) = by_tag.get(tag) else {
            return false;
        };
        if visiting.iter().any(|seen| seen == tag) {
            return false;
        }
        let kind = py_str_or(items[index].get("type")).to_lowercase();
        if NON_PROXY_TYPES.contains(&kind.as_str()) {
            return false;
        }
        if !GROUP_TYPES.contains(&kind.as_str()) {
            return true;
        }
        // Clone the member tags before recursing so the recursive borrows of
        // `items` do not overlap with this item's borrow.
        let Some(members) = items[index].get("outbounds").and_then(Value::as_array) else {
            return false;
        };
        let member_tags: Vec<String> = members.iter().map(py_str).collect();
        visiting.push(tag.to_string());
        let mut retained: Vec<Value> = Vec::new();
        for member in member_tags {
            if visit(&member, items, by_tag, visiting) {
                retained.push(Value::String(member));
            }
        }
        visiting.pop();
        items[index]
            .as_object_mut()
            .map(|object| object.insert("outbounds".into(), Value::Array(retained.clone())));
        !retained.is_empty()
    }

    let retained_root = visit(root, &mut items, &by_tag, &mut visiting);
    drop(items);
    if !retained_root {
        bail!("AUTO test has no proxy group member");
    }
    if had_outbounds {
        config["outbounds"] = Value::Array(outbounds);
    }
    if had_endpoints {
        config["endpoints"] = Value::Array(endpoints);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lumen_core::models::Node;
    use serde_json::json;

    use super::*;

    fn node_with(outbound: Value) -> Node {
        Node {
            id: "n1".into(),
            name: "N".into(),
            scheme: "ss".into(),
            outbound,
            ..Node::new()
        }
    }

    #[test]
    fn plain_node_gets_isolated_listener() {
        let node = node_with(json!({
            "protocol": "shadowsocks",
            "settings": {"servers": [{
                "address": "1.2.3.4", "port": 8388,
                "method": "aes-256-gcm", "password": "pw",
            }]},
        }));
        let config = build_native_test_config(&node, 19081).unwrap();
        assert_eq!(
            config["inbounds"],
            json!([{"type": "mixed", "tag": "speed-http", "listen": "127.0.0.1", "listen_port": 19081}])
        );
        assert_eq!(config["outbounds"][0]["tag"], "speed-proxy");
        assert_eq!(config["outbounds"][0]["server"], json!("1.2.3.4"));
        assert_eq!(config["dns"]["final"], "test-dns");
        assert_eq!(config["route"]["final"], "speed-proxy");
        assert_eq!(config["route"]["rules"][0]["outbound"], "speed-proxy");
        assert_eq!(config["log"], json!({"disabled": true}));
        assert!(config.get("experimental").is_none());
        assert!(config.get("services").is_none());
    }

    #[test]
    fn wireguard_outbound_moves_to_endpoints_without_system_interface() {
        let node = node_with(json!({
            "protocol": "wireguard",
            "singbox": {
                "type": "wireguard", "server": "1.2.3.4", "server_port": 51820,
                "peer_public_key": "cHVi", "local_address": ["10.0.0.2/32"],
            },
        }));
        let config = build_native_test_config(&node, 19082).unwrap();
        assert_eq!(config["outbounds"], json!([]));
        assert_eq!(config["endpoints"][0]["tag"], "speed-proxy");
        assert_eq!(config["endpoints"][0]["type"], json!("wireguard"));
        assert_eq!(config["endpoints"][0]["system"], Value::Bool(false));
    }

    #[test]
    fn full_document_without_final_uses_first_outbound_tag() {
        let node = node_with(json!({
            "protocol": "singbox_config",
            "singbox_config": {
                "outbounds": [
                    {"type": "selector", "tag": "auto", "outbounds": ["proxy-1"]},
                    {"type": "shadowsocks", "tag": "proxy-1", "server": "s.example"},
                ],
            },
        }));
        let config = build_native_test_config(&node, 19083).unwrap();
        assert_eq!(config["route"]["final"], "auto");
        assert_eq!(config["route"]["rules"][0]["outbound"], "auto");
    }

    #[test]
    fn full_document_rejects_direct_and_empty_groups() {
        let node = node_with(json!({
            "protocol": "singbox_config",
            "singbox_config": {
                "route": {"final": "auto"},
                "outbounds": [
                    {"type": "selector", "tag": "auto", "outbounds": ["direct"]},
                    {"type": "direct", "tag": "direct"},
                ],
            },
        }));
        let error = build_native_test_config(&node, 19084)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "AUTO test has no proxy group member");

        let node = node_with(json!({
            "protocol": "singbox_config",
            "singbox_config": {
                "route": {"final": "auto"},
                "outbounds": [
                    {"type": "selector", "tag": "auto", "outbounds": []},
                ],
            },
        }));
        let error = build_native_test_config(&node, 19084)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "AUTO test has no proxy group member");
    }

    #[test]
    fn cyclic_selector_is_rejected() {
        let node = node_with(json!({
            "protocol": "singbox_config",
            "singbox_config": {
                "route": {"final": "one"},
                "outbounds": [
                    {"type": "selector", "tag": "one", "outbounds": ["two"]},
                    {"type": "selector", "tag": "two", "outbounds": ["one"]},
                ],
            },
        }));
        let error = build_native_test_config(&node, 19085)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "AUTO test has no proxy group member");
    }

    #[test]
    fn missing_singbox_document_is_an_error() {
        let node = node_with(json!({"protocol": "singbox_config"}));
        let error = build_native_test_config(&node, 19086)
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Full sing-box profile is missing");
    }

    fn awg_key() -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode((0u8..32).collect::<Vec<_>>())
    }

    /// Python `test_awg3_probe_uses_the_same_windows_bind_as_normal_connections`.
    #[test]
    fn awg3_probe_uses_the_same_windows_bind_as_normal_connections() {
        let key = awg_key();
        for version31 in [false, true] {
            for full_profile in [false, true] {
                let mut amnezia = json!({"header_protection_key": key, "h1": "100-200", "content_padding_addition": "0-64"});
                if version31 {
                    amnezia["random_trailers"] = json!(true);
                    amnezia["disable_cookies"] = json!(true);
                }
                let endpoint = json!({
                    "type": "wireguard", "tag": "vpn", "system": false,
                    "address": ["10.0.0.2/32"], "private_key": key, "amnezia": amnezia,
                    "peers": [{"address": "192.0.2.1", "port": 51820, "public_key": key,
                               "allowed_ips": ["0.0.0.0/0"]}],
                });
                let outbound = if full_profile {
                    json!({"protocol": "singbox_config", "singbox_config": {
                        "endpoints": [endpoint], "outbounds": [], "route": {"final": "vpn"},
                    }})
                } else {
                    json!({"protocol": "awg", "singbox": endpoint})
                };
                let original = outbound.clone();
                let mut node = node_with(outbound);
                node.scheme = "awg".into();
                let config = build_native_test_config(&node, 19080).unwrap();
                let tested = &config["endpoints"][0];
                let detour = tested["detour"].as_str().unwrap_or_default();
                assert!(!detour.is_empty(), "v31={version31} full={full_profile}");
                assert!(config["outbounds"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"type": "direct", "tag": detour})));
                assert_eq!(tested["amnezia"], amnezia);
                assert_eq!(tested["system"], json!(false));
                assert_eq!(node.outbound, original);
            }
        }
    }

    /// Python `test_explicit_awg_detour_is_preserved`.
    #[test]
    fn explicit_awg_detour_is_preserved() {
        let key = awg_key();
        let config = json!({"endpoints": [{"type": "wireguard", "tag": "vpn", "detour": "custom",
                                           "amnezia": {"header_protection_key": key}}],
                            "outbounds": [{"type": "direct", "tag": "custom"}], "route": {"final": "vpn"}});
        let node = node_with(json!({"protocol": "singbox_config", "singbox_config": config}));
        assert_eq!(build_native_test_config(&node, 19080).unwrap()["endpoints"][0]["detour"], "custom");
    }
}
