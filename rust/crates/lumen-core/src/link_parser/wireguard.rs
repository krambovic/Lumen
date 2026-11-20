//! WireGuard/AWG/WARP URI and INI parsing, including AmneziaWG option validation.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{json, Map, Value};

use super::helpers::{
    clean_name, get_param, py_int_or, py_int_value, py_parse_int, py_str, py_str_or, split_csv,
    to_bool_str, unquote, PyUrlSplit,
};
use super::json_import::native_singbox_outbound;
use super::schemes::make_node;
use super::{LinkParseError, Node};

/// `_AMNEZIA_AWG3_*` tables: source key → (conf name, dash name, json key).
pub(super) const AMNEZIA_AWG3_KEYS: [(&str, &str, &str, &str); 6] = [
    ("contentpaddingaddition", "ContentPaddingAddition", "content-padding-addition", "content_padding_addition"),
    ("rekeyaftertime", "RekeyAfterTime", "rekey-after-time", "rekey_after_time"),
    ("rekeytimeout", "RekeyTimeout", "rekey-timeout", "rekey_timeout"),
    ("rejectaftertime", "RejectAfterTime", "reject-after-time", "reject_after_time"),
    ("keepalivetimeout", "KeepaliveTimeout", "keepalive-timeout", "keepalive_timeout"),
    ("maxhandshakeattempts", "MaxHandshakeAttempts", "max-handshake-attempts", "max_handshake_attempts"),
];

const AWG_UINT32_MAX: i64 = 0xFFFFFFFF;

fn awg_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+)-(\d+)$").expect("static regex"))
}

// ── Endpoint parsing helpers ──

fn ip_in_network(ip: IpAddr, net: IpAddr, prefix: u8) -> bool {
    match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let bits = u32::from(a);
            let net_bits = u32::from(n);
            if prefix == 0 {
                return true;
            }
            if prefix > 32 {
                return false;
            }
            bits >> (32 - prefix) == net_bits >> (32 - prefix)
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let bits = u128::from(a);
            let net_bits = u128::from(n);
            if prefix == 0 {
                return true;
            }
            if prefix > 128 {
                return false;
            }
            bits >> (128 - prefix) == net_bits >> (128 - prefix)
        }
        _ => false,
    }
}

/// Python `_is_cloudflare_warp_peer`.
pub(super) fn is_cloudflare_warp_peer(value: &str) -> bool {
    let host = value.trim().to_lowercase();
    let host = host.trim_matches('.');
    if host == "engage.cloudflareclient.com" || host.ends_with(".cloudflareclient.com") {
        return true;
    }
    let address: IpAddr = match host.parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    [
        (IpAddr::V4(Ipv4Addr::new(162, 159, 192, 0)), 24u8),
        (IpAddr::V4(Ipv4Addr::new(162, 159, 193, 0)), 24),
        (IpAddr::V4(Ipv4Addr::new(162, 159, 195, 0)), 24),
        (IpAddr::V4(Ipv4Addr::new(188, 114, 96, 0)), 20),
        (IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0xd0, 0, 0, 0, 0, 0)), 48),
        (IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0xd1, 0, 0, 0, 0, 0)), 48),
    ]
    .iter()
    .any(|(net, prefix)| ip_in_network(address, *net, *prefix))
}

/// link_parser `_is_warp_endpoint` (identical to `_is_cloudflare_warp_peer`).
pub(super) fn is_warp_endpoint(server: &str) -> bool {
    is_cloudflare_warp_peer(server)
}

/// Python `wireguard_normalization._positive_int`.
fn positive_int(value: Option<&Value>, fallback: i64) -> i64 {
    match value {
        Some(v) => match py_int_or(Some(v), 0) {
            Ok(parsed) if parsed > 0 => parsed,
            _ => fallback,
        },
        None => fallback,
    }
}

/// Python `wireguard_normalization._split_endpoint`.
pub(super) fn split_endpoint(value: &str) -> (String, i64) {
    let text = py_str_or(Some(&Value::String(value.to_string())), "");
    let text = text.trim();
    if text.is_empty() {
        return (String::new(), 0);
    }
    if text.starts_with('[') {
        if let Some(closing) = text.find(']') {
            if closing > 0 {
                let host = text[1..closing].to_string();
                let remainder = &text[closing + 1..];
                if remainder.starts_with(':') {
                    let port = positive_int(Some(&Value::String(remainder[1..].to_string())), 0);
                    return (host, port);
                }
                return (host, 0);
            }
        }
    }
    match text.rsplit_once(':') {
        Some((host, port_text))
            if !host.is_empty()
                && !port_text.is_empty()
                && port_text.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let port = port_text.parse::<i64>().unwrap_or(0);
            (host.to_string(), port)
        }
        _ => (text.to_string(), 0),
    }
}

