//! Per-scheme link parsers (`_parse_vless`, `_parse_vmess`, ... from
//! link_parser.py) plus the SIP002/SIP003 Shadowsocks helpers and the
//! Happ server-description metadata.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::helpers::{
    clean_name, decode_b64, first_of, get_param, parse_query, py_int_or, py_int_value, py_parse_int,
    py_str_or, py_str, to_bool_str, unquote, PyUrlSplit,
};
use super::json_import::native_singbox_outbound;
use super::stream::build_stream_settings;
use super::{LinkParseError, Node};

/// Build the `params` map used by the URI parsers: `{k: _first(query, k)}`.
pub(super) fn query_params(parsed: &PyUrlSplit) -> BTreeMap<String, String> {
    let query = parse_query(&parsed.query);
    let mut params: BTreeMap<String, String> = BTreeMap::new();
    for (key, _) in &query {
        params.insert(key.clone(), first_of(&query, key));
    }
    params
}

pub(super) fn make_node(name: String, scheme: &str, server: String, port: i64, link: &str, outbound: Value) -> Node {
    let mut node = Node::new();
    node.name = name;
    node.scheme = scheme.to_string();
    node.server = server;
    node.port = port;
    node.link = link.to_string();
    node.outbound = outbound;
    node
}

// ── _parse_vless ──

pub(super) fn parse_vless(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);

    let user_id = unquote(parsed.username().unwrap_or(""));
    let server = parsed.hostname();
    let port = parsed.port()?.unwrap_or(443);
    let port = if port == 0 { 443 } else { port };

    if user_id.is_empty() || server.is_empty() {
        return Err(LinkParseError::new("invalid vless link"));
    }

    let mut user = Map::new();
    user.insert("id".to_string(), Value::String(user_id));
    user.insert(
        "encryption".to_string(),
        Value::String({
            let encryption = get_param(&params, &["encryption"], "");
            if encryption.is_empty() {
                "none".to_string()
            } else {
                encryption
            }
        }),
    );
    let flow = get_param(&params, &["flow"], "");
    if !flow.is_empty() {
        user.insert("flow".to_string(), Value::String(flow));
    }

    let default_security = params.get("security").cloned().unwrap_or_else(|| "none".to_string());
    let stream_settings = build_stream_settings(&params, "tcp", &default_security);
    let mut outbound = Map::new();
    outbound.insert("protocol".to_string(), Value::String("vless".into()));
    outbound.insert(
        "settings".to_string(),
        json!({ "vnext": [{ "address": server, "port": port, "users": [Value::Object(user)] }] }),
    );
    outbound.insert("streamSettings".to_string(), stream_settings.clone());
    if stream_settings
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
        == "xhttp"
    {
        outbound.insert("mux".to_string(), json!({ "enabled": false, "concurrency": -1 }));
    }

    let name = clean_name(Some(&parsed.fragment), format!("vless-{server}:{port}"));
    Ok(make_node(
        name,
        "vless",
        server,
        port,
        link,
        Value::Object(outbound),
    ))
}

// ── _parse_vmess ──

/// Python `_normalize_vmess_security_value` — provider null sentinels → "auto".
fn normalize_vmess_security_value(value: Option<&Value>) -> String {
    let security = py_str_or(value, "").trim().to_lowercase();
    if security.is_empty() || security == "null" || security == "undefined" {
        "auto".to_string()
    } else {
        security
    }
}

pub(super) fn normalize_vmess_security_value_pub(value: Option<&Value>) -> String {
    normalize_vmess_security_value(value)
}

pub(super) fn parse_vmess(link: &str) -> Result<Node, LinkParseError> {
    let encoded = &link["vmess://".len()..];
    let decoded = decode_b64(encoded)?;
    let payload: Value = serde_json::from_str(&decoded)
        .map_err(|e| LinkParseError::new(e.to_string()))?;
    let payload = payload.as_object().ok_or_else(|| LinkParseError::new("invalid vmess link"))?;

    let server = py_str_or(payload.get("add"), "");
    let port = py_int_or(payload.get("port"), 443)?;
    let user_id = py_str_or(payload.get("id"), "");
    if server.is_empty() || user_id.is_empty() {
        return Err(LinkParseError::new("invalid vmess link"));
    }

    let security = py_str_or(payload.get("tls"), "none").to_lowercase();
    let net = py_str_or(payload.get("net"), "tcp");
    let mut params: BTreeMap<String, String> = BTreeMap::new();
    params.insert("net".to_string(), net.clone());
    params.insert("type".to_string(), net.clone());
    params.insert(
        "security".to_string(),
        if security == "tls" || security == "reality" { "tls".into() } else { "none".into() },
    );
    params.insert("host".to_string(), py_str_or(payload.get("host"), ""));
    params.insert("path".to_string(), py_str_or(payload.get("path"), ""));
    let sni = {
        let raw_sni = py_str_or(payload.get("sni"), "");
        if raw_sni.is_empty() {
            py_str_or(payload.get("host"), "")
        } else {
            raw_sni
        }
    };
    params.insert("sni".to_string(), sni);
    params.insert("alpn".to_string(), py_str_or(payload.get("alpn"), ""));
    params.insert("fp".to_string(), py_str_or(payload.get("fp"), ""));
    params.insert(
        "serviceName".to_string(),
        py_str_or(payload.get("serviceName"), ""),
    );

    let users = json!({
        "id": user_id,
        "alterId": py_int_or(payload.get("aid"), 0)?,
        "security": normalize_vmess_security_value(payload.get("scy")),
    });
    let outbound = json!({
        "protocol": "vmess",
        "settings": { "vnext": [{ "address": server, "port": port, "users": [users] }] },
        "streamSettings": build_stream_settings(&params, &net, params.get("security").map(String::as_str).unwrap_or("none")),
    });

    let ps = py_str_or(payload.get("ps"), "");
    let name = clean_name(Some(&ps), format!("vmess-{server}:{port}"));
    Ok(make_node(name, "vmess", server, port, link, outbound))
}

// ── _parse_trojan ──

