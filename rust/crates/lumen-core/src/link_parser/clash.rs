//! Clash YAML subscription and Clash proxy-payload parsing
//! (`_parse_clash_*` helpers from link_parser.py).

use std::sync::OnceLock;

use regex::Regex;
use serde_json::{json, Map, Value};

use super::helpers::{py_int_or, py_int_value, py_str_or, py_str, py_truthy, to_bool_value};
use super::json_import::{native_singbox_config, native_singbox_outbound, parse_json_outbound_payload};
use super::schemes::{normalize_vmess_security_value_pub, sip003_options_from_mapping};
use super::wireguard::{
    amnezia_from_params, build_wireguard_endpoint, build_warp_profile_endpoint, is_warp_endpoint,
    normalize_ip_prefixes, parse_reserved_bytes,
};
use super::{LinkParseError, Node, MAX_IMPORT_NODES};

fn clash_section_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?mi)^\s*(proxies|proxy-providers|proxy-groups|rules)\s*:").expect("static regex")
    })
}

fn port_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*(\d{1,5})").expect("static regex"))
}

/// Python `_looks_like_clash_yaml`.
pub(super) fn looks_like_clash_yaml(text: &str) -> bool {
    let mut sections: Vec<String> = Vec::new();
    for captures in clash_section_regex().captures_iter(text) {
        let name = captures
            .get(1)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        if !sections.contains(&name) {
            sections.push(name);
        }
    }
    sections.contains(&"proxies".to_string())
        || sections.contains(&"proxy-providers".to_string())
        || (sections.contains(&"proxy-groups".to_string()) && sections.contains(&"rules".to_string()))
}

fn expandtabs(text: &str, tabsize: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for c in text.chars() {
        if c == '\t' {
            let spaces = tabsize - (col % tabsize);
            for _ in 0..spaces {
                out.push(' ');
            }
            col += spaces;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Python `_parse_clash_yaml_nodes_text`.
pub(super) fn parse_clash_yaml_nodes_text(text: &str) -> Result<(Vec<Node>, Vec<String>), LinkParseError> {
    // A few generators emit tabs in indentation or as trailing whitespace.
    // YAML forbids those tabs even though they do not change the document's
    // meaning, so normalize structural whitespace before parsing.
    let mut normalized_lines: Vec<String> = Vec::new();
    for raw_line in super::helpers::py_splitlines(text) {
        let mut line = raw_line.trim_end_matches([' ', '\t']).to_string();
        let prefix_bytes = line
            .char_indices()
            .find(|(_, c)| *c != ' ' && *c != '\t')
            .map(|(pos, _)| pos)
            .unwrap_or(line.len());
        if prefix_bytes > 0 && line[..prefix_bytes].contains('\t') {
            let expanded = expandtabs(&line[..prefix_bytes], 2);
            line = format!("{}{}", expanded, &line[prefix_bytes..]);
        }
        normalized_lines.push(line);
    }
    let payload: Value = serde_yaml::from_str(&normalized_lines.join("\n"))
        .map_err(|e| LinkParseError::new(e.to_string()))?;
    let payload = payload
        .as_object()
        .ok_or_else(|| LinkParseError::new("YAML root must be an object"))?;
    let proxies = payload
        .get("proxies")
        .and_then(Value::as_array)
        .ok_or_else(|| LinkParseError::new("YAML must contain a proxies list"))?;
    if proxies.len() > MAX_IMPORT_NODES {
        return Err(LinkParseError::new(format!(
            "YAML contains more than {MAX_IMPORT_NODES} proxies"
        )));
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for (idx, item) in proxies.iter().enumerate() {
        let idx = idx + 1;
        match item {
            Value::Object(_) => match parse_clash_proxy_payload(item) {
                Ok(node) => nodes.push(node),
                Err(exc) => errors.push(format!("Clash proxy {idx}: {exc}")),
            },
            other => errors.push(format!(
                "Clash proxy {idx}: unsupported proxy item type: {}",
                super::helpers::python_type_name(other)
            )),
        }
    }
    let auto_nodes = parse_clash_wireguard_auto_groups(payload, &nodes, &mut errors)?;
    nodes.extend(auto_nodes);
    Ok((nodes, errors))
}

/// Python `_parse_clash_wireguard_auto_groups`.
pub(super) fn parse_clash_wireguard_auto_groups(
    payload: &Map<String, Value>,
    nodes: &[Node],
    errors: &mut Vec<String>,
) -> Result<Vec<Node>, LinkParseError> {
    // Translate Clash url-test groups made from WG endpoints to sing-box AUTO.
    let groups = match payload
        .get("proxy-groups")
        .or_else(|| payload.get("proxy_groups"))
    {
        Some(Value::Array(items)) => items.clone(),
        _ => return Ok(Vec::new()),
    };

    let mut node_by_name: std::collections::BTreeMap<String, Node> = std::collections::BTreeMap::new();
    for node in nodes {
        if !node.name.is_empty() {
            node_by_name.insert(node.name.clone(), node.clone());
        }
    }
    let mut auto_nodes: Vec<Node> = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        let index = group_index + 1;
        let group = match group.as_object() {
            Some(g) => g,
            None => continue,
        };
        let group_type = py_str_or(group.get("type"), "").trim().to_lowercase();
        if group_type != "url-test" && group_type != "urltest" {
            continue;
        }
        let refs = match group.get("proxies").and_then(Value::as_array) {
            Some(items) => items.clone(),
            None => continue,
        };
        let members: Vec<Node> = refs
            .iter()
            .filter_map(|r| node_by_name.get(&py_str_or(Some(r), "").trim().to_string()))
            .cloned()
            .collect();
        if members.len() < 2
            || !members.iter().all(|node| {
                matches!(
                    py_str_or(node.outbound.get("protocol"), "")
                        .to_lowercase()
                        .as_str(),
                    "wireguard" | "awg" | "warp"
                )
            })
        {
            continue;
        }
        let mut endpoint_tags: Vec<String> = Vec::new();
        let mut endpoints: Vec<Value> = Vec::new();
        let mut failed = false;
        for (member_index, member) in members.iter().enumerate() {
            let member_index = member_index + 1;
            let native = member
                .outbound
                .get("singbox")
                .filter(|v| v.is_object())
                .cloned();
            let Some(mut endpoint) = native else {
                errors.push(format!(
                    "Clash AUTO WG group {index}: {}: missing native WireGuard endpoint",
                    member.name
                ));
                failed = true;
                break;
            };
            let endpoint_tag = format!("wg-auto-{index}-{member_index}");
            if let Some(obj) = endpoint.as_object_mut() {
                obj.insert("tag".to_string(), Value::String(endpoint_tag.clone()));
            }
            endpoint_tags.push(endpoint_tag);
            endpoints.push(endpoint);
        }
        if failed {
            continue;
        }

        let group_name = {
            let name = py_str_or(group.get("name"), "");
            if name.trim().is_empty() {
                format!("AUTO WG {index}")
            } else {
                name.trim().to_string()
            }
        };
        let group_tag = format!("auto-wg-{index}");
        let interval = group.get("interval").cloned();
        let interval_text = match &interval {
            Some(v) if v.is_number() => {
                let as_int = py_int_value(v).unwrap_or(0);
                if as_int > 0 {
                    format!("{as_int}s")
                } else {
                    py_str_or(Some(v), "3m")
                }
            }
            Some(v) => {
                let text = py_str_or(Some(v), "");
                if text.is_empty() { "3m".to_string() } else { text }
            }
            None => "3m".to_string(),
        };
        let urltest = json!({
            "type": "urltest",
            "tag": group_tag,
            "outbounds": endpoint_tags,
            "url": py_str_or(group.get("url"), "https://www.gstatic.com/generate_204"),
            "interval": interval_text,
            "tolerance": py_int_or(group.get("tolerance"), 50).unwrap_or(50),
            "interrupt_exist_connections": true,
        });
        let config = json!({
            "endpoints": endpoints,
            "outbounds": [
                { "type": "direct", "tag": "direct" },
                urltest,
            ],
            "route": { "final": group_tag, "auto_detect_interface": true },
        });
        let mut wrapper = native_singbox_config(&config)?;
        if let Some(obj) = wrapper.as_object_mut() {
            obj.insert("tag".to_string(), Value::String(group_name.clone()));
            obj.insert("remarks".to_string(), Value::String(group_name.clone()));
            obj.insert("__lumen_scheme".to_string(), Value::String("auto".into()));
            obj.insert("__lumen_server".to_string(), Value::String(members[0].server.clone()));
            obj.insert("__lumen_port".to_string(), json!(members[0].port));
        }
        let mut node = parse_json_outbound_payload(&wrapper)?;
        node.name = group_name;
        node.description = "Автовыбор лучших серверов".to_string();
        auto_nodes.push(node);
    }
    Ok(auto_nodes)
}

// ── small clash value helpers ──

/// Python `d.get(k1, d.get(k2))`.
pub(super) fn get_or<'a>(payload: &'a Value, key1: &str, key2: &str) -> Option<&'a Value> {
    if payload.get(key1).is_some() {
        payload.get(key1)
    } else {
        payload.get(key2)
    }
}

fn get_or3<'a>(payload: &'a Value, key1: &str, key2: &str, key3: &str) -> Option<&'a Value> {
    if payload.get(key1).is_some() {
        payload.get(key1)
    } else if payload.get(key2).is_some() {
        payload.get(key2)
    } else {
        payload.get(key3)
    }
}