/// Python `wireguard_normalization.normalize_ip_prefix`.
pub(super) fn normalize_ip_prefix(value: &str) -> String {
    let text = value.trim().trim_matches('"').trim_matches('\'');
    if text.is_empty() {
        return String::new();
    }
    if let Some((addr_text, prefix_text)) = text.split_once('/') {
        // ipaddress.ip_interface semantics: reject bad address or oversized
        // prefix, otherwise canonicalize "addr/prefix".
        if let Ok(addr) = addr_text.parse::<Ipv4Addr>() {
            if let Ok(prefix) = py_parse_int(prefix_text) {
                if (0..=32).contains(&prefix) {
                    return format!("{addr}/{prefix}");
                }
            }
            return text.to_string();
        }
        if let Ok(addr) = addr_text.parse::<Ipv6Addr>() {
            if let Ok(prefix) = py_parse_int(prefix_text) {
                if (0..=128).contains(&prefix) {
                    return format!("{addr}/{prefix}");
                }
            }
        }
        return text.to_string();
    }
    if let Ok(addr) = text.parse::<Ipv4Addr>() {
        return format!("{addr}/32");
    }
    if let Ok(addr) = text.parse::<Ipv6Addr>() {
        return format!("{addr}/128");
    }
    text.to_string()
}

/// Python `wireguard_normalization.normalize_ip_prefixes` (list or CSV input).
pub(super) fn normalize_ip_prefixes(value: &Value) -> Vec<String> {
    let raw_items: Vec<String> = match value {
        Value::Array(items) => items.iter().map(py_str).collect(),
        other => {
            let text = py_str_or(Some(other), "");
            text.split(',').map(|s| s.to_string()).collect()
        }
    };
    let mut result: Vec<String> = Vec::new();
    for item in raw_items {
        let normalized = normalize_ip_prefix(&item);
        if !normalized.is_empty() && !result.contains(&normalized) {
            result.push(normalized);
        }
    }
    result
}

// ── AmneziaWG helpers ──

/// Python `_validate_awg3_header_key` — base64 32-byte key (strict, unpadded
/// input is rejected exactly like `b64decode(validate=True)`).
pub(super) fn validate_awg3_header_key(value: &str) -> Result<String, LinkParseError> {
    let text = value.trim();
    let invalid = || {
        LinkParseError::new(
            "некорректный HeaderProtectionKey: ожидается base64 ключ длиной 32 байта",
        )
    };
    if text.len() % 4 != 0 || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=') {
        return Err(invalid());
    }
    let raw = super::helpers::decode_b64_strict(text).map_err(|_| invalid())?;
    if raw.len() != 32 {
        return Err(LinkParseError::new(format!(
            "некорректный HeaderProtectionKey: {} байт вместо 32",
            raw.len()
        )));
    }
    Ok(text.to_string())
}

/// Python `_validate_awg3_range` — uint32 or inclusive `A-B` range.
pub(super) fn validate_awg3_range(value: &str, field: &str) -> Result<Value, LinkParseError> {
    let text = value.trim();
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        let number = py_parse_int(text)
            .map_err(|_| LinkParseError::new(format!("слишком большое значение в поле {field}: {value}")))?;
        if number > AWG_UINT32_MAX {
            return Err(LinkParseError::new(format!(
                "слишком большое значение в поле {field}: {value}"
            )));
        }
        return Ok(json!(number));
    }
    if let Some(captures) = awg_range_regex().captures(text) {
        let low = py_parse_int(captures.get(1).map(|m| m.as_str()).unwrap_or(""))
            .map_err(|_| LinkParseError::new(format!("некорректный диапазон в поле {field}: {value}")))?;
        let high = py_parse_int(captures.get(2).map(|m| m.as_str()).unwrap_or(""))
            .map_err(|_| LinkParseError::new(format!("некорректный диапазон в поле {field}: {value}")))?;
        if high > AWG_UINT32_MAX || low > high {
            return Err(LinkParseError::new(format!(
                "некорректный диапазон в поле {field}: {value}"
            )));
        }
        return Ok(Value::String(format!("{low}-{high}")));
    }
    Err(LinkParseError::new(format!(
        "некорректный диапазон в поле {field}: {value} (ожидается число или A-B)"
    )))
}