pub(super) fn parse_trojan(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);

    let password = unquote(parsed.username().unwrap_or(""));
    let server = parsed.hostname();
    let port = parsed.port()?.unwrap_or(443);
    let port = if port == 0 { 443 } else { port };
    if password.is_empty() || server.is_empty() {
        return Err(LinkParseError::new("invalid trojan link"));
    }

    let default_security = params.get("security").cloned().unwrap_or_else(|| "tls".to_string());
    let outbound = json!({
        "protocol": "trojan",
        "settings": { "servers": [{ "address": server, "port": port, "password": password }] },
        "streamSettings": build_stream_settings(&params, "tcp", &default_security),
    });

    let name = clean_name(Some(&parsed.fragment), format!("trojan-{server}:{port}"));
    Ok(make_node(name, "trojan", server, port, link, outbound))
}

// ── Shadowsocks helpers ──

/// Python `_decode_shadowsocks_b64` — strict SIP002 credential decoding.
fn decode_shadowsocks_b64(data: &str) -> Result<String, LinkParseError> {
    let cleaned = super::helpers::remove_all_whitespace(&unquote(data));
    if cleaned.is_empty() {
        return Err(LinkParseError::new("invalid shadowsocks base64 credentials"));
    }
    let mut padded = cleaned;
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let raw = super::helpers::decode_b64_strict(&padded)
        .map_err(|_| LinkParseError::new("invalid shadowsocks base64 credentials"))?;
    let decoded = String::from_utf8(raw)
        .map_err(|_| LinkParseError::new("invalid shadowsocks base64 credentials"))?;
    if decoded.chars().any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F) {
        return Err(LinkParseError::new("invalid shadowsocks credentials"));
    }
    Ok(decoded)
}

/// Python `_split_shadowsocks_host_port` — SIP002 authority parser.
fn split_shadowsocks_host_port(authority: &str) -> Result<(String, i64), LinkParseError> {
    let mut value = authority.trim().to_string();
    if value.ends_with('/') {
        value.pop();
    }
    let host;
    let mut port_text = String::new();
    if value.starts_with('[') {
        let close = match value.find(']') {
            Some(close) => close,
            None => return Err(LinkParseError::new("invalid shadowsocks host")),
        };
        if close <= 1 {
            return Err(LinkParseError::new("invalid shadowsocks host"));
        }
        host = value[1..close].to_string();
        let suffix = &value[close + 1..];
        if !suffix.is_empty() {
            if !suffix.starts_with(':') {
                return Err(LinkParseError::new("invalid shadowsocks host or port"));
            }
            port_text = suffix[1..].to_string();
        }
    } else {
        if value.matches(':').count() > 1 {
            return Err(LinkParseError::new(
                "IPv6 shadowsocks hosts must be enclosed in brackets",
            ));
        }
        match value.rsplit_once(':') {
            Some((h, p)) => {
                host = h.to_string();
                port_text = p.to_string();
            }
            None => {
                host = value.clone();
            }
        }
    }
    let host = unquote(&host).trim().to_lowercase();
    if host.is_empty() {
        return Err(LinkParseError::new("invalid shadowsocks host"));
    }
    if port_text.is_empty() {
        return Ok((host, 8388));
    }
    let valid = !port_text.is_empty() && port_text.bytes().all(|b| b.is_ascii_digit());
    if !valid {
        return Err(LinkParseError::new(format!(
            "invalid shadowsocks port `{port_text}`"
        )));
    }
    let port: i64 = port_text.parse().map_err(|_| {
        LinkParseError::new(format!("invalid shadowsocks port `{port_text}`"))
    })?;
    if port <= 0 || port > 65535 {
        return Err(LinkParseError::new(format!(
            "invalid shadowsocks port `{port_text}`"
        )));
    }
    Ok((host, port))
}

/// Python `_shadowsocks_query` — SIP002 query parsing preserving literal `+`.
fn shadowsocks_query(query: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for item in query.split('&') {
        if item.is_empty() {
            continue;
        }
        let (key_raw, value_raw, has_sep) = match item.split_once('=') {
            Some((k, v)) => (k, v, true),
            None => (item, "", false),
        };
        let decoded_key = unquote(key_raw).trim().to_lowercase();
        if decoded_key.is_empty() {
            continue;
        }
        result.insert(
            decoded_key,
            if has_sep { unquote(value_raw) } else { String::new() },
        );
    }
    result
}