/// Python `_clash_list`.
pub(super) fn clash_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(py_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        other => super::helpers::split_csv(&py_str_or(other, "")),
    }
}

/// Python `a or b` truthiness chain over two dict keys → str result.
fn py_str_or2(payload: &Value, key1: &str, key2: &str) -> String {
    match payload.get(key1) {
        Some(v) if py_truthy(v) => py_str(v),
        _ => match payload.get(key2) {
            Some(v) if py_truthy(v) => py_str(v),
            _ => String::new(),
        },
    }
}

/// Python `a or b` truthiness chain returning the raw Value.
fn py_or2(payload: &Value, key1: &str, key2: &str) -> Option<Value> {
    match payload.get(key1) {
        Some(v) if py_truthy(v) => Some(v.clone()),
        _ => payload.get(key2).cloned(),
    }
}

/// Python `payload.get(key) or ""` → Some(value) when truthy, else None.
fn py_or_default(payload: &Value, key: &str) -> Option<Value> {
    payload.get(key).filter(|v| py_truthy(v)).cloned()
}

/// Python `_representative_clash_port`.
fn representative_clash_port(ranges: &[String]) -> i64 {
    for value in ranges {
        if let Some(captures) = port_prefix_regex().captures(value) {
            let digits = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Ok(number) = digits.parse::<i64>() {
                if (1..=65535).contains(&number) {
                    return number;
                }
            }
        }
    }
    0
}

/// Python `_clash_speed_mbps`.
fn clash_speed_mbps(value: Option<&Value>) -> Option<i64> {
    let digits: String = py_str_or(value, "")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Python `_clash_duration`.
fn clash_duration(value: Option<&Value>) -> String {
    let text = py_str_or(value, "").trim().to_string();
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        format!("{text}s")
    } else {
        text
    }
}

/// Python `_clash_certificate_pins`.
fn clash_certificate_pins(payload: &Value) -> Vec<String> {
    for key in [
        "certificate-public-key-sha256",
        "certificate_public_key_sha256",
        "pin-sha256",
        "pinSHA256",
    ] {
        let pins = clash_list(payload.get(key));
        if !pins.is_empty() {
            return pins;
        }
    }
    Vec::new()
}

// ── payload entry ──

/// Python `_is_clash_proxy_payload`.
pub(super) fn is_clash_proxy_payload(payload: &Value) -> bool {
    let Some(map) = payload.as_object() else {
        return false;
    };
    let kind = py_str_or(map.get("type"), "").trim().to_lowercase();
    let known = [
        "vless", "vmess", "trojan", "ss", "shadowsocks", "hy", "hysteria", "hy2", "hysteria2",
        "tuic", "wireguard", "awg", "amneziawg", "amnezia-wg", "masque", "naive", "mieru",
        "anytls", "snell", "socks", "socks5", "http", "https",
    ];
    if !known.contains(&kind.as_str()) {
        return false;
    }
    (map.contains_key("server")
        && ["port", "server-port", "server-ports", "server_ports", "ports"]
            .iter()
            .any(|key| map.contains_key(*key))
        && !map.contains_key("server_port"))
        || map.contains_key("private-key")
        || map.contains_key("public-key")
        || map.contains_key("amnezia-wg-option")
}