/// Python `_amnezia_from_params(params)` — URI/INI options → amnezia dict.
pub(super) fn amnezia_from_params(
    params: &BTreeMap<String, String>,
) -> Result<Map<String, Value>, LinkParseError> {
    let mut result = Map::new();
    for key in ["jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "h1", "h2", "h3", "h4", "itime"] {
        let upper = key.to_uppercase();
        let value = get_param(params, &[key, upper.as_str()], "");
        if !value.is_empty() {
            match py_parse_int(&value) {
                Ok(number) => {
                    result.insert(key.to_string(), json!(number));
                }
                Err(_) => {
                    result.insert(key.to_string(), Value::String(value));
                }
            }
        }
    }
    for key in ["i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3"] {
        let upper = key.to_uppercase();
        let value = get_param(params, &[key, upper.as_str()], "");
        if !value.is_empty() {
            result.insert(key.to_string(), Value::String(value));
        }
    }
    let header_key = get_param(
        params,
        &[
            "header_protection_key",
            "header-protection-key",
            "headerprotectionkey",
            "headerProtectionKey",
            "HeaderProtectionKey",
        ],
        "",
    );
    if !header_key.is_empty() {
        let validated = validate_awg3_header_key(&header_key)?;
        result.insert("header_protection_key".to_string(), Value::String(validated));
    }
    // AWG 3.x boolean flags (Python `_amnezia_from_params`).
    for (key, conf_name) in [("random_trailers", "RandomTrailers"), ("disable_cookies", "DisableCookies")] {
        let dashed = key.replace('_', "-");
        let joined = key.replace('_', "");
        let value = get_param(params, &[key, dashed.as_str(), joined.as_str(), conf_name], "");
        if !value.is_empty() {
            let lowered = value.trim().to_lowercase();
            if !["true", "false", "1", "0", "yes", "no", "on", "off"].contains(&lowered.as_str()) {
                return Err(LinkParseError::new(format!(
                    "некорректное логическое значение в поле {conf_name}"
                )));
            }
            result.insert(
                key.to_string(),
                Value::Bool(["true", "1", "yes", "on"].contains(&lowered.as_str())),
            );
        }
    }
    for (source_key, conf_name, dash_name, json_key) in AMNEZIA_AWG3_KEYS {
        let no_underscore = source_key.replace('_', "");
        let value = get_param(
            params,
            &[source_key, no_underscore.as_str(), dash_name, conf_name, json_key],
            "",
        );
        if !value.is_empty() {
            let validated = validate_awg3_range(&value, conf_name)?;
            result.insert(json_key.to_string(), validated);
        }
    }
    Ok(result)
}

/// Python `_parse_reserved_bytes(value)` (raising variant).
pub(super) fn parse_reserved_bytes(value: Option<&Value>) -> Result<Vec<i64>, LinkParseError> {
    let empty = match value {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    };
    if empty {
        return Ok(Vec::new());
    }
    let value = value.unwrap();
    let (parts, part_values): (Vec<String>, Option<Vec<Value>>) = match value {
        Value::Array(items) => (
            items.iter().map(py_str).collect(),
            Some(items.clone()),
        ),
        other => {
            let text = py_str(other);
            // str(value).strip() then .strip("[]")
            let text = text.trim().trim_matches(|c| c == '[' || c == ']').trim().to_string();
            let split: Vec<String> = text
                .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            (split.clone(), Some(split.into_iter().map(Value::String).collect()))
        }
    };
    if parts.len() == 1 && part_values.is_some() && !matches!(value, Value::Array(_)) {
        let encoded = parts[0].replace('-', "+").replace('_', "/");
        let mut padded = encoded.clone();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        if let Ok(decoded) = super::helpers::decode_b64_strict(&padded) {
            if decoded.len() == 3 {
                return Ok(decoded.iter().map(|b| i64::from(*b)).collect());
            }
        }
    }
    let mut result: Vec<i64> = Vec::with_capacity(parts.len());
    let items = part_values.unwrap_or_default();
    for (index, part) in parts.iter().enumerate() {
        let candidate = items.get(index).cloned().unwrap_or(Value::String(part.clone()));
        match py_int_value(&candidate) {
            Ok(v) => result.push(v),
            Err(_) => {
                return Err(LinkParseError::new(
                    "wireguard reserved must contain three bytes",
                ))
            }
        }
    }
    if result.len() != 3 || result.iter().any(|item| *item < 0 || *item > 255) {
        return Err(LinkParseError::new(
            "wireguard reserved must contain three bytes",
        ));
    }
    Ok(result)
}

pub(super) fn clean_b64_key(value: &str) -> String {
    value.trim().replace(' ', "+")
}

/// Python `_wireguard_display_name`.
pub(super) fn wireguard_display_name(server: &str, port: i64, _amnezia: bool, _scheme: &str) -> String {
    format!("{server}:{}", if port == 0 { 51820 } else { port })
}

/// Python `_canonical_wireguard_key`.
fn canonical_wireguard_key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .collect()
}

// ── endpoint builders ──