fn normalize_shadowsocks_plugin_name(value: Option<&Value>) -> String {
    let plugin = py_str_or(value, "").trim().to_lowercase();
    match plugin.as_str() {
        "obfs" | "simple-obfs" => "obfs-local".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn normalize_shadowsocks_plugin_name_pub(value: Option<&Value>) -> String {
    normalize_shadowsocks_plugin_name(value)
}

/// Python `_split_sip003_options` — split on unescaped `;`.
fn split_sip003_options(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for c in value.chars() {
        if c == ';' && !escaped {
            if !current.is_empty() {
                parts.push(current.clone());
            }
            current.clear();
            continue;
        }
        current.push(c);
        if c == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Python `_split_sip003_option` — split at the first unescaped `=`.
fn split_sip003_option(value: &str) -> (String, bool, String) {
    let mut escaped = false;
    for (index, c) in value.char_indices() {
        if c == '=' && !escaped {
            return (value[..index].to_string(), true, value[index + 1..].to_string());
        }
        if c == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    (value.to_string(), false, String::new())
}

fn split_unescaped(source: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for c in source.chars() {
        if c == delimiter && !escaped {
            parts.push(current.clone());
            current.clear();
            continue;
        }
        current.push(c);
        if c == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    parts.push(current);
    parts
}

/// Python `_normalize_obfs_plugin_options` — Clash mode/host → obfs/obfs-host.
pub(super) fn normalize_obfs_plugin_options(value: &str) -> String {
    let text = value.trim();
    if text.is_empty() {
        return String::new();
    }
    let mut normalized: Vec<String> = Vec::new();
    for item in split_unescaped(text, ';') {
        if item.is_empty() {
            continue;
        }
        let key_and_value = split_unescaped(&item, '=');
        let key = key_and_value[0].clone();
        let separator = if key_and_value.len() > 1 { "=" } else { "" };
        let option_value = key_and_value[1..].join("=");
        let mapped = match key.trim().to_lowercase().as_str() {
            "mode" => "obfs".to_string(),
            "host" => "obfs-host".to_string(),
            _ => key.clone(),
        };
        if separator.is_empty() {
            normalized.push(mapped);
        } else {
            normalized.push(format!("{mapped}={option_value}"));
        }
    }
    normalized.join(";")
}

/// Python `_normalize_v2ray_plugin_options` — integer mux values.
pub(super) fn normalize_v2ray_plugin_options(value: &str) -> String {
    let text = value.trim();
    if text.is_empty() {
        return String::new();
    }
    let mut normalized: Vec<String> = Vec::new();
    for item in split_sip003_options(text) {
        let (key, separator, option_value) = split_sip003_option(&item);
        if !key.trim().to_lowercase().eq_ignore_ascii_case("mux") {
            normalized.push(item);
            continue;
        }
        let lowered = option_value.trim().to_lowercase();
        if !separator || lowered.is_empty() || lowered == "true" || lowered == "yes" || lowered == "on" {
            normalized.push(format!("{key}=1"));
        } else if lowered == "false" || lowered == "no" || lowered == "off" {
            normalized.push(format!("{key}=0"));
        } else {
            match py_parse_int(&option_value) {
                Ok(number) => normalized.push(format!("{key}={number}")),
                Err(_) => continue,
            }
        }
    }
    normalized.join(";")
}

/// Python `_sip003_options_from_mapping` — mapping → escaped SIP003 string.
pub(super) fn sip003_options_from_mapping(value: &Value, obfs: bool) -> String {
    fn escape(component: &Value) -> String {
        py_str(component)
            .replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace('=', "\\=")
    }
    let mut options: Vec<String> = Vec::new();
    if let Some(map) = value.as_object() {
        for (raw_key, raw_value) in map {
            let mut key = raw_key.clone();
            if obfs {
                key = match key.trim().to_lowercase().as_str() {
                    "mode" => "obfs".to_string(),
                    "host" => "obfs-host".to_string(),
                    _ => key,
                };
            }
            if matches!(raw_value, Value::Bool(true)) {
                options.push(escape(&Value::String(key)));
            } else {
                options.push(format!("{}={}", escape(&Value::String(key)), escape(raw_value)));
            }
        }
    }
    options.join(";")
}

// ── _parse_shadowsocks ──

pub(super) fn parse_shadowsocks(link: &str) -> Result<Node, LinkParseError> {
    let body = link.split_once("://").map(|(_, rest)| rest).unwrap_or(link);
    let body = body.trim();
    let (body_without_fragment, has_fragment, fragment) = match body.split_once('#') {
        Some((before, after)) => (before, true, after),
        None => (body, false, ""),
    };
    let (main_part, query_text) = match body_without_fragment.split_once('?') {
        Some((before, after)) => (before, after),
        None => (body_without_fragment, ""),
    };
    let query = shadowsocks_query(query_text);

    let method: String;
    let password: String;
    let server: String;
    let port: i64;
    if main_part.contains('@') {
        let (credentials, host_port) = main_part.rsplit_once('@').unwrap();
        let (s, p) = split_shadowsocks_host_port(host_port)?;
        server = s;
        port = p;
        if credentials.contains(':') {
            // SIP022 requires this plaintext, percent-encoded form for 2022
            // ciphers; accepting it for older methods is part of SIP002.
            let (method_text, password_text) = credentials.split_once(':').unwrap();
            method = unquote(method_text);
            password = unquote(password_text);
        } else {
            let decoded = decode_shadowsocks_b64(credentials)?;
            let (m, p) = decoded
                .split_once(':')
                .ok_or_else(|| LinkParseError::new("invalid shadowsocks credentials"))?;
            method = m.to_string();
            password = p.to_string();
        }
    } else {
        // Legacy URI: Base64(method:password@host:port).  '/' is part of
        // standard Base64 and must not be treated as a URL path separator.
        let decoded = decode_shadowsocks_b64(main_part)?;
        let (credentials, host_port) = decoded
            .rsplit_once('@')
            .ok_or_else(|| LinkParseError::new("invalid shadowsocks link"))?;
        if !credentials.contains(':') {
            return Err(LinkParseError::new("invalid shadowsocks link"));
        }
        let (s, p) = split_shadowsocks_host_port(host_port)?;
        server = s;
        port = p;
        let (m, p) = credentials.split_once(':').unwrap();
        method = m.to_string();
        password = p.to_string();
    }

    let method = method.trim().to_string();
    if method.is_empty()
        || server.is_empty()
        || (password.is_empty() && !method.eq_ignore_ascii_case("none"))
    {
        return Err(LinkParseError::new("invalid shadowsocks link"));
    }

    let plugin = query.get("plugin").cloned().unwrap_or_default().trim().to_string();
    let mut outbound_server = Map::new();
    outbound_server.insert("address".to_string(), Value::String(server.clone()));
    outbound_server.insert("port".to_string(), json!(port));
    outbound_server.insert("method".to_string(), Value::String(method.clone()));
    outbound_server.insert("password".to_string(), Value::String(password.clone()));
    if !plugin.is_empty() {
        let (plugin_name_raw, _, plugin_opts_from_uri) = match plugin.split_once(';') {
            Some((name, opts)) => (name.to_string(), true, opts.to_string()),
            None => (plugin.clone(), false, String::new()),
        };
        let plugin_name = normalize_shadowsocks_plugin_name(Some(&Value::String(plugin_name_raw)));
        outbound_server.insert("plugin".to_string(), Value::String(plugin_name.clone()));
        let mut plugin_opts = {
            let a = query.get("plugin_opts").cloned().unwrap_or_default();
            if !a.is_empty() {
                a
            } else {
                let b = query.get("plugin-opts").cloned().unwrap_or_default();
                if !b.is_empty() {
                    b
                } else {
                    plugin_opts_from_uri
                }
            }
        };
        if plugin_name == "obfs-local" {
            plugin_opts = normalize_obfs_plugin_options(&plugin_opts);
        } else if plugin_name == "v2ray-plugin" {
            plugin_opts = normalize_v2ray_plugin_options(&plugin_opts);
        }
        if !plugin_opts.is_empty() {
            outbound_server.insert("plugin_opts".to_string(), Value::String(plugin_opts));
        }
    }

    let outbound = json!({
        "protocol": "shadowsocks",
        "settings": { "servers": [Value::Object(outbound_server)] },
    });

    let fragment_name = if has_fragment { Some(fragment) } else { None };
    let name = clean_name(fragment_name, format!("ss-{server}:{port}"));
    Ok(make_node(name, "ss", server, port, link, outbound))
}

// ── _parse_socks / _parse_http_proxy ──

pub(super) fn parse_socks(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let server = parsed.hostname();
    let port = parsed.port()?.unwrap_or(1080);
    let port = if port == 0 { 1080 } else { port };
    if server.is_empty() {
        return Err(LinkParseError::new("invalid socks link"));
    }

    let user = unquote(parsed.username().unwrap_or(""));
    let password = unquote(parsed.password().unwrap_or(""));

    let mut server_item = Map::new();
    server_item.insert("address".to_string(), Value::String(server.clone()));
    server_item.insert("port".to_string(), json!(port));
    if !user.is_empty() {
        server_item.insert("users".to_string(), json!([{ "user": user, "pass": password }]));
    }

    let outbound = json!({ "protocol": "socks", "settings": { "servers": [Value::Object(server_item)] } });
    let name = clean_name(Some(&parsed.fragment), format!("socks-{server}:{port}"));
    Ok(make_node(name, "socks", server, port, link, outbound))
}

pub(super) fn parse_http_proxy(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let server = parsed.hostname();
    let default_port = if parsed.scheme == "https" { 443 } else { 80 };
    let port = parsed.port()?.unwrap_or(default_port);
    let port = if port == 0 { default_port } else { port };
    if server.is_empty() {
        return Err(LinkParseError::new("invalid http proxy link"));
    }

    let user = unquote(parsed.username().unwrap_or(""));
    let password = unquote(parsed.password().unwrap_or(""));

    let mut server_item = Map::new();
    server_item.insert("address".to_string(), Value::String(server.clone()));
    server_item.insert("port".to_string(), json!(port));
    if !user.is_empty() {
        server_item.insert("users".to_string(), json!([{ "user": user, "pass": password }]));
    }

    let mut outbound = Map::new();
    outbound.insert("protocol".to_string(), Value::String("http".into()));
    outbound.insert("settings".to_string(), json!({ "servers": [Value::Object(server_item)] }));
    if parsed.scheme == "https" {
        outbound.insert(
            "streamSettings".to_string(),
            json!({
                "network": "tcp",
                "security": "tls",
                "tlsSettings": { "serverName": server },
            }),
        );
    }

    let name = clean_name(Some(&parsed.fragment), format!("http-{server}:{port}"));
    Ok(make_node(
        name,
        "http",
        server,
        port,
        link,
        Value::Object(outbound),
    ))
}

// ── NaiveProxy ──

pub(super) fn parse_naive_link(link: &str, source_scheme: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let scheme = if source_scheme.is_empty() {
        parsed.scheme.clone()
    } else {
        source_scheme.trim().to_lowercase()
    };
    let is_quic = scheme == "quic" || scheme == "naive+quic";
    if !matches!(scheme.as_str(), "naive" | "naive+https" | "naive+quic" | "https" | "quic") {
        return Err(LinkParseError::new(format!(
            "unsupported NaiveProxy transport: {}",
            if scheme.is_empty() { "unknown" } else { &scheme }
        )));
    }

    let server = parsed.hostname().trim().to_string();
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => 443,
    };
    if server.is_empty() || port <= 0 {
        return Err(LinkParseError::new("NaiveProxy URI must contain server and port"));
    }

    let query = parse_query(&parsed.query);
    let username = unquote(parsed.username().unwrap_or(""));
    let password = unquote(parsed.password().unwrap_or(""));
    let mut server_name = first_of(&query, "sni");
    if server_name.is_empty() {
        server_name = first_of(&query, "server_name");
    }
    if server_name.is_empty() {
        server_name = server.clone();
    }
    let mut native = Map::new();
    native.insert("type".to_string(), Value::String("naive".into()));
    native.insert("tag".to_string(), Value::String("proxy".into()));
    native.insert("server".to_string(), Value::String(server.clone()));
    native.insert("server_port".to_string(), json!(port));
    native.insert("username".to_string(), Value::String(username));
    native.insert("password".to_string(), Value::String(password));
    native.insert(
        "quic".to_string(),
        Value::Bool(is_quic || to_bool_str(&first_of(&query, "quic"))),
    );
    native.insert(
        "tls".to_string(),
        json!({ "enabled": true, "server_name": server_name }),
    );
    let concurrency = {
        let a = first_of(&query, "insecure_concurrency");
        if !a.is_empty() {
            a
        } else {
            first_of(&query, "insecure-concurrency")
        }
    };
    if !concurrency.is_empty() {
        let value = py_parse_int(&concurrency).map_err(|_| {
            LinkParseError::new(format!(
                "invalid literal for int() with base 10: {concurrency:?}"
            ))
        })?;
        native.insert("insecure_concurrency".to_string(), json!(value));
    }
    let congestion = {
        let a = first_of(&query, "quic_congestion_control");
        if !a.is_empty() {
            a
        } else {
            first_of(&query, "quic-congestion-control")
        }
    };
    if !congestion.is_empty() {
        native.insert("quic_congestion_control".to_string(), Value::String(congestion));
    }
    let udp_over_tcp = {
        let a = first_of(&query, "udp_over_tcp");
        if !a.is_empty() {
            a
        } else {
            first_of(&query, "udp-over-tcp")
        }
    };
    if !udp_over_tcp.is_empty() {
        let decoded_uot = match serde_json::from_str::<Value>(&udp_over_tcp) {
            Ok(v) => v,
            Err(_) => Value::Bool(to_bool_str(&udp_over_tcp)),
        };
        if decoded_uot.is_boolean() || decoded_uot.is_object() {
            native.insert("udp_over_tcp".to_string(), decoded_uot);
        }
    }
    let extra_headers = {
        let a = first_of(&query, "extra_headers");
        if !a.is_empty() {
            a
        } else {
            first_of(&query, "extra-headers")
        }
    };
    if !extra_headers.is_empty() {
        let decoded_headers: Value = serde_json::from_str(&extra_headers).map_err(|_| {
            LinkParseError::new("NaiveProxy extra_headers must be a JSON object")
        })?;
        if !decoded_headers.is_object() {
            return Err(LinkParseError::new(
                "NaiveProxy extra_headers must be a JSON object",
            ));
        }
        native.insert("extra_headers".to_string(), decoded_headers);
    }

    let name = clean_name(Some(&parsed.fragment), format!("naive-{server}:{port}"));
    let outbound = native_singbox_outbound(&Value::Object(native))?;
    Ok(make_node(name, "naive", server, port, link, outbound))
}

// ── Hysteria / Hysteria2 / TUIC ──

fn apply_hysteria_tls(outbound: &mut Map<String, Value>, params: &BTreeMap<String, String>, server: &str) {
    let mut tls = Map::new();
    tls.insert("enabled".to_string(), Value::Bool(true));
    let sni = get_param(params, &["sni", "peer", "server_name", "serverName"], "");
    if !sni.is_empty() {
        tls.insert("server_name".to_string(), Value::String(sni));
    } else if !server.is_empty() {
        tls.insert("server_name".to_string(), Value::String(server.to_string()));
    }
    let insecure = get_param(params, &["insecure", "allowInsecure"], "");
    if !insecure.is_empty() {
        tls.insert("insecure".to_string(), Value::Bool(to_bool_str(&insecure)));
    }
    let alpn = get_param(params, &["alpn"], "");
    if !alpn.is_empty() {
        let items: Vec<Value> = alpn
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect();
        tls.insert("alpn".to_string(), Value::Array(items));
    }
    outbound.insert("tls".to_string(), Value::Object(tls));
}

fn apply_hysteria_obfs(outbound: &mut Map<String, Value>, params: &BTreeMap<String, String>) {
    let obfs_type = get_param(params, &["obfs", "obfs_type", "obfsType"], "");
    let obfs_password = get_param(
        params,
        &["obfs-password", "obfs_password", "obfsPassword", "obfs-pass", "obfsPass"],
        "",
    );
    if obfs_type.is_empty() && obfs_password.is_empty() {
        return;
    }
    if obfs_type == "none" {
        return;
    }
    let outbound_type = outbound.get("type").and_then(Value::as_str).unwrap_or("");
    if outbound_type == "hysteria" {
        let value = if !obfs_password.is_empty() { obfs_password } else { obfs_type };
        outbound.insert("obfs".to_string(), Value::String(value));
        return;
    }
    let mut obfs = Map::new();
    obfs.insert(
        "type".to_string(),
        Value::String(if obfs_type.is_empty() { "salamander".into() } else { obfs_type }),
    );
    if !obfs_password.is_empty() {
        obfs.insert("password".to_string(), Value::String(obfs_password));
    }
    outbound.insert("obfs".to_string(), Value::Object(obfs));
}

/// int() of a query string used as a numeric field (raises like Python int()).
fn param_int(params: &BTreeMap<String, String>, keys: &[&str], default: &str) -> Result<i64, LinkParseError> {
    let value = get_param(params, keys, default);
    py_int_value(&Value::String(value))
}

pub(super) fn parse_hysteria(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);
    let mut server = parsed.hostname();
    if server.is_empty() {
        server = get_param(&params, &["server", "address", "host"], "");
    }
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => {
            let q = param_int(&params, &["port"], "0")?;
            if q != 0 {
                q
            } else {
                443
            }
        }
    };
    let mut auth = unquote(parsed.username().unwrap_or(""));
    if auth.is_empty() {
        auth = get_param(&params, &["auth", "auth_str", "authStr", "password"], "");
    }
    if server.is_empty() || auth.is_empty() {
        return Err(LinkParseError::new(
            "hysteria link must contain server and auth/password",
        ));
    }

    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String("hysteria".into()));
    outbound.insert("tag".to_string(), Value::String("proxy".into()));
    outbound.insert("server".to_string(), Value::String(server.clone()));
    outbound.insert("server_port".to_string(), json!(port));
    if !auth.is_empty() {
        outbound.insert("auth_str".to_string(), Value::String(auth));
    }
    let protocol = get_param(&params, &["protocol"], "");
    if !protocol.is_empty() {
        outbound.insert("protocol".to_string(), Value::String(protocol));
    }
    let up_mbps = get_param(&params, &["upmbps", "up_mbps", "up"], "");
    if !up_mbps.is_empty() {
        outbound.insert("up_mbps".to_string(), json!(param_int(&params, &["upmbps", "up_mbps", "up"], "")?));
    }
    let down_mbps = get_param(&params, &["downmbps", "down_mbps", "down"], "");
    if !down_mbps.is_empty() {
        outbound.insert(
            "down_mbps".to_string(),
            json!(param_int(&params, &["downmbps", "down_mbps", "down"], "")?),
        );
    }
    let server_ports = get_param(&params, &["server_ports", "server-ports", "ports"], "");
    if !server_ports.is_empty() {
        let items: Vec<Value> = server_ports
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect();
        outbound.insert("server_ports".to_string(), Value::Array(items));
    }
    let hop_interval = get_param(&params, &["hop_interval", "hop-interval"], "");
    if !hop_interval.is_empty() {
        outbound.insert("hop_interval".to_string(), Value::String(hop_interval));
    }
    for key in ["recv_window_conn", "recv-window-conn", "recv_window", "recv-window"] {
        let value = get_param(&params, &[key], "");
        if !value.is_empty() {
            outbound.insert(
                key.replace('-', "_"),
                json!(py_int_value(&Value::String(value))?),
            );
        }
    }
    if to_bool_str(&get_param(&params, &["disable_mtu_discovery", "disable-mtu-discovery"], "")) {
        outbound.insert("disable_mtu_discovery".to_string(), Value::Bool(true));
    }
    apply_hysteria_tls(&mut outbound, &params, &server);
    apply_hysteria_obfs(&mut outbound, &params);
    let name = clean_name(Some(&parsed.fragment), format!("hysteria-{server}:{port}"));
    let native = native_singbox_outbound(&Value::Object(outbound))?;
    Ok(make_node(name, "hysteria", server, port, link, native))
}

pub(super) fn parse_hysteria2(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);
    let mut server = parsed.hostname();
    if server.is_empty() {
        server = get_param(&params, &["server", "address", "host"], "");
    }
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => {
            let q = param_int(&params, &["port"], "0")?;
            if q != 0 {
                q
            } else {
                443
            }
        }
    };
    let mut password = unquote(parsed.username().unwrap_or(""));
    if password.is_empty() {
        password = get_param(&params, &["password", "auth", "auth_str", "authStr"], "");
    }
    if server.is_empty() || password.is_empty() {
        return Err(LinkParseError::new("hysteria2 link must contain server and password"));
    }

    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String("hysteria2".into()));
    outbound.insert("tag".to_string(), Value::String("proxy".into()));
    outbound.insert("server".to_string(), Value::String(server.clone()));
    outbound.insert("server_port".to_string(), json!(port));
    outbound.insert("password".to_string(), Value::String(password));
    let server_ports = get_param(&params, &["server_ports", "server-ports", "ports"], "");
    if !server_ports.is_empty() {
        let items: Vec<Value> = server_ports
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect();
        outbound.insert("server_ports".to_string(), Value::Array(items));
    }
    let hop_interval = get_param(&params, &["hop_interval", "hop-interval"], "");
    if !hop_interval.is_empty() {
        outbound.insert("hop_interval".to_string(), Value::String(hop_interval));
    }
    let up_mbps = get_param(&params, &["upmbps", "up_mbps", "up"], "");
    if !up_mbps.is_empty() {
        outbound.insert("up_mbps".to_string(), json!(param_int(&params, &["upmbps", "up_mbps", "up"], "")?));
    }
    let down_mbps = get_param(&params, &["downmbps", "down_mbps", "down"], "");
    if !down_mbps.is_empty() {
        outbound.insert(
            "down_mbps".to_string(),
            json!(param_int(&params, &["downmbps", "down_mbps", "down"], "")?),
        );
    }
    for key in ["recv_window_conn", "recv-window-conn", "recv_window", "recv-window"] {
        let value = get_param(&params, &[key], "");
        if !value.is_empty() {
            outbound.insert(
                key.replace('-', "_"),
                json!(py_int_value(&Value::String(value))?),
            );
        }
    }
    if to_bool_str(&get_param(&params, &["disable_mtu_discovery", "disable-mtu-discovery"], "")) {
        outbound.insert("disable_mtu_discovery".to_string(), Value::Bool(true));
    }
    apply_hysteria_tls(&mut outbound, &params, &server);
    apply_hysteria_obfs(&mut outbound, &params);
    let name = clean_name(Some(&parsed.fragment), format!("hysteria2-{server}:{port}"));
    let native = native_singbox_outbound(&Value::Object(outbound))?;
    Ok(make_node(name, "hysteria2", server, port, link, native))
}

