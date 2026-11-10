//! Inspect the ports and remote endpoints of runtime configurations.

use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, TcpListener};

use lumen_core::constants::DEFAULT_XRAY_STATS_API_PORT;
use lumen_core::models::{Node, int_field, str_field};
use serde_json::{Map, Value, json};

fn items<'a>(payload: &'a Value, key: &str) -> &'a [Value] {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub fn config_has_proxy_outbound(payload: &Value) -> bool {
    items(payload, "outbounds")
        .iter()
        .any(|outbound| outbound["tag"] == "proxy")
}

pub fn is_local_runtime_host(host: &str) -> bool {
    matches!(
        host.trim().to_ascii_lowercase().as_str(),
        "" | "127.0.0.1" | "::1" | "localhost"
    )
}

pub fn infer_singbox_outbound_endpoint(outbound: &Value) -> (String, i64) {
    endpoint(outbound, "server", "server_port")
}

pub fn infer_xray_outbound_endpoint(outbound: &Value) -> (String, i64) {
    let protocol = str_field(outbound, "protocol");
    let field = match protocol.trim().to_ascii_lowercase().as_str() {
        "vless" | "vmess" => "vnext",
        "trojan" | "shadowsocks" | "socks" | "http" => "servers",
        _ => return (String::new(), 0),
    };
    items(&outbound["settings"], field)
        .first()
        .map_or((String::new(), 0), |server| {
            endpoint(server, "address", "port")
        })
}

fn endpoint(value: &Value, address_field: &str, port_field: &str) -> (String, i64) {
    let host = str_field(value, address_field);
    let port = runtime_port(value, port_field);
    if port > 0 && !is_local_runtime_host(&host) {
        (host.trim().to_owned(), port)
    } else {
        (String::new(), 0)
    }
}

pub fn infer_singbox_ping_target(payload: &Value, node: Option<&Node>) -> (String, i64) {
    infer_ping_target(payload, node, infer_singbox_outbound_endpoint)
}

pub fn infer_xray_ping_target(payload: &Value, node: Option<&Node>) -> (String, i64) {
    infer_ping_target(payload, node, infer_xray_outbound_endpoint)
}

fn infer_ping_target(
    payload: &Value,
    node: Option<&Node>,
    infer_endpoint: fn(&Value) -> (String, i64),
) -> (String, i64) {
    if let Some(node) = node.filter(|node| !node.server.is_empty() && node.port > 0) {
        return (node.server.clone(), node.port);
    }
    items(payload, "outbounds")
        .iter()
        .map(infer_endpoint)
        .find(|(host, _)| !host.is_empty())
        .unwrap_or_default()
}

pub fn ensure_dict<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    let value = parent.entry(key).or_insert(Value::Null);
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("object ensured")
}

pub fn ensure_list<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    let value = parent.entry(key).or_insert(Value::Null);
    if !value.is_array() {
        *value = json!([]);
    }
    value.as_array_mut().expect("array ensured")
}

pub fn replace_or_append_tagged(items: &mut Vec<Value>, tag: &str, payload: Value) {
    if let Some(item) = items.iter_mut().find(|item| item["tag"] == tag) {
        *item = payload;
    } else {
        items.push(payload);
    }
}

pub(crate) fn runtime_port(value: &Value, key: &str) -> i64 {
    if value[key] == true {
        1
    } else {
        int_field(value, key, 0)
    }
}

pub fn collect_xray_inbound_ports(payload: &Value) -> HashSet<i64> {
    items(payload, "inbounds")
        .iter()
        .map(|inbound| runtime_port(inbound, "port"))
        .filter(|port| *port > 0)
        .collect()
}

pub fn extract_xray_runtime_ports(payload: &Value) -> (i64, i64, i64) {
    let (mut socks, mut http, mut api, mut mixed) = (0, 0, 0, 0);
    for inbound in items(payload, "inbounds") {
        let port = runtime_port(inbound, "port");
        let tag = str_field(inbound, "tag").trim().to_ascii_lowercase();
        if port <= 0 || tag == "discord-socks-in" {
            continue;
        }
        match str_field(inbound, "protocol")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "socks" if socks == 0 => socks = port,
            "http" if http == 0 => http = port,
            "mixed" if mixed == 0 => mixed = port,
            _ if api == 0 && matches!(tag.as_str(), "api" | "__app_metrics_api_in") => api = port,
            _ => {}
        }
    }
    if mixed != 0 {
        (mixed, mixed, api)
    } else {
        (socks, http, api)
    }
}

pub fn find_free_api_port(preferred: Option<u16>, excluded: &HashSet<i64>) -> io::Result<u16> {
    let first = u32::from(preferred.unwrap_or(DEFAULT_XRAY_STATS_API_PORT));
    let end = first + 100;
    for port in first..end {
        let Ok(port) = u16::try_from(port) else { break };
        if !excluded.contains(&i64::from(port))
            && TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
        {
            return Ok(port);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        format!("No free port in range {first}-{end}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mixed_port_overrides_separate_ports_and_ignores_discord() {
        let payload = json!({"inbounds": [
            {"protocol":"socks","tag":"discord-socks-in","port":10818},
            {"protocol":"socks","port":10808},
            {"protocol":"http","port":10809},
            {"protocol":"mixed","port":"10810"},
            {"tag":"__app_metrics_api_in","port":19085}
        ]});
        assert_eq!(extract_xray_runtime_ports(&payload), (10810, 10810, 19085));
        assert!(collect_xray_inbound_ports(&payload).contains(&10818));
    }

    #[test]
    fn finds_the_first_remote_endpoint_and_prefers_a_selected_node() {
        let payload = json!({"outbounds":[
            {"protocol":"http","settings":{"servers":[{"address":"localhost","port":80}]}},
            {"protocol":"vless","settings":{"vnext":[{"address":" example.test ","port":"443"}]}}
        ]});
        assert_eq!(
            infer_xray_ping_target(&payload, None),
            ("example.test".into(), 443)
        );
        let node = Node::from_dict(&json!({"server":"chosen.example","port":1443}));
        assert_eq!(
            infer_xray_ping_target(&payload, Some(&node)),
            ("chosen.example".into(), 1443)
        );
        assert_eq!(
            infer_singbox_outbound_endpoint(&json!({"server":"s.example","server_port":7443})),
            ("s.example".into(), 7443)
        );
    }
}