/// Python `_apply_wireguard_performance_options`.
fn apply_wireguard_performance_options(
    endpoint: &mut Map<String, Value>,
    workers: Option<&Value>,
    preallocated_buffers_per_pool: Option<&Value>,
    disable_pauses: Option<&Value>,
) -> Result<(), LinkParseError> {
    let non_empty = |v: Option<&Value>| match v {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    };
    if non_empty(workers) {
        endpoint.insert("workers".to_string(), json!(py_int_value(workers.unwrap())?));
    }
    if non_empty(preallocated_buffers_per_pool) {
        endpoint.insert(
            "preallocated_buffers_per_pool".to_string(),
            json!(py_int_value(preallocated_buffers_per_pool.unwrap())?),
        );
    }
    if non_empty(disable_pauses) {
        let raw = disable_pauses.unwrap();
        let value = match raw {
            Value::Bool(b) => *b,
            other => to_bool_str(&py_str(other)),
        };
        endpoint.insert("disable_pauses".to_string(), Value::Bool(value));
    }
    Ok(())
}

/// Python `_build_wireguard_peer`.
pub(super) fn build_wireguard_peer(
    server: &str,
    port: i64,
    public_key: &str,
    allowed_ips: &[String],
    pre_shared_key: &str,
    persistent_keepalive: Option<&Value>,
) -> Result<Map<String, Value>, LinkParseError> {
    let mut peer = Map::new();
    peer.insert("address".to_string(), Value::String(server.to_string()));
    peer.insert("port".to_string(), json!(port));
    peer.insert(
        "public_key".to_string(),
        Value::String(clean_b64_key(public_key)),
    );
    let ips: Vec<Value> = if allowed_ips.is_empty() {
        vec![Value::String("0.0.0.0/0".into()), Value::String("::/0".into())]
    } else {
        allowed_ips.iter().map(|s| Value::String(s.clone())).collect()
    };
    let normalized = normalize_ip_prefixes(&Value::Array(ips));
    peer.insert(
        "allowed_ips".to_string(),
        Value::Array(normalized.into_iter().map(Value::String).collect()),
    );
    if !pre_shared_key.is_empty() {
        peer.insert(
            "pre_shared_key".to_string(),
            Value::String(clean_b64_key(pre_shared_key)),
        );
    }
    let keepalive_text = persistent_keepalive
        .map(|v| py_str_or(Some(v), ""))
        .unwrap_or_default();
    if !keepalive_text.is_empty() {
        peer.insert(
            "persistent_keepalive_interval".to_string(),
            json!(py_int_value(persistent_keepalive.unwrap())?),
        );
    }
    Ok(peer)
}

/// Python `_build_wireguard_endpoint`.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_wireguard_endpoint(
    server: &str,
    port: i64,
    private_key: &str,
    public_key: &str,
    address: &[String],
    allowed_ips: &[String],
    pre_shared_key: &str,
    mtu: i64,
    amnezia: Option<Map<String, Value>>,
    persistent_keepalive: Option<&Value>,
    listen_port: Option<&Value>,
    udp_timeout: &str,
    workers: Option<&Value>,
    preallocated_buffers_per_pool: Option<&Value>,
    disable_pauses: Option<&Value>,
) -> Result<Map<String, Value>, LinkParseError> {
    let peer = build_wireguard_peer(server, port, public_key, allowed_ips, pre_shared_key, persistent_keepalive)?;
    let mut endpoint = Map::new();
    endpoint.insert("type".to_string(), Value::String("wireguard".into()));
    endpoint.insert("tag".to_string(), Value::String("proxy".into()));
    endpoint.insert("mtu".to_string(), json!(if mtu == 0 { 1408 } else { mtu }));
    let addr_items: Vec<Value> = if address.is_empty() {
        vec![Value::String("10.0.0.2/32".into())]
    } else {
        address.iter().map(|s| Value::String(s.clone())).collect()
    };
    let normalized_address = normalize_ip_prefixes(&Value::Array(addr_items));
    endpoint.insert(
        "address".to_string(),
        Value::Array(normalized_address.into_iter().map(Value::String).collect()),
    );
    endpoint.insert(
        "private_key".to_string(),
        Value::String(clean_b64_key(private_key)),
    );
    endpoint.insert("peers".to_string(), Value::Array(vec![Value::Object(peer)]));
    let udp_timeout = udp_timeout.trim();
    endpoint.insert(
        "udp_timeout".to_string(),
        Value::String(if udp_timeout.is_empty() { "5m0s".into() } else { udp_timeout.to_string() }),
    );
    let listen_text = listen_port.map(|v| py_str_or(Some(v), "")).unwrap_or_default();
    if !listen_text.is_empty() {
        endpoint.insert("listen_port".to_string(), json!(py_int_value(listen_port.unwrap())?));
    }
    if let Some(amnezia) = amnezia {
        if !amnezia.is_empty() {
            endpoint.insert("amnezia".to_string(), Value::Object(amnezia));
        }
    }
    apply_wireguard_performance_options(&mut endpoint, workers, preallocated_buffers_per_pool, disable_pauses)?;
    Ok(endpoint)
}