pub(super) fn parse_tuic(link: &str) -> Result<Node, LinkParseError> {
    // tuic://uuid:password@host:port?congestion_control=&udp_relay_mode=&sni=&alpn=#name
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);
    let mut server = parsed.hostname();
    if server.is_empty() {
        server = get_param(&params, &["server", "address", "host"], "");
    }
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => {
            let q = param_int(&params, &["port"], "0")?;
            if q != 0 {
                q
            } else {
                443
            }
        }
    };
    let mut uuid = unquote(parsed.username().unwrap_or(""));
    if uuid.is_empty() {
        uuid = get_param(&params, &["uuid", "id"], "");
    }
    let mut password = unquote(parsed.password().unwrap_or(""));
    if password.is_empty() {
        password = get_param(&params, &["password", "passwd", "pass"], "");
    }
    if server.is_empty() || uuid.is_empty() {
        return Err(LinkParseError::new("tuic link must contain server and uuid"));
    }
    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String("tuic".into()));
    outbound.insert("tag".to_string(), Value::String("proxy".into()));
    outbound.insert("server".to_string(), Value::String(server.clone()));
    outbound.insert("server_port".to_string(), json!(port));
    outbound.insert("uuid".to_string(), Value::String(uuid));
    if !password.is_empty() {
        outbound.insert("password".to_string(), Value::String(password));
    }
    let congestion = get_param(&params, &["congestion_control", "congestion", "cc"], "");
    if !congestion.is_empty() {
        outbound.insert("congestion_control".to_string(), Value::String(congestion));
    }
    let udp_relay = get_param(&params, &["udp_relay_mode", "udp_relay", "udpRelayMode"], "");
    if !udp_relay.is_empty() {
        outbound.insert("udp_relay_mode".to_string(), Value::String(udp_relay));
    }
    if to_bool_str(&get_param(&params, &["zero_rtt_handshake", "zero_rtt", "reduce_rtt"], "")) {
        outbound.insert("zero_rtt_handshake".to_string(), Value::Bool(true));
    }
    apply_hysteria_tls(&mut outbound, &params, &server);
    let name = clean_name(Some(&parsed.fragment), format!("tuic-{server}:{port}"));
    let native = native_singbox_outbound(&Value::Object(outbound))?;
    Ok(make_node(name, "tuic", server, port, link, native))
}