/// Python `_parse_clash_proxy_payload`.
pub(super) fn parse_clash_proxy_payload(payload: &Value) -> Result<Node, LinkParseError> {
    let mut payload = payload.clone();
    let mut kind = py_str_or(payload.get("type"), "").trim().to_lowercase();
    let mut tls_override = false;
    if kind == "ss" || kind == "shadowsocks" {
        kind = "shadowsocks".to_string();
    } else if kind == "hy" || kind == "hysteria" {
        kind = "hysteria".to_string();
    } else if kind == "hy2" || kind == "hysteria2" {
        kind = "hysteria2".to_string();
    } else if kind == "socks" || kind == "socks5" {
        kind = "socks".to_string();
    } else if kind == "https" {
        kind = "http".to_string();
        tls_override = true;
    } else if matches!(kind.as_str(), "awg" | "amneziawg" | "amnezia-wg") {
        kind = "wireguard".to_string();
    }
    if tls_override {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("tls".to_string(), Value::Bool(true));
        }
    }
    if !"vless vmess trojan shadowsocks socks http wireguard hysteria hysteria2 tuic masque naive mieru anytls snell"
        .split(' ')
        .any(|k| k == kind)
    {
        return Err(LinkParseError::new(format!(
            "unsupported Clash proxy type: {}",
            if kind.is_empty() { "unknown" } else { &kind }
        )));
    }

    let server = py_str_or(payload.get("server"), "").trim().to_string();
    let server_ports = clash_list(get_or3(&payload, "server-ports", "server_ports", "ports"));
    let port_raw = py_or2(&payload, "port", "server-port");
    let port = match port_raw {
        Some(v) if py_truthy(&v) => py_int_value(&v)?,
        _ => representative_clash_port(&server_ports),
    };
    let name = {
        let n = py_str_or(payload.get("name"), "");
        let n = if !n.is_empty() {
            n
        } else {
            let t = py_str_or(payload.get("tag"), "");
            if !t.is_empty() {
                t
            } else {
                format!("{kind}-{server}:{port}")
            }
        };
        n.trim().to_string()
    };
    if server.is_empty() || port <= 0 {
        return Err(LinkParseError::new("proxy must contain server and port"));
    }

    let outbound: Value = if kind == "wireguard" {
        let node = parse_clash_wireguard_payload(&payload, &name, &server, port)?;
        return Ok(node);
    } else if kind == "masque" {
        let node = parse_clash_masque_payload(&payload, &name, &server, port)?;
        return Ok(node);
    } else if matches!(
        kind.as_str(),
        "hysteria" | "hysteria2" | "tuic" | "naive" | "mieru" | "anytls" | "snell"
    ) {
        native_singbox_outbound(&clash_to_singbox_outbound(&payload, &kind)?)?
    } else {
        clash_to_xray_outbound(&payload, &kind)?
    };

    let mut node = Node::new();
    node.name = name;
    node.scheme = kind;
    node.server = server;
    node.port = port;
    node.link = serde_json::to_string(&outbound).unwrap_or_default();
    node.outbound = outbound;
    Ok(node)
}

/// Python `_parse_clash_masque_payload`.
fn parse_clash_masque_payload(
    payload: &Value,
    name: &str,
    server: &str,
    port: i64,
) -> Result<Node, LinkParseError> {
    // Convert WARP-generator Clash MASQUE entries to sing-box-extended.
    let network = py_str_or(payload.get("network"), "").trim().to_lowercase();
    let mut profile = Map::new();
    profile.insert("detour".to_string(), Value::String("direct".into()));
    for (source, target) in [
        ("profile-id", "id"),
        ("profile_id", "id"),
        ("auth-token", "auth_token"),
        ("auth_token", "auth_token"),
        ("masque-private-key", "private_key"),
        ("masque_private_key", "private_key"),
    ] {
        let value = py_str_or(payload.get(source), "").trim().to_string();
        if !value.is_empty() {
            profile.insert(target.to_string(), Value::String(value));
        }
    }
    let private_key = py_str_or2(payload, "private-key", "private_key").trim().to_string();
    let public_key = py_str_or2(payload, "public-key", "public_key").trim().to_string();
    let mut address = normalize_ip_prefixes(&Value::Array(
        clash_list(py_or2(payload, "ip", "address").as_ref())
            .into_iter()
            .map(Value::String)
            .collect(),
    ));
    for item in normalize_ip_prefixes(&Value::Array(
        clash_list(payload.get("ipv6")).into_iter().map(Value::String).collect(),
    )) {
        if !address.contains(&item) {
            address.push(item);
        }
    }
    let mut native = Map::new();
    native.insert("type".to_string(), Value::String("masque".into()));
    native.insert("tag".to_string(), Value::String("proxy".into()));
    native.insert("system".to_string(), Value::Bool(false));
    native.insert("name".to_string(), Value::String("masque0".into()));
    native.insert(
        "use_http2".to_string(),
        Value::Bool(network == "h2" || network == "http2"),
    );
    let use_ipv6 = to_bool_value(py_or2(payload, "use-ipv6", "use_ipv6").as_ref());
    native.insert("use_ipv6".to_string(), Value::Bool(use_ipv6));
    native.insert("profile".to_string(), Value::Object(profile));
    native.insert("udp_timeout".to_string(), Value::String("5m0s".into()));
    native.insert("udp_keepalive_period".to_string(), Value::String("30s".into()));
    native.insert("reconnect_delay".to_string(), Value::String("5s".into()));
    native.insert("congestion_controller".to_string(), Value::String("bbr".into()));
    // Mihomo/usque MASQUE profiles contain a ready-to-use ECDSA client key,
    // endpoint public key and tunnel addresses.  Keep those values together;
    // treating this format as an empty Cloudflare registration profile leaves
    // the outbound permanently in "tunnel not initialized" state.
    if !private_key.is_empty() || !public_key.is_empty() {
        if private_key.is_empty() || public_key.is_empty() || server.is_empty() {
            return Err(LinkParseError::new(
                "direct MASQUE proxy must contain server, private-key and public-key",
            ));
        }
        if address.is_empty() {
            return Err(LinkParseError::new(
                "direct MASQUE proxy must contain ip, ipv6 or address",
            ));
        }
        native.insert("server".to_string(), Value::String(server.to_string()));
        native.insert("server_port".to_string(), json!(if port == 0 { 443 } else { port }));
        native.insert("private_key".to_string(), Value::String(private_key));
        native.insert("public_key".to_string(), Value::String(public_key));
        native.insert(
            "tunnel_address".to_string(),
            Value::Array(address.into_iter().map(Value::String).collect()),
        );
        native.insert(
            "mtu".to_string(),
            json!(py_int_or(payload.get("mtu"), 1280)?),
        );
    }
    let allowed_ips = normalize_ip_prefixes(&Value::Array(
        clash_list(get_or(payload, "allowed-ips", "allowed_ips"))
            .into_iter()
            .map(Value::String)
            .collect(),
    ));
    if !allowed_ips.is_empty() {
        native.insert(
            "allowed_ips".to_string(),
            Value::Array(allowed_ips.into_iter().map(Value::String).collect()),
        );
    }
    let server_name = py_str_or(get_or(payload, "sni", "servername"), "")
        .trim()
        .to_string();
    if !server_name.is_empty() {
        native.insert(
            "tls".to_string(),
            json!({ "server_name": server_name }),
        );
    }
    let mut outbound = native_singbox_outbound(&Value::Object(native))?;
    let dns_servers = clash_list(payload.get("dns"));
    if !dns_servers.is_empty() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert(
                "_dns".to_string(),
                Value::Array(dns_servers.into_iter().map(Value::String).collect()),
            );
        }
    }
    let mut node = Node::new();
    node.name = name.to_string();
    node.scheme = "masque".to_string();
    node.server = server.to_string();
    node.port = port;
    node.link = serde_json::to_string(payload).unwrap_or_default();
    node.outbound = outbound;
    node.tags = vec!["WARP".to_string()];
    Ok(node)
}