/// Python `_build_warp_profile_endpoint`.
pub(super) fn build_warp_profile_endpoint(
    private_key: &str,
    persistent_keepalive: Option<&Value>,
    amnezia: Option<Map<String, Value>>,
    listen_port: Option<&Value>,
) -> Result<Map<String, Value>, LinkParseError> {
    let mut endpoint = Map::new();
    endpoint.insert("type".to_string(), Value::String("warp".into()));
    endpoint.insert("tag".to_string(), Value::String("proxy".into()));
    endpoint.insert("udp_timeout".to_string(), Value::String("5m0s".into()));
    endpoint.insert(
        "profile".to_string(),
        json!({ "detour": "direct", "private_key": clean_b64_key(private_key) }),
    );
    let listen_text = listen_port.map(|v| py_str_or(Some(v), "")).unwrap_or_default();
    if !listen_text.is_empty() {
        endpoint.insert("listen_port".to_string(), json!(py_int_value(listen_port.unwrap())?));
    }
    let keepalive_text = persistent_keepalive
        .map(|v| py_str_or(Some(v), ""))
        .unwrap_or_default();
    if !keepalive_text.is_empty() {
        endpoint.insert(
            "persistent_keepalive_interval".to_string(),
            json!(py_int_value(persistent_keepalive.unwrap())?),
        );
    }
    if let Some(amnezia) = amnezia {
        if !amnezia.is_empty() {
            endpoint.insert("amnezia".to_string(), Value::Object(amnezia));
        }
    }
    Ok(endpoint)
}

/// Python `_build_warp_endpoint(params)` — `warp://` URI form.
fn build_warp_endpoint(params: &BTreeMap<String, String>) -> Result<Map<String, Value>, LinkParseError> {
    let mut endpoint = Map::new();
    endpoint.insert("type".to_string(), Value::String("warp".into()));
    endpoint.insert("tag".to_string(), Value::String("proxy".into()));
    let udp_timeout = get_param(params, &["udp_timeout", "udpTimeout"], "5m0s");
    endpoint.insert(
        "udp_timeout".to_string(),
        Value::String(if udp_timeout.is_empty() { "5m0s".into() } else { udp_timeout }),
    );
    let listen_port = get_param(params, &["listen_port", "listenPort"], "");
    if !listen_port.is_empty() {
        endpoint.insert(
            "listen_port".to_string(),
            json!(py_int_value(&Value::String(listen_port))?),
        );
    }
    let amnezia = amnezia_from_params(params)?;
    if !amnezia.is_empty() {
        endpoint.insert("amnezia".to_string(), Value::Object(amnezia));
    }
    // `reserved` is validated but never emitted: sing-box-extended 2.5.1+
    // rejects the field on WARP endpoints and derives the bytes from the
    // profile itself.
    let reserved = get_param(params, &["reserved"], "");
    if !reserved.is_empty() {
        parse_reserved_bytes(Some(&Value::String(reserved)))?;
    }
    let keepalive = get_param(
        params,
        &[
            "persistent_keepalive",
            "persistentKeepalive",
            "persistent_keepalive_interval",
            "keepalive",
        ],
        "",
    );
    if !keepalive.is_empty() {
        endpoint.insert(
            "persistent_keepalive_interval".to_string(),
            json!(py_int_value(&Value::String(keepalive))?),
        );
    }
    let mut profile = Map::new();
    profile.insert("detour".to_string(), Value::String("direct".into()));
    for (source, target) in [
        ("id", "id"),
        ("private_key", "private_key"),
        ("privateKey", "private_key"),
        ("auth_token", "auth_token"),
        ("authToken", "auth_token"),
    ] {
        let value = get_param(params, &[source], "");
        if !value.is_empty() {
            profile.insert(target.to_string(), Value::String(value));
        }
    }
    endpoint.insert("profile".to_string(), Value::Object(profile));
    Ok(endpoint)
}

// ── URI parsing ──