// ── Mieru ──

pub(super) fn parse_mieru(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);
    let mut server = parsed.hostname();
    if server.is_empty() {
        server = get_param(&params, &["server", "address", "host"], "");
    }
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => param_int(&params, &["port", "server_port"], "0")?,
    };
    let mut username = unquote(parsed.username().unwrap_or(""));
    if username.is_empty() {
        username = get_param(&params, &["username", "user"], "");
    }
    let mut password = unquote(parsed.password().unwrap_or(""));
    if password.is_empty() {
        password = get_param(&params, &["password", "pass"], "");
    }
    if server.is_empty() || username.is_empty() || password.is_empty() {
        return Err(LinkParseError::new("mieru link must contain server, username and password"));
    }

    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String("mieru".into()));
    outbound.insert("tag".to_string(), Value::String("proxy".into()));
    outbound.insert("server".to_string(), Value::String(server.clone()));
    let transport = {
        let t = get_param(&params, &["transport"], "TCP");
        if t.is_empty() {
            "TCP".to_string()
        } else {
            t
        }
    };
    outbound.insert("transport".to_string(), Value::String(transport.to_uppercase()));
    outbound.insert("username".to_string(), Value::String(username));
    outbound.insert("password".to_string(), Value::String(password));
    if port != 0 {
        outbound.insert("server_port".to_string(), json!(port));
    }
    let ports = get_param(&params, &["server_ports", "ports"], "");
    if !ports.is_empty() {
        let items: Vec<Value> = ports
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect();
        outbound.insert("server_ports".to_string(), Value::Array(items));
    }
    let multiplexing = get_param(&params, &["multiplexing", "mux"], "");
    if !multiplexing.is_empty() {
        outbound.insert("multiplexing".to_string(), Value::String(multiplexing));
    }
    let traffic_pattern = get_param(&params, &["traffic_pattern", "trafficPattern"], "");
    if !traffic_pattern.is_empty() {
        outbound.insert("traffic_pattern".to_string(), Value::String(traffic_pattern));
    }
    let name = clean_name(
        Some(&parsed.fragment),
        format!("mieru-{server}:{}", if port != 0 { port.to_string() } else { "range".to_string() }),
    );
    let native = native_singbox_outbound(&Value::Object(outbound))?;
    Ok(make_node(name, "mieru", server, port, link, native))
}