/// Python `_parse_clash_wireguard_payload`.
fn parse_clash_wireguard_payload(
    payload: &Value,
    name: &str,
    server: &str,
    port: i64,
) -> Result<Node, LinkParseError> {
    let private_key = py_str_or2(payload, "private-key", "private_key").trim().to_string();
    let public_key = py_str_or2(payload, "public-key", "public_key").trim().to_string();
    if private_key.is_empty() || public_key.is_empty() {
        return Err(LinkParseError::new(
            "wireguard proxy must contain private-key and public-key",
        ));
    }
    let mut address = normalize_ip_prefixes(&Value::Array(
        clash_list(py_or2(payload, "ip", "address").as_ref())
            .into_iter()
            .map(Value::String)
            .collect(),
    ));
    for item in normalize_ip_prefixes(&Value::Array(
        clash_list(payload.get("ipv6")).into_iter().map(Value::String).collect(),
    )) {
        if !address.contains(&item) {
            address.push(item);
        }
    }
    let allowed_ips = normalize_ip_prefixes(&Value::Array(
        clash_list(py_or2(payload, "allowed-ips", "allowed_ips").as_ref())
            .into_iter()
            .map(Value::String)
            .collect(),
    ));
    let amnezia_options = {
        let a = payload.get("amnezia-wg-option");
        match a {
            Some(v) if py_truthy(v) => Some(v.clone()),
            _ => payload.get("amnezia").cloned(),
        }
    };
    let amnezia = match &amnezia_options {
        Some(Value::Object(map)) => {
            let params: std::collections::BTreeMap<String, String> = map
                .iter()
                .map(|(k, v)| (k.clone(), py_str(v)))
                .collect();
            amnezia_from_params(&params)?
        }
        _ => Map::new(),
    };
    let has_amnezia = !amnezia.is_empty();
    let reserved_param = payload.get("reserved").cloned();
    let reserved_bytes = parse_reserved_bytes(reserved_param.as_ref())?;
    let keepalive = {
        let a = payload.get("persistent-keepalive");
        match a {
            Some(v) if py_truthy(v) => Some(v.clone()),
            _ => payload.get("persistent_keepalive_interval").cloned(),
        }
    };
    let keepalive_live = match &keepalive {
        Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
        None => false,
    };

    if !reserved_bytes.is_empty() && !is_warp_endpoint(server) {
        return Err(LinkParseError::new(
            "reserved bytes are supported only for Cloudflare WARP profiles",
        ));
    }

    let (scheme, endpoint) = if !reserved_bytes.is_empty() {
        let listen_port = py_or2(payload, "listen-port", "listen_port");
        (
            if has_amnezia { "awg" } else { "warp" },
            build_warp_profile_endpoint(
                &private_key,
                if keepalive_live { keepalive.as_ref() } else { None },
                Some(amnezia),
                listen_port.as_ref(),
            )?,
        )
    } else {
        let listen_port = py_or2(payload, "listen-port", "listen_port");
        let udp_timeout = {
            let a = py_str_or(get_or(payload, "udp-timeout", "udp_timeout"), "");
            if !a.is_empty() {
                a
            } else {
                "5m0s".to_string()
            }
        };
        let workers = py_or_default(payload, "workers");
        let preallocated = get_or(payload, "preallocated-buffers-per-pool", "preallocated_buffers_per_pool")
            .cloned()
            .filter(|v| py_truthy(v));
        let disable_pauses = if payload.get("disable-pauses").is_some() {
            payload.get("disable-pauses").cloned()
        } else {
            payload.get("disable_pauses").cloned()
        };
        let pre_shared_key = {
            let a = py_str_or(payload.get("pre-shared-key"), "");
            if !a.is_empty() {
                a
            } else {
                let b = py_str_or(payload.get("preshared-key"), "");
                if !b.is_empty() {
                    b
                } else {
                    py_str_or(payload.get("pre_shared_key"), "")
                }
            }
        };
        let mtu = py_int_or(payload.get("mtu"), 1408)?;
        let endpoint = build_wireguard_endpoint(
            server,
            port,
            &private_key,
            &public_key,
            &address,
            &allowed_ips,
            &pre_shared_key,
            mtu,
            Some(amnezia),
            if keepalive_live { keepalive.as_ref() } else { None },
            listen_port.as_ref(),
            &udp_timeout,
            workers.as_ref(),
            preallocated.as_ref(),
            disable_pauses.as_ref(),
        )?;
        (if has_amnezia { "awg" } else { "wireguard" }, endpoint)
    };

    let mut outbound = native_singbox_outbound(&Value::Object(endpoint))?;
    let dns_servers = clash_list(payload.get("dns"));
    if !dns_servers.is_empty() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert(
                "_dns".to_string(),
                Value::Array(dns_servers.into_iter().map(Value::String).collect()),
            );
        }
    }
    let mut node = Node::new();
    node.name = name.to_string();
    node.scheme = scheme.to_string();
    node.server = server.to_string();
    node.port = port;
    node.link = serde_json::to_string(payload).unwrap_or_default();
    node.outbound = outbound;
    node.tags = if is_warp_endpoint(server) {
        vec!["WARP".to_string()]
    } else {
        Vec::new()
    };
    Ok(node)
}