/// Python `_parse_wireguard_like_link(link, scheme)`.
pub(super) fn parse_wireguard_like_link(link: &str, scheme: &str) -> Result<Node, LinkParseError> {
    let parsed = PyUrlSplit::parse(link)?;
    let params = super::schemes::query_params(&parsed);

    if scheme == "warp" {
        let endpoint = build_warp_endpoint(&params)?;
        let dns_param = get_param(&params, &["dns", "dns_servers"], "");
        let dns_servers = split_csv(&dns_param);
        let has_amnezia = endpoint.contains_key("amnezia");
        let mut outbound = native_singbox_outbound(&Value::Object(endpoint))?;
        if !dns_servers.is_empty() {
            if let Some(obj) = outbound.as_object_mut() {
                obj.insert(
                    "_dns".to_string(),
                    Value::Array(dns_servers.into_iter().map(Value::String).collect()),
                );
            }
        }
        let name = clean_name(
            Some(&parsed.fragment),
            if has_amnezia { "AWG/WARP".to_string() } else { "WARP".to_string() },
        );
        let mut node = make_node(
            name,
            "warp",
            "engage.cloudflareclient.com".to_string(),
            2408,
            link,
            outbound,
        );
        node.tags = vec!["WARP".to_string()];
        return Ok(node);
    }

    let mut server = parsed.hostname();
    if server.is_empty() {
        server = get_param(&params, &["server", "endpoint", "address"], "");
    }
    let port = match parsed.port()? {
        Some(p) if p != 0 => p,
        _ => {
            let q = py_int_value(&Value::String(get_param(&params, &["port"], "0")))?;
            if q != 0 {
                q
            } else {
                51820
            }
        }
    };
    let username = unquote(parsed.username().unwrap_or(""));
    let mut private_key = clean_b64_key(&username);
    if private_key.is_empty() {
        private_key = clean_b64_key(&get_param(&params, &["private_key", "privateKey", "key"], ""));
    }
    let public_key = clean_b64_key(&get_param(
        &params,
        &["public_key", "publicKey", "peer_public_key", "peerPublicKey", "pbk"],
        "",
    ));
    let address_param = get_param(
        &params,
        &["address", "addresses", "local_address", "localAddress"],
        "10.0.0.2/32",
    );
    let address = normalize_ip_prefixes(&Value::Array(
        split_csv(&address_param).into_iter().map(Value::String).collect(),
    ));
    let allowed_param = get_param(&params, &["allowed_ips", "allowedIPs", "allowed"], "0.0.0.0/0,::/0");
    let allowed_ips = normalize_ip_prefixes(&Value::Array(
        split_csv(&allowed_param).into_iter().map(Value::String).collect(),
    ));
    let pre_shared_key = get_param(&params, &["pre_shared_key", "preshared_key", "psk"], "");
    let reserved_param = get_param(&params, &["reserved"], "");
    let reserved_value = if reserved_param.is_empty() {
        None
    } else {
        Some(Value::String(reserved_param))
    };
    let reserved = parse_reserved_bytes(reserved_value.as_ref())?;
    let keepalive_param = get_param(
        &params,
        &[
            "persistent_keepalive",
            "persistentKeepalive",
            "persistent_keepalive_interval",
            "keepalive",
        ],
        "",
    );
    let keepalive = if keepalive_param.is_empty() {
        None
    } else {
        Some(Value::String(keepalive_param))
    };
    let mtu_param = get_param(&params, &["mtu"], "1408");
    let mtu = {
        let raw = py_int_value(&Value::String(mtu_param.clone()))?;
        if raw == 0 {
            1408
        } else {
            raw
        }
    };

    if private_key.is_empty() || public_key.is_empty() || server.is_empty() {
        return Err(LinkParseError::new(
            "wireguard/awg link must contain server, private_key and public_key",
        ));
    }

    let amnezia = amnezia_from_params(&params)?;
    let has_amnezia = !amnezia.is_empty();
    let endpoint = if (is_warp_endpoint(&server) || scheme == "warp") && !reserved.is_empty() {
        build_warp_profile_endpoint(
            &private_key,
            keepalive.as_ref(),
            if has_amnezia { Some(amnezia) } else { None },
            Some(&Value::String(get_param(&params, &["listen_port", "listenPort"], ""))),
        )?
    } else {
        let listen_port = get_param(&params, &["listen_port", "listenPort"], "");
        let udp_timeout = get_param(&params, &["udp_timeout", "udpTimeout"], "5m0s");
        let workers = {
            let w = get_param(&params, &["workers"], "");
            if w.is_empty() { None } else { Some(Value::String(w)) }
        };
        let preallocated = {
            let p = get_param(&params, &["preallocated_buffers_per_pool", "preallocatedBuffersPerPool"], "");
            if p.is_empty() { None } else { Some(Value::String(p)) }
        };
        let disable_pauses = {
            let d = get_param(&params, &["disable_pauses", "disablePauses"], "");
            if d.is_empty() { None } else { Some(Value::String(d)) }
        };
        build_wireguard_endpoint(
            &server,
            port,
            &clean_b64_key(&private_key),
            &clean_b64_key(&public_key),
            &address,
            &allowed_ips,
            &pre_shared_key,
            mtu,
            if has_amnezia { Some(amnezia) } else { None },
            keepalive.as_ref(),
            if listen_port.is_empty() {
                None
            } else {
                Some(Value::String(listen_port.clone()))
            }
            .as_ref(),
            &udp_timeout,
            workers.as_ref(),
            preallocated.as_ref(),
            disable_pauses.as_ref(),
        )?
    };
    let is_warp_type = endpoint.get("type").and_then(Value::as_str) == Some("warp");
    let endpoint_amnezia = endpoint.get("amnezia").cloned();
    let dns_param = get_param(&params, &["dns", "dns_servers"], "");
    let dns_servers = split_csv(&dns_param);
    let mut outbound = native_singbox_outbound(&Value::Object(endpoint))?;
    if !dns_servers.is_empty() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert(
                "_dns".to_string(),
                Value::Array(dns_servers.into_iter().map(Value::String).collect()),
            );
        }
    }
    let name = clean_name(
        Some(&parsed.fragment),
        wireguard_display_name(&server, port, has_amnezia, scheme),
    );
    let mut node = make_node(
        name,
        if has_amnezia {
            "awg"
        } else if is_warp_type {
            "warp"
        } else {
            scheme
        },
        server.clone(),
        port,
        link,
        outbound,
    );
    if is_warp_endpoint(&server) {
        node.tags = vec!["WARP".to_string()];
    }
    let _ = endpoint_amnezia;
    Ok(node)
}

