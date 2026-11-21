//! GUI-free port of `ui/node_edit_dialog.py` outbound load/build logic
//! (port of `qml_app/bridge/node_edit_helpers.py`).
//!
//! Editor field payloads and outbound mutations stay JSON-shaped
//! (`serde_json::Value`) exactly like the Python dicts.

use base64::Engine;
use serde_json::{Map, Value, json};

use lumen_core::models::Node;
use lumen_core::pyjson;

// Option lists — identical to ui/node_edit_dialog.py module constants.
pub const FINGERPRINTS: [&str; 11] = [
    "",
    "chrome",
    "firefox",
    "safari",
    "ios",
    "android",
    "edge",
    "360",
    "qq",
    "random",
    "randomized",
];
pub const NETWORKS: [&str; 8] = [
    "tcp",
    "ws",
    "grpc",
    "http",
    "h2",
    "xhttp",
    "httpupgrade",
    "kcp",
];
pub const SECURITY: [&str; 3] = ["none", "tls", "reality"];
pub const FLOWS: [&str; 3] = ["", "xtls-rprx-vision", "xtls-rprx-vision-udp443"];
pub const RAW_HEADERS: [&str; 2] = ["none", "http"];

pub const XRAY_ADVANCED_PROTOCOLS: [&str; 2] = ["vless", "vmess"];
pub const ENDPOINT_EDITABLE_PROTOCOLS: [&str; 19] = [
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "ss",
    "socks",
    "http",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "wireguard",
    "awg",
    "warp",
    "masque",
    "openvpn",
    "naive",
    "anytls",
    "snell",
];
pub const NATIVE_PROTOCOLS: [&str; 13] = [
    "wireguard",
    "awg",
    "warp",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "masque",
    "openvpn",
    "naive",
    "singbox_config",
    "anytls",
    "snell",
];
pub const MANUAL_NODE_PROTOCOLS: [&str; 17] = [
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "socks",
    "http",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "wireguard",
    "awg",
    "warp",
    "masque",
    "openvpn",
    "naive",
    "anytls",
];

const AMNEZIA_EDITOR_KEYS: [&str; 27] = [
    "jc",
    "jmin",
    "jmax",
    "s1",
    "s2",
    "s3",
    "s4",
    "h1",
    "h2",
    "h3",
    "h4",
    "i1",
    "i2",
    "i3",
    "i4",
    "i5",
    "j1",
    "j2",
    "j3",
    "itime",
    "header_protection_key",
    "content_padding_addition",
    "rekey_after_time",
    "rekey_timeout",
    "reject_after_time",
    "keepalive_timeout",
    "max_handshake_attempts",
];
const AMNEZIA_NUMERIC_EDITOR_KEYS: [&str; 8] =
    ["jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime"];
const AMNEZIA_RANGE_EDITOR_KEYS: [&str; 6] = [
    "content_padding_addition",
    "rekey_after_time",
    "rekey_timeout",
    "reject_after_time",
    "keepalive_timeout",
    "max_handshake_attempts",
];

// AWG can talk to a standard WireGuard peer when the packet padding remains
// disabled and H1-H4 retain WireGuard's original message identifiers.  Client-
// side junk packets are safe because a regular WireGuard server ignores them.
static WG_COMPATIBLE_AMNEZIA_DEFAULTS: std::sync::LazyLock<[(&str, Value); 11]> =
    std::sync::LazyLock::new(|| {
        [
            ("jc", json!(4)),
            ("jmin", json!(40)),
            ("jmax", json!(70)),
            ("s1", json!(0)),
            ("s2", json!(0)),
            ("s3", json!(0)),
            ("s4", json!(0)),
            ("h1", json!("1")),
            ("h2", json!("2")),
            ("h3", json!("3")),
            ("h4", json!("4")),
        ]
    });

// ── JSON helpers (Python dict idioms) ───────────────────────────────────────

/// Python `_first_dict`.
fn first_dict(value: &Value) -> Map<String, Value> {
    if let Some(items) = value.as_array()
        && let Some(first) = items.first()
        && first.is_object()
    {
        return first.as_object().unwrap().clone();
    }
    Map::new()
}

/// Python `_ensure_first_dict(parent, key)`.
fn ensure_first_dict<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    if !parent.get(key).map(Value::is_array).unwrap_or(false) {
        parent.insert(key.to_string(), json!([]));
    }
    let items = parent.get_mut(key).unwrap().as_array_mut().unwrap();
    if items.is_empty() || !items[0].is_object() {
        items.insert(0, json!({}));
    }
    items[0].as_object_mut().unwrap()
}

fn ensure_dict<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = parent
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("object just ensured")
}

/// Python `_set_or_remove`.
fn set_or_remove(parent: &mut Map<String, Value>, key: &str, value: &str) {
    if value.is_empty() {
        parent.shift_remove(key);
    } else {
        parent.insert(key.to_string(), json!(value));
    }
}

/// Python `_safe_port`.
fn safe_port(value: &Value, fallback: i64) -> i64 {
    let text = value_to_text(value);
    let parsed = text
        .trim()
        .parse::<i64>()
        .unwrap_or_else(|_| fallback.max(0));
    parsed.clamp(0, 65535)
}

/// Python `_set_param(params, key, value)`.
/// Python `_set_param`: `params[key] = value` for a non-empty value. The query
/// is a dict there, so a repeated key replaces the value in place.
fn set_param(params: &mut Vec<(String, String)>, key: &str, value: impl AsRef<str>) {
    let value = value.as_ref().to_string();
    if !value.is_empty() {
        update_param(params, key, value);
    }
}

/// Python `params[key] = value` / `params.update(...)` on the query dict.
fn update_param(params: &mut Vec<(String, String)>, key: &str, value: String) {
    match params.iter_mut().find(|(existing, _)| existing == key) {
        Some(entry) => entry.1 = value,
        None => params.push((key.to_string(), value)),
    }
}

/// Python `urllib.parse.quote(value, safe="")`.
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Python `str(value or "")`.
/// Python `str(value or "")`: falsy values (`None`, `False`, `0`, `0.0`,
/// `""`) become empty, `True` is `"True"` and floats use `repr`.
fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(false) => String::new(),
        Value::Bool(true) => "True".into(),
        Value::String(text) => text.clone(),
        Value::Number(number) => match (number.as_i64(), number.as_f64()) {
            (Some(0), _) => String::new(),
            (Some(integer), _) => integer.to_string(),
            (None, Some(float)) if float == 0.0 => String::new(),
            (None, Some(float)) if number.is_f64() => pyjson::float_repr(float),
            _ => number.to_string(),
        },
        _ => String::new(),
    }
}

/// Python `int(value or fallback)` over a JSON scalar (strings included).
fn value_int(value: &Value, fallback: i64) -> i64 {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64))
            .unwrap_or(fallback),
        Value::String(text) => text.trim().parse().unwrap_or(fallback),
        Value::Bool(flag) => {
            if *flag {
                1
            } else {
                0
            }
        }
        _ => fallback,
    }
}

/// Python `int(native.get(key) or fallback)` — Python truthiness first (0 and
/// "" fall through), then the integer parse.
fn py_int_or(value: Option<&Value>, fallback: i64) -> i64 {
    let truthy = match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().map(|value| value != 0.0).unwrap_or(false),
        Some(Value::String(text)) => !text.is_empty(),
        _ => true,
    };
    if !truthy {
        return fallback;
    }
    value_int(value.unwrap(), fallback)
}

fn is_dict(value: Option<&Value>) -> bool {
    value.map(Value::is_object).unwrap_or(false)
}