/// Python `_clash_to_xray_outbound`.
fn clash_to_xray_outbound(payload: &Value, kind: &str) -> Result<Value, LinkParseError> {
    let server = py_str_or(payload.get("server"), "").trim().to_string();
    let port = py_int_or(get_or(payload, "port", "server-port"), 0)?;
    let user_id = py_str_or(get_or(payload, "uuid", "id"), "").trim().to_string();
    let password = py_str_or(payload.get("password"), "").trim().to_string();

    let settings: Value = if kind == "vless" || kind == "vmess" {
        let mut user = Map::new();
        user.insert("id".to_string(), Value::String(user_id));
        if kind == "vless" {
            user.insert(
                "encryption".to_string(),
                Value::String(py_str_or(payload.get("encryption"), "none")),
            );
            let flow = py_str_or(payload.get("flow"), "").trim().to_string();
            if !flow.is_empty() {
                user.insert("flow".to_string(), Value::String(flow));
            }
        } else {
            user.insert(
                "alterId".to_string(),
                json!(py_int_or(get_or(payload, "alterId", "alter-id"), 0)?),
            );
            user.insert(
                "security".to_string(),
                Value::String(normalize_vmess_security_value_pub(payload.get("cipher"))),
            );
        }
        json!({ "vnext": [{ "address": server, "port": port, "users": [Value::Object(user)] }] })
    } else if kind == "trojan" {
        json!({ "servers": [{ "address": server, "port": port, "password": password }] })
    } else if kind == "socks" || kind == "http" {
        let mut server_item = Map::new();
        server_item.insert("address".to_string(), Value::String(server.clone()));
        server_item.insert("port".to_string(), json!(port));
        let username = py_str_or(get_or(payload, "username", "user"), "")
            .trim()
            .to_string();
        if !username.is_empty() {
            server_item.insert(
                "users".to_string(),
                json!([{ "user": username, "pass": password }]),
            );
        }
        json!({ "servers": [Value::Object(server_item)] })
    } else {
        let mut plugin_options_value = get_or(payload, "plugin-opts", "plugin_opts")
            .cloned()
            .unwrap_or(Value::String(String::new()));
        let plugin_name = super::schemes::normalize_shadowsocks_plugin_name_pub(payload.get("plugin"));
        if let Some(map) = plugin_options_value.as_object() {
            let obfs = plugin_name == "obfs-local";
            let rendered = sip003_options_from_mapping(&Value::Object(map.clone()), obfs);
            plugin_options_value = Value::String(rendered);
        }
        let mut plugin_options = py_str_or(Some(&plugin_options_value), "");
        if plugin_name == "obfs-local" {
            plugin_options = super::schemes::normalize_obfs_plugin_options(&plugin_options);
        } else if plugin_name == "v2ray-plugin" {
            plugin_options = super::schemes::normalize_v2ray_plugin_options(&plugin_options);
        }
        let mut server_item = Map::new();
        server_item.insert("address".to_string(), Value::String(server.clone()));
        server_item.insert("port".to_string(), json!(port));
        server_item.insert(
            "method".to_string(),
            Value::String({
                let a = py_str_or(payload.get("cipher"), "");
                if !a.is_empty() {
                    a
                } else {
                    let b = py_str_or(payload.get("method"), "");
                    if !b.is_empty() { b } else { "none".to_string() }
                }
            }),
        );
        server_item.insert("password".to_string(), Value::String(password));
        if !plugin_name.is_empty() {
            server_item.insert("plugin".to_string(), Value::String(plugin_name));
        }
        if !plugin_options.is_empty() {
            server_item.insert("plugin_opts".to_string(), Value::String(plugin_options));
        }
        json!({ "servers": [Value::Object(server_item)] })
    };

    let mut outbound = Map::new();
    outbound.insert("protocol".to_string(), Value::String(kind.to_string()));
    outbound.insert("settings".to_string(), settings);
    let attach_stream = !(kind == "socks" || kind == "http")
        || (kind == "http"
            && (to_bool_value(payload.get("tls"))
                || !py_str_or(get_or(payload, "servername", "sni"), "")
                    .trim()
                    .is_empty()));
    if attach_stream {
        outbound.insert("streamSettings".to_string(), clash_stream_settings(payload));
    }
    Ok(Value::Object(outbound))
}