// ── AnyTLS ──

pub(super) fn parse_anytls(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let query = parse_query(&parsed.query);
    let mut params: BTreeMap<String, String> = BTreeMap::new();
    for (key, _) in &query {
        let lower = key.to_lowercase();
        params.insert(lower, first_of(&query, key));
    }
    let server = parsed.hostname().trim().to_string();
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => 443,
    };
    let mut password = unquote(parsed.username().unwrap_or(""));
    if password.is_empty() {
        password = get_param(&params, &["password"], "");
    }
    if server.is_empty() || password.is_empty() {
        return Err(LinkParseError::new("anytls link must contain server and password"));
    }
    let mut tls = Map::new();
    tls.insert("enabled".to_string(), Value::Bool(true));
    let server_name = get_param(
        &params,
        &["sni", "peer", "server_name", "servername"],
        &server,
    );
    tls.insert("server_name".to_string(), Value::String(server_name));
    if to_bool_str(&get_param(&params, &["insecure", "allowinsecure", "allow_insecure"], "")) {
        tls.insert("insecure".to_string(), Value::Bool(true));
    }
    let pins = get_param(
        &params,
        &[
            "pinsha256",
            "certificate_public_key_sha256",
            "certificate-public-key-sha256",
            "certsha256",
        ],
        "",
    );
    if !pins.is_empty() {
        let items: Vec<Value> = super::helpers::split_csv(&pins).into_iter().map(Value::String).collect();
        tls.insert("certificate_public_key_sha256".to_string(), Value::Array(items));
    }
    let alpn = get_param(&params, &["alpn"], "");
    if !alpn.is_empty() {
        let items: Vec<Value> = super::helpers::split_csv(&alpn).into_iter().map(Value::String).collect();
        tls.insert("alpn".to_string(), Value::Array(items));
    }
    let mut native = Map::new();
    native.insert("type".to_string(), Value::String("anytls".into()));
    native.insert("tag".to_string(), Value::String("proxy".into()));
    native.insert("server".to_string(), Value::String(server.clone()));
    native.insert("server_port".to_string(), json!(port));
    native.insert("password".to_string(), Value::String(password));
    native.insert("tls".to_string(), Value::Object(tls));
    for (source, target) in [
        ("idle_session_check_interval", "idle_session_check_interval"),
        ("idle-session-check-interval", "idle_session_check_interval"),
        ("idle_session_timeout", "idle_session_timeout"),
        ("idle-session-timeout", "idle_session_timeout"),
    ] {
        if let Some(value) = params.get(source) {
            if !value.is_empty() {
                native.insert(target.to_string(), Value::String(value.clone()));
            }
        }
    }
    let min_idle = get_param(&params, &["min_idle_session", "min-idle-session"], "");
    if !min_idle.is_empty() {
        match py_parse_int(&min_idle) {
            Ok(value) => {
                native.insert("min_idle_session".to_string(), json!(value));
            }
            Err(_) => {
                return Err(LinkParseError::new("AnyTLS min_idle_session must be an integer"));
            }
        }
    }
    let name = clean_name(Some(&parsed.fragment), format!("anytls-{server}:{port}"));
    let outbound = native_singbox_outbound(&Value::Object(native))?;
    Ok(make_node(name, "anytls", server, port, link, outbound))
}