fn obj_or_empty(value: Option<&Value>) -> Map<String, Value> {
    value
        .filter(|value| value.is_object())
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn text_of(value: Option<&Value>) -> String {
    value.map(value_to_text).unwrap_or_default()
}

/// Python `json.dumps(value, ensure_ascii=False, separators=(",", ":"))`.
fn compact_json(value: &Value) -> String {
    match value {
        Value::String(text) => {
            let mut out = String::new();
            pyjson::write_string(&mut out, text, false);
            out
        }
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(compact_json).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(key, value)| {
                    let mut key_out = String::new();
                    pyjson::write_string(&mut key_out, key, false);
                    format!("{key_out}:{}", compact_json(value))
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => "null".to_string(),
    }
}

/// Python `_protocol_from_node`.
fn protocol_from_node(node: &Node) -> String {
    let outbound = obj_or_empty(if node.outbound.is_object() {
        Some(&node.outbound)
    } else {
        None
    });
    let protocol = text_of(outbound.get("protocol"));
    let protocol = if protocol.is_empty() {
        node.scheme.clone()
    } else {
        protocol
    };
    let protocol = protocol.trim().to_lowercase();
    match protocol.as_str() {
        "ss" => "shadowsocks".into(),
        "socks5" => "socks".into(),
        "hy" => "hysteria".into(),
        "hy2" => "hysteria2".into(),
        "mierus" => "mieru".into(),
        "any-tls" => "anytls".into(),
        "wg" => "wireguard".into(),
        _ => protocol,
    }
}

fn protocol_in(list: &[&str], protocol: &str) -> bool {
    list.contains(&protocol)
}

/// Python `_field_capabilities`.
fn field_capabilities(protocol: &str) -> Value {
    let protocol = protocol.trim().to_lowercase();
    let xray_advanced = protocol_in(&XRAY_ADVANCED_PROTOCOLS, &protocol);
    let tls_editable = protocol_in(&["vless", "vmess", "trojan"], &protocol);
    let endpoint = protocol_in(&ENDPOINT_EDITABLE_PROTOCOLS, &protocol);
    let fixed_endpoint = protocol_in(&["singbox_config", "xray_config"], &protocol);
    json!({
        "protocol": protocol,
        "xrayAdvanced": xray_advanced,
        "nativeSingbox": protocol_in(&NATIVE_PROTOCOLS, &protocol),
        "endpoint": endpoint && !fixed_endpoint,
        "identity": xray_advanced,
        "flow": protocol == "vless",
        "encryption": xray_advanced,
        "transport": xray_advanced,
        "rawHeader": xray_advanced,
        "tls": tls_editable,
        "reality": xray_advanced,
        "finalmask": xray_advanced,
        "basicOnly": !xray_advanced,
        "readOnlyConfig": protocol_in(&["singbox_config", "xray_config"], &protocol),
    })
}

/// Python `_share_query` — params keep insertion order like the Python dict.
fn share_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{}={}", quote(key), quote(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Python `_share_host` — format a host for URI authority syntax (including
/// IPv6 literals).
fn share_host(server: &Value) -> String {
    let value = value_to_text(server).trim().to_string();
    if value.contains(':') && !value.starts_with('[') {
        format!("[{value}]")
    } else {
        value
    }
}

/// Python `_share_network`.
fn share_network(stream: &Map<String, Value>) -> String {
    let network = text_of(stream.get("network"));
    let network = if network.is_empty() {
        "tcp"
    } else {
        network.as_str()
    };
    if network.eq_ignore_ascii_case("raw") {
        "tcp".to_string()
    } else {
        network.to_string()
    }
}

/// Python `_stream_share_params`.
fn stream_share_params(stream: &Map<String, Value>) -> Vec<(String, String)> {
    let mut params: Vec<(String, String)> = Vec::new();
    set_param(&mut params, "type", share_network(stream));
    let security = {
        let text = text_of(stream.get("security"));
        if text.is_empty() {
            "none"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    if security != "none" {
        set_param(&mut params, "security", &security);
    }
    let tls = obj_or_empty(stream.get("tlsSettings"));
    let reality = obj_or_empty(stream.get("realitySettings"));
    let payload = if security == "reality" {
        &reality
    } else {
        &tls
    };
    set_param(&mut params, "sni", text_of(payload.get("serverName")));
    set_param(&mut params, "fp", text_of(payload.get("fingerprint")));
    set_param(&mut params, "pcs", text_of(tls.get("pinnedPeerCertSha256")));
    let alpn = tls.get("alpn");
    set_param(
        &mut params,
        "alpn",
        match alpn {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    if let Some(Value::Bool(allow)) = tls.get("allowInsecure") {
        set_param(&mut params, "allowInsecure", if *allow { "1" } else { "0" });
    }
    let network = {
        let text = text_of(stream.get("network"));
        if text.is_empty() {
            "tcp"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    match network.as_str() {
        "ws" => {
            let ws = obj_or_empty(stream.get("wsSettings"));
            set_param(&mut params, "path", text_of(ws.get("path")));
            let headers = obj_or_empty(ws.get("headers"));
            let host = if headers.get("Host").is_some() {
                text_of(headers.get("Host"))
            } else {
                text_of(headers.get("host"))
            };
            set_param(&mut params, "host", host);
        }
        "http" | "h2" => {
            let http = obj_or_empty(stream.get("httpSettings"));
            set_param(&mut params, "path", text_of(http.get("path")));
            let host = http.get("host");
            set_param(
                &mut params,
                "host",
                match host {
                    Some(Value::Array(items)) => items
                        .iter()
                        .map(value_to_text)
                        .collect::<Vec<_>>()
                        .join(","),
                    other => text_of(other),
                },
            );
        }
        "grpc" => {
            let grpc = obj_or_empty(stream.get("grpcSettings"));
            set_param(&mut params, "serviceName", text_of(grpc.get("serviceName")));
        }
        "xhttp" => {
            let xhttp = obj_or_empty(stream.get("xhttpSettings"));
            set_param(&mut params, "path", text_of(xhttp.get("path")));
            set_param(&mut params, "host", text_of(xhttp.get("host")));
            set_param(&mut params, "mode", text_of(xhttp.get("mode")));
            let extra = xhttp.get("extra");
            // Python: `extra not in (None, "")`; a JSON object is kept.
            if let Some(extra) = extra
                && !extra.is_null()
                && extra != ""
            {
                set_param(
                    &mut params,
                    "extra",
                    if extra.is_string() {
                        value_to_text(extra)
                    } else {
                        compact_json(extra)
                    },
                );
            }
        }
        "httpupgrade" => {
            let httpupgrade = obj_or_empty(stream.get("httpupgradeSettings"));
            set_param(&mut params, "path", text_of(httpupgrade.get("path")));
            set_param(&mut params, "host", text_of(httpupgrade.get("host")));
        }
        "kcp" | "mkcp" => {
            let kcp = obj_or_empty(stream.get("kcpSettings"));
            let header = obj_or_empty(kcp.get("header"));
            let header_type = text_of(header.get("type"));
            set_param(
                &mut params,
                "headerType",
                if header_type.is_empty() {
                    "none"
                } else {
                    header_type.as_str()
                },
            );
            for key in [
                "mtu",
                "tti",
                "uplinkCapacity",
                "downlinkCapacity",
                "readBufferSize",
                "writeBufferSize",
                "seed",
            ] {
                set_param(
                    &mut params,
                    key,
                    value_to_text(kcp.get(key).unwrap_or(&Value::Null)),
                );
            }
            if kcp.get("congestion") == Some(&Value::Bool(true)) {
                set_param(&mut params, "congestion", "1");
            }
        }
        _ => {}
    }
    if security == "reality" {
        set_param(&mut params, "pbk", text_of(reality.get("publicKey")));
        set_param(&mut params, "sid", text_of(reality.get("shortId")));
        set_param(&mut params, "spx", text_of(reality.get("spiderX")));
        set_param(&mut params, "pqv", text_of(reality.get("mldsa65Verify")));
    }
    let finalmask = stream.get("finalmask");
    if let Some(finalmask) = finalmask
        && !finalmask.is_null()
        && !value_to_text(finalmask).is_empty()
    {
        set_param(
            &mut params,
            "fm",
            if finalmask.is_string() {
                value_to_text(finalmask)
            } else {
                compact_json(finalmask)
            },
        );
    }
    params
}

/// Python `_native_share_tls_params`.
fn native_share_tls_params(params: &mut Vec<(String, String)>, native: &Map<String, Value>) {
    let tls = obj_or_empty(native.get("tls"));
    set_param(params, "sni", text_of(tls.get("server_name")));
    let alpn = tls.get("alpn");
    set_param(
        params,
        "alpn",
        match alpn {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    if let Some(Value::Bool(insecure)) = tls.get("insecure") {
        set_param(params, "insecure", if *insecure { "1" } else { "0" });
    }
}

/// Python `_native_share_transport_params` — convert sing-box transport/TLS
/// fields to the common share URI query.
fn native_share_transport_params(native: &Map<String, Value>) -> Vec<(String, String)> {
    let transport = obj_or_empty(native.get("transport"));
    let transport_type = {
        let text = text_of(transport.get("type"));
        if text.is_empty() {
            "tcp"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    let mut params: Vec<(String, String)> = Vec::new();
    set_param(
        &mut params,
        "type",
        if transport_type.is_empty() || transport_type == "raw" {
            "tcp"
        } else {
            transport_type.as_str()
        },
    );
    set_param(&mut params, "path", text_of(transport.get("path")));
    let host = transport.get("host");
    if let Some(Value::Array(items)) = host {
        set_param(
            &mut params,
            "host",
            items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
        );
    } else {
        set_param(&mut params, "host", text_of(host));
    }
    let headers = obj_or_empty(transport.get("headers"));
    let header_host = if headers.get("Host").is_some() {
        text_of(headers.get("Host"))
    } else {
        text_of(headers.get("host"))
    };
    if header_host.is_empty() {
        let existing = params
            .iter()
            .find(|(key, _)| key == "host")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        set_param(&mut params, "host", existing);
    } else {
        set_param(&mut params, "host", header_host);
    }
    let service_name = if text_of(transport.get("service_name")).is_empty() {
        text_of(transport.get("serviceName"))
    } else {
        text_of(transport.get("service_name"))
    };
    set_param(&mut params, "serviceName", service_name);
    set_param(&mut params, "mode", text_of(transport.get("mode")));

    let tls = obj_or_empty(native.get("tls"));
    let reality = obj_or_empty(tls.get("reality"));
    let reality_enabled = reality
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if reality_enabled || !reality.is_empty() {
        set_param(&mut params, "security", "reality");
        let pbk = if text_of(reality.get("public_key")).is_empty() {
            text_of(reality.get("publicKey"))
        } else {
            text_of(reality.get("public_key"))
        };
        set_param(&mut params, "pbk", pbk);
        let sid = if text_of(reality.get("short_id")).is_empty() {
            text_of(reality.get("shortId"))
        } else {
            text_of(reality.get("short_id"))
        };
        set_param(&mut params, "sid", sid);
        let spx = if text_of(reality.get("spider_x")).is_empty() {
            text_of(reality.get("spiderX"))
        } else {
            text_of(reality.get("spider_x"))
        };
        set_param(&mut params, "spx", spx);
    } else if tls.get("enabled") != Some(&Value::Bool(false)) && !tls.is_empty() {
        set_param(&mut params, "security", "tls");
    }
    let sni = if text_of(tls.get("server_name")).is_empty() {
        text_of(tls.get("serverName"))
    } else {
        text_of(tls.get("server_name"))
    };
    set_param(&mut params, "sni", sni);
    let alpn = tls.get("alpn");
    set_param(
        &mut params,
        "alpn",
        match alpn {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => value_to_text(other.unwrap_or(&Value::Null)),
        },
    );
    if let Some(Value::Bool(insecure)) = tls.get("insecure") {
        set_param(&mut params, "insecure", if *insecure { "1" } else { "0" });
    }
    params
}

/// Python `_native_payload`.
fn native_payload(outbound: &Map<String, Value>) -> Map<String, Value> {
    obj_or_empty(outbound.get("singbox"))
}

/// `query: None` for the Python builders without a query part (socks,
/// http); the others always write `?`, even before an empty query. Likewise
/// `userinfo: Some` always writes the `@` (Python `f"{auth}@..."`), `None`
/// writes none (credentials that are only present with a username).
fn build_link(
    scheme: &str,
    userinfo: Option<&str>,
    authority: &str,
    query: Option<&str>,
    fragment: &str,
) -> String {
    let query = query.map(|query| format!("?{query}")).unwrap_or_default();
    let userinfo = userinfo.map(|userinfo| format!("{userinfo}@")).unwrap_or_default();
    format!("{scheme}://{userinfo}{authority}{query}#{fragment}")
}

/// Authority for the standard `host:port` schemes.
fn host_authority(host: &str, port: i64) -> String {
    format!("{host}:{port}")
}

/// Python `_build_vless_link`.
fn build_vless_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let fragment = quote(name);
    if !native.is_empty() {
        let mut params = native_share_transport_params(&native);
        let encryption = text_of(native.get("encryption"));
        set_param(
            &mut params,
            "encryption",
            if encryption.is_empty() {
                "none"
            } else {
                encryption.as_str()
            },
        );
        set_param(&mut params, "flow", text_of(native.get("flow")));
        let user_id = if text_of(native.get("uuid")).is_empty() {
            text_of(native.get("id"))
        } else {
            text_of(native.get("uuid"))
        };
        let host = native
            .get("server")
            .filter(|value| !value_to_text(value).is_empty())
            .map(share_host)
            .unwrap_or_else(|| share_host(&json!(server)));
        let server_port = py_int_or(native.get("server_port"), port);
        return build_link(
            "vless",
            Some(&quote(&user_id)),
            &host_authority(&host, server_port),
            Some(&share_query(&params)),
            &fragment,
        );
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let vnext = first_dict(settings.get("vnext").unwrap_or(&Value::Null));
    let user = first_dict(vnext.get("users").unwrap_or(&Value::Null));
    let user_id = text_of(user.get("id"));
    let stream = obj_or_empty(outbound.get("streamSettings"));
    let mut params: Vec<(String, String)> = Vec::new();
    let encryption = text_of(user.get("encryption"));
    set_param(
        &mut params,
        "encryption",
        if encryption.is_empty() {
            "none"
        } else {
            encryption.as_str()
        },
    );
    set_param(&mut params, "type", share_network(&stream));
    set_param(&mut params, "flow", text_of(user.get("flow")));
    for (key, value) in stream_share_params(&stream) {
        update_param(&mut params, &key, value);
    }
    build_link(
        "vless",
        Some(&quote(&user_id)),
        &host_authority(&share_host(&json!(server)), port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_vmess_link`.
fn build_vmess_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    if !native.is_empty() {
        let params = native_share_transport_params(&native);
        let get = |key: &str| {
            params
                .iter()
                .find(|(param_key, _)| param_key == key)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        let payload = json!({
            "v": "2",
            "ps": name,
            "add": if text_of(native.get("server")).is_empty() { server.to_owned() } else { text_of(native.get("server")) },
            "port": py_int_or(native.get("server_port"), port).to_string(),
            "id": if text_of(native.get("uuid")).is_empty() { text_of(native.get("id")) } else { text_of(native.get("uuid")) },
            "aid": ({
                let alter = value_int(native.get("alter_id").unwrap_or(&Value::Null), 0);
                if alter != 0 { alter } else { value_int(native.get("alterId").unwrap_or(&Value::Null), 0) }
            }).to_string(),
            "scy": if text_of(native.get("security")).is_empty() { "auto".to_owned() } else { text_of(native.get("security")) },
            "net": if get("type").is_empty() { "tcp".to_string() } else { get("type") },
            "type": if get("headerType").is_empty() { "none".to_string() } else { get("headerType") },
            "host": get("host"),
            "path": if get("path").is_empty() { get("serviceName") } else { get("path") },
            "tls": if get("security") == "tls" || get("security") == "reality" { "tls".to_string() } else { String::new() },
            "sni": get("sni"),
            "alpn": get("alpn"),
        });
        // Python json.dumps(..., ensure_ascii=False, separators=(",", ":")).
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(compact_json(&payload).as_bytes());
        return format!("vmess://{encoded}");
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let vnext = first_dict(settings.get("vnext").unwrap_or(&Value::Null));
    let user = first_dict(vnext.get("users").unwrap_or(&Value::Null));
    let stream = obj_or_empty(outbound.get("streamSettings"));
    let params = stream_share_params(&stream);
    let get = |key: &str| {
        params
            .iter()
            .find(|(param_key, _)| param_key == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let security = {
        let text = text_of(stream.get("security"));
        if text.is_empty() {
            "none"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    let mut payload = Map::new();
    payload.insert("v".into(), json!("2"));
    payload.insert("ps".into(), json!(name));
    payload.insert("add".into(), json!(server));
    payload.insert("port".into(), json!(port.to_string()));
    payload.insert("id".into(), json!(text_of(user.get("id"))));
    let alter_id = text_of(user.get("alterId"));
    payload.insert(
        "aid".into(),
        json!(if alter_id.is_empty() {
            "0".to_string()
        } else {
            alter_id
        }),
    );
    let security_field = text_of(user.get("security"));
    payload.insert(
        "scy".into(),
        json!(if security_field.is_empty() {
            "auto"
        } else {
            security_field.as_str()
        }),
    );
    payload.insert(
        "net".into(),
        json!(if get("type").is_empty() {
            "tcp".to_string()
        } else {
            get("type")
        }),
    );
    payload.insert(
        "type".into(),
        json!(if get("headerType").is_empty() {
            "none".to_string()
        } else {
            get("headerType")
        }),
    );
    payload.insert("host".into(), json!(get("host")));
    payload.insert(
        "path".into(),
        json!(if get("path").is_empty() {
            get("serviceName")
        } else {
            get("path")
        }),
    );
    payload.insert(
        "tls".into(),
        json!(if security == "tls" || security == "reality" {
            "tls"
        } else {
            ""
        }),
    );
    payload.insert("sni".into(), json!(get("sni")));
    payload.insert("alpn".into(), json!(get("alpn")));
    payload.insert("fp".into(), json!(get("fp")));
    let service_name = get("serviceName");
    if !service_name.is_empty() {
        payload.insert("serviceName".into(), json!(service_name));
    }
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(compact_json(&Value::Object(payload)).as_bytes());
    format!("vmess://{encoded}")
}

/// Python `_build_trojan_link`.
fn build_trojan_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let fragment = quote(name);
    if !native.is_empty() {
        let mut params = native_share_transport_params(&native);
        // Trojan URI parsers default to TLS; preserve an explicitly disabled
        // native TLS block as ``security=none``.
        if !params.iter().any(|(key, _)| key == "security") {
            set_param(&mut params, "security", "none");
        }
        let password = quote(&value_to_text(
            native.get("password").unwrap_or(&Value::Null),
        ));
        let host = native
            .get("server")
            .filter(|value| !value_to_text(value).is_empty())
            .map(share_host)
            .unwrap_or_else(|| share_host(&json!(server)));
        let server_port = py_int_or(native.get("server_port"), port);
        return build_link(
            "trojan",
            Some(&password),
            &host_authority(&host, server_port),
            Some(&share_query(&params)),
            &fragment,
        );
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
    let stream = obj_or_empty(outbound.get("streamSettings"));
    let mut params = stream_share_params(&stream);
    // trojan defaults to TLS on import, so an explicit "none" must survive the round trip.
    if !params.iter().any(|(key, _)| key == "security") {
        set_param(&mut params, "security", "none");
    }
    let password = quote(&value_to_text(
        server_item.get("password").unwrap_or(&Value::Null),
    ));
    build_link(
        "trojan",
        Some(&password),
        &host_authority(&share_host(&json!(server)), port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_shadowsocks_link`.
fn build_shadowsocks_link(
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> String {
    let userinfo = |method: &Value, password: &Value| -> String {
        let method_text = value_to_text(method).trim().to_string();
        let password_text = value_to_text(password);
        if method_text.to_lowercase().starts_with("2022-") {
            return format!("{}:{}", quote(&method_text), quote(&password_text));
        }
        let credentials = format!("{method_text}:{password_text}");
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(credentials.as_bytes())
    };
    let query_suffix = |params: &[(String, String)]| -> String {
        let query = share_query(params);
        if query.is_empty() {
            String::new()
        } else {
            format!("/?{query}")
        }
    };
    let native = native_payload(outbound);
    let fragment = quote(name);
    if !native.is_empty() {
        let mut params: Vec<(String, String)> = Vec::new();
        let plugin = value_to_text(native.get("plugin").unwrap_or(&Value::Null));
        let plugin_opts = value_to_text(native.get("plugin_opts").unwrap_or(&Value::Null));
        let joined = [plugin.as_str(), plugin_opts.as_str()]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(";");
        set_param(&mut params, "plugin", joined);
        let credentials = userinfo(
            native.get("method").unwrap_or(&Value::Null),
            native.get("password").unwrap_or(&Value::Null),
        );
        let host = native
            .get("server")
            .filter(|value| !value_to_text(value).is_empty())
            .map(share_host)
            .unwrap_or_else(|| share_host(&json!(server)));
        let server_port = py_int_or(native.get("server_port"), port);
        return format!(
            "ss://{}@{}:{}{}#{}",
            credentials,
            host,
            server_port,
            query_suffix(&params),
            fragment
        );
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
    let mut params: Vec<(String, String)> = Vec::new();
    let plugin = value_to_text(server_item.get("plugin").unwrap_or(&Value::Null));
    let plugin_opts = value_to_text(server_item.get("plugin_opts").unwrap_or(&Value::Null));
    let joined = [plugin.as_str(), plugin_opts.as_str()]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(";");
    set_param(&mut params, "plugin", joined);
    let credentials = userinfo(
        server_item.get("method").unwrap_or(&Value::Null),
        server_item.get("password").unwrap_or(&Value::Null),
    );
    format!(
        "ss://{}@{}:{}{}#{}",
        credentials,
        share_host(&json!(server)),
        port,
        query_suffix(&params),
        fragment
    )
}

/// Python `_build_socks_link`.
fn build_socks_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let fragment = quote(name);
    if !native.is_empty() {
        let username = value_to_text(native.get("username").unwrap_or(&Value::Null));
        let password = value_to_text(native.get("password").unwrap_or(&Value::Null));
        let credentials = if username.is_empty() {
            String::new()
        } else {
            format!("{}:{}", quote(&username), quote(&password))
        };
        let host = native
            .get("server")
            .filter(|value| !value_to_text(value).is_empty())
            .map(share_host)
            .unwrap_or_else(|| share_host(&json!(server)));
        let server_port = py_int_or(native.get("server_port"), port);
        return build_link(
            "socks",
            Some(&credentials).map(|userinfo| userinfo.as_ref()).filter(|userinfo: &&str| !userinfo.is_empty()),
            &host_authority(&host, server_port),
            None,
            &fragment,
        );
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
    let user = first_dict(server_item.get("users").unwrap_or(&Value::Null));
    let username = value_to_text(user.get("user").unwrap_or(&Value::Null));
    let credentials = if username.is_empty() {
        String::new()
    } else {
        format!(
            "{}:{}@",
            quote(&username),
            quote(&value_to_text(user.get("pass").unwrap_or(&Value::Null)))
        )
    };
    build_link(
        "socks",
        Some(credentials.trim_end_matches('@')).map(|userinfo| userinfo.as_ref()).filter(|userinfo: &&str| !userinfo.is_empty()),
        &host_authority(&share_host(&json!(server)), port),
        None,
        &fragment,
    )
}

/// Python `_build_http_link`.
fn build_http_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let fragment = quote(name);
    let credentials = |user: &Value, pass: &Value| -> String {
        let username = value_to_text(user);
        if username.is_empty() {
            String::new()
        } else {
            format!("{}:{}", quote(&username), quote(&value_to_text(pass)))
        }
    };
    if !native.is_empty() {
        let host = native
            .get("server")
            .filter(|value| !value_to_text(value).is_empty())
            .map(share_host)
            .unwrap_or_else(|| share_host(&json!(server)));
        let server_port = py_int_or(native.get("server_port"), port);
        return build_link(
            "http",
            Some(&credentials(
                native.get("username").unwrap_or(&Value::Null),
                native.get("password").unwrap_or(&Value::Null),
            )).map(|userinfo| userinfo.as_ref()).filter(|userinfo: &&str| !userinfo.is_empty()),
            &host_authority(&host, server_port),
            None,
            &fragment,
        );
    }
    let settings = obj_or_empty(outbound.get("settings"));
    let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
    let user = first_dict(server_item.get("users").unwrap_or(&Value::Null));
    build_link(
        "http",
        Some(&credentials(
            user.get("user").unwrap_or(&Value::Null),
            user.get("pass").unwrap_or(&Value::Null),
        )).map(|userinfo| userinfo.as_ref()).filter(|userinfo: &&str| !userinfo.is_empty()),
        &host_authority(&share_host(&json!(server)), port),
        None,
        &fragment,
    )
}

/// Python `_build_hysteria_link`.
fn build_hysteria_link(
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> String {
    let native = native_payload(outbound);
    let mut params: Vec<(String, String)> = Vec::new();
    native_share_tls_params(&mut params, &native);
    set_param(&mut params, "protocol", text_of(native.get("protocol")));
    set_param(&mut params, "upmbps", text_of(native.get("up_mbps")));
    set_param(&mut params, "downmbps", text_of(native.get("down_mbps")));
    let ports = native.get("server_ports");
    set_param(
        &mut params,
        "server_ports",
        match ports {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    set_param(
        &mut params,
        "hop_interval",
        text_of(native.get("hop_interval")),
    );
    for key in ["recv_window_conn", "recv_window", "disable_mtu_discovery"] {
        let value = native.get(key).unwrap_or(&Value::Null);
        if key == "disable_mtu_discovery" {
            if value == &Value::Bool(true) {
                set_param(&mut params, key, "1");
            }
        } else {
            set_param(&mut params, key, value_to_text(value));
        }
    }
    let obfs = native.get("obfs");
    if let Some(obfs_obj) = obfs.filter(|value| value.is_object()) {
        let obfs = obfs_obj.as_object().unwrap();
        set_param(&mut params, "obfs", text_of(obfs.get("type")));
        set_param(&mut params, "obfs-password", text_of(obfs.get("password")));
    } else if let Some(obfs) = obfs
        && !value_to_text(obfs).is_empty()
    {
        set_param(&mut params, "obfs", value_to_text(obfs));
    }
    let fragment = quote(name);
    let auth_source = if text_of(native.get("auth_str")).is_empty() {
        text_of(native.get("password"))
    } else {
        text_of(native.get("auth_str"))
    };
    let auth = quote(&auth_source);
    let host = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    let server_port = py_int_or(native.get("server_port"), port);
    build_link(
        "hysteria",
        Some(&auth),
        &host_authority(&host, server_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_hysteria2_link`.
fn build_hysteria2_link(
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> String {
    let native = native_payload(outbound);
    let mut params: Vec<(String, String)> = Vec::new();
    native_share_tls_params(&mut params, &native);
    if let Some(obfs) = native.get("obfs").filter(|value| value.is_object()) {
        let obfs = obfs.as_object().unwrap();
        set_param(&mut params, "obfs", text_of(obfs.get("type")));
        set_param(&mut params, "obfs-password", text_of(obfs.get("password")));
    }
    let ports = native.get("server_ports");
    set_param(
        &mut params,
        "server_ports",
        match ports {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    set_param(
        &mut params,
        "hop_interval",
        text_of(native.get("hop_interval")),
    );
    set_param(&mut params, "upmbps", text_of(native.get("up_mbps")));
    set_param(&mut params, "downmbps", text_of(native.get("down_mbps")));
    for key in ["recv_window_conn", "recv_window", "disable_mtu_discovery"] {
        let value = native.get(key).unwrap_or(&Value::Null);
        if key == "disable_mtu_discovery" {
            if value == &Value::Bool(true) {
                set_param(&mut params, key, "1");
            }
        } else {
            set_param(&mut params, key, value_to_text(value));
        }
    }
    let fragment = quote(name);
    let password = quote(&value_to_text(
        native.get("password").unwrap_or(&Value::Null),
    ));
    let host = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    let server_port = py_int_or(native.get("server_port"), port);
    build_link(
        "hysteria2",
        Some(&password),
        &host_authority(&host, server_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_tuic_link`.
fn build_tuic_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let mut params: Vec<(String, String)> = Vec::new();
    set_param(
        &mut params,
        "congestion_control",
        text_of(native.get("congestion_control")),
    );
    set_param(
        &mut params,
        "udp_relay_mode",
        text_of(native.get("udp_relay_mode")),
    );
    if native.get("zero_rtt_handshake") == Some(&Value::Bool(true)) {
        set_param(&mut params, "zero_rtt_handshake", "1");
    }
    native_share_tls_params(&mut params, &native);
    let fragment = quote(name);
    let mut credentials = quote(&value_to_text(native.get("uuid").unwrap_or(&Value::Null)));
    let password = value_to_text(native.get("password").unwrap_or(&Value::Null));
    if !password.is_empty() {
        credentials = format!("{credentials}:{}", quote(&password));
    }
    let host = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    let server_port = py_int_or(native.get("server_port"), port);
    build_link(
        "tuic",
        Some(&credentials),
        &host_authority(&host, server_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_mieru_link`.
fn build_mieru_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let username = quote(&value_to_text(
        native.get("username").unwrap_or(&Value::Null),
    ));
    let password = quote(&value_to_text(
        native.get("password").unwrap_or(&Value::Null),
    ));
    let mut params: Vec<(String, String)> = Vec::new();
    let transport = text_of(native.get("transport"));
    set_param(
        &mut params,
        "transport",
        if transport.is_empty() {
            "TCP"
        } else {
            transport.as_str()
        }
        .to_uppercase(),
    );
    let ports = native.get("server_ports");
    if let Some(Value::Array(items)) = ports {
        set_param(
            &mut params,
            "server_ports",
            items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
        );
    } else {
        set_param(&mut params, "server_ports", text_of(ports));
    }
    set_param(
        &mut params,
        "multiplexing",
        text_of(native.get("multiplexing")),
    );
    set_param(
        &mut params,
        "traffic_pattern",
        text_of(native.get("traffic_pattern")),
    );
    let fragment = quote(name);
    let authority_port = py_int_or(native.get("server_port"), port);
    let mut authority = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    if authority_port != 0 {
        authority = format!("{authority}:{authority_port}");
    }
    // Python always writes `?`, even before an empty query.
    let query = format!("?{}", share_query(&params));
    format!("mieru://{username}:{password}@{authority}{query}#{fragment}")
}

/// Python `_build_naive_link`.
fn build_naive_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let scheme = if native.get("quic") == Some(&Value::Bool(true)) {
        "naive+quic"
    } else {
        "naive"
    };
    let username = quote(&value_to_text(
        native.get("username").unwrap_or(&Value::Null),
    ));
    let password = quote(&value_to_text(
        native.get("password").unwrap_or(&Value::Null),
    ));
    // `build_link` adds the `@` after the credentials.
    let credentials = if username.is_empty() && password.is_empty() {
        String::new()
    } else {
        format!("{username}:{password}")
    };
    let mut params: Vec<(String, String)> = Vec::new();
    let tls = obj_or_empty(native.get("tls"));
    set_param(&mut params, "sni", text_of(tls.get("server_name")));
    set_param(
        &mut params,
        "insecure_concurrency",
        text_of(native.get("insecure_concurrency")),
    );
    set_param(
        &mut params,
        "quic_congestion_control",
        text_of(native.get("quic_congestion_control")),
    );
    let udp_over_tcp = native.get("udp_over_tcp");
    let udp_skipped = matches!(udp_over_tcp, None | Some(Value::Null))
        || udp_over_tcp == Some(&Value::Bool(false))
        || udp_over_tcp == Some(&json!(""));
    if !udp_skipped {
        let value = udp_over_tcp.unwrap();
        set_param(
            &mut params,
            "udp_over_tcp",
            if value.is_string() {
                value_to_text(value)
            } else {
                compact_json(value)
            },
        );
    }
    if let Some(extra_headers) = native
        .get("extra_headers")
        .filter(|value| value.is_object())
    {
        set_param(&mut params, "extra_headers", compact_json(extra_headers));
    }
    let fragment = quote(name);
    let host = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    let server_port = py_int_or(
        native.get("server_port"),
        if port != 0 { port } else { 443 },
    );
    build_link(
        scheme,
        Some(&credentials).map(|userinfo| userinfo.as_ref()).filter(|userinfo: &&str| !userinfo.is_empty()),
        &host_authority(&host, server_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_anytls_link`.
fn build_anytls_link(name: &str, server: &str, port: i64, outbound: &Map<String, Value>) -> String {
    let native = native_payload(outbound);
    let mut params: Vec<(String, String)> = Vec::new();
    let tls = obj_or_empty(native.get("tls"));
    set_param(&mut params, "sni", text_of(tls.get("server_name")));
    if tls.get("insecure") == Some(&Value::Bool(true)) {
        set_param(&mut params, "insecure", "1");
    }
    let alpn = tls.get("alpn");
    set_param(
        &mut params,
        "alpn",
        match alpn {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => value_to_text(other.unwrap_or(&Value::Null)),
        },
    );
    let pins = tls.get("certificate_public_key_sha256");
    set_param(
        &mut params,
        "pinsha256",
        match pins {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    for key in [
        "idle_session_check_interval",
        "idle_session_timeout",
        "min_idle_session",
    ] {
        set_param(&mut params, key, text_of(native.get(key)));
    }
    let fragment = quote(name);
    let password = quote(&value_to_text(
        native.get("password").unwrap_or(&Value::Null),
    ));
    let host = native
        .get("server")
        .filter(|value| !value_to_text(value).is_empty())
        .map(share_host)
        .unwrap_or_else(|| share_host(&json!(server)));
    let server_port = py_int_or(
        native.get("server_port"),
        if port != 0 { port } else { 443 },
    );
    build_link(
        "anytls",
        Some(&password),
        &host_authority(&host, server_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_wireguard_link` (also serves awg/warp).
fn build_wireguard_link(
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> String {
    let native = native_payload(outbound);
    let protocol = {
        let text = text_of(outbound.get("protocol"));
        if text.is_empty() {
            "wireguard"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    let native_type = {
        let text = text_of(native.get("type"));
        if text.is_empty() {
            protocol.as_str()
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    let fragment = quote(name);
    let join_list = |value: Option<&Value>| -> String {
        match value {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        }
    };
    let join_map = |map: &Map<String, Value>, params: &mut Vec<(String, String)>| {
        for (key, value) in map {
            set_param(params, key, value_to_text(value));
        }
    };
    if native_type == "warp" || protocol == "warp" {
        let scheme =
            if protocol == "awg" || native.get("amnezia").map(Value::is_object).unwrap_or(false) {
                "awg"
            } else {
                "warp"
            };
        let profile = obj_or_empty(native.get("profile"));
        let mut params: Vec<(String, String)> = Vec::new();
        for key in [
            "id",
            "private_key",
            "auth_token",
            "listen_port",
            "persistent_keepalive_interval",
            "udp_timeout",
        ] {
            let value = if profile.get(key).is_some() {
                profile.get(key).unwrap()
            } else {
                native.get(key).unwrap_or(&Value::Null)
            };
            set_param(&mut params, key, value_to_text(value));
        }
        if let Some(amnezia) = native.get("amnezia").filter(|value| value.is_object()) {
            join_map(amnezia.as_object().unwrap(), &mut params);
        }
        let dns = outbound.get("_dns");
        set_param(&mut params, "dns", join_list(dns));
        return build_link(
            scheme,
            None,
            "engage.cloudflareclient.com:2408",
            Some(&share_query(&params)),
            &fragment,
        );
    }

    let peers = native
        .get("peers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let peer = peers
        .first()
        .filter(|value| value.is_object())
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let endpoint_server = if !text_of(peer.get("address")).is_empty() {
        text_of(peer.get("address"))
    } else if !text_of(peer.get("server")).is_empty() {
        text_of(peer.get("server"))
    } else if !text_of(native.get("server")).is_empty() {
        text_of(native.get("server"))
    } else {
        server.to_string()
    };
    let endpoint_port = {
        let candidate = py_int_or(peer.get("port").or_else(|| peer.get("server_port")), 0);
        let candidate = if candidate != 0 {
            candidate
        } else {
            py_int_or(native.get("server_port"), 0)
        };
        if candidate != 0 {
            candidate
        } else if port != 0 {
            port
        } else {
            51820
        }
    };
    let mut params: Vec<(String, String)> = Vec::new();
    let public_key = if text_of(peer.get("public_key")).is_empty() {
        text_of(peer.get("publicKey"))
    } else {
        text_of(peer.get("public_key"))
    };
    set_param(&mut params, "public_key", public_key);
    let addresses = if native.get("address").is_some() {
        native.get("address")
    } else {
        native.get("addresses")
    };
    set_param(&mut params, "address", join_list(addresses));
    let allowed = if peer.get("allowed_ips").is_some() {
        peer.get("allowed_ips")
    } else if peer.get("allowedIPs").is_some() {
        peer.get("allowedIPs")
    } else {
        native.get("allowed_ips")
    };
    set_param(&mut params, "allowed_ips", join_list(allowed));
    let pre_shared = if text_of(peer.get("pre_shared_key")).is_empty() {
        text_of(peer.get("preshared_key"))
    } else {
        text_of(peer.get("pre_shared_key"))
    };
    set_param(&mut params, "pre_shared_key", pre_shared);
    let keepalive = if text_of(peer.get("persistent_keepalive_interval")).is_empty() {
        text_of(peer.get("persistent_keepalive"))
    } else {
        text_of(peer.get("persistent_keepalive_interval"))
    };
    set_param(&mut params, "persistent_keepalive", keepalive);
    for key in [
        "mtu",
        "listen_port",
        "udp_timeout",
        "workers",
        "preallocated_buffers_per_pool",
        "disable_pauses",
    ] {
        set_param(&mut params, key, text_of(native.get(key)));
    }
    let dns = outbound.get("_dns");
    set_param(&mut params, "dns", join_list(dns));
    let amnezia_obj = native
        .get("amnezia")
        .filter(|value| value.is_object())
        .map(Value::as_object)
        .flatten();
    if let Some(amnezia) = amnezia_obj {
        join_map(amnezia, &mut params);
    }
    let private_key = quote(&value_to_text(
        native.get("private_key").unwrap_or(&Value::Null),
    ));
    let scheme = if protocol == "awg" || amnezia_obj.is_some() {
        "awg"
    } else {
        "wireguard"
    };
    build_link(
        scheme,
        Some(&private_key),
        &host_authority(&share_host(&json!(endpoint_server)), endpoint_port),
        Some(&share_query(&params)),
        &fragment,
    )
}

/// Python `_build_masque_link`.
fn build_masque_link(
    name: &str,
    server: &str,
    _port: i64,
    outbound: &Map<String, Value>,
) -> String {
    let native = native_payload(outbound);
    let profile = obj_or_empty(native.get("profile"));
    let profile_id = {
        let id = text_of(profile.get("id"));
        if id.is_empty() {
            server.to_string()
        } else {
            id
        }
    };
    let token = quote(&value_to_text(
        profile.get("auth_token").unwrap_or(&Value::Null),
    ));
    let mut params: Vec<(String, String)> = Vec::new();
    let native_name = text_of(native.get("name"));
    set_param(
        &mut params,
        "name",
        if native_name.is_empty() {
            "masque0"
        } else {
            native_name.as_str()
        },
    );
    set_param(
        &mut params,
        "system",
        if native.get("system") == Some(&Value::Bool(true)) {
            "true"
        } else {
            "false"
        },
    );
    set_param(
        &mut params,
        "use_http2",
        if native.get("use_http2") == Some(&Value::Bool(true)) {
            "true"
        } else {
            "false"
        },
    );
    set_param(
        &mut params,
        "use_ipv6",
        if native.get("use_ipv6") == Some(&Value::Bool(true)) {
            "true"
        } else {
            "false"
        },
    );
    let allowed = native.get("allowed_ips");
    set_param(
        &mut params,
        "allowed_ips",
        match allowed {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .collect::<Vec<_>>()
                .join(","),
            other => text_of(other),
        },
    );
    let sni = if is_dict(native.get("tls")) {
        text_of(
            native
                .get("tls")
                .unwrap()
                .as_object()
                .unwrap()
                .get("server_name"),
        )
    } else {
        String::new()
    };
    set_param(&mut params, "sni", sni);
    if is_dict(native.get("tls"))
        && native
            .get("tls")
            .unwrap()
            .as_object()
            .unwrap()
            .get("insecure")
            == Some(&Value::Bool(true))
    {
        set_param(&mut params, "insecure", "true");
    }
    for key in [
        "udp_timeout",
        "udp_keepalive_period",
        "reconnect_delay",
        "congestion_controller",
    ] {
        set_param(&mut params, key, text_of(native.get(key)));
    }
    let fragment = quote(name);
    let userinfo = if token.is_empty() {
        String::new()
    } else {
        format!("{token}@")
    };
    // Python always writes `?`, even before an empty query.
    let query = format!("?{}", share_query(&params));
    format!(
        "masque://{userinfo}{}{query}#{fragment}",
        quote(&profile_id)
    )
}

/// Protocols with a portable share URI used by the node editor.  Keep this
/// compatibility set stable: native-only editor saves historically retained
/// canonical JSON in ``node.link``.
fn build_share_link(
    protocol: &str,
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> Option<String> {
    Some(match protocol {
        "vless" => build_vless_link(name, server, port, outbound),
        "vmess" => build_vmess_link(name, server, port, outbound),
        "trojan" => build_trojan_link(name, server, port, outbound),
        "shadowsocks" => build_shadowsocks_link(name, server, port, outbound),
        "socks" => build_socks_link(name, server, port, outbound),
        "hysteria2" => build_hysteria2_link(name, server, port, outbound),
        "tuic" => build_tuic_link(name, server, port, outbound),
        "http" => build_http_link(name, server, port, outbound),
        "hysteria" => build_hysteria_link(name, server, port, outbound),
        "mieru" => build_mieru_link(name, server, port, outbound),
        "naive" => build_naive_link(name, server, port, outbound),
        "anytls" => build_anytls_link(name, server, port, outbound),
        "wireguard" | "awg" | "warp" => build_wireguard_link(name, server, port, outbound),
        "masque" => build_masque_link(name, server, port, outbound),
        _ => return None,
    })
}

/// Python `_build_node_link` — share builders keep their URI form, everything
/// else falls back to compact JSON.
fn build_node_link(
    protocol: &str,
    name: &str,
    server: &str,
    port: i64,
    outbound: &Map<String, Value>,
) -> String {
    // Python `_SHARE_LINK_BUILDERS`: an editor save keeps a URI only for
    // these; the wider `_EXPORT_LINK_BUILDERS` set is for export.
    const SAVED_AS_URI: [&str; 7] = ["vless", "vmess", "trojan", "shadowsocks", "socks", "hysteria2", "tuic"];
    match SAVED_AS_URI
        .contains(&protocol)
        .then(|| build_share_link(protocol, name, server, port, outbound))
        .flatten()
    {
        Some(link) => link,
        None => compact_json(&Value::Object(outbound.clone())),
    }
}

/// Python `_share_identity_present` — whether a portable URI can be produced
/// without empty credentials.
fn share_identity_present(protocol: &str, outbound: &Map<String, Value>) -> bool {
    let native = native_payload(outbound);
    if !native.is_empty() {
        if protocol == "vless" || protocol == "vmess" {
            return !text_of(native.get("uuid")).is_empty()
                || !text_of(native.get("id")).is_empty();
        }
        if protocol == "trojan" {
            return !text_of(native.get("password")).is_empty();
        }
        if protocol == "shadowsocks" {
            let method = value_to_text(native.get("method").unwrap_or(&Value::Null))
                .trim()
                .to_lowercase();
            return !method.is_empty()
                && (!text_of(native.get("password")).is_empty() || method == "none");
        }
        if protocol == "hysteria" || protocol == "hysteria2" {
            return !text_of(native.get("auth_str")).is_empty()
                || !text_of(native.get("password")).is_empty();
        }
        if protocol == "tuic" {
            return !text_of(native.get("uuid")).is_empty();
        }
        if protocol == "mieru" {
            return !text_of(native.get("username")).is_empty()
                && !text_of(native.get("password")).is_empty();
        }
        if protocol == "anytls" {
            return !text_of(native.get("password")).is_empty();
        }
        if protocol == "wireguard" || protocol == "awg" {
            let peers = native
                .get("peers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let peer = peers
                .first()
                .filter(|value| value.is_object())
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let public_key = if text_of(peer.get("public_key")).is_empty() {
                text_of(peer.get("publicKey"))
            } else {
                text_of(peer.get("public_key"))
            };
            return !text_of(native.get("private_key")).is_empty() && !public_key.is_empty();
        }
        if protocol == "warp" {
            let profile = obj_or_empty(native.get("profile"));
            return !text_of(profile.get("id")).is_empty()
                || !text_of(profile.get("private_key")).is_empty()
                || !text_of(profile.get("auth_token")).is_empty();
        }
        if protocol == "masque" {
            let profile = obj_or_empty(native.get("profile"));
            // The URI form represents a registered profile.  Direct MASQUE
            // endpoints carry tunnel keys/addresses that the URI parser cannot
            // reconstruct, so retain those as JSON instead of dropping them.
            let direct_fields = [
                "private_key",
                "public_key",
                "address",
                "server",
                "server_port",
            ];
            let empty_marker = json!([]);
            let empty_obj = json!({});
            if direct_fields.iter().any(|key| {
                let value = native.get(*key).unwrap_or(&Value::Null);
                !(value.is_null()
                    || value == &json!("")
                    || value == &empty_marker
                    || value == &empty_obj)
            }) {
                return false;
            }
            if !text_of(profile.get("private_key")).is_empty() {
                return false;
            }
            return !text_of(profile.get("id")).is_empty();
        }
    }
    if protocol == "vless" || protocol == "vmess" {
        let settings = obj_or_empty(outbound.get("settings"));
        let vnext = first_dict(settings.get("vnext").unwrap_or(&Value::Null));
        let user = first_dict(vnext.get("users").unwrap_or(&Value::Null));
        return !text_of(user.get("id")).is_empty();
    }
    if protocol == "trojan" {
        let settings = obj_or_empty(outbound.get("settings"));
        let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
        return !text_of(server_item.get("password")).is_empty();
    }
    if protocol == "shadowsocks" {
        let settings = obj_or_empty(outbound.get("settings"));
        let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
        let method = value_to_text(server_item.get("method").unwrap_or(&Value::Null))
            .trim()
            .to_lowercase();
        return !method.is_empty()
            && (!text_of(server_item.get("password")).is_empty() || method == "none");
    }
    true
}

/// Python `normalized_node_export_link` — a portable, canonical share link for
/// a node when one exists.
pub fn normalized_node_export_link(node: Option<&Node>) -> String {
    let Some(node) = node else {
        return String::new();
    };
    let raw = node.link.trim().to_string();
    let outbound = obj_or_empty(if node.outbound.is_object() {
        Some(&node.outbound)
    } else {
        None
    });
    let protocol = protocol_from_node(node);
    let name = node.name.trim().to_string();
    let server = node.server.trim().to_string();
    let port = node.port;
    let native = native_payload(&outbound);
    let native_server = text_of(native.get("server"));
    let mut effective_server = if server.is_empty() {
        native_server
    } else {
        server.clone()
    };
    if effective_server.is_empty() && (protocol == "wireguard" || protocol == "awg") {
        let peers = native
            .get("peers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(peer) = peers
            .first()
            .filter(|value| value.is_object())
            .and_then(Value::as_object)
        {
            let address = if text_of(peer.get("address")).is_empty() {
                text_of(peer.get("server"))
            } else {
                text_of(peer.get("address"))
            };
            effective_server = address;
        }
    }
    let endpoint_available =
        protocol == "warp" || protocol == "masque" || !effective_server.is_empty();
    if endpoint_available && share_identity_present(&protocol, &outbound) {
        let candidate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            build_share_link(&protocol, &name, &server, port, &outbound)
        }))
        .unwrap_or(None);
        if let Some(candidate) = candidate {
            let candidate = candidate.trim().to_string();
            if !candidate.is_empty() && candidate.contains("://") && !candidate.starts_with('{') {
                return candidate;
            }
        }
    }
    if raw.starts_with('{') {
        if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
            return compact_json(&parsed);
        }
    }
    raw
}

// ── Editor field schema ─────────────────────────────────────────────────────

/// Python `_text_list`.
fn text_list(value: &Value) -> String {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .map(value_to_text)
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");
    }
    value_to_text(value)
}

/// Python `_json_field`.
fn json_field(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    if let Some(text) = value.as_str() {
        if text.is_empty() {
            return String::new();
        }
        return text.to_string();
    }
    compact_json(value)
}

/// Python `_editor_field`.
#[allow(clippy::too_many_arguments)]
fn editor_field(
    key: &str,
    label: &str,
    value: &Value,
    kind: &str,
    options: &[&str],
    secret: bool,
    placeholder: &str,
    when_key: &str,
    when_values: &[&str],
) -> Value {
    let mut result = json!({
        "key": key,
        "label": label,
        "value": if value.is_null() { Value::String(String::new()) } else { value.clone() },
        "kind": kind,
    });
    if !options.is_empty() {
        result["options"] = json!(options);
    }
    if secret {
        result["secret"] = json!(true);
    }
    if !placeholder.is_empty() {
        result["placeholder"] = json!(placeholder);
    }
    if !when_key.is_empty() {
        result["whenKey"] = json!(when_key);
        result["whenValues"] = json!(when_values);
    }
    result
}

/// Python `_endpoint_fields`.
fn endpoint_fields(values: &Map<String, Value>) -> Vec<Value> {
    vec![
        editor_field(
            "server",
            "Адрес",
            values.get("server").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "",
            &[],
        ),
        editor_field(
            "port",
            "Порт",
            values.get("port").unwrap_or(&json!("")),
            "number",
            &[],
            false,
            "",
            "",
            &[],
        ),
    ]
}

/// Python `_stream_editor_fields`.
fn stream_editor_fields(values: &Map<String, Value>) -> Vec<Value> {
    vec![
        editor_field(
            "network",
            "Транспорт",
            values.get("network").unwrap_or(&json!("tcp")),
            "combo",
            &NETWORKS,
            false,
            "",
            "",
            &[],
        ),
        editor_field(
            "transportPath",
            "Путь",
            values.get("transportPath").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "network",
            &["ws", "http", "h2", "xhttp", "httpupgrade"],
        ),
        editor_field(
            "transportHost",
            "Host",
            values.get("transportHost").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "network",
            &["ws", "http", "h2", "xhttp", "httpupgrade"],
        ),
        editor_field(
            "grpcServiceName",
            "gRPC service name",
            values.get("grpcServiceName").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "network",
            &["grpc"],
        ),
        editor_field(
            "xhttpMode",
            "XHTTP mode",
            values.get("xhttpMode").unwrap_or(&json!("auto")),
            "combo",
            &["auto", "packet-up", "stream-up", "stream-one"],
            false,
            "",
            "network",
            &["xhttp"],
        ),
        editor_field(
            "xhttpExtra",
            "XHTTP extra (JSON)",
            values.get("xhttpExtra").unwrap_or(&json!("")),
            "area",
            &[],
            false,
            "",
            "network",
            &["xhttp"],
        ),
        editor_field(
            "kcpHeader",
            "mKCP header",
            values.get("kcpHeader").unwrap_or(&json!("none")),
            "combo",
            &["none", "srtp", "utp", "wechat-video", "dtls", "wireguard"],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpSeed",
            "mKCP seed",
            values.get("kcpSeed").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpMtu",
            "mKCP MTU",
            values.get("kcpMtu").unwrap_or(&json!("")),
            "number",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpTti",
            "mKCP TTI",
            values.get("kcpTti").unwrap_or(&json!("")),
            "number",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpUplink",
            "mKCP upload",
            values.get("kcpUplink").unwrap_or(&json!("")),
            "number",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpDownlink",
            "mKCP download",
            values.get("kcpDownlink").unwrap_or(&json!("")),
            "number",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
        editor_field(
            "kcpCongestion",
            "mKCP congestion",
            values.get("kcpCongestion").unwrap_or(&json!(false)),
            "bool",
            &[],
            false,
            "",
            "network",
            &["kcp"],
        ),
    ]
}

/// Python `_stream_security_fields`.
fn stream_security_fields(values: &Map<String, Value>) -> Vec<Value> {
    let secure = ["tls", "reality"];
    vec![
        editor_field(
            "security",
            "Защита",
            values.get("security").unwrap_or(&json!("none")),
            "combo",
            &SECURITY,
            false,
            "",
            "",
            &[],
        ),
        editor_field(
            "sni",
            "SNI",
            values.get("sni").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "security",
            &secure,
        ),
        editor_field(
            "fingerprint",
            "Fingerprint",
            values.get("fingerprint").unwrap_or(&json!("")),
            "combo",
            &FINGERPRINTS,
            false,
            "",
            "security",
            &secure,
        ),
        editor_field(
            "allowInsecure",
            "Пропускать проверку сертификата",
            values.get("allowInsecure").unwrap_or(&json!(false)),
            "bool",
            &[],
            false,
            "",
            "security",
            &["tls"],
        ),
        editor_field(
            "alpn",
            "ALPN",
            values.get("alpn").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "h2, http/1.1",
            "security",
            &["tls"],
        ),
        editor_field(
            "pinnedPeerCertSha256",
            "TLS SHA-256 pin",
            values.get("pinnedPeerCertSha256").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "64 hex-символа",
            "security",
            &["tls"],
        ),
        editor_field(
            "publicKey",
            "REALITY public key",
            values.get("publicKey").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "security",
            &["reality"],
        ),
        editor_field(
            "shortId",
            "REALITY ShortId",
            values.get("shortId").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "security",
            &["reality"],
        ),
        editor_field(
            "spiderX",
            "REALITY SpiderX",
            values.get("spiderX").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "security",
            &["reality"],
        ),
        editor_field(
            "pqv",
            "ML-DSA-65 verify",
            values.get("pqv").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "security",
            &["reality"],
        ),
        editor_field(
            "finalmask",
            "Finalmask",
            values.get("finalmask").unwrap_or(&json!("")),
            "area",
            &[],
            false,
            "",
            "",
            &[],
        ),
    ]
}

/// Python `_native_tls_fields`.
fn native_tls_fields(values: &Map<String, Value>) -> Vec<Value> {
    vec![
        editor_field(
            "sni",
            "SNI",
            values.get("sni").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "",
            "",
            &[],
        ),
        editor_field(
            "allowInsecure",
            "Пропускать проверку сертификата",
            values.get("allowInsecure").unwrap_or(&json!(false)),
            "bool",
            &[],
            false,
            "",
            "",
            &[],
        ),
        editor_field(
            "alpn",
            "ALPN",
            values.get("alpn").unwrap_or(&json!("")),
            "text",
            &[],
            false,
            "h3, h2, http/1.1",
            "",
            &[],
        ),
    ]
}

/// Python `_amnezia_editor_fields`.
fn amnezia_editor_fields(values: &Map<String, Value>, conditional: bool) -> Vec<Value> {
    let mut result = Vec::new();
    for key in AMNEZIA_EDITOR_KEYS {
        let kind = if AMNEZIA_NUMERIC_EDITOR_KEYS.contains(&key) {
            "number"
        } else if (key.starts_with('i') || key.starts_with('j')) && key != "jmin" && key != "jmax" {
            "area"
        } else {
            "text"
        };
        let field_key = format!("awg_{key}");
        let label = format!("AWG {}", key.to_uppercase());
        let when_key = if conditional { "amneziaEnabled" } else { "" };
        let when_values: &[&str] = if conditional { &["true"] } else { &[] };
        result.push(editor_field(
            &field_key,
            &label,
            values.get(&field_key).unwrap_or(&json!("")),
            kind,
            &[],
            false,
            "",
            when_key,
            when_values,
        ));
    }
    result
}

/// Python `_protocol_editor_fields`.
fn protocol_editor_fields(protocol: &str, values: &Map<String, Value>) -> Vec<Value> {
    if protocol == "singbox_config" || protocol == "xray_config" {
        return Vec::new();
    }

    let mut fields: Vec<Value> = Vec::new();
    if protocol != "warp" {
        fields.extend(endpoint_fields(values));
    }

    match protocol {
        "vless" => {
            fields.extend([
                editor_field(
                    "uuid",
                    "UUID / id",
                    values.get("uuid").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "encryption",
                    "Шифрование",
                    values.get("encryption").unwrap_or(&json!("none")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "flow",
                    "Flow",
                    values.get("flow").unwrap_or(&json!("")),
                    "combo",
                    &FLOWS,
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(stream_editor_fields(values));
            fields.extend(stream_security_fields(values));
        }
        "vmess" => {
            fields.extend([
                editor_field(
                    "uuid",
                    "UUID / id",
                    values.get("uuid").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "alterId",
                    "Alter ID",
                    values.get("alterId").unwrap_or(&json!(0)),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "vmessSecurity",
                    "Шифрование",
                    values.get("vmessSecurity").unwrap_or(&json!("auto")),
                    "combo",
                    &["auto", "none", "zero", "aes-128-gcm", "chacha20-poly1305"],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(stream_editor_fields(values));
            fields.extend(stream_security_fields(values));
        }
        "trojan" => {
            fields.push(editor_field(
                "password",
                "Пароль",
                values.get("password").unwrap_or(&json!("")),
                "text",
                &[],
                false,
                "",
                "",
                &[],
            ));
            fields.extend(stream_editor_fields(values));
            fields.extend(stream_security_fields(values));
        }
        "shadowsocks" => {
            fields.extend([
                editor_field(
                    "method",
                    "Метод шифрования",
                    values.get("method").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "plugin",
                    "Плагин",
                    values.get("plugin").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "pluginOpts",
                    "Параметры плагина",
                    values.get("pluginOpts").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
        }
        "socks" | "http" => {
            fields.extend([
                editor_field(
                    "username",
                    "Имя пользователя",
                    values.get("username").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
        }
        "naive" => {
            fields.extend([
                editor_field(
                    "username",
                    "Имя пользователя",
                    values.get("username").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "insecureConcurrency",
                    "Insecure concurrency",
                    values.get("insecureConcurrency").unwrap_or(&json!(0)),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "extraHeaders",
                    "Дополнительные заголовки (JSON)",
                    values.get("extraHeaders").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "{\"Host\":\"example.com\"}",
                    "",
                    &[],
                ),
                editor_field(
                    "udpOverTcpEnabled",
                    "UDP поверх TCP",
                    values.get("udpOverTcpEnabled").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpOverTcpOptions",
                    "UDP over TCP (JSON)",
                    values.get("udpOverTcpOptions").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "{}",
                    "",
                    &[],
                ),
                editor_field(
                    "naiveQuic",
                    "Использовать QUIC",
                    values.get("naiveQuic").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "quicCongestionControl",
                    "QUIC congestion control",
                    values.get("quicCongestionControl").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            // Naive uses Cronet and accepts only the restricted Naive TLS schema.
            // Keep SNI editable without exposing options that make the core reject
            // the generated config (ALPN, insecure and uTLS).
            fields.push(editor_field(
                "sni",
                "SNI / Server name",
                values.get("sni").unwrap_or(&json!("")),
                "text",
                &[],
                false,
                "",
                "",
                &[],
            ));
        }
        "hysteria" => {
            fields.extend([
                editor_field(
                    "auth",
                    "Auth / пароль",
                    values.get("auth").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "hysteriaProtocol",
                    "Протокол",
                    values.get("hysteriaProtocol").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "upMbps",
                    "Скорость отдачи, Мбит/с",
                    values.get("upMbps").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "downMbps",
                    "Скорость загрузки, Мбит/с",
                    values.get("downMbps").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "obfsPassword",
                    "Obfs",
                    values.get("obfsPassword").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(native_tls_fields(values));
        }
        "hysteria2" => {
            fields.extend([
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "obfsType",
                    "Obfs",
                    values.get("obfsType").unwrap_or(&json!("")),
                    "combo",
                    &["", "salamander"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "obfsPassword",
                    "Пароль obfs",
                    values.get("obfsPassword").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(native_tls_fields(values));
        }
        "tuic" => {
            fields.extend([
                editor_field(
                    "uuid",
                    "UUID",
                    values.get("uuid").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "congestionControl",
                    "Congestion control",
                    values.get("congestionControl").unwrap_or(&json!("cubic")),
                    "combo",
                    &["cubic", "bbr", "new_reno"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpRelayMode",
                    "UDP relay",
                    values.get("udpRelayMode").unwrap_or(&json!("native")),
                    "combo",
                    &["native", "quic"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "zeroRtt",
                    "0-RTT handshake",
                    values.get("zeroRtt").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(native_tls_fields(values));
        }
        "mieru" => {
            fields.extend([
                editor_field(
                    "serverPorts",
                    "Диапазоны портов",
                    values.get("serverPorts").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "20000-30000, 40000",
                    "",
                    &[],
                ),
                editor_field(
                    "username",
                    "Имя пользователя",
                    values.get("username").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Пароль",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "nativeTransport",
                    "Транспорт",
                    values.get("nativeTransport").unwrap_or(&json!("TCP")),
                    "combo",
                    &["TCP"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "multiplexing",
                    "Multiplexing",
                    values.get("multiplexing").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "trafficPattern",
                    "Traffic pattern",
                    values.get("trafficPattern").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
        }
        "wireguard" | "awg" => {
            fields.extend([
                editor_field(
                    "interfaceAddresses",
                    "Адреса интерфейса",
                    values.get("interfaceAddresses").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "10.0.0.2/32, fd00::2/128",
                    "",
                    &[],
                ),
                editor_field(
                    "privateKey",
                    "Private key",
                    values.get("privateKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "peersJson",
                    "Peers (JSON)",
                    values.get("peersJson").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "[{\"address\":\"example.com\",\"port\":51820,...}]",
                    "",
                    &[],
                ),
                editor_field(
                    "wgPublicKey",
                    "Peer public key",
                    values.get("wgPublicKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "preSharedKey",
                    "Pre-shared key",
                    values.get("preSharedKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "allowedIps",
                    "Allowed IPs",
                    values.get("allowedIps").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "mtu",
                    "MTU",
                    values.get("mtu").unwrap_or(&json!(1408)),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "listenPort",
                    "Локальный порт",
                    values.get("listenPort").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "keepalive",
                    "Persistent keepalive",
                    values.get("keepalive").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpTimeout",
                    "UDP timeout",
                    values.get("udpTimeout").unwrap_or(&json!("5m0s")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "workers",
                    "WireGuard workers",
                    values.get("workers").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "preallocatedBuffers",
                    "Preallocated buffers",
                    values.get("preallocatedBuffers").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "disablePauses",
                    "Disable pauses",
                    values.get("disablePauses").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "dns",
                    "DNS профиля",
                    values.get("dns").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "amneziaEnabled",
                    "AmneziaWG-совместимость",
                    &values
                        .get("amneziaEnabled")
                        .cloned()
                        .unwrap_or(json!(protocol == "awg")),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(amnezia_editor_fields(values, true));
        }
        "warp" => {
            fields.extend([
                editor_field(
                    "profileId",
                    "WARP profile ID",
                    values.get("profileId").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "profilePrivateKey",
                    "WARP private key",
                    values.get("profilePrivateKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "authToken",
                    "WARP auth token",
                    values.get("authToken").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "reserved",
                    "Reserved bytes",
                    values.get("reserved").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "1, 2, 3",
                    "",
                    &[],
                ),
                editor_field(
                    "listenPort",
                    "Локальный порт",
                    values.get("listenPort").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "keepalive",
                    "Persistent keepalive",
                    values.get("keepalive").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpTimeout",
                    "UDP timeout",
                    values.get("udpTimeout").unwrap_or(&json!("5m0s")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "dns",
                    "DNS профиля",
                    values.get("dns").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
            fields.extend(amnezia_editor_fields(values, false));
        }
        "openvpn" => {
            fields.extend([
                editor_field(
                    "openvpnServersJson",
                    "OpenVPN servers (JSON)",
                    values.get("openvpnServersJson").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "[{\"server\":\"vpn.example.com\",\"server_port\":1194}]",
                    "",
                    &[],
                ),
                editor_field(
                    "openvpnProto",
                    "OpenVPN transport",
                    values.get("openvpnProto").unwrap_or(&json!("udp")),
                    "combo",
                    &["udp", "tcp"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "openvpnCipher",
                    "Data cipher",
                    values.get("openvpnCipher").unwrap_or(&json!("")),
                    "combo",
                    &[
                        "",
                        "AES-128-GCM",
                        "AES-192-GCM",
                        "AES-256-GCM",
                        "AES-128-CBC",
                        "AES-192-CBC",
                        "AES-256-CBC",
                        "CHACHA20-POLY1305",
                    ],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "openvpnAuth",
                    "HMAC auth",
                    values.get("openvpnAuth").unwrap_or(&json!("")),
                    "combo",
                    &["", "SHA1", "SHA256", "SHA384", "SHA512"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "username",
                    "Username",
                    values.get("username").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "password",
                    "Password",
                    values.get("password").unwrap_or(&json!("")),
                    "text",
                    &[],
                    true,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "keyPassword",
                    "Private key password",
                    values.get("keyPassword").unwrap_or(&json!("")),
                    "text",
                    &[],
                    true,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "openvpnProxyType",
                    "OpenVPN proxy",
                    values.get("openvpnProxyType").unwrap_or(&json!("")),
                    "combo",
                    &["", "http", "socks"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "openvpnProxyServer",
                    "Proxy server",
                    values.get("openvpnProxyServer").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "openvpnProxyType",
                    &["http", "socks"],
                ),
                editor_field(
                    "openvpnProxyPort",
                    "Proxy port",
                    values.get("openvpnProxyPort").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "openvpnProxyType",
                    &["http", "socks"],
                ),
                editor_field(
                    "openvpnProxyUsername",
                    "Proxy username",
                    values.get("openvpnProxyUsername").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "openvpnProxyType",
                    &["http", "socks"],
                ),
                editor_field(
                    "openvpnProxyPassword",
                    "Proxy password",
                    values.get("openvpnProxyPassword").unwrap_or(&json!("")),
                    "text",
                    &[],
                    true,
                    "",
                    "openvpnProxyType",
                    &["http", "socks"],
                ),
                editor_field(
                    "tlsCrypt",
                    "TLS Crypt",
                    values.get("tlsCrypt").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "tlsCryptV2",
                    "TLS Crypt v2",
                    values.get("tlsCryptV2").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "tlsAuth",
                    "TLS Auth",
                    values.get("tlsAuth").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "keyDirection",
                    "Key direction",
                    values.get("keyDirection").unwrap_or(&json!(-1)),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "certificate",
                    "Client certificate",
                    values.get("certificate").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "privateKey",
                    "Client private key",
                    values.get("privateKey").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "caCertificate",
                    "CA certificate",
                    values.get("caCertificate").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "tlsCipherSuites",
                    "TLS cipher suites",
                    values.get("tlsCipherSuites").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "verifyX509Name",
                    "Verify X.509 name",
                    values.get("verifyX509Name").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "verifyX509NameMode",
                    "X.509 name mode",
                    values.get("verifyX509NameMode").unwrap_or(&json!("name")),
                    "combo",
                    &["name", "name-prefix", "name-suffix"],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "allowedIps",
                    "Allowed IPs",
                    values.get("allowedIps").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "interfaceName",
                    "Interface name",
                    values.get("interfaceName").unwrap_or(&json!("openvpn0")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "reconnectDelay",
                    "Reconnect delay",
                    values.get("reconnectDelay").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "pingInterval",
                    "Ping interval",
                    values.get("pingInterval").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "pingRestart",
                    "Ping restart",
                    values.get("pingRestart").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "kernelTx",
                    "Kernel TX",
                    values.get("kernelTx").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "kernelRx",
                    "Kernel RX",
                    values.get("kernelRx").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "dns",
                    "Profile DNS",
                    values.get("dns").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
        }
        "masque" => {
            fields.extend([
                editor_field(
                    "interfaceName",
                    "Имя интерфейса",
                    values.get("interfaceName").unwrap_or(&json!("masque0")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "systemInterface",
                    "Системный интерфейс",
                    values.get("systemInterface").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "useHttp2",
                    "Использовать HTTP/2",
                    values.get("useHttp2").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "useIpv6",
                    "Использовать IPv6",
                    values.get("useIpv6").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "interfaceAddresses",
                    "Адреса интерфейса",
                    values.get("interfaceAddresses").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "privateKey",
                    "Private key",
                    values.get("privateKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "wgPublicKey",
                    "Server public key",
                    values.get("wgPublicKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "profileId",
                    "Profile ID",
                    values.get("profileId").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "profilePrivateKey",
                    "Profile private key",
                    values.get("profilePrivateKey").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "authToken",
                    "Auth token",
                    values.get("authToken").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "allowedIps",
                    "Allowed IPs",
                    values.get("allowedIps").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "mtu",
                    "MTU",
                    values.get("mtu").unwrap_or(&json!("")),
                    "number",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpTimeout",
                    "UDP timeout",
                    values.get("udpTimeout").unwrap_or(&json!("5m0s")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "udpKeepalive",
                    "UDP keepalive",
                    values.get("udpKeepalive").unwrap_or(&json!("30s")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "reconnectDelay",
                    "Задержка переподключения",
                    values.get("reconnectDelay").unwrap_or(&json!("5s")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "congestionController",
                    "Congestion controller",
                    values.get("congestionController").unwrap_or(&json!("bbr")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "sni",
                    "SNI",
                    values.get("sni").unwrap_or(&json!("")),
                    "text",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "allowInsecure",
                    "Пропускать проверку сертификата",
                    values.get("allowInsecure").unwrap_or(&json!(false)),
                    "bool",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
                editor_field(
                    "dns",
                    "DNS профиля",
                    values.get("dns").unwrap_or(&json!("")),
                    "area",
                    &[],
                    false,
                    "",
                    "",
                    &[],
                ),
            ]);
        }
        _ => {}
    }
    fields
}

/// Python `_edit_hint` (always empty in the current dialog).
fn edit_hint(_protocol: &str, _capabilities: &Value) -> String {
    String::new()
}

/// Python `load_node_edit_fields` — flatten only fields supported by the
/// selected node protocol.
pub fn load_node_edit_fields(node: &Node) -> Value {
    let outbound = obj_or_empty(if node.outbound.is_object() {
        Some(&node.outbound)
    } else {
        None
    });
    let protocol = protocol_from_node(node);
    let capabilities = field_capabilities(&protocol);
    let native = native_payload(&outbound);
    let amnezia_enabled =
        protocol == "awg" || native.get("amnezia").map(Value::is_object).unwrap_or(false);
    let mut fields = json!({
        "name": node.name,
        "group": node.group,
        "server": node.server,
        "port": if node.port != 0 { node.port.to_string() } else { String::new() },
        "protocol": if protocol.to_uppercase().is_empty() { "?".to_owned() } else { protocol.to_uppercase() },
        "protocolKey": protocol,
        "uuid": "",
        "encryption": "none",
        "flow": "",
        "alterId": 0,
        "vmessSecurity": "auto",
        "username": "",
        "password": "",
        "method": "",
        "plugin": "",
        "pluginOpts": "",
        "network": "tcp",
        "rawHeader": "none",
        "transportPath": "",
        "transportHost": "",
        "grpcServiceName": "",
        "xhttpMode": "auto",
        "xhttpExtra": "",
        "kcpHeader": "none",
        "kcpSeed": "",
        "kcpMtu": "",
        "kcpTti": "",
        "kcpUplink": "",
        "kcpDownlink": "",
        "kcpCongestion": false,
        "security": "none",
        "sni": "",
        "fingerprint": "",
        "allowInsecure": false,
        "alpn": "",
        "pinnedPeerCertSha256": "",
        "publicKey": "",
        "shortId": "",
        "spiderX": "",
        "pqv": "",
        "finalmask": "",
        "amneziaEnabled": amnezia_enabled,
        "capabilities": capabilities,
        "editHint": edit_hint(&protocol, &capabilities),
    });
    let fields_obj = fields.as_object_mut().unwrap();

    let settings = obj_or_empty(outbound.get("settings"));
    if protocol == "vless" || protocol == "vmess" {
        let vnext = first_dict(settings.get("vnext").unwrap_or(&Value::Null));
        let user = first_dict(vnext.get("users").unwrap_or(&Value::Null));
        fields_obj.insert("uuid".into(), json!(text_of(user.get("id"))));
        let encryption = text_of(user.get("encryption"));
        fields_obj.insert(
            "encryption".into(),
            json!(if encryption.is_empty() {
                "none"
            } else {
                encryption.as_str()
            }),
        );
        fields_obj.insert("flow".into(), json!(text_of(user.get("flow"))));
        let alter_id = user.get("alterId").cloned().unwrap_or(json!(0));
        fields_obj.insert("alterId".into(), json!(value_int(&alter_id, 0)));
        let security = text_of(user.get("security"));
        fields_obj.insert(
            "vmessSecurity".into(),
            json!(if security.is_empty() {
                "auto"
            } else {
                security.as_str()
            }),
        );
    } else if protocol_in(&["trojan", "shadowsocks", "socks", "http"], &protocol) {
        let server_item = first_dict(settings.get("servers").unwrap_or(&Value::Null));
        fields_obj.insert(
            "password".into(),
            json!(text_of(server_item.get("password"))),
        );
        fields_obj.insert("method".into(), json!(text_of(server_item.get("method"))));
        fields_obj.insert("plugin".into(), json!(text_of(server_item.get("plugin"))));
        fields_obj.insert(
            "pluginOpts".into(),
            json!(text_of(server_item.get("plugin_opts"))),
        );
        let user = first_dict(server_item.get("users").unwrap_or(&Value::Null));
        fields_obj.insert("username".into(), json!(text_of(user.get("user"))));
        if protocol == "socks" || protocol == "http" {
            fields_obj.insert("password".into(), json!(text_of(user.get("pass"))));
        }
    }

    let stream = obj_or_empty(outbound.get("streamSettings"));
    let network_raw = text_of(stream.get("network"));
    fields_obj.insert(
        "network".into(),
        json!(if network_raw.is_empty() {
            "tcp"
        } else {
            network_raw.as_str()
        }),
    );
    let raw = obj_or_empty(stream.get("rawSettings"));
    let header = obj_or_empty(raw.get("header"));
    let header_type = text_of(header.get("type"));
    fields_obj.insert(
        "rawHeader".into(),
        json!(if header_type.is_empty() {
            "none"
        } else {
            header_type.as_str()
        }),
    );
    let network = {
        let text = text_of(stream.get("network"));
        if text.is_empty() {
            "tcp"
        } else {
            text.as_str()
        }
        .to_lowercase()
    };
    match network.as_str() {
        "ws" => {
            let transport = obj_or_empty(stream.get("wsSettings"));
            fields_obj.insert(
                "transportPath".into(),
                json!(text_of(transport.get("path"))),
            );
            let headers = obj_or_empty(transport.get("headers"));
            let host = if headers.get("Host").is_some() {
                text_of(headers.get("Host"))
            } else {
                text_of(headers.get("host"))
            };
            fields_obj.insert("transportHost".into(), json!(host));
        }
        "http" | "h2" => {
            let transport = if is_dict(stream.get("httpSettings")) {
                obj_or_empty(stream.get("httpSettings"))
            } else if is_dict(stream.get("h2Settings")) {
                obj_or_empty(stream.get("h2Settings"))
            } else {
                Map::new()
            };
            fields_obj.insert(
                "transportPath".into(),
                json!(text_of(transport.get("path"))),
            );
            fields_obj.insert(
                "transportHost".into(),
                json!(text_list(transport.get("host").unwrap_or(&Value::Null))),
            );
        }
        "grpc" => {
            let transport = obj_or_empty(stream.get("grpcSettings"));
            fields_obj.insert(
                "grpcServiceName".into(),
                json!(text_of(transport.get("serviceName"))),
            );
        }
        "xhttp" => {
            let transport = obj_or_empty(stream.get("xhttpSettings"));
            fields_obj.insert(
                "transportPath".into(),
                json!(text_of(transport.get("path"))),
            );
            fields_obj.insert(
                "transportHost".into(),
                json!(text_of(transport.get("host"))),
            );
            let mode = text_of(transport.get("mode"));
            fields_obj.insert(
                "xhttpMode".into(),
                json!(if mode.is_empty() {
                    "auto"
                } else {
                    mode.as_str()
                }),
            );
            fields_obj.insert(
                "xhttpExtra".into(),
                json!(json_field(transport.get("extra").unwrap_or(&Value::Null))),
            );
        }
        "httpupgrade" => {
            let transport = obj_or_empty(stream.get("httpupgradeSettings"));
            fields_obj.insert(
                "transportPath".into(),
                json!(text_of(transport.get("path"))),
            );
            fields_obj.insert(
                "transportHost".into(),
                json!(text_of(transport.get("host"))),
            );
        }
        "kcp" | "mkcp" => {
            let transport = obj_or_empty(stream.get("kcpSettings"));
            let kcp_header = obj_or_empty(transport.get("header"));
            fields_obj.insert("network".into(), json!("kcp"));
            let kcp_header_type = text_of(kcp_header.get("type"));
            fields_obj.insert(
                "kcpHeader".into(),
                json!(if kcp_header_type.is_empty() {
                    "none"
                } else {
                    kcp_header_type.as_str()
                }),
            );
            fields_obj.insert("kcpSeed".into(), json!(text_of(transport.get("seed"))));
            fields_obj.insert(
                "kcpMtu".into(),
                transport.get("mtu").cloned().unwrap_or(json!("")),
            );
            fields_obj.insert(
                "kcpTti".into(),
                transport.get("tti").cloned().unwrap_or(json!("")),
            );
            fields_obj.insert(
                "kcpUplink".into(),
                transport
                    .get("uplinkCapacity")
                    .cloned()
                    .unwrap_or(json!("")),
            );
            fields_obj.insert(
                "kcpDownlink".into(),
                transport
                    .get("downlinkCapacity")
                    .cloned()
                    .unwrap_or(json!("")),
            );
            fields_obj.insert(
                "kcpCongestion".into(),
                json!(
                    transport
                        .get("congestion")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                ),
            );
        }
        _ => {}
    }
    let security = text_of(stream.get("security"));
    fields_obj.insert(
        "security".into(),
        json!(if security.is_empty() {
            "none"
        } else {
            security.as_str()
        }),
    );

    let tls = obj_or_empty(stream.get("tlsSettings"));
    let reality = obj_or_empty(stream.get("realitySettings"));
    let active_security = text_of(stream.get("security")).to_lowercase();
    let security_payload = if active_security == "reality" {
        &reality
    } else {
        &tls
    };
    fields_obj.insert(
        "sni".into(),
        json!(text_of(security_payload.get("serverName"))),
    );
    fields_obj.insert(
        "fingerprint".into(),
        json!(text_of(security_payload.get("fingerprint"))),
    );
    fields_obj.insert(
        "allowInsecure".into(),
        json!(
            tls.get("allowInsecure")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
    );
    fields_obj.insert(
        "alpn".into(),
        json!(text_list(tls.get("alpn").unwrap_or(&Value::Null))),
    );
    let pinned = tls.get("pinnedPeerCertSha256");
    fields_obj.insert(
        "pinnedPeerCertSha256".into(),
        json!(match pinned {
            Some(Value::Array(items)) => items
                .iter()
                .map(value_to_text)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
                .join("~"),
            other => text_of(other),
        }),
    );
    fields_obj.insert("publicKey".into(), json!(text_of(reality.get("publicKey"))));
    fields_obj.insert("shortId".into(), json!(text_of(reality.get("shortId"))));
    fields_obj.insert("spiderX".into(), json!(text_of(reality.get("spiderX"))));
    fields_obj.insert("pqv".into(), json!(text_of(reality.get("mldsa65Verify"))));
    let finalmask = stream.get("finalmask");
    if let Some(finalmask) = finalmask
        && !finalmask.is_null()
        && !value_to_text(finalmask).is_empty()
    {
        fields_obj.insert(
            "finalmask".into(),
            json!(if finalmask.is_string() {
                value_to_text(finalmask)
            } else {
                compact_json(finalmask)
            }),
        );
    }

    if !native.is_empty() {
        let tls_native = obj_or_empty(native.get("tls"));
        let native_sni = text_of(tls_native.get("server_name"));
        if !native_sni.is_empty() {
            fields_obj.insert("sni".into(), json!(native_sni));
        }
        fields_obj.insert(
            "allowInsecure".into(),
            json!(
                tls_native
                    .get("insecure")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
        );
        fields_obj.insert(
            "alpn".into(),
            json!(text_list(tls_native.get("alpn").unwrap_or(&Value::Null))),
        );
        match protocol.as_str() {
            "shadowsocks" => {
                let native_server = text_of(native.get("server"));
                if !native_server.is_empty() {
                    fields_obj.insert("server".into(), json!(native_server));
                }
                let native_port = text_of(native.get("server_port"));
                if !native_port.is_empty() {
                    fields_obj.insert("port".into(), json!(native_port));
                }
                fields_obj.insert("method".into(), json!(text_of(native.get("method"))));
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                fields_obj.insert("plugin".into(), json!(text_of(native.get("plugin"))));
                fields_obj.insert(
                    "pluginOpts".into(),
                    json!(text_of(native.get("plugin_opts"))),
                );
            }
            "hysteria" => {
                fields_obj.insert("auth".into(), json!(text_of(native.get("auth_str"))));
                fields_obj.insert(
                    "hysteriaProtocol".into(),
                    json!(text_of(native.get("protocol"))),
                );
                let up = if native.get("up_mbps").is_some() {
                    native.get("up_mbps").unwrap().clone()
                } else {
                    native.get("up").cloned().unwrap_or(json!(""))
                };
                fields_obj.insert("upMbps".into(), up);
                let down = if native.get("down_mbps").is_some() {
                    native.get("down_mbps").unwrap().clone()
                } else {
                    native.get("down").cloned().unwrap_or(json!(""))
                };
                fields_obj.insert("downMbps".into(), down);
                fields_obj.insert("obfsPassword".into(), json!(text_of(native.get("obfs"))));
            }
            "hysteria2" => {
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                let obfs = obj_or_empty(native.get("obfs"));
                fields_obj.insert("obfsType".into(), json!(text_of(obfs.get("type"))));
                fields_obj.insert("obfsPassword".into(), json!(text_of(obfs.get("password"))));
            }
            "tuic" => {
                fields_obj.insert("uuid".into(), json!(text_of(native.get("uuid"))));
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                let congestion = text_of(native.get("congestion_control"));
                fields_obj.insert(
                    "congestionControl".into(),
                    json!(if congestion.is_empty() {
                        "cubic"
                    } else {
                        congestion.as_str()
                    }),
                );
                let relay = text_of(native.get("udp_relay_mode"));
                fields_obj.insert(
                    "udpRelayMode".into(),
                    json!(if relay.is_empty() {
                        "native"
                    } else {
                        relay.as_str()
                    }),
                );
                fields_obj.insert(
                    "zeroRtt".into(),
                    json!(
                        native
                            .get("zero_rtt_handshake")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
            }
            "mieru" => {
                fields_obj.insert("username".into(), json!(text_of(native.get("username"))));
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                let transport = text_of(native.get("transport"));
                fields_obj.insert(
                    "nativeTransport".into(),
                    json!(
                        if transport.is_empty() {
                            "TCP"
                        } else {
                            transport.as_str()
                        }
                        .to_uppercase()
                    ),
                );
                fields_obj.insert(
                    "serverPorts".into(),
                    json!(text_list(
                        native.get("server_ports").unwrap_or(&Value::Null)
                    )),
                );
                fields_obj.insert(
                    "multiplexing".into(),
                    json!(text_of(native.get("multiplexing"))),
                );
                fields_obj.insert(
                    "trafficPattern".into(),
                    json!(text_of(native.get("traffic_pattern"))),
                );
            }
            "naive" => {
                fields_obj.insert("username".into(), json!(text_of(native.get("username"))));
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                fields_obj.insert(
                    "insecureConcurrency".into(),
                    native
                        .get("insecure_concurrency")
                        .cloned()
                        .unwrap_or(json!(0)),
                );
                fields_obj.insert(
                    "extraHeaders".into(),
                    json!(json_field(
                        native.get("extra_headers").unwrap_or(&Value::Null)
                    )),
                );
                let udp_over_tcp = native.get("udp_over_tcp").cloned().unwrap_or(json!(false));
                fields_obj.insert(
                    "udpOverTcpEnabled".into(),
                    json!(udp_over_tcp.as_bool().unwrap_or(false)),
                );
                fields_obj.insert(
                    "udpOverTcpOptions".into(),
                    json!(if udp_over_tcp.is_object() {
                        json_field(&udp_over_tcp)
                    } else {
                        String::new()
                    }),
                );
                fields_obj.insert(
                    "naiveQuic".into(),
                    json!(native.get("quic").and_then(Value::as_bool).unwrap_or(false)),
                );
                fields_obj.insert(
                    "quicCongestionControl".into(),
                    json!(text_of(native.get("quic_congestion_control"))),
                );
            }
            "wireguard" | "awg" => {
                let peer = first_dict(native.get("peers").unwrap_or(&Value::Null));
                let peer_server = if text_of(peer.get("address")).is_empty() {
                    text_of(peer.get("server"))
                } else {
                    text_of(peer.get("address"))
                };
                if !peer_server.is_empty() {
                    fields_obj.insert("server".into(), json!(peer_server));
                }
                let peer_port = if text_of(peer.get("port")).is_empty() {
                    text_of(peer.get("server_port"))
                } else {
                    text_of(peer.get("port"))
                };
                if !peer_port.is_empty() {
                    fields_obj.insert("port".into(), json!(peer_port));
                }
                fields_obj.insert(
                    "interfaceAddresses".into(),
                    json!(text_list(native.get("address").unwrap_or(&Value::Null))),
                );
                fields_obj.insert(
                    "privateKey".into(),
                    json!(text_of(native.get("private_key"))),
                );
                let peers_json =
                    if let Some(peers) = native.get("peers").filter(|value| value.is_array()) {
                        compact_json(peers)
                    } else {
                        "[]".to_string()
                    };
                fields_obj.insert("peersJson".into(), json!(peers_json));
                fields_obj.insert("wgPublicKey".into(), json!(text_of(peer.get("public_key"))));
                fields_obj.insert(
                    "preSharedKey".into(),
                    json!(text_of(peer.get("pre_shared_key"))),
                );
                fields_obj.insert(
                    "allowedIps".into(),
                    json!(text_list(peer.get("allowed_ips").unwrap_or(&Value::Null))),
                );
                fields_obj.insert(
                    "mtu".into(),
                    native.get("mtu").cloned().unwrap_or(json!(1408)),
                );
                fields_obj.insert(
                    "listenPort".into(),
                    native.get("listen_port").cloned().unwrap_or(json!("")),
                );
                fields_obj.insert(
                    "keepalive".into(),
                    peer.get("persistent_keepalive_interval")
                        .cloned()
                        .unwrap_or(json!("")),
                );
                let udp_timeout = text_of(native.get("udp_timeout"));
                fields_obj.insert(
                    "udpTimeout".into(),
                    json!(if udp_timeout.is_empty() {
                        "5m0s"
                    } else {
                        udp_timeout.as_str()
                    }),
                );
                fields_obj.insert(
                    "workers".into(),
                    native.get("workers").cloned().unwrap_or(json!("")),
                );
                fields_obj.insert(
                    "preallocatedBuffers".into(),
                    native
                        .get("preallocated_buffers_per_pool")
                        .cloned()
                        .unwrap_or(json!("")),
                );
                fields_obj.insert(
                    "disablePauses".into(),
                    json!(
                        native
                            .get("disable_pauses")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "dns".into(),
                    json!(text_list(outbound.get("_dns").unwrap_or(&Value::Null))),
                );
            }
            "warp" => {
                let profile = obj_or_empty(native.get("profile"));
                fields_obj.insert("profileId".into(), json!(text_of(profile.get("id"))));
                fields_obj.insert(
                    "profilePrivateKey".into(),
                    json!(text_of(profile.get("private_key"))),
                );
                fields_obj.insert(
                    "authToken".into(),
                    json!(text_of(profile.get("auth_token"))),
                );
                fields_obj.insert(
                    "reserved".into(),
                    json!(text_list(native.get("reserved").unwrap_or(&Value::Null))),
                );
                fields_obj.insert(
                    "listenPort".into(),
                    native.get("listen_port").cloned().unwrap_or(json!("")),
                );
                fields_obj.insert(
                    "keepalive".into(),
                    native
                        .get("persistent_keepalive_interval")
                        .cloned()
                        .unwrap_or(json!("")),
                );
                let udp_timeout = text_of(native.get("udp_timeout"));
                fields_obj.insert(
                    "udpTimeout".into(),
                    json!(if udp_timeout.is_empty() {
                        "5m0s"
                    } else {
                        udp_timeout.as_str()
                    }),
                );
                fields_obj.insert(
                    "dns".into(),
                    json!(text_list(outbound.get("_dns").unwrap_or(&Value::Null))),
                );
            }
            "openvpn" => {
                let openvpn_server = first_dict(native.get("servers").unwrap_or(&Value::Null));
                let tls_openvpn = obj_or_empty(native.get("tls"));
                let openvpn_server_host = text_of(openvpn_server.get("server"));
                if !openvpn_server_host.is_empty() {
                    fields_obj.insert("server".into(), json!(openvpn_server_host));
                }
                let openvpn_server_port = text_of(openvpn_server.get("server_port"));
                if !openvpn_server_port.is_empty() {
                    fields_obj.insert("port".into(), json!(openvpn_server_port));
                }
                let servers_json =
                    if let Some(servers) = native.get("servers").filter(|value| value.is_array()) {
                        compact_json(servers)
                    } else {
                        "[]".to_string()
                    };
                fields_obj.insert("openvpnServersJson".into(), json!(servers_json));
                let proto = if text_of(native.get("proto")).is_empty() {
                    text_of(native.get("network"))
                } else {
                    text_of(native.get("proto"))
                };
                fields_obj.insert(
                    "openvpnProto".into(),
                    json!(if proto.is_empty() {
                        "udp"
                    } else {
                        proto.as_str()
                    }),
                );
                fields_obj.insert("openvpnCipher".into(), json!(text_of(native.get("cipher"))));
                fields_obj.insert("openvpnAuth".into(), json!(text_of(native.get("auth"))));
                fields_obj.insert("username".into(), json!(text_of(native.get("username"))));
                fields_obj.insert("password".into(), json!(text_of(native.get("password"))));
                fields_obj.insert(
                    "keyPassword".into(),
                    json!(text_of(native.get("key_password"))),
                );
                let openvpn_proxy = obj_or_empty(native.get("lumen_proxy"));
                fields_obj.insert(
                    "openvpnProxyType".into(),
                    json!(text_of(openvpn_proxy.get("type"))),
                );
                fields_obj.insert(
                    "openvpnProxyServer".into(),
                    json!(text_of(openvpn_proxy.get("server"))),
                );
                fields_obj.insert(
                    "openvpnProxyPort".into(),
                    json!(text_of(openvpn_proxy.get("server_port"))),
                );
                fields_obj.insert(
                    "openvpnProxyUsername".into(),
                    json!(text_of(openvpn_proxy.get("username"))),
                );
                fields_obj.insert(
                    "openvpnProxyPassword".into(),
                    json!(text_of(openvpn_proxy.get("password"))),
                );
                fields_obj.insert("tlsCrypt".into(), json!(text_of(native.get("tls_crypt"))));
                fields_obj.insert(
                    "tlsCryptV2".into(),
                    json!(
                        native
                            .get("tls_crypt_v2")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert("tlsAuth".into(), json!(text_of(native.get("tls_auth"))));
                fields_obj.insert(
                    "keyDirection".into(),
                    native.get("key_direction").cloned().unwrap_or(json!(-1)),
                );
                fields_obj.insert(
                    "certificate".into(),
                    json!(text_of(tls_openvpn.get("certificate"))),
                );
                fields_obj.insert("privateKey".into(), json!(text_of(tls_openvpn.get("key"))));
                fields_obj.insert(
                    "caCertificate".into(),
                    json!(text_of(tls_openvpn.get("ca"))),
                );
                fields_obj.insert(
                    "tlsCipherSuites".into(),
                    json!(text_list(
                        tls_openvpn.get("cipher_suites").unwrap_or(&Value::Null)
                    )),
                );
                fields_obj.insert(
                    "verifyX509Name".into(),
                    json!(text_of(tls_openvpn.get("verify_x509_name"))),
                );
                let mode = text_of(tls_openvpn.get("verify_x509_name_mode"));
                let mode = if mode.is_empty() {
                    "name"
                } else {
                    mode.as_str()
                };
                fields_obj.insert(
                    "verifyX509NameMode".into(),
                    json!(mode.replace("exact", "name")),
                );
                fields_obj.insert(
                    "allowedIps".into(),
                    json!(text_list(native.get("allowed_ips").unwrap_or(&Value::Null))),
                );
                let interface_name = text_of(native.get("name"));
                fields_obj.insert(
                    "interfaceName".into(),
                    json!(if interface_name.is_empty() {
                        "openvpn0"
                    } else {
                        interface_name.as_str()
                    }),
                );
                fields_obj.insert(
                    "reconnectDelay".into(),
                    json!(text_of(native.get("reconnect_delay"))),
                );
                fields_obj.insert(
                    "pingInterval".into(),
                    json!(text_of(native.get("ping_interval"))),
                );
                fields_obj.insert(
                    "pingRestart".into(),
                    json!(text_of(native.get("ping_restart"))),
                );
                fields_obj.insert(
                    "kernelTx".into(),
                    json!(
                        tls_openvpn
                            .get("kernel_tx")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "kernelRx".into(),
                    json!(
                        tls_openvpn
                            .get("kernel_rx")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "dns".into(),
                    json!(text_list(outbound.get("_dns").unwrap_or(&Value::Null))),
                );
            }
            "masque" => {
                let profile = obj_or_empty(native.get("profile"));
                let native_server = text_of(native.get("server"));
                if !native_server.is_empty() {
                    fields_obj.insert("server".into(), json!(native_server));
                }
                let native_port = text_of(native.get("server_port"));
                if !native_port.is_empty() {
                    fields_obj.insert("port".into(), json!(native_port));
                }
                let interface_name = text_of(native.get("name"));
                fields_obj.insert(
                    "interfaceName".into(),
                    json!(if interface_name.is_empty() {
                        "masque0"
                    } else {
                        interface_name.as_str()
                    }),
                );
                fields_obj.insert(
                    "systemInterface".into(),
                    json!(
                        native
                            .get("system")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "useHttp2".into(),
                    json!(
                        native
                            .get("use_http2")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "useIpv6".into(),
                    json!(
                        native
                            .get("use_ipv6")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                );
                fields_obj.insert(
                    "interfaceAddresses".into(),
                    json!(text_list(native.get("address").unwrap_or(&Value::Null))),
                );
                fields_obj.insert(
                    "privateKey".into(),
                    json!(text_of(native.get("private_key"))),
                );
                fields_obj.insert(
                    "wgPublicKey".into(),
                    json!(text_of(native.get("public_key"))),
                );
                fields_obj.insert("profileId".into(), json!(text_of(profile.get("id"))));
                fields_obj.insert(
                    "profilePrivateKey".into(),
                    json!(text_of(profile.get("private_key"))),
                );
                fields_obj.insert(
                    "authToken".into(),
                    json!(text_of(profile.get("auth_token"))),
                );
                fields_obj.insert(
                    "allowedIps".into(),
                    json!(text_list(native.get("allowed_ips").unwrap_or(&Value::Null))),
                );
                fields_obj.insert(
                    "mtu".into(),
                    native.get("mtu").cloned().unwrap_or(json!("")),
                );
                let udp_timeout = text_of(native.get("udp_timeout"));
                fields_obj.insert(
                    "udpTimeout".into(),
                    json!(if udp_timeout.is_empty() {
                        "5m0s"
                    } else {
                        udp_timeout.as_str()
                    }),
                );
                let keepalive = text_of(native.get("udp_keepalive_period"));
                fields_obj.insert(
                    "udpKeepalive".into(),
                    json!(if keepalive.is_empty() {
                        "30s"
                    } else {
                        keepalive.as_str()
                    }),
                );
                let delay = text_of(native.get("reconnect_delay"));
                fields_obj.insert(
                    "reconnectDelay".into(),
                    json!(if delay.is_empty() {
                        "5s"
                    } else {
                        delay.as_str()
                    }),
                );
                let congestion = text_of(native.get("congestion_controller"));
                fields_obj.insert(
                    "congestionController".into(),
                    json!(if congestion.is_empty() {
                        "bbr"
                    } else {
                        congestion.as_str()
                    }),
                );
                fields_obj.insert(
                    "dns".into(),
                    json!(text_list(outbound.get("_dns").unwrap_or(&Value::Null))),
                );
            }
            _ => {}
        }

        let amnezia = obj_or_empty(native.get("amnezia"));
        for key in AMNEZIA_EDITOR_KEYS {
            let value = amnezia.get(key).cloned().unwrap_or(json!(""));
            fields_obj.insert(format!("awg_{key}"), value);
        }
    }

    fields_obj.insert(
        "protocolFields".into(),
        Value::Array(protocol_editor_fields(&protocol, fields_obj)),
    );
    fields
}

/// Python `new_node_edit_fields` — the same protocol-aware schema used by the
/// normal node editor.
pub fn new_node_edit_fields(protocol: &str, group: &str) -> Value {
    let mut normalized = protocol.trim().to_lowercase();
    if normalized.is_empty() {
        normalized = "vless".into();
    }
    if !MANUAL_NODE_PROTOCOLS.contains(&normalized.as_str()) {
        normalized = "vless".into();
    }
    let mut draft = Node::new();
    draft.scheme = normalized;
    let group = group.trim().to_string();
    draft.group = if group.is_empty() {
        "Default".into()
    } else {
        group
    };
    draft.outbound = json!({"protocol": draft.scheme});
    load_node_edit_fields(&draft)
}

// ── Mutation side (Python build_node_updates) ───────────────────────────────

/// Python `_split_editor_list`.
fn split_editor_list(value: &Value) -> Vec<String> {
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .map(value_to_text)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    let text = value_to_text(value);
    text.split(|c: char| c == ',' || c == ';' || c == '\n' || c == '\r')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Python `_editor_bool`.
fn editor_bool(value: &Value) -> bool {
    if let Some(flag) = value.as_bool() {
        return flag;
    }
    matches!(
        value_to_text(value).trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Python `_editor_int` — errors carry the Python message.
fn editor_int(value: &Value, fallback: Option<i64>) -> Result<Option<i64>, String> {
    if value.is_null() {
        return Ok(fallback);
    }
    if let Some(text) = value.as_str() {
        if text.is_empty() {
            return Ok(fallback);
        }
        return text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("Ожидалось целое число, получено: {}", value_to_text(value)));
    }
    if let Some(number) = value.as_i64() {
        return Ok(Some(number));
    }
    if let Some(number) = value.as_f64() {
        return Ok(Some(number as i64));
    }
    if value.is_boolean() {
        return Ok(Some(if value.as_bool().unwrap() { 1 } else { 0 }));
    }
    Err(format!(
        "Ожидалось целое число, получено: {}",
        value_to_text(value)
    ))
}

/// Python `_set_optional`.
fn set_optional(parent: &mut Map<String, Value>, key: &str, value: Value) {
    let empty = value.is_null()
        || value == json!("")
        || (value.is_array() && value.as_array().unwrap().is_empty())
        || (value.is_object() && value.as_object().unwrap().is_empty());
    if empty {
        parent.shift_remove(key);
    } else {
        parent.insert(key.to_string(), value);
    }
}

fn set_optional_text(parent: &mut Map<String, Value>, key: &str, value: &str) {
    set_optional(parent, key, json!(value));
}

/// Python `_update_stream_transport` — the `value` accessor mirrors the `g`
/// closure from `build_node_updates`.
fn update_stream_transport(
    stream: &mut Map<String, Value>,
    value: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let network = {
        let text = value("network").to_lowercase();
        if text.is_empty() {
            "tcp".to_string()
        } else {
            text
        }
    };
    stream.insert("network".into(), json!(network));
    let settings_keys = [
        "wsSettings",
        "httpSettings",
        "h2Settings",
        "grpcSettings",
        "xhttpSettings",
        "httpupgradeSettings",
        "kcpSettings",
        "rawSettings",
        "quicSettings",
    ];
    let active_key = match network.as_str() {
        "ws" => "wsSettings",
        "http" => "httpSettings",
        "h2" => "httpSettings",
        "grpc" => "grpcSettings",
        "xhttp" => "xhttpSettings",
        "httpupgrade" => "httpupgradeSettings",
        "kcp" => "kcpSettings",
        _ => "",
    };
    for key in settings_keys {
        if key != active_key {
            stream.shift_remove(key);
        }
    }

    let path = value("transportPath");
    let host = value("transportHost");
    match network.as_str() {
        "ws" => {
            let settings = ensure_dict(stream, "wsSettings");
            set_optional_text(settings, "path", &path);
            let headers = ensure_dict(settings, "headers");
            if !host.is_empty() {
                headers.insert("Host".into(), json!(host));
            } else {
                headers.shift_remove("Host");
                headers.shift_remove("host");
            }
            if headers.is_empty() {
                settings.shift_remove("headers");
            }
        }
        "http" | "h2" => {
            let settings = ensure_dict(stream, "httpSettings");
            set_optional_text(settings, "path", &path);
            set_optional(settings, "host", json!(split_editor_list(&json!(host))));
        }
        "grpc" => {
            let settings = ensure_dict(stream, "grpcSettings");
            set_optional_text(settings, "serviceName", &value("grpcServiceName"));
        }
        "xhttp" => {
            let settings = ensure_dict(stream, "xhttpSettings");
            set_optional_text(settings, "path", &path);
            set_optional_text(settings, "host", &host);
            let mode = {
                let text = value("xhttpMode");
                if text.is_empty() {
                    "auto".to_string()
                } else {
                    text
                }
            };
            settings.insert("mode".into(), json!(mode));
            let extra_text = value("xhttpExtra");
            if !extra_text.is_empty() {
                let extra: Value = serde_json::from_str(&extra_text)
                    .map_err(|_| "XHTTP extra должен быть корректным JSON".to_string())?;
                if !extra.is_object() {
                    return Err("XHTTP extra должен быть JSON-объектом".to_string());
                }
                settings.insert("extra".into(), extra);
            } else {
                settings.shift_remove("extra");
            }
        }
        "httpupgrade" => {
            let settings = ensure_dict(stream, "httpupgradeSettings");
            set_optional_text(settings, "path", &path);
            set_optional_text(settings, "host", &host);
        }
        "kcp" => {
            let settings = ensure_dict(stream, "kcpSettings");
            let header = {
                let text = value("kcpHeader");
                if text.is_empty() {
                    "none".to_string()
                } else {
                    text
                }
            };
            settings.insert("header".into(), json!({"type": header}));
            set_optional_text(settings, "seed", &value("kcpSeed"));
            for (field_key, config_key) in [
                ("kcpMtu", "mtu"),
                ("kcpTti", "tti"),
                ("kcpUplink", "uplinkCapacity"),
                ("kcpDownlink", "downlinkCapacity"),
            ] {
                let parsed = editor_int(&json!(value(field_key)), None)?;
                set_optional(
                    settings,
                    config_key,
                    parsed.map(|value| json!(value)).unwrap_or(Value::Null),
                );
            }
            settings.insert(
                "congestion".into(),
                json!(editor_bool(&json!(value("kcpCongestion")))),
            );
        }
        _ => {}
    }
    Ok(())
}

/// Python `_update_stream_security`.
fn update_stream_security(
    stream: &mut Map<String, Value>,
    value: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let security = {
        let text = value("security").to_lowercase();
        if text.is_empty() {
            "none".to_string()
        } else {
            text
        }
    };
    stream.insert("security".into(), json!(security));
    match security.as_str() {
        "reality" => {
            stream.shift_remove("tlsSettings");
            let reality = ensure_dict(stream, "realitySettings");
            set_optional_text(reality, "serverName", &value("sni"));
            set_optional_text(reality, "fingerprint", &value("fingerprint"));
            set_optional_text(reality, "publicKey", &value("publicKey"));
            set_optional_text(reality, "shortId", &value("shortId"));
            set_optional_text(reality, "spiderX", &value("spiderX"));
            set_optional_text(reality, "mldsa65Verify", &value("pqv"));
        }
        "tls" => {
            stream.shift_remove("realitySettings");
            let tls = ensure_dict(stream, "tlsSettings");
            set_optional_text(tls, "serverName", &value("sni"));
            set_optional_text(tls, "fingerprint", &value("fingerprint"));
            set_optional_text(
                tls,
                "pinnedPeerCertSha256",
                &value("pinnedPeerCertSha256").to_lowercase(),
            );
            tls.insert(
                "allowInsecure".into(),
                json!(editor_bool(&json!(value("allowInsecure")))),
            );
            set_optional(tls, "alpn", json!(split_editor_list(&json!(value("alpn")))));
        }
        _ => {
            stream.shift_remove("tlsSettings");
            stream.shift_remove("realitySettings");
        }
    }

    let finalmask = value("finalmask");
    if !finalmask.is_empty() {
        match serde_json::from_str::<Value>(&finalmask) {
            Ok(parsed) => {
                stream.insert("finalmask".into(), parsed);
            }
            Err(_) => {
                stream.insert("finalmask".into(), json!(finalmask));
            }
        }
    } else {
        stream.shift_remove("finalmask");
    }
    Ok(())
}

/// Python `_update_native_tls`.
fn update_native_tls(
    native: &mut Map<String, Value>,
    value: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let tls = ensure_dict(native, "tls");
    tls.insert("enabled".into(), json!(true));
    set_optional_text(tls, "server_name", &value("sni"));
    tls.insert(
        "insecure".into(),
        json!(editor_bool(&json!(value("allowInsecure")))),
    );
    set_optional(tls, "alpn", json!(split_editor_list(&json!(value("alpn")))));
    Ok(())
}

/// Python `_ensure_native`.
fn ensure_native<'a>(
    outbound: &'a mut Map<String, Value>,
    protocol: &str,
) -> &'a mut Map<String, Value> {
    if !outbound
        .get("singbox")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        let native_type = if protocol == "awg" {
            "wireguard"
        } else {
            protocol
        };
        outbound.insert(
            "singbox".into(),
            json!({"type": native_type, "tag": "proxy"}),
        );
    }
    outbound
        .get_mut("singbox")
        .unwrap()
        .as_object_mut()
        .unwrap()
}

fn raw_fields<'a>(
    fields: &'a Map<String, Value>,
    current: &'a Map<String, Value>,
) -> impl Fn(&str) -> Value + 'a {
    move |key: &str| {
        if let Some(value) = fields.get(key) {
            return value.clone();
        }
        current.get(key).cloned().unwrap_or(Value::Null)
    }
}

fn raw_with_default(raw: &dyn Fn(&str) -> Value, key: &str, default: &Value) -> Value {
    let value = raw(key);
    if value.is_null() {
        default.clone()
    } else {
        value
    }
}

/// Python `build_node_updates` — rebuild a node using the field schema for its
/// actual protocol.
pub fn build_node_updates(node: &Node, fields: &Map<String, Value>) -> Result<Value, String> {
    let current = obj_or_empty(Some(&load_node_edit_fields(node)));
    let raw = raw_fields(fields, &current);
    let g = |key: &str| -> String { value_to_text(&raw(key)).trim().to_string() };

    let mut outbound = obj_or_empty(if node.outbound.is_object() {
        Some(&node.outbound)
    } else {
        None
    });
    let protocol = protocol_from_node(node);
    let capabilities = field_capabilities(&protocol);
    let endpoint = capabilities
        .get("endpoint")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let server = g("server");
    let port = safe_port(
        &raw_with_default(&raw, "port", &json!(node.port)),
        node.port,
    );

    let name = {
        let text = g("name");
        if text.is_empty() {
            node.name.clone()
        } else {
            text
        }
    };
    let mut updates = Map::new();
    let group = {
        let text = g("group");
        if text.is_empty() {
            node.group.clone()
        } else {
            text
        }
    };
    updates.insert("name".into(), json!(name));
    updates.insert(
        "group".into(),
        json!(if group.is_empty() {
            "Default"
        } else {
            group.as_str()
        }),
    );

    if !endpoint {
        updates.insert("outbound".into(), Value::Object(outbound));
        return Ok(Value::Object(updates));
    }

    if protocol != "warp" {
        updates.insert("server".into(), json!(server));
        updates.insert("port".into(), json!(port));
    }

    let native_shadowsocks = protocol == "shadowsocks"
        && outbound
            .get("singbox")
            .map(Value::is_object)
            .unwrap_or(false);
    if protocol_in(
        &["vless", "vmess", "trojan", "shadowsocks", "socks", "http"],
        &protocol,
    ) && !native_shadowsocks
    {
        let settings = ensure_dict(&mut outbound, "settings");
        if protocol == "vless" || protocol == "vmess" {
            let vnext = ensure_first_dict(settings, "vnext");
            vnext.insert("address".into(), json!(server));
            vnext.insert("port".into(), json!(port));
            let user = ensure_first_dict(vnext, "users");
            user.insert("id".into(), json!(g("uuid")));
            if protocol == "vless" {
                let encryption = g("encryption");
                user.insert(
                    "encryption".into(),
                    json!(if encryption.is_empty() {
                        "none"
                    } else {
                        encryption.as_str()
                    }),
                );
                set_or_remove(user, "flow", &g("flow"));
            } else {
                let alter_id = editor_int(&raw_with_default(&raw, "alterId", &json!(0)), Some(0))?
                    .unwrap_or(0);
                user.insert("alterId".into(), json!(alter_id));
                let security = g("vmessSecurity");
                user.insert(
                    "security".into(),
                    json!(if security.is_empty() {
                        "auto"
                    } else {
                        security.as_str()
                    }),
                );
            }
        } else {
            let server_item = ensure_first_dict(settings, "servers");
            server_item.insert("address".into(), json!(server));
            server_item.insert("port".into(), json!(port));
            if protocol == "trojan" {
                server_item.insert("password".into(), json!(g("password")));
            } else if protocol == "shadowsocks" {
                server_item.insert("method".into(), json!(g("method")));
                server_item.insert("password".into(), json!(g("password")));
                set_optional_text(server_item, "plugin", &g("plugin"));
                set_optional_text(server_item, "plugin_opts", &g("pluginOpts"));
            } else {
                let username = g("username");
                if !username.is_empty() {
                    server_item.insert(
                        "users".into(),
                        json!([{"user": username, "pass": g("password")}]),
                    );
                } else {
                    server_item.shift_remove("users");
                }
            }
        }

        if protocol_in(&["vless", "vmess", "trojan"], &protocol) {
            // Python: re.fullmatch(r"[0-9a-f]{64}", certificate_pin.lower()).
            let certificate_pin = g("pinnedPeerCertSha256").to_lowercase();
            let valid_pin = certificate_pin.len() == 64
                && certificate_pin
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
            if !certificate_pin.is_empty() && !valid_pin {
                return Err("TLS SHA-256 pin должен содержать ровно 64 hex-символа".to_string());
            }
            let stream = ensure_dict(&mut outbound, "streamSettings");
            update_stream_transport(stream, &g)?;
            update_stream_security(stream, &g)?;
        }
    } else {
        if matches!(
            protocol.as_str(),
            "wireguard" | "warp" | "awg" | "anytls" | "masque"
        ) {
            set_optional(
                &mut outbound,
                "_dns",
                json!(split_editor_list(&raw_with_default(
                    &raw,
                    "dns",
                    &json!("")
                ))),
            );
        }
        let native = ensure_native(&mut outbound, &protocol);
        if protocol_in(
            &[
                "shadowsocks",
                "hysteria",
                "hysteria2",
                "tuic",
                "mieru",
                "naive",
            ],
            &protocol,
        ) {
            native.insert("server".into(), json!(server));
            native.insert("server_port".into(), json!(port));
        }
        match protocol.as_str() {
            "shadowsocks" => {
                native.insert("method".into(), json!(g("method")));
                native.insert("password".into(), json!(g("password")));
                set_optional_text(native, "plugin", &g("plugin"));
                set_optional_text(native, "plugin_opts", &g("pluginOpts"));
            }
            "hysteria" => {
                native.insert("auth_str".into(), json!(g("auth")));
                set_optional_text(native, "protocol", &g("hysteriaProtocol"));
                let up = editor_int(&raw_with_default(&raw, "upMbps", &Value::Null), None)?;
                set_optional(
                    native,
                    "up_mbps",
                    up.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                let down = editor_int(&raw_with_default(&raw, "downMbps", &Value::Null), None)?;
                set_optional(
                    native,
                    "down_mbps",
                    down.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                set_optional_text(native, "obfs", &g("obfsPassword"));
                update_native_tls(native, &g)?;
            }
            "hysteria2" => {
                native.insert("password".into(), json!(g("password")));
                let obfs_type = g("obfsType");
                let obfs_password = g("obfsPassword");
                if !obfs_type.is_empty() || !obfs_password.is_empty() {
                    // Python replaces the whole obfs object with a fresh dict.
                    let mut obfs = Map::new();
                    obfs.insert(
                        "type".into(),
                        json!(if obfs_type.is_empty() {
                            "salamander"
                        } else {
                            obfs_type.as_str()
                        }),
                    );
                    set_optional_text(&mut obfs, "password", &obfs_password);
                    native.insert("obfs".into(), Value::Object(obfs));
                } else {
                    native.shift_remove("obfs");
                }
                update_native_tls(native, &g)?;
            }
            "tuic" => {
                native.insert("uuid".into(), json!(g("uuid")));
                set_optional_text(native, "password", &g("password"));
                let congestion = g("congestionControl");
                native.insert(
                    "congestion_control".into(),
                    json!(if congestion.is_empty() {
                        "cubic"
                    } else {
                        congestion.as_str()
                    }),
                );
                let relay = g("udpRelayMode");
                native.insert(
                    "udp_relay_mode".into(),
                    json!(if relay.is_empty() {
                        "native"
                    } else {
                        relay.as_str()
                    }),
                );
                native.insert(
                    "zero_rtt_handshake".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "zeroRtt",
                        &json!(false)
                    ))),
                );
                update_native_tls(native, &g)?;
            }
            "mieru" => {
                native.insert("username".into(), json!(g("username")));
                native.insert("password".into(), json!(g("password")));
                let transport = g("nativeTransport");
                native.insert(
                    "transport".into(),
                    json!(
                        if transport.is_empty() {
                            "TCP"
                        } else {
                            transport.as_str()
                        }
                        .to_uppercase()
                    ),
                );
                set_optional(
                    native,
                    "server_ports",
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "serverPorts",
                        &json!("")
                    ))),
                );
                set_optional_text(native, "multiplexing", &g("multiplexing"));
                set_optional_text(native, "traffic_pattern", &g("trafficPattern"));
            }
            "naive" => {
                native.insert("username".into(), json!(g("username")));
                native.insert("password".into(), json!(g("password")));
                let insecure = editor_int(
                    &raw_with_default(&raw, "insecureConcurrency", &json!(0)),
                    Some(0),
                )?
                .unwrap_or(0);
                native.insert("insecure_concurrency".into(), json!(insecure));
                let headers_text = g("extraHeaders");
                if !headers_text.is_empty() {
                    let headers: Value = serde_json::from_str(&headers_text).map_err(|error| {
                        format!(
                            "Дополнительные заголовки должны быть JSON-объектом: {}",
                            py_json_error_message(&headers_text, &error)
                        )
                    })?;
                    if !headers.is_object() {
                        return Err(
                            "Дополнительные заголовки должны быть JSON-объектом".to_string()
                        );
                    }
                    native.insert("extra_headers".into(), headers);
                } else {
                    native.shift_remove("extra_headers");
                }
                let udp_options_text = g("udpOverTcpOptions");
                if !udp_options_text.is_empty() {
                    let udp_options: Value =
                        serde_json::from_str(&udp_options_text).map_err(|error| {
                            format!(
                                "UDP over TCP должен быть JSON-объектом: {}",
                                py_json_error_message(&udp_options_text, &error)
                            )
                        })?;
                    if !udp_options.is_object() {
                        return Err("UDP over TCP должен быть JSON-объектом".to_string());
                    }
                    native.insert("udp_over_tcp".into(), udp_options);
                } else {
                    native.insert(
                        "udp_over_tcp".into(),
                        json!(editor_bool(&raw_with_default(
                            &raw,
                            "udpOverTcpEnabled",
                            &json!(false)
                        ))),
                    );
                }
                native.insert(
                    "quic".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "naiveQuic",
                        &json!(false)
                    ))),
                );
                set_optional_text(
                    native,
                    "quic_congestion_control",
                    &g("quicCongestionControl"),
                );
                let tls = ensure_dict(native, "tls");
                tls.insert("enabled".into(), json!(true));
                let sni = g("sni");
                set_optional_text(
                    tls,
                    "server_name",
                    if sni.is_empty() { &server } else { &sni },
                );
                for unsupported_key in ["alpn", "insecure", "utls"] {
                    tls.shift_remove(unsupported_key);
                }
            }
            "wireguard" | "awg" => {
                let had_amnezia = native.get("amnezia").map(Value::is_object).unwrap_or(false);
                let peers_json = g("peersJson");
                if !peers_json.is_empty() {
                    let parsed_peers: Value =
                        serde_json::from_str(&peers_json).map_err(|error| {
                            format!(
                                "Peers должен быть корректным JSON-массивом: {}",
                                py_json_error_message(&peers_json, &error)
                            )
                        })?;
                    if !parsed_peers.is_array()
                        || !parsed_peers
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(Value::is_object)
                    {
                        return Err("Peers должен быть JSON-массивом объектов".to_string());
                    }
                    native.insert("peers".into(), parsed_peers);
                }
                native.insert(
                    "address".into(),
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "interfaceAddresses",
                        &json!("")
                    ))),
                );
                native.insert("private_key".into(), json!(g("privateKey")));
                let mtu = editor_int(&raw_with_default(&raw, "mtu", &json!(1408)), Some(1408))?
                    .unwrap_or(1408);
                native.insert("mtu".into(), json!(mtu));
                native.insert("system".into(), json!(false));
                let listen_port =
                    editor_int(&raw_with_default(&raw, "listenPort", &Value::Null), None)?;
                set_optional(
                    native,
                    "listen_port",
                    listen_port.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                let peer = ensure_first_dict(native, "peers");
                peer.insert("address".into(), json!(server));
                peer.insert("port".into(), json!(port));
                peer.shift_remove("server");
                peer.shift_remove("server_port");
                peer.insert("public_key".into(), json!(g("wgPublicKey")));
                set_optional_text(peer, "pre_shared_key", &g("preSharedKey"));
                peer.insert(
                    "allowed_ips".into(),
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "allowedIps",
                        &json!("")
                    ))),
                );
                let keepalive =
                    editor_int(&raw_with_default(&raw, "keepalive", &Value::Null), None)?;
                set_optional(
                    peer,
                    "persistent_keepalive_interval",
                    keepalive.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                let udp_timeout = g("udpTimeout");
                set_optional_text(
                    native,
                    "udp_timeout",
                    if udp_timeout.is_empty() {
                        "5m0s"
                    } else {
                        udp_timeout.as_str()
                    },
                );
                let workers = editor_int(&raw_with_default(&raw, "workers", &Value::Null), None)?;
                set_optional(
                    native,
                    "workers",
                    workers.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                let preallocated = editor_int(
                    &raw_with_default(&raw, "preallocatedBuffers", &Value::Null),
                    None,
                )?;
                set_optional(
                    native,
                    "preallocated_buffers_per_pool",
                    preallocated
                        .map(|value| json!(value))
                        .unwrap_or(Value::Null),
                );
                native.insert(
                    "disable_pauses".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "disablePauses",
                        &json!(false)
                    ))),
                );
                let amnezia_enabled = editor_bool(&raw_with_default(
                    &raw,
                    "amneziaEnabled",
                    &json!(protocol == "awg" || had_amnezia),
                ));
                if amnezia_enabled {
                    let amnezia = ensure_dict(native, "amnezia");
                    for key in AMNEZIA_EDITOR_KEYS {
                        let field_key = format!("awg_{key}");
                        let mut raw_value = raw(&field_key);
                        if raw_value.is_null() {
                            raw_value = json!("");
                        }
                        if protocol == "wireguard" && value_to_text(&raw_value).is_empty() {
                            raw_value = WG_COMPATIBLE_AMNEZIA_DEFAULTS
                                .iter()
                                .find(|(default_key, _)| *default_key == key)
                                .map(|(_, value)| value.clone())
                                .unwrap_or(json!(""));
                        }
                        let parsed = if AMNEZIA_NUMERIC_EDITOR_KEYS.contains(&key) {
                            match editor_int(&raw_value, None)? {
                                Some(number) => json!(number),
                                None => json!(""),
                            }
                        } else if AMNEZIA_RANGE_EDITOR_KEYS.contains(&key) {
                            let text_value = value_to_text(&raw_value).trim().to_string();
                            if text_value.is_ascii()
                                && text_value.chars().all(|c| c.is_ascii_digit())
                                && !text_value.is_empty()
                            {
                                json!(text_value.parse::<i64>().unwrap_or(0))
                            } else {
                                json!(text_value)
                            }
                        } else {
                            json!(value_to_text(&raw_value).trim())
                        };
                        set_optional(amnezia, key, parsed);
                    }
                    if native
                        .get("amnezia")
                        .map(|value| value.as_object().unwrap().is_empty())
                        .unwrap_or(false)
                    {
                        native.shift_remove("amnezia");
                    }
                    outbound.insert("protocol".into(), json!("awg"));
                    updates.insert("scheme".into(), json!("awg"));
                } else {
                    native.shift_remove("amnezia");
                    outbound.insert("protocol".into(), json!("wireguard"));
                    updates.insert("scheme".into(), json!("wireguard"));
                }
            }
            "warp" => {
                let profile = ensure_dict(native, "profile");
                profile.insert("detour".into(), json!("direct"));
                set_optional_text(profile, "id", &g("profileId"));
                set_optional_text(profile, "private_key", &g("profilePrivateKey"));
                set_optional_text(profile, "auth_token", &g("authToken"));
                let reserved = split_editor_list(&raw_with_default(&raw, "reserved", &json!("")));
                if !reserved.is_empty() {
                    let mut reserved_bytes: Vec<i64> = Vec::new();
                    for item in &reserved {
                        let parsed = item.parse::<i64>().map_err(|_| {
                            "Reserved должен содержать три числа от 0 до 255".to_string()
                        })?;
                        reserved_bytes.push(parsed);
                    }
                    if reserved_bytes.len() != 3
                        || reserved_bytes.iter().any(|item| *item < 0 || *item > 255)
                    {
                        return Err("Reserved должен содержать три числа от 0 до 255".to_string());
                    }
                    native.insert("reserved".into(), json!(reserved_bytes));
                } else {
                    native.shift_remove("reserved");
                }
                let listen_port =
                    editor_int(&raw_with_default(&raw, "listenPort", &Value::Null), None)?;
                set_optional(
                    native,
                    "listen_port",
                    listen_port.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                let keepalive =
                    editor_int(&raw_with_default(&raw, "keepalive", &Value::Null), None)?;
                set_optional(
                    native,
                    "persistent_keepalive_interval",
                    keepalive.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                set_optional_text(native, "udp_timeout", &g("udpTimeout"));
                let amnezia = ensure_dict(native, "amnezia");
                for key in AMNEZIA_EDITOR_KEYS {
                    let field_key = format!("awg_{key}");
                    let mut raw_value = raw(&field_key);
                    if raw_value.is_null() {
                        raw_value = json!("");
                    }
                    let parsed = if AMNEZIA_NUMERIC_EDITOR_KEYS.contains(&key) {
                        match editor_int(&raw_value, None)? {
                            Some(number) => json!(number),
                            None => json!(""),
                        }
                    } else if AMNEZIA_RANGE_EDITOR_KEYS.contains(&key) {
                        let text_value = value_to_text(&raw_value).trim().to_string();
                        if text_value.is_ascii()
                            && text_value.chars().all(|c| c.is_ascii_digit())
                            && !text_value.is_empty()
                        {
                            json!(text_value.parse::<i64>().unwrap_or(0))
                        } else {
                            json!(text_value)
                        }
                    } else {
                        json!(value_to_text(&raw_value).trim())
                    };
                    set_optional(amnezia, key, parsed);
                }
                if native
                    .get("amnezia")
                    .map(|value| value.as_object().unwrap().is_empty())
                    .unwrap_or(false)
                {
                    native.shift_remove("amnezia");
                }
            }
            "openvpn" => {
                let servers_json = g("openvpnServersJson");
                if !servers_json.is_empty() {
                    let servers: Value = serde_json::from_str(&servers_json).map_err(|error| {
                        format!(
                            "OpenVPN servers must be a valid JSON array: {}",
                            py_json_error_message(&servers_json, &error)
                        )
                    })?;
                    if !servers.is_array()
                        || !servers.as_array().unwrap().iter().all(Value::is_object)
                    {
                        return Err("OpenVPN servers must be a JSON array of objects".to_string());
                    }
                    native.insert("servers".into(), servers);
                }
                let server_item = ensure_first_dict(native, "servers");
                server_item.insert("server".into(), json!(server));
                server_item.insert("server_port".into(), json!(port));
                native.insert("system".into(), json!(false));
                let interface = g("interfaceName");
                native.insert(
                    "name".into(),
                    json!(if interface.is_empty() {
                        "openvpn0"
                    } else {
                        interface.as_str()
                    }),
                );
                let proto = {
                    let text = g("openvpnProto").to_lowercase();
                    if text.is_empty() {
                        "udp".to_string()
                    } else {
                        text
                    }
                };
                native.insert("proto".into(), json!(proto));
                let cipher = g("openvpnCipher").to_uppercase();
                set_optional_text(native, "cipher", &cipher);
                let auth = g("openvpnAuth").to_uppercase();
                set_optional_text(native, "auth", &auth);
                set_optional_text(native, "username", &g("username"));
                set_optional_text(native, "password", &g("password"));
                set_optional_text(native, "key_password", &g("keyPassword"));
                let proxy_type = g("openvpnProxyType").to_lowercase();
                if !proxy_type.is_empty() {
                    let proxy_server = g("openvpnProxyServer");
                    let proxy_port = editor_int(
                        &raw_with_default(&raw, "openvpnProxyPort", &Value::Null),
                        None,
                    )?;
                    if proxy_type != "http" && proxy_type != "socks" {
                        return Err("OpenVPN proxy type must be HTTP or SOCKS".to_string());
                    }
                    if proxy_server.is_empty() || proxy_port.map(|value| value == 0).unwrap_or(true)
                    {
                        return Err("OpenVPN proxy server and port are required".to_string());
                    }
                    let mut openvpn_proxy = Map::new();
                    openvpn_proxy.insert("type".into(), json!(proxy_type));
                    openvpn_proxy.insert("server".into(), json!(proxy_server));
                    openvpn_proxy.insert("server_port".into(), json!(proxy_port.unwrap_or(0)));
                    set_optional_text(&mut openvpn_proxy, "username", &g("openvpnProxyUsername"));
                    set_optional_text(&mut openvpn_proxy, "password", &g("openvpnProxyPassword"));
                    native.insert("lumen_proxy".into(), Value::Object(openvpn_proxy));
                } else {
                    native.shift_remove("lumen_proxy");
                }
                set_optional(
                    native,
                    "allowed_ips",
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "allowedIps",
                        &json!("")
                    ))),
                );
                set_optional_text(native, "reconnect_delay", &g("reconnectDelay"));
                set_optional_text(native, "ping_interval", &g("pingInterval"));
                set_optional_text(native, "ping_restart", &g("pingRestart"));

                let tls_auth = value_to_text(&raw_with_default(&raw, "tlsAuth", &json!("")))
                    .trim()
                    .to_string();
                let tls_crypt = value_to_text(&raw_with_default(&raw, "tlsCrypt", &json!("")))
                    .trim()
                    .to_string();
                if !tls_auth.is_empty() && !tls_crypt.is_empty() {
                    return Err(
                        "OpenVPN TLS Auth and TLS Crypt cannot be enabled together".to_string()
                    );
                }
                if !tls_auth.is_empty() {
                    native.insert("tls_auth".into(), json!(tls_auth));
                    native.shift_remove("tls_auth_path");
                    native.shift_remove("tls_crypt");
                    native.shift_remove("tls_crypt_path");
                    native.shift_remove("tls_crypt_v2");
                    // Python: int(_editor_int(raw("keyDirection", -1), -1) or 0).
                    let key_direction = editor_int(
                        &raw_with_default(&raw, "keyDirection", &json!(-1)),
                        Some(-1),
                    )?;
                    native.insert("key_direction".into(), json!(key_direction.unwrap_or(0)));
                } else {
                    native.shift_remove("tls_auth");
                    if !native
                        .get("tls_auth_path")
                        .map(|v| !value_to_text(v).is_empty())
                        .unwrap_or(false)
                    {
                        native.shift_remove("key_direction");
                    }
                    if !tls_crypt.is_empty() {
                        native.insert("tls_crypt".into(), json!(tls_crypt));
                        native.shift_remove("tls_crypt_path");
                        native.insert(
                            "tls_crypt_v2".into(),
                            json!(editor_bool(&raw_with_default(
                                &raw,
                                "tlsCryptV2",
                                &json!(false)
                            ))),
                        );
                    } else {
                        native.shift_remove("tls_crypt");
                        if !native
                            .get("tls_crypt_path")
                            .map(|v| !value_to_text(v).is_empty())
                            .unwrap_or(false)
                        {
                            native.shift_remove("tls_crypt_v2");
                        }
                    }
                }

                let tls = ensure_dict(native, "tls");
                for (field_key, config_key, path_key) in [
                    ("certificate", "certificate", "certificate_path"),
                    ("privateKey", "key", "key_path"),
                    ("caCertificate", "ca", "ca_path"),
                ] {
                    let content = value_to_text(&raw(field_key)).trim().to_string();
                    if !content.is_empty() {
                        tls.insert(config_key.into(), json!(content));
                        tls.shift_remove(path_key);
                    } else {
                        tls.shift_remove(config_key);
                    }
                }
                set_optional(
                    tls,
                    "cipher_suites",
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "tlsCipherSuites",
                        &json!("")
                    ))),
                );
                set_optional_text(tls, "verify_x509_name", &g("verifyX509Name"));
                if tls
                    .get("verify_x509_name")
                    .map(|v| !value_to_text(v).is_empty())
                    .unwrap_or(false)
                {
                    let mode = g("verifyX509NameMode");
                    tls.insert(
                        "verify_x509_name_mode".into(),
                        json!(if mode.is_empty() {
                            "name"
                        } else {
                            mode.as_str()
                        }),
                    );
                } else {
                    tls.shift_remove("verify_x509_name_mode");
                }
                tls.insert(
                    "kernel_tx".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "kernelTx",
                        &json!(false)
                    ))),
                );
                tls.insert(
                    "kernel_rx".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "kernelRx",
                        &json!(false)
                    ))),
                );
                let tls_any = tls.values().any(|value| {
                    !(value.is_null()
                        || value == &json!("")
                        || value == &Value::Bool(false)
                        || (value.is_array() && value.as_array().unwrap().is_empty())
                        || (value.is_object() && value.as_object().unwrap().is_empty()))
                });
                if !tls_any {
                    native.shift_remove("tls");
                }
            }
            "masque" => {
                native.insert("server".into(), json!(server));
                native.insert("server_port".into(), json!(port));
                let interface = g("interfaceName");
                native.insert(
                    "name".into(),
                    json!(if interface.is_empty() {
                        "masque0"
                    } else {
                        interface.as_str()
                    }),
                );
                native.insert(
                    "system".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "systemInterface",
                        &json!(false)
                    ))),
                );
                native.insert(
                    "use_http2".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "useHttp2",
                        &json!(false)
                    ))),
                );
                native.insert(
                    "use_ipv6".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "useIpv6",
                        &json!(false)
                    ))),
                );
                set_optional(
                    native,
                    "address",
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "interfaceAddresses",
                        &json!("")
                    ))),
                );
                set_optional_text(native, "private_key", &g("privateKey"));
                set_optional_text(native, "public_key", &g("wgPublicKey"));
                let profile = ensure_dict(native, "profile");
                profile.insert("detour".into(), json!("direct"));
                set_optional_text(profile, "id", &g("profileId"));
                set_optional_text(profile, "private_key", &g("profilePrivateKey"));
                set_optional_text(profile, "auth_token", &g("authToken"));
                set_optional(
                    native,
                    "allowed_ips",
                    json!(split_editor_list(&raw_with_default(
                        &raw,
                        "allowedIps",
                        &json!("")
                    ))),
                );
                let mtu = editor_int(&raw_with_default(&raw, "mtu", &Value::Null), None)?;
                set_optional(
                    native,
                    "mtu",
                    mtu.map(|value| json!(value)).unwrap_or(Value::Null),
                );
                set_optional_text(native, "udp_timeout", &g("udpTimeout"));
                set_optional_text(native, "udp_keepalive_period", &g("udpKeepalive"));
                set_optional_text(native, "reconnect_delay", &g("reconnectDelay"));
                set_optional_text(native, "congestion_controller", &g("congestionController"));
                let tls = ensure_dict(native, "tls");
                set_optional_text(tls, "server_name", &g("sni"));
                tls.insert(
                    "insecure".into(),
                    json!(editor_bool(&raw_with_default(
                        &raw,
                        "allowInsecure",
                        &json!(false)
                    ))),
                );
                let has_tls = tls
                    .get("server_name")
                    .map(|v| !value_to_text(v).is_empty())
                    .unwrap_or(false)
                    || tls
                        .get("insecure")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                if !has_tls {
                    native.shift_remove("tls");
                }
            }
            _ => {}
        }
    }

    let outbound_value = Value::Object(outbound);
    let link = build_node_link(
        &protocol,
        &name,
        &server,
        port,
        outbound_value.as_object().unwrap(),
    );
    updates.insert("outbound".into(), outbound_value);
    updates.insert("link".into(), json!(link));
    Ok(Value::Object(updates))
}

/// Python `json.JSONDecodeError.msg` (the human-readable part of the message).
fn py_json_error_message(source: &str, error: &serde_json::Error) -> String {
    lumen_core::pyjson::decode_error_msg(source).unwrap_or_else(|| {
        let text = error.to_string();
        match text.find(" at line") {
            Some(index) => text[..index].to_string(),
            None => text,
        }
    })
}

/// Python `_node_export_family` (bridge helper, kept beside the link builders).
pub fn node_export_family(node: &Node) -> &'static str {
    let outbound = obj_or_empty(if node.outbound.is_object() {
        Some(&node.outbound)
    } else {
        None
    });
    let proto = text_of(outbound.get("protocol")).to_lowercase();
    let proto = if proto.is_empty() {
        node.scheme.to_lowercase()
    } else {
        proto
    };
    if proto == "warp" || proto == "wireguard" || proto == "awg" || proto == "amneziawg" {
        "wireguard"
    } else if proto == "openvpn" || proto == "singbox_config" {
        "singbox-config"
    } else {
        "uri"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node_with(name: &str, scheme: &str, server: &str, port: i64, outbound: Value) -> Node {
        let mut node = Node::new();
        node.name = name.into();
        node.scheme = scheme.into();
        node.server = server.into();
        node.port = port;
        node.outbound = outbound;
        node
    }

    #[test]
    fn test_field_capabilities_matrix() {
        let caps = field_capabilities("awg");
        assert_eq!(caps["nativeSingbox"], json!(true));
        assert_eq!(caps["xrayAdvanced"], json!(false));
        assert_eq!(caps["identity"], json!(false));
        assert_eq!(caps["endpoint"], json!(true));

        let caps = field_capabilities("vless");
        assert_eq!(caps["xrayAdvanced"], json!(true));
        assert_eq!(caps["identity"], json!(true));
        assert_eq!(caps["tls"], json!(true));

        let caps = field_capabilities("singbox_config");
        assert_eq!(caps["readOnlyConfig"], json!(true));
        assert_eq!(caps["endpoint"], json!(false));
    }

    #[test]
    fn test_awg_editor_hides_xray_fields_and_updates_native_endpoint() {
        let node = node_with(
            "WARP",
            "awg",
            "engage.cloudflareclient.com",
            2408,
            json!({
                "protocol": "awg",
                "singbox": {
                    "type": "wireguard",
                    "peers": [{"server": "engage.cloudflareclient.com", "server_port": 2408}],
                    "amnezia": {"jc": 4},
                },
            }),
        );

        let fields = load_node_edit_fields(&node);
        assert_eq!(fields["capabilities"]["nativeSingbox"], json!(true));
        assert_eq!(fields["capabilities"]["xrayAdvanced"], json!(false));
        assert_eq!(fields["capabilities"]["identity"], json!(false));
        assert_eq!(fields["capabilities"]["endpoint"], json!(true));

        let updates = build_node_updates(
            &node,
            &serde_json::from_value(json!({
                "name": "WARP new",
                "group": "Native",
                "server": "162.159.192.1",
                "port": "2408",
                "uuid": "must-not-be-used",
                "network": "xhttp",
                "security": "reality",
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(updates["name"], json!("WARP new"));
        assert_eq!(updates["server"], json!("162.159.192.1"));
        assert!(updates["outbound"].get("settings").is_none());
        assert!(updates["outbound"].get("streamSettings").is_none());
        let peer = &updates["outbound"]["singbox"]["peers"][0];
        assert_eq!(peer["address"], json!("162.159.192.1"));
        assert_eq!(peer["port"], json!(2408));
        assert!(peer.get("server").is_none());
        assert!(peer.get("server_port").is_none());
    }

    #[test]
    fn test_singbox_config_editor_is_metadata_only() {
        let node = node_with(
            "Full config",
            "singbox_config",
            "",
            0,
            json!({"protocol": "singbox_config", "singbox": {"outbounds": []}}),
        );

        let fields = load_node_edit_fields(&node);
        let updates = build_node_updates(
            &node,
            &serde_json::from_value(json!({
                "name": "Renamed",
                "group": "Configs",
                "server": "bad",
                "port": "443",
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(fields["capabilities"]["readOnlyConfig"], json!(true));
        assert_eq!(fields["capabilities"]["endpoint"], json!(false));
        assert!(fields.get("tags").is_none());
        assert_eq!(
            updates,
            json!({
                "name": "Renamed",
                "group": "Configs",
                "outbound": {"protocol": "singbox_config", "singbox": {"outbounds": []}},
            })
        );
    }

    #[test]
    fn test_vless_editor_updates_and_share_link_round_trip() {
        let node = node_with(
            "VLESS",
            "vless",
            "old.example",
            443,
            json!({
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": "old.example",
                        "port": 443,
                        "users": [{"id": "old"}],
                    }],
                },
                "streamSettings": {"network": "tcp", "security": "none"},
            }),
        );

        let fields = load_node_edit_fields(&node);
        assert_eq!(fields["capabilities"]["xrayAdvanced"], json!(true));

        let updates = build_node_updates(
            &node,
            &serde_json::from_value(json!({
                "name": "VLESS new",
                "group": "Default",
                "server": "new.example",
                "port": "8443",
                "uuid": "new-id",
                "encryption": "none",
                "flow": "",
                "network": "xhttp",
                "security": "tls",
                "sni": "front.example",
                "fingerprint": "edge",
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(updates["name"], json!("VLESS new"));
        assert_eq!(updates["server"], json!("new.example"));
        assert_eq!(updates["port"], json!(8443));
        assert_eq!(
            updates["outbound"]["settings"]["vnext"][0]["address"],
            json!("new.example")
        );
        assert_eq!(
            updates["outbound"]["streamSettings"]["network"],
            json!("xhttp")
        );
        assert_eq!(
            updates["outbound"]["streamSettings"]["security"],
            json!("tls")
        );
        assert!(
            updates["link"]
                .as_str()
                .unwrap()
                .starts_with("vless://new-id@new.example:8443?")
        );
    }

    #[test]
    fn test_vless_editor_validates_tls_certificate_pin() {
        let node = node_with(
            "VLESS",
            "vless",
            "old.example",
            443,
            json!({
                "protocol": "vless",
                "settings": {"vnext": [{"address": "old.example", "port": 443, "users": [{"id": "old"}]}]},
                "streamSettings": {"network": "tcp", "security": "none"},
            }),
        );
        let mut fields = serde_json::Map::new();
        fields.insert("pinnedPeerCertSha256".into(), json!("zz123"));
        let error = build_node_updates(&node, &fields).unwrap_err();
        assert_eq!(
            error,
            "TLS SHA-256 pin должен содержать ровно 64 hex-символа"
        );
    }

    #[test]
    fn test_trojan_editor_persists_share_uri() {
        let node = node_with(
            "Trojan",
            "trojan",
            "old.example",
            443,
            json!({
                "protocol": "trojan",
                "settings": {"servers": [{"address": "old.example", "port": 443, "password": "old"}]},
                "streamSettings": {"network": "tcp", "security": "none"},
            }),
        );
        let updates = build_node_updates(
            &node,
            &serde_json::from_value(json!({
                "name": "Trojan new",
                "group": "Default",
                "server": "new.example",
                "port": "443",
                "password": "secret",
                "network": "ws",
                "transportPath": "/ws",
                "security": "none",
            }))
            .unwrap(),
        )
        .unwrap();
        let link = updates["link"].as_str().unwrap();
        assert!(link.starts_with("trojan://secret@new.example:443?"));
        assert!(link.contains("type=ws"));
        assert!(link.contains("security=none"));
    }

    #[test]
    fn test_vmess_editor_emits_vmess_share_uri() {
        let node = node_with(
            "VM",
            "vmess",
            "v.example",
            443,
            json!({
                "protocol": "vmess",
                "settings": {"vnext": [{"address": "v.example", "port": 443, "users": [{"id": "u1", "security": "auto"}]}]},
                "streamSettings": {"network": "tcp", "security": "none"},
            }),
        );
        let updates = build_node_updates(
            &node,
            &serde_json::from_value(json!({
                "name": "VM new",
                "group": "Default",
                "server": "v.example",
                "port": "443",
                "uuid": "u2",
                "alterId": 0,
                "vmessSecurity": "auto",
                "network": "tcp",
                "security": "none",
            }))
            .unwrap(),
        )
        .unwrap();
        let link = updates["link"].as_str().unwrap();
        assert!(link.starts_with("vmess://"));
        let payload = link.trim_start_matches("vmess://");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        let parsed: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(parsed["id"], json!("u2"));
        assert_eq!(parsed["add"], json!("v.example"));
    }

    #[test]
    fn test_shadowsocks_export_preserves_plugin_options_and_none_method() {
        let node = node_with(
            "SS",
            "shadowsocks",
            "s.example",
            8388,
            json!({
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [{
                        "address": "s.example",
                        "port": 8388,
                        "method": "none",
                        "password": "",
                        "plugin": "v2ray-plugin",
                        "plugin_opts": "mode=quic",
                    }],
                },
            }),
        );
        let link = normalized_node_export_link(Some(&node));
        assert!(link.starts_with("ss://"), "{link}");
        // method=none with an empty password still exports.
        assert!(
            link.contains("plugin=v2ray-plugin%3Bmode%3Dquic")
                || link.contains("plugin=v2ray-plugin;mode=quic")
        );
    }

    #[test]
    fn test_export_keeps_full_config_as_compact_json() {
        let payload = json!({"outbounds": [], "route": {"rules": []}});
        let mut node = node_with(
            "Full",
            "singbox_config",
            "",
            0,
            json!({"protocol": "singbox_config", "singbox": payload}),
        );
        node.link = r#"{"outbounds":[],"route":{"rules":[]}}"#.into();
        let link = normalized_node_export_link(Some(&node));
        assert_eq!(link, r#"{"outbounds":[],"route":{"rules":[]}}"#);
    }

    #[test]
    fn test_new_node_schema_changes_with_selected_protocol() {
        let vless = new_node_edit_fields("vless", "Default");
        assert_eq!(vless["protocolKey"], json!("vless"));
        assert_eq!(vless["capabilities"]["xrayAdvanced"], json!(true));

        let unknown = new_node_edit_fields("bogus", "Default");
        assert_eq!(unknown["protocolKey"], json!("vless"));

        let awg = new_node_edit_fields("awg", "Native");
        assert_eq!(awg["amneziaEnabled"], json!(true));

        let wg = new_node_edit_fields("wireguard", "Native");
        assert_eq!(wg["amneziaEnabled"], json!(false));
    }

    #[test]
    fn test_manual_node_creates_wireguard_editor_fields() {
        let fields = new_node_edit_fields("wireguard", "Default");
        let keys: Vec<&str> = fields["protocolFields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["key"].as_str().unwrap())
            .collect();
        assert!(keys.contains(&"interfaceAddresses"));
        assert!(keys.contains(&"privateKey"));
        assert!(keys.contains(&"peersJson"));
        assert!(keys.contains(&"amneziaEnabled"));
    }

    #[test]
    fn test_warp_and_masque_editors_have_separate_supported_fields() {
        let warp = new_node_edit_fields("warp", "Default");
        let warp_keys: Vec<&str> = warp["protocolFields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["key"].as_str().unwrap())
            .collect();
        assert!(warp_keys.contains(&"profileId"));
        assert!(!warp_keys.contains(&"server"));

        let masque = new_node_edit_fields("masque", "Default");
        let masque_keys: Vec<&str> = masque["protocolFields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["key"].as_str().unwrap())
            .collect();
        assert!(masque_keys.contains(&"profileId"));
        assert!(masque_keys.contains(&"server"));
        assert!(masque_keys.contains(&"congestionController"));
    }

    #[test]
    fn test_quote_matches_python() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("a b/c"), "a%20b%2Fc");
        assert_eq!(quote("ü"), "%C3%BC");
        assert_eq!(quote("a.b-c~d_e"), "a.b-c~d_e");
    }
}