/// Python `_clash_stream_settings`.
pub(super) fn clash_stream_settings(payload: &Value) -> Value {
    let mut network = py_str_or(payload.get("network"), "tcp").trim().to_lowercase();
    if network == "httpupgrade" || network == "http-upgrade" {
        network = "httpupgrade".to_string();
    }
    let security = if to_bool_value(payload.get("tls")) {
        "tls"
    } else {
        "none"
    };
    let reality_trigger = match payload.get("reality-opts") {
        Some(v) if py_truthy(v) => !py_str(v).trim().is_empty(),
        _ => false,
    } || payload.get("reality").map(py_truthy).unwrap_or(false);
    let security = if reality_trigger { "reality" } else { security };
    let mut stream = Map::new();
    stream.insert("network".to_string(), Value::String(network.clone()));
    stream.insert("security".to_string(), Value::String(security.to_string()));

    let ws_opts = get_or(payload, "ws-opts", "ws_opts")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let grpc_opts = get_or(payload, "grpc-opts", "grpc_opts")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut http_opts = get_or(payload, "h2-opts", "h2_opts")
        .filter(|v| v.is_object())
        .cloned();
    if http_opts.is_none() {
        http_opts = get_or(payload, "http-opts", "http_opts").filter(|v| v.is_object()).cloned();
    }
    let http_opts = http_opts.unwrap_or_else(|| json!({}));
    let xhttp_opts = payload
        .get("xhttp-opts")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    if network == "ws" {
        let headers = ws_opts
            .get("headers")
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut ws_settings = Map::new();
        ws_settings.insert(
            "path".to_string(),
            Value::String({
                let a = py_str_or(ws_opts.get("path"), "");
                if !a.is_empty() {
                    a
                } else {
                    let b = py_str_or(payload.get("path"), "");
                    if !b.is_empty() { b } else { "/".to_string() }
                }
            }),
        );
        ws_settings.insert("headers".to_string(), headers);
        let early_data = ws_opts
            .get("max-early-data")
            .or_else(|| ws_opts.get("max_early_data"))
            .or_else(|| payload.get("max-early-data"))
            .cloned();
        let early_text = early_data.map(|v| py_str_or(Some(&v), "")).unwrap_or_default();
        if !early_text.is_empty() && early_text.bytes().all(|b| b.is_ascii_digit()) {
            ws_settings.insert("maxEarlyData".to_string(), json!(py_int_value(&Value::String(early_text)).unwrap_or(0)));
        }
        let early_header = ws_opts
            .get("early-data-header-name")
            .or_else(|| ws_opts.get("early_data_header_name"))
            .cloned();
        if let Some(header) = early_header {
            if !py_str_or(Some(&header), "").is_empty() {
                ws_settings.insert("earlyDataHeaderName".to_string(), Value::String(py_str_or(Some(&header), "")));
            }
        }
        stream.insert("wsSettings".to_string(), Value::Object(ws_settings));
    } else if network == "grpc" {
        let mut grpc_settings = Map::new();
        grpc_settings.insert(
            "serviceName".to_string(),
            Value::String({
                let candidates = [
                    "grpc-service-name",
                    "service-name",
                    "service_name",
                    "serviceName",
                ];
                let mut value = String::new();
                for key in candidates {
                    let v = py_str_or(grpc_opts.get(key), "");
                    if !v.is_empty() {
                        value = v;
                        break;
                    }
                }
                if value.is_empty() {
                    py_str_or(payload.get("service-name"), "")
                } else {
                    value
                }
            }),
        );
        let authority = grpc_opts
            .get("authority")
            .or_else(|| grpc_opts.get("grpc-authority"))
            .or_else(|| grpc_opts.get("grpc_authority"))
            .or_else(|| payload.get("grpc-authority"))
            .or_else(|| payload.get("grpc_authority"))
            .cloned();
        if let Some(authority) = authority {
            if !py_str_or(Some(&authority), "").is_empty() {
                grpc_settings.insert("authority".to_string(), Value::String(py_str_or(Some(&authority), "")));
            }
        }
        for (clash_key, target_key) in [
            ("idle-timeout", "idleTimeout"),
            ("idle_timeout", "idleTimeout"),
            ("ping-timeout", "pingTimeout"),
            ("ping_timeout", "pingTimeout"),
        ] {
            if let Some(value) = grpc_opts.get(clash_key) {
                if !matches!(value, Value::Null) && !py_str_or(Some(value), "").is_empty() {
                    grpc_settings.insert(target_key.to_string(), value.clone());
                }
            }
        }
        let permit = grpc_opts
            .get("permit-without-stream")
            .or_else(|| grpc_opts.get("permit_without_stream"))
            .cloned();
        if let Some(permit) = permit {
            if !matches!(permit, Value::Null) && !py_str_or(Some(&permit), "").is_empty() {
                grpc_settings.insert(
                    "permitWithoutStream".to_string(),
                    Value::Bool(to_bool_value(Some(&permit))),
                );
            }
        }
        if to_bool_value(grpc_opts.get("multi-mode").or_else(|| grpc_opts.get("multi_mode"))) {
            grpc_settings.insert("multiMode".to_string(), Value::Bool(true));
        }
        stream.insert("grpcSettings".to_string(), Value::Object(grpc_settings));
    } else if network == "http" || network == "h2" {
        let mut host = http_opts
            .get("host")
            .or_else(|| payload.get("host"))
            .cloned()
            .unwrap_or_else(|| json!([]));
        if host.is_string() {
            host = json!([host]);
        }
        let mut path = http_opts
            .get("path")
            .or_else(|| payload.get("path"))
            .cloned()
            .unwrap_or_else(|| Value::String("/".into()));
        if path.is_array() {
            let items = path.as_array().cloned().unwrap_or_default();
            path = if items.is_empty() {
                Value::String("/".into())
            } else {
                items[0].clone()
            };
        }
        let mut http_settings = Map::new();
        http_settings.insert("host".to_string(), host);
        http_settings.insert("path".to_string(), Value::String(py_str_or(Some(&path), "/")));
        if let Some(headers) = http_opts.get("headers").filter(|v| v.is_object()) {
            http_settings.insert("headers".to_string(), headers.clone());
        }
        if let Some(method) = http_opts.get("method") {
            if !py_str_or(Some(method), "").is_empty() {
                http_settings.insert("method".to_string(), Value::String(py_str_or(Some(method), "")));
            }
        }
        for (clash_key, target_key) in [
            ("idle-timeout", "idleTimeout"),
            ("idle_timeout", "idleTimeout"),
            ("ping-timeout", "pingTimeout"),
            ("ping_timeout", "pingTimeout"),
        ] {
            if let Some(value) = http_opts.get(clash_key) {
                if !matches!(value, Value::Null) && !py_str_or(Some(value), "").is_empty() {
                    http_settings.insert(target_key.to_string(), value.clone());
                }
            }
        }
        stream.insert("httpSettings".to_string(), Value::Object(http_settings));
    } else if network == "xhttp" {
        let mut xhttp_settings = match xhttp_opts.as_object() {
            Some(map) => map.clone(),
            None => Map::new(),
        };
        let path_entry = xhttp_settings
            .entry("path".to_string())
            .or_insert_with(|| Value::String(py_str_or(payload.get("path"), "/")));
        if path_entry.is_null() {
            *path_entry = Value::String(py_str_or(payload.get("path"), "/"));
        }
        let host_entry = xhttp_settings
            .entry("host".to_string())
            .or_insert_with(|| Value::String(py_str_or(payload.get("host"), "")));
        if host_entry.is_null() {
            *host_entry = Value::String(py_str_or(payload.get("host"), ""));
        }
        let mode_entry = xhttp_settings
            .entry("mode".to_string())
            .or_insert_with(|| Value::String(py_str_or(payload.get("mode"), "auto")));
        if mode_entry.is_null() {
            *mode_entry = Value::String(py_str_or(payload.get("mode"), "auto"));
        }
        stream.insert("xhttpSettings".to_string(), Value::Object(xhttp_settings));
    }

    if security == "tls" {
        let mut tls_settings = Map::new();
        tls_settings.insert(
            "serverName".to_string(),
            Value::String({
                let a = py_str_or(payload.get("servername"), "");
                if !a.is_empty() {
                    a
                } else {
                    let b = py_str_or(payload.get("sni"), "");
                    if !b.is_empty() {
                        b
                    } else {
                        py_str_or(payload.get("server"), "")
                    }
                }
            }),
        );
        tls_settings.insert(
            "fingerprint".to_string(),
            Value::String({
                let a = py_str_or(payload.get("client-fingerprint"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(payload.get("fingerprint"), "")
                }
            }),
        );
        tls_settings.insert(
            "allowInsecure".to_string(),
            Value::Bool(to_bool_value(payload.get("skip-cert-verify"))),
        );
        let alpn = clash_list(payload.get("alpn"));
        if !alpn.is_empty() {
            tls_settings.insert(
                "alpn".to_string(),
                Value::Array(alpn.into_iter().map(Value::String).collect()),
            );
        }
        let pins = clash_certificate_pins(payload);
        if !pins.is_empty() {
            tls_settings.insert(
                "pinSHA256".to_string(),
                Value::Array(pins.into_iter().map(Value::String).collect()),
            );
        }
        stream.insert("tlsSettings".to_string(), Value::Object(tls_settings));
    } else if security == "reality" {
        let reality_opts = payload
            .get("reality-opts")
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut reality_settings = Map::new();
        reality_settings.insert(
            "serverName".to_string(),
            Value::String({
                let a = py_str_or(payload.get("servername"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(payload.get("sni"), "")
                }
            }),
        );
        reality_settings.insert(
            "fingerprint".to_string(),
            Value::String({
                let a = py_str_or(payload.get("client-fingerprint"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(payload.get("fingerprint"), "")
                }
            }),
        );
        reality_settings.insert(
            "publicKey".to_string(),
            Value::String({
                let a = py_str_or(get_or(&reality_opts, "public-key", "publicKey"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(payload.get("pbk"), "")
                }
            }),
        );
        reality_settings.insert(
            "shortId".to_string(),
            Value::String({
                let a = py_str_or(get_or(&reality_opts, "short-id", "shortId"), "");
                if !a.is_empty() {
                    a
                } else {
                    py_str_or(payload.get("sid"), "")
                }
            }),
        );
        reality_settings.insert(
            "spiderX".to_string(),
            Value::String({
                let a = py_str_or(get_or(&reality_opts, "spider-x", "spiderX"), "");
                if !a.is_empty() {
                    a
                } else {
                    let b = py_str_or(payload.get("spx"), "");
                    if !b.is_empty() { b } else { "/".to_string() }
                }
            }),
        );
        let mldsa = reality_opts
            .get("mldsa65-verify")
            .or_else(|| reality_opts.get("mldsa65_verify"))
            .or_else(|| reality_opts.get("mldsa65Verify"))
            .or_else(|| payload.get("pqv"))
            .cloned();
        if let Some(mldsa) = mldsa {
            if !py_str_or(Some(&mldsa), "").is_empty() {
                if let Some(settings) = stream.get_mut("realitySettings").and_then(Value::as_object_mut) {
                    settings.insert(
                        "mldsa65Verify".to_string(),
                        Value::String(py_str_or(Some(&mldsa), "")),
                    );
                }
            }
        }
        stream.insert("realitySettings".to_string(), Value::Object(reality_settings));
    }
    Value::Object(stream)
}

/// Python `_clash_to_singbox_outbound`.
pub(super) fn clash_to_singbox_outbound(payload: &Value, kind: &str) -> Result<Value, LinkParseError> {
    let mut native = Map::new();
    native.insert("type".to_string(), Value::String(kind.to_string()));
    native.insert(
        "tag".to_string(),
        Value::String({
            let n = py_str_or(payload.get("name"), "");
            if n.is_empty() { kind.to_string() } else { n }
        }),
    );
    native.insert(
        "server".to_string(),
        Value::String(py_str_or(payload.get("server"), "")),
    );
    native.insert(
        "server_port".to_string(),
        json!(py_int_or(get_or(payload, "port", "server-port"), 0)?),
    );
    for (clash_key, singbox_key) in [
        ("password", "password"),
        ("auth", "auth_str"),
        ("auth-str", "auth_str"),
        ("uuid", "uuid"),
        ("congestion-controller", "congestion_control"),
        ("udp-relay-mode", "udp_relay_mode"),
        ("username", "username"),
        ("insecure-concurrency", "insecure_concurrency"),
        ("quic-congestion-control", "quic_congestion_control"),
        ("min-idle-session", "min_idle_session"),
        ("idle-session-check-interval", "idle_session_check_interval"),
        ("psk", "psk"),
        ("version", "version"),
    ] {
        if let Some(value) = payload.get(clash_key) {
            if !matches!(value, Value::Null) && !py_str_or(Some(value), "").is_empty() {
                native.insert(singbox_key.to_string(), value.clone());
            }
        }
    }
    if kind == "snell" {
        let has_psk = native
            .get("psk")
            .map(|v| !py_str_or(Some(v), "").is_empty())
            .unwrap_or(false);
        if !has_psk {
            native.insert("psk".to_string(), Value::String(py_str_or(payload.get("password"), "")));
        }
    }
    if kind == "mieru" {
        native.insert(
            "transport".to_string(),
            Value::String(py_str_or(payload.get("transport"), "TCP").to_uppercase()),
        );
        let server_ports = clash_list(get_or3(payload, "server-ports", "server_ports", "ports"));
        if !server_ports.is_empty() {
            native.insert(
                "server_ports".to_string(),
                Value::Array(server_ports.into_iter().map(Value::String).collect()),
            );
        }
        let traffic_pattern = get_or(payload, "traffic-pattern", "traffic_pattern").cloned();
        if let Some(pattern) = traffic_pattern {
            if py_truthy(&pattern) {
                native.insert("traffic_pattern".to_string(), Value::String(py_str(&pattern)));
            }
        }
        if let Some(mux) = payload.get("multiplexing") {
            if py_truthy(mux) {
                native.insert("multiplexing".to_string(), mux.clone());
            }
        }
    }
    if kind == "hysteria" || kind == "hysteria2" {
        let server_ports = clash_list(get_or3(payload, "ports", "server-ports", "server_ports"));
        if !server_ports.is_empty() {
            native.insert(
                "server_ports".to_string(),
                Value::Array(server_ports.into_iter().map(Value::String).collect()),
            );
        }
        let hop_interval = get_or(payload, "hop-interval", "hop_interval").cloned();
        if let Some(interval) = hop_interval {
            if !matches!(interval, Value::Null) && !py_str_or(Some(&interval), "").is_empty() {
                native.insert(
                    "hop_interval".to_string(),
                    Value::String(clash_duration(Some(&interval))),
                );
            }
        }
        let up_value = get_or3(payload, "up", "up-mbps", "up_mbps")
            .or_else(|| payload.get("up-speed"))
            .cloned();
        let down_value = get_or3(payload, "down", "down-mbps", "down_mbps")
            .or_else(|| payload.get("down-speed"))
            .cloned();
        if let Some(mbps) = clash_speed_mbps(up_value.as_ref()) {
            native.insert("up_mbps".to_string(), json!(mbps));
        }
        if let Some(mbps) = clash_speed_mbps(down_value.as_ref()) {
            native.insert("down_mbps".to_string(), json!(mbps));
        }
        for (clash_key, singbox_key) in [
            ("recv-window-conn", "recv_window_conn"),
            ("recv_window_conn", "recv_window_conn"),
            ("recv-window", "recv_window"),
            ("recv_window", "recv_window"),
        ] {
            if let Some(value) = payload.get(clash_key) {
                if !matches!(value, Value::Null) && !py_str_or(Some(value), "").is_empty() {
                    native.insert(singbox_key.to_string(), json!(py_int_value(value)?));
                }
            }
        }
        if to_bool_value(
            get_or(payload, "disable-mtu-discovery", "disable_mtu_discovery"),
        ) {
            native.insert("disable_mtu_discovery".to_string(), Value::Bool(true));
        }
        if kind == "hysteria" {
            let auth = get_or3(payload, "auth-str", "auth_str", "auth")
                .or_else(|| payload.get("password"))
                .cloned();
            if let Some(auth) = auth {
                if py_truthy(&auth) {
                    native.insert("auth_str".to_string(), Value::String(py_str(&auth)));
                }
            }
            let protocol = py_str_or(payload.get("protocol"), "").trim().to_string();
            if !protocol.is_empty() && protocol.to_lowercase() != "hysteria" {
                native.insert("protocol".to_string(), Value::String(protocol));
            }
            if let Some(obfs) = payload.get("obfs") {
                if py_truthy(obfs) {
                    native.insert("obfs".to_string(), Value::String(py_str(obfs)));
                }
            }
            native.shift_remove("password");
        } else {
            let has_password = native
                .get("password")
                .map(|v| !py_str_or(Some(v), "").is_empty())
                .unwrap_or(false);
            if !has_password {
                if let Some(auth) = payload.get("auth") {
                    if py_truthy(auth) {
                        native.insert("password".to_string(), Value::String(py_str(auth)));
                    }
                }
            }
            native.shift_remove("auth_str");
            let obfs_type = py_str_or(payload.get("obfs"), "").trim().to_string();
            let obfs_password = py_str_or(get_or(payload, "obfs-password", "obfs_password"), "");
            if !obfs_type.is_empty() {
                native.insert(
                    "obfs".to_string(),
                    json!({ "type": obfs_type, "password": obfs_password }),
                );
            }
        }
    }
    let server_name = py_str_or(get_or(payload, "servername", "sni"), "")
        .trim()
        .to_string();
    if to_bool_value(payload.get("tls"))
        || !server_name.is_empty()
        || kind == "naive"
        || kind == "anytls"
    {
        let mut tls = Map::new();
        tls.insert("enabled".to_string(), Value::Bool(true));
        if !server_name.is_empty() {
            tls.insert("server_name".to_string(), Value::String(server_name.clone()));
        }
        // The Cronet-backed Naive outbound intentionally supports only a
        // restricted TLS subset.  Passing insecure/ALPN makes sing-box reject
        // the whole config instead of merely ignoring the unsupported option.
        if to_bool_value(payload.get("skip-cert-verify")) && kind != "naive" {
            tls.insert("insecure".to_string(), Value::Bool(true));
        }
        let alpn = clash_list(payload.get("alpn"));
        if !alpn.is_empty() && kind != "naive" {
            tls.insert(
                "alpn".to_string(),
                Value::Array(alpn.into_iter().map(Value::String).collect()),
            );
        }
        let pins = clash_certificate_pins(payload);
        if !pins.is_empty() && kind != "naive" {
            tls.insert(
                "certificate_public_key_sha256".to_string(),
                Value::Array(pins.into_iter().map(Value::String).collect()),
            );
        }
        native.insert("tls".to_string(), Value::Object(tls));
    }
    if kind == "naive" {
        native.insert(
            "quic".to_string(),
            Value::Bool(to_bool_value(payload.get("quic"))),
        );
        let udp_over_tcp = get_or(payload, "udp-over-tcp", "udp_over_tcp").cloned();
        if let Some(uot) = udp_over_tcp {
            if uot.is_boolean() || uot.is_object() {
                native.insert("udp_over_tcp".to_string(), uot);
            }
        }
        let extra_headers = get_or(payload, "extra-headers", "extra_headers").cloned();
        if let Some(headers) = extra_headers {
            if headers.is_object() {
                native.insert("extra_headers".to_string(), headers);
            }
        }
    }
    Ok(Value::Object(native))
}