// ── MASQUE ──

pub(super) fn parse_masque(link: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = query_params(&parsed);
    let mut profile_id = unquote(&parsed.hostname());
    if profile_id.is_empty() {
        profile_id = get_param(&params, &["id", "profile_id"], "");
    }
    let mut auth_token = unquote(parsed.username().unwrap_or(""));
    if auth_token.is_empty() {
        auth_token = get_param(&params, &["auth_token", "token"], "");
    }
    let mut outbound = Map::new();
    outbound.insert("type".to_string(), Value::String("masque".into()));
    outbound.insert("tag".to_string(), Value::String("proxy".into()));
    outbound.insert(
        "system".to_string(),
        Value::Bool(to_bool_str(&get_param(&params, &["system"], "false"))),
    );
    let masque_name = get_param(&params, &["name"], "masque0");
    outbound.insert(
        "name".to_string(),
        Value::String(if masque_name.is_empty() { "masque0".into() } else { masque_name }),
    );
    outbound.insert(
        "use_http2".to_string(),
        Value::Bool(to_bool_str(&get_param(&params, &["use_http2", "http2"], "false"))),
    );
    outbound.insert(
        "use_ipv6".to_string(),
        Value::Bool(to_bool_str(&get_param(&params, &["use_ipv6", "ipv6"], "false"))),
    );
    outbound.insert("profile".to_string(), json!({ "detour": "direct" }));
    let udp_timeout = get_param(&params, &["udp_timeout", "udpTimeout"], "5m0s");
    outbound.insert(
        "udp_timeout".to_string(),
        Value::String(if udp_timeout.is_empty() { "5m0s".into() } else { udp_timeout }),
    );
    let udp_keepalive = get_param(&params, &["udp_keepalive_period", "udpKeepalivePeriod"], "30s");
    outbound.insert(
        "udp_keepalive_period".to_string(),
        Value::String(if udp_keepalive.is_empty() { "30s".into() } else { udp_keepalive }),
    );
    let reconnect_delay = get_param(&params, &["reconnect_delay", "reconnectDelay"], "5s");
    outbound.insert(
        "reconnect_delay".to_string(),
        Value::String(if reconnect_delay.is_empty() { "5s".into() } else { reconnect_delay }),
    );
    let congestion_controller = get_param(&params, &["congestion_controller", "congestionController"], "bbr");
    outbound.insert(
        "congestion_controller".to_string(),
        Value::String(if congestion_controller.is_empty() { "bbr".into() } else { congestion_controller }),
    );
    if let Some(profile) = outbound.get_mut("profile").and_then(Value::as_object_mut) {
        if !profile_id.is_empty() {
            profile.insert("id".to_string(), Value::String(profile_id.clone()));
        }
        if !auth_token.is_empty() {
            profile.insert("auth_token".to_string(), Value::String(auth_token));
        }
    }
    let allowed_ips = get_param(&params, &["allowed_ips", "allowedIPs"], "");
    if !allowed_ips.is_empty() {
        let items: Vec<Value> = super::helpers::split_csv(&allowed_ips)
            .into_iter()
            .map(Value::String)
            .collect();
        outbound.insert("allowed_ips".to_string(), Value::Array(items));
    }
    let server_name = get_param(&params, &["sni", "server_name", "serverName"], "");
    let insecure = get_param(&params, &["insecure", "allowInsecure"], "");
    if !server_name.is_empty() || !insecure.is_empty() {
        let mut tls = Map::new();
        if !server_name.is_empty() {
            tls.insert("server_name".to_string(), Value::String(server_name));
        }
        if !insecure.is_empty() {
            tls.insert("insecure".to_string(), Value::Bool(to_bool_str(&insecure)));
        }
        outbound.insert("tls".to_string(), Value::Object(tls));
    }
    let name = clean_name(Some(&parsed.fragment), "MASQUE".to_string());
    let native = native_singbox_outbound(&Value::Object(outbound))?;
    Ok(make_node(name, "masque", profile_id, 0, link, native))
}