// ── INI config ──

/// Python `_parse_wireguard_config(text)`.
pub(super) fn parse_wireguard_config(text: &str) -> Result<Node, LinkParseError> {
    let mut interface: BTreeMap<String, String> = BTreeMap::new();
    let mut peers: Vec<BTreeMap<String, String>> = Vec::new();
    let mut current: Option<usize> = None;
    let stripped_text = text.trim_start_matches('\u{feff}');
    for raw_line in super::helpers::py_splitlines(stripped_text) {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_start_matches('[').trim_end_matches(']').trim().to_lowercase();
            if section == "interface" {
                // usize::MAX marks the [Interface] dict, distinct from peers.
                current = Some(usize::MAX);
            } else if section.starts_with("peer") {
                peers.push(BTreeMap::new());
                current = Some(peers.len() - 1);
            } else {
                current = None;
            }
            continue;
        }
        let target: Option<&mut BTreeMap<String, String>> = match current {
            Some(usize::MAX) => Some(&mut interface),
            Some(idx) if idx < peers.len() => Some(&mut peers[idx]),
            _ => None,
        };
        let Some(map) = target else { continue };
        let Some((key, value)) = line.split_once('=') else { continue };
        let normalized_key = canonical_wireguard_key(key);
        let normalized_value = value.trim().to_string();
        if matches!(normalized_key.as_str(), "address" | "allowedips" | "dns") && map.contains_key(&normalized_key) {
            let existing = map.get(&normalized_key).cloned().unwrap_or_default();
            map.insert(normalized_key, format!("{existing},{normalized_value}"));
        } else {
            map.insert(normalized_key, normalized_value);
        }
    }

    let private_key = interface.get("privatekey").cloned().unwrap_or_default();
    if peers.is_empty() || private_key.is_empty() {
        return Err(LinkParseError::new(
            "wireguard config must contain [Interface] PrivateKey and [Peer] Endpoint/PublicKey",
        ));
    }

    let mut parsed_peers: Vec<Value> = Vec::new();
    let mut first_server = String::new();
    let mut first_port: i64 = 0;
    for peer in &peers {
        let (server, port) = split_endpoint(peer.get("endpoint").map(String::as_str).unwrap_or(""));
        let public_key = peer.get("publickey").cloned().unwrap_or_default();
        if server.is_empty() || public_key.is_empty() {
            return Err(LinkParseError::new("each [Peer] must contain Endpoint and PublicKey"));
        }
        if first_server.is_empty() {
            first_server = server.clone();
            first_port = if port == 0 { 51820 } else { port };
        }
        let allowed_raw = peer.get("allowedips").cloned().unwrap_or_else(|| "0.0.0.0/0,::/0".to_string());
        let allowed = normalize_ip_prefixes(&Value::Array(
            split_csv(&allowed_raw).into_iter().map(Value::String).collect(),
        ));
        let keepalive = peer.get("persistentkeepalive").cloned().unwrap_or_default();
        parsed_peers.push(Value::Object(build_wireguard_peer(
            &server,
            if port == 0 { 51820 } else { port },
            &public_key,
            &allowed,
            peer.get("presharedkey").cloned().unwrap_or_default().as_str(),
            if keepalive.is_empty() {
                None
            } else {
                Some(Value::String(keepalive.clone()))
            }
            .as_ref(),
        )?));
    }

    let amnezia = amnezia_from_params(&interface)?;
    let has_amnezia = !amnezia.is_empty();
    let reserved_raw = {
        let a = interface.get("reserved").cloned().unwrap_or_default();
        if !a.is_empty() {
            a
        } else {
            peers
                .first()
                .and_then(|p| p.get("reserved"))
                .cloned()
                .unwrap_or_default()
        }
    };
    let reserved_value = if reserved_raw.is_empty() {
        None
    } else {
        Some(Value::String(reserved_raw))
    };
    let reserved = parse_reserved_bytes(reserved_value.as_ref())?;
    let endpoint: Map<String, Value> = if is_warp_endpoint(&first_server) && !reserved.is_empty() {
        let keepalive = peers[0].get("persistentkeepalive").cloned().unwrap_or_default();
        let listen_port = interface.get("listenport").cloned().unwrap_or_default();
        build_warp_profile_endpoint(
            &private_key,
            if keepalive.is_empty() {
                None
            } else {
                Some(Value::String(keepalive.clone()))
            }
            .as_ref(),
            if has_amnezia { Some(amnezia.clone()) } else { None },
            if listen_port.is_empty() {
                None
            } else {
                Some(Value::String(listen_port.clone()))
            }
            .as_ref(),
        )?
    } else {
        let mut endpoint = Map::new();
        endpoint.insert("type".to_string(), Value::String("wireguard".into()));
        endpoint.insert("tag".to_string(), Value::String("proxy".into()));
        let mtu_raw = interface.get("mtu").cloned().unwrap_or_else(|| "1408".to_string());
        let mtu = py_int_or(Some(&Value::String(mtu_raw)), 1408)?;
        endpoint.insert("mtu".to_string(), json!(if mtu == 0 { 1408 } else { mtu }));
        let address_raw = interface.get("address").cloned().unwrap_or_else(|| "10.0.0.2/32".to_string());
        let address = normalize_ip_prefixes(&Value::Array(
            split_csv(&address_raw).into_iter().map(Value::String).collect(),
        ));
        endpoint.insert(
            "address".to_string(),
            Value::Array(address.into_iter().map(Value::String).collect()),
        );
        endpoint.insert(
            "private_key".to_string(),
            Value::String(clean_b64_key(&private_key)),
        );
        endpoint.insert("peers".to_string(), Value::Array(parsed_peers));
        let udp_timeout = interface.get("udptimeout").cloned().unwrap_or_else(|| "5m0s".to_string());
        endpoint.insert(
            "udp_timeout".to_string(),
            Value::String(if udp_timeout.is_empty() { "5m0s".into() } else { udp_timeout }),
        );
        if let Some(listen_port) = interface.get("listenport") {
            if !listen_port.is_empty() {
                endpoint.insert(
                    "listen_port".to_string(),
                    json!(py_int_value(&Value::String(listen_port.clone()))?),
                );
            }
        }
        if has_amnezia {
            endpoint.insert("amnezia".to_string(), Value::Object(amnezia.clone()));
        }
        let workers = interface.get("workers").cloned().unwrap_or_default();
        let preallocated = interface.get("preallocatedbuffersperpool").cloned().unwrap_or_default();
        let disable_pauses = interface
            .get("disablepauses")
            .cloned()
            .map(|v| if v.is_empty() { Value::Null } else { Value::String(v) });
        let disable_pauses = match disable_pauses {
            Some(Value::Null) | None => None,
            other => other,
        };
        let as_value = |s: &str| if s.is_empty() { None } else { Some(Value::String(s.to_string())) };
        apply_wireguard_performance_options(
            &mut endpoint,
            as_value(&workers).as_ref(),
            as_value(&preallocated).as_ref(),
            disable_pauses.as_ref(),
        )?;
        endpoint
    };
    let scheme = if has_amnezia {
        "awg"
    } else if endpoint.get("type").and_then(Value::as_str) == Some("warp") {
        "warp"
    } else {
        "wireguard"
    };
    let mut outbound = native_singbox_outbound(&Value::Object(endpoint))?;
    let dns_raw = interface.get("dns").cloned().unwrap_or_default();
    let dns_servers = split_csv(&dns_raw);
    if !dns_servers.is_empty() {
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert(
                "_dns".to_string(),
                Value::Array(dns_servers.into_iter().map(Value::String).collect()),
            );
        }
    }
    let mut protocol_masking = Map::new();
    for key in ["id", "ip", "ib"] {
        if let Some(value) = interface.get(key) {
            if !value.trim().is_empty() {
                protocol_masking.insert(key.to_string(), Value::String(value.clone()));
            }
        }
    }
    if !protocol_masking.is_empty() {
        // sing-box-extended has no Id/Ip/Ib schema; preserve the source values
        // outside the native endpoint.
        if let Some(obj) = outbound.as_object_mut() {
            obj.insert("_protocol_masking".to_string(), Value::Object(protocol_masking));
        }
    }
    let mut node = make_node(
        wireguard_display_name(&first_server, first_port, has_amnezia, scheme),
        scheme,
        first_server.clone(),
        first_port,
        text,
        outbound,
    );
    if is_warp_endpoint(&first_server) {
        node.tags = vec!["WARP".to_string()];
    }
    Ok(node)
}