// ── Happ metadata ──

fn py_is_printable_or_space(c: char) -> bool {
    if c.is_control() {
        return super::helpers::is_py_space(c);
    }
    true
}

/// Python `_decode_happ_description`.
fn decode_happ_description(value: &str) -> String {
    let raw = unquote(value).trim().to_string();
    if raw.is_empty() {
        return String::new();
    }
    if let Ok(decoded) = decode_b64(&raw) {
        let decoded = decoded.trim().to_string();
        if !decoded.is_empty() && decoded.chars().all(py_is_printable_or_space) {
            return decoded.chars().take(30).collect();
        }
    }
    raw.chars().take(30).collect()
}

/// Python `_apply_happ_server_metadata(node, raw)`.
pub(super) fn apply_happ_server_metadata(node: &mut Node, raw: &str) {
    let mut description = String::new();
    let text = raw.trim().to_string();
    let parsed = match PyUrlSplit::parse(&text) {
        Ok(p) => p,
        Err(_) => return,
    };
    let fragment = unquote(&parsed.fragment);
    if fragment.contains('?') {
        let (title, query_text) = fragment.split_once('?').unwrap();
        let query = parse_query(query_text);
        let values = {
            let a = first_of(&query, "serverDescription");
            if !a.is_empty() || query.iter().any(|(k, _)| k == "serverDescription") {
                a
            } else {
                first_of(&query, "serverdescription")
            }
        };
        if !values.is_empty() {
            description = decode_happ_description(&values);
        }
        let title = title.trim();
        if !title.is_empty() {
            node.name = title.to_string();
        }
    }
    if description.is_empty() && parsed.scheme == "vmess" {
        if let Some((_, payload_text)) = text.split_once("://") {
            if let Ok(decoded) = decode_b64(payload_text) {
                if let Ok(payload) = serde_json::from_str::<Value>(&decoded) {
                    description =
                        decode_happ_description(&py_str_or(payload.get("serverDescription"), ""));
                }
            }
        }
    }
    if description.is_empty() && text.starts_with('{') {
        if let Ok(payload) = serde_json::from_str::<Value>(&text) {
            let meta = payload.get("meta").filter(|m| m.is_object());
            if let Some(meta) = meta {
                let raw_desc = py_str_or(meta.get("serverDescription"), "");
                description = raw_desc.trim().chars().take(30).collect();
            }
        }
    }
    if !description.is_empty() {
        node.description = description;
    }
}

/// Python `_json_name(payload, fallback)`.
pub(super) fn json_name(payload: &Value, fallback: String) -> String {
    for key in ["remarks", "remark", "ps", "name", "tag"] {
        let value = py_str_or(payload.get(key), "").trim().to_string();
        if !value.is_empty() {
            return value;
        }
    }
    fallback
}
