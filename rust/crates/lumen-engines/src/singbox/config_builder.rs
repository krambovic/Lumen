//! Sing-box outbound conversion (`engines/singbox/config_builder.py`).
//!
//! Converts a stored node outbound into a native sing-box outbound: TLS and
//! transport translation from the Xray stream schema, native `singbox`
//! payloads, and the shared normalizers applied to runtime copies of full
//! imported profiles (`_normalize_singbox_runtime_copy` in the runtime
//! planner consumes the public normalizers below).

use anyhow::{bail, Result};
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig, STANDARD};
use base64::engine::DecodePaddingMode;
use base64::{alphabet, Engine as _};
use serde_json::{json, Map, Value};

use lumen_core::config::multiplex::apply_singbox_multiplex;
use lumen_core::models::Node;
use lumen_core::openvpn_import::openvpn_requires_user_auth;
use lumen_core::openvpn_normalization::{
    normalize_openvpn_outbound, openvpn_private_key_is_encrypted,
};
use lumen_core::wireguard_normalization::normalize_wireguard_endpoint;

use crate::singbox::runtime_planner::{
    is_falsy_json, json_str_or, lower_trim, py_int, py_int_or, py_str_list_item as py_str,
};

const SUPPORTED_NATIVE_PROTOCOLS: [&str; 18] = [
    "vless",
    "vmess",
    "trojan",
    "shadowsocks",
    "socks",
    "http",
    "warp",
    "wireguard",
    "awg",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "masque",
    "openvpn",
    "naive",
    "anytls",
    "snell",
];

const DEFAULT_HYSTERIA_UP_MBPS: i64 = 50;
const DEFAULT_HYSTERIA_DOWN_MBPS: i64 = 200;

const SUPPORTED_SHADOWSOCKS_METHODS: [&str; 18] = [
    "none",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
    "chacha20-ietf-poly1305",
    "xchacha20-ietf-poly1305",
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
    "2022-blake3-chacha20-poly1305",
    "aes-128-ctr",
    "aes-192-ctr",
    "aes-256-ctr",
    "aes-128-cfb",
    "aes-192-cfb",
    "aes-256-cfb",
    "rc4-md5",
    "chacha20-ietf",
    "xchacha20",
];

const SUPPORTED_SHADOWSOCKS_PLUGINS: [&str; 2] = ["obfs-local", "v2ray-plugin"];

const SUPPORTED_VMESS_SECURITY: [&str; 5] = [
    "auto",
    "none",
    "zero",
    "aes-128-gcm",
    "chacha20-poly1305",
];

const MISSING_VMESS_SECURITY: [&str; 3] = ["", "null", "undefined"];

// ── Python dict/str semantics helpers ───────────────────────────────────────

/// Python `str(value)` for JSON scalars (`None` renders as `None`, booleans
/// as `True`/`False`).
fn json_str_plain(value: Option<&Value>) -> String {
    value.map(py_str).unwrap_or_else(|| "None".to_string())
}

/// Python `str(a or b or ... or "")` over JSON values: the first truthy item
/// is stringified, otherwise the result is empty.
fn or_str(items: &[Option<&Value>]) -> String {
    items
        .iter()
        .flatten()
        .find(|v| !is_falsy_json(v))
        .map(|v| py_str(v))
        .unwrap_or_default()
}

/// Python membership test `value not in (None, "", [], {})`.
fn present_nonempty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        Some(_) => true,
    }
}

/// Python `dict.get(key, default)`: a stored `None` still counts as present.
fn get_with_default<'a>(
    parent: &'a Value,
    key: &str,
    default: Option<&'a Value>,
) -> Option<&'a Value> {
    parent.get(key).or(default)
}

fn set_key(value: &mut Value, key: &str, item: Value) {
    if let Some(map) = value.as_object_mut() {
        map.insert(key.to_string(), item);
    }
}

fn remove_key(value: &mut Value, key: &str) {
    if let Some(map) = value.as_object_mut() {
        map.shift_remove(key);
    }
}

/// Python `dict.setdefault(key, value)`.
fn setdefault_key(map: &mut Map<String, Value>, key: &str, item: Value) {
    map.entry(key.to_string()).or_insert(item);
}

fn setdefault_unsupported(sb: &mut Value, message: &str) {
    if let Some(map) = sb.as_object_mut() {
        map.entry("_unsupported_transport".to_string())
            .or_insert_with(|| Value::String(message.to_string()));
    }
}

fn as_map(value: &Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// Python `(x or [{}])[0]`: the first item of a truthy list, otherwise a
/// missing value (reads on it return nothing, matching the empty dict).
fn first_list_item(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Array(items)) => items.first().cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// Python `str.partition(sep)`; the flag marks whether the separator was
/// found (Python always returns three parts).
fn py_partition(text: &str, sep: char) -> (&str, bool, &str) {
    match text.find(sep) {
        Some(index) => (&text[..index], true, &text[index + sep.len_utf8()..]),
        None => (text, false, ""),
    }
}

/// Walks every object/list node of a JSON tree. Objects are visited first,
/// then their children recurse (lists are never visited themselves) — the
/// traversal order of the Python recursive normalizers.
fn walk_values_mut(
    value: &mut Value,
    visit: &mut dyn FnMut(&mut Value) -> Result<()>,
) -> Result<()> {
    if value.is_object() {
        visit(value)?;
    }
    match value {
        Value::Object(map) => {
            for child in map.values_mut() {
                walk_values_mut(child, visit)?;
            }
        }
        Value::Array(items) => {
            for child in items.iter_mut() {
                walk_values_mut(child, visit)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// ── Base64 engines (Python `base64` module semantics) ───────────────────────

const B64_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";

/// Python `base64.b64decode(..., validate=True)`: canonical padding is
/// required, foreign characters raise, non-zero trailing bits are accepted.
fn strict_standard_engine() -> GeneralPurpose {
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(DecodePaddingMode::RequireCanonical),
    )
}

/// Python `base64.b64decode(text, altchars=b"-_", validate=True)`; returns
/// the decoded length. Foreign characters raise like `binascii.Error`.
fn python_altchars_decode_len(text: &str) -> Result<usize> {
    if !text.is_ascii() {
        bail!("invalid Base64 key for Shadowsocks 2022");
    }
    let mapped = text.replace('-', "+").replace('_', "/");
    if mapped.chars().any(|ch| !B64_ALPHABET.contains(ch)) {
        bail!("invalid Base64 key for Shadowsocks 2022");
    }
    let decoded = strict_standard_engine()
        .decode(mapped.as_bytes())
        .map_err(|_| anyhow::anyhow!("invalid Base64 key for Shadowsocks 2022"))?;
    Ok(decoded.len())
}

/// Python `base64.b64decode(padded, validate=True)` over the plain alphabet.
fn python_standard_decode_strict(text: &str) -> Result<Vec<u8>> {
    if text.chars().any(|ch| !B64_ALPHABET.contains(ch)) {
        bail!("invalid Base64 payload");
    }
    strict_standard_engine()
        .decode(text.as_bytes())
        .map_err(|_| anyhow::anyhow!("invalid Base64 payload"))
}

// ── Hysteria speeds ─────────────────────────────────────────────────────────

fn hysteria_speed_missing(sb: &Value, mbps_key: &str, str_key: &str) -> bool {
    // Python `isinstance(value, (int, float))` also matches bools.
    let positive = match sb.get(mbps_key) {
        Some(Value::Number(number)) => number.as_f64().map(|f| f > 0.0).unwrap_or(false),
        Some(Value::Bool(true)) => true,
        _ => false,
    };
    if positive {
        return false;
    }
    json_str_or(sb.get(str_key), "").trim().is_empty()
}

/// Python `_ensure_hysteria_speeds`: sing-box Hysteria v1 refuses to
/// initialize without non-zero up/down speeds; supply defaults when the
/// source link omitted them.
pub fn ensure_hysteria_speeds(sb: &mut Value) {
    if lower_trim(&json_str_or(sb.get("type"), "")) != "hysteria" {
        return;
    }
    if hysteria_speed_missing(sb, "up_mbps", "up") {
        set_key(sb, "up_mbps", json!(DEFAULT_HYSTERIA_UP_MBPS));
    }
    if hysteria_speed_missing(sb, "down_mbps", "down") {
        set_key(sb, "down_mbps", json!(DEFAULT_HYSTERIA_DOWN_MBPS));
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

/// Python `build_singbox_outbound`: convert a stored node outbound into a
/// native sing-box outbound.
pub fn build_singbox_outbound(
    node: &Node,
    tag: &str,
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
) -> Result<Value> {
    let protocol = lower_trim(&json_str_or(node.outbound.get("protocol"), ""));
    if !SUPPORTED_NATIVE_PROTOCOLS.contains(&protocol.as_str()) {
        let shown = if protocol.is_empty() { "unknown" } else { protocol.as_str() };
        bail!(
            "Текущий сервер нельзя конвертировать в native sing-box outbound: protocol `{shown}`"
        );
    }

    let mut outbound = convert_outbound(&node.outbound, tag)?;
    preserve_or_reject_semantic_fields(&mut outbound)?;
    // Current sing-box no longer accepts Xray's grpcSettings.authority.
    // Sanitize native/imported payloads before validation as well.
    strip_removed_transport_fields(&mut outbound)?;
    normalize_vmess_security(&mut outbound)?;
    normalize_vless_vision(&mut outbound);
    normalize_shadowsocks_methods(&mut outbound)?;

    let unsupported_transport = {
        let value = outbound.get("_unsupported_transport").cloned();
        remove_key(&mut outbound, "_unsupported_transport");
        or_str(&[value.as_ref()]).trim().to_string()
    };
    if !unsupported_transport.is_empty() {
        bail!(
            "Текущий сервер нельзя конвертировать в native sing-box outbound: transport `{unsupported_transport}` не поддерживается"
        );
    }

    set_key(&mut outbound, "tag", json!(tag));
    if ["openvpn", "openvpn-client"]
        .contains(&lower_trim(&json_str_or(outbound.get("type"), "")).as_str())
    {
        let popped_auth = outbound
            .get("lumen_requires_user_auth")
            .cloned()
            .unwrap_or(Value::Bool(false));
        remove_key(&mut outbound, "lumen_requires_user_auth");
        let requires_user_auth =
            !is_falsy_json(&popped_auth) || openvpn_requires_user_auth(&node.link);
        let key_password = json_str_or(outbound.get("key_password"), "")
            .trim()
            .to_string();
        normalize_openvpn_outbound(&mut outbound)?;
        if openvpn_private_key_is_encrypted(&outbound) {
            if key_password.is_empty() {
                bail!("OpenVPN private key is encrypted and requires a private key password");
            }
            bail!("Encrypted OpenVPN private keys are not supported by the current core");
        }
        if requires_user_auth {
            let username = json_str_or(outbound.get("username"), "").trim().to_string();
            let password = json_str_or(outbound.get("password"), "").trim().to_string();
            if username.is_empty() || password.is_empty() {
                bail!("OpenVPN node requires a username and password");
            }
        }
    }
    ensure_hysteria_speeds(&mut outbound);
    apply_singbox_multiplex(&mut outbound, multiplex_enabled, multiplex_concurrency);
    Ok(outbound)
}

/// Python `_convert_outbound`.
pub fn convert_outbound(xray_ob: &Value, tag: &str) -> Result<Value> {
    let protocol = lower_trim(&json_str_or(xray_ob.get("protocol"), ""));

    if let Some(native) = xray_ob.get("singbox").filter(|value| value.is_object()) {
        let mut sb = normalize_wireguard_endpoint(native)?;
        if ["openvpn", "openvpn-client"]
            .contains(&lower_trim(&json_str_or(sb.get("type"), "")).as_str())
        {
            set_key(&mut sb, "system", json!(false));
            let name = json_str_or(sb.get("name"), "openvpn0");
            set_key(&mut sb, "name", json!(name));
        }
        set_key(&mut sb, "tag", json!(tag));
        return Ok(sb);
    }

    let empty = json!({});
    let settings = xray_ob
        .get("settings")
        .filter(|value| value.is_object())
        .unwrap_or(&empty);
    let stream = xray_ob
        .get("streamSettings")
        .filter(|value| value.is_object())
        .unwrap_or(&empty);

    let mut sb = Map::new();
    sb.insert("type".into(), json!(protocol.clone()));

    if protocol == "vless" || protocol == "vmess" {
        let vnext = first_list_item(settings.get("vnext"));
        let users = first_list_item(vnext.get("users"));
        sb.insert("server".into(), json!(json_str_or(vnext.get("address"), "")));
        sb.insert("server_port".into(), json!(py_int_or(vnext.get("port"), 0)));
        sb.insert("uuid".into(), json!(json_str_or(users.get("id"), "")));
        if protocol == "vless" {
            let encryption = json_str_or(users.get("encryption"), "").trim().to_string();
            // Classic VLESS stores the literal value `none`, while the
            // extended core accepts the complete VLESS Encryption descriptor
            // (for example mlkem768x25519plus.native.0rtt.<key>) on the
            // outbound itself.  Dropping it makes otherwise valid servers
            // fail only after the first dial.
            if !encryption.is_empty() && lower_trim(&encryption) != "none" {
                sb.insert("encryption".into(), json!(encryption));
            }
            // State XUDP explicitly.  The current extended fork defaults to it,
            // but older/newer compatible cores do not all share that default.
            sb.insert("packet_encoding".into(), json!("xudp"));
            let flow = json_str_or(users.get("flow"), "");
            if !flow.is_empty() {
                sb.insert("flow".into(), json!(flow));
            }
        } else {
            sb.insert("alter_id".into(), json!(py_int_or(users.get("alterId"), 0)));
            sb.insert(
                "security".into(),
                json!(json_str_or(users.get("security"), "auto")),
            );
        }
    } else if protocol == "trojan" {
        let servers = first_list_item(settings.get("servers"));
        sb.insert("server".into(), json!(json_str_or(servers.get("address"), "")));
        sb.insert("server_port".into(), json!(py_int_or(servers.get("port"), 0)));
        sb.insert(
            "password".into(),
            json!(json_str_or(servers.get("password"), "")),
        );
    } else if protocol == "shadowsocks" {
        let servers = first_list_item(settings.get("servers"));
        sb.insert("server".into(), json!(json_str_or(servers.get("address"), "")));
        sb.insert("server_port".into(), json!(py_int_or(servers.get("port"), 0)));
        sb.insert("method".into(), json!(json_str_or(servers.get("method"), "")));
        sb.insert(
            "password".into(),
            json!(json_str_or(servers.get("password"), "")),
        );
        let plugin = json_str_or(servers.get("plugin"), "").trim().to_string();
        if !plugin.is_empty() {
            sb.insert("plugin".into(), json!(plugin));
        }
        let plugin_opts = json_str_or(servers.get("plugin_opts"), "")
            .trim()
            .to_string();
        if !plugin_opts.is_empty() {
            sb.insert("plugin_opts".into(), json!(plugin_opts));
        }
    } else if protocol == "socks" || protocol == "http" {
        let servers = first_list_item(settings.get("servers"));
        sb.insert("server".into(), json!(json_str_or(servers.get("address"), "")));
        sb.insert("server_port".into(), json!(py_int_or(servers.get("port"), 0)));
        if let Some(user_list) = servers
            .get("users")
            .filter(|v| !is_falsy_json(v))
            .and_then(Value::as_array)
        {
            if let Some(first) = user_list.first() {
                sb.insert("username".into(), json!(json_str_or(first.get("user"), "")));
                sb.insert("password".into(), json!(json_str_or(first.get("pass"), "")));
            }
        }
    }

    let mut sb = Value::Object(sb);
    let server = json_str_or(sb.get("server"), "");
    apply_tls(&mut sb, stream, &server)?;
    apply_transport(&mut sb, stream)?;
    if ["shadowsocks", "socks", "http"].contains(&protocol.as_str())
        && sb.get("transport").is_some()
    {
        // These exact outbound schemas do not embed V2Ray transports. The
        // Xray runtime can still handle the original streamSettings, so mark
        // the conversion unsupported instead of emitting fatal JSON.
        remove_key(&mut sb, "transport");
        set_key(
            &mut sb,
            "_unsupported_transport",
            json!(json_str_or(stream.get("network"), "unknown")),
        );
    }
    if ["shadowsocks", "socks"].contains(&protocol.as_str()) && sb.get("tls").is_some() {
        remove_key(&mut sb, "tls");
        set_key(
            &mut sb,
            "_unsupported_transport",
            json!(format!("{protocol} with TLS")),
        );
    }
    Ok(sb)
}

// ── SIP003 option strings ───────────────────────────────────────────────────

/// Python `_split_sip003_options`: split SIP003 options without breaking
/// escaped separators.
pub fn split_sip003_options(value: Option<&Value>) -> Vec<String> {
    let text = or_str(&[value]).trim().to_string();
    if text.is_empty() {
        return Vec::new();
    }
    let mut result: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in text.chars() {
        if ch == ';' && !escaped {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
        if ch == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

/// Index of the first unescaped `=` in a SIP003 option.
fn find_unescaped_separator(option: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in option.char_indices() {
        if ch == '=' && !escaped {
            return Some(index);
        }
        if ch == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    None
}

fn map_obfs_key(key: &str) -> String {
    match key.trim().to_lowercase().as_str() {
        "mode" => "obfs".to_string(),
        "host" => "obfs-host".to_string(),
        _ => key.to_string(),
    }
}

/// Python `_normalize_obfs_options`.
pub fn normalize_obfs_options(value: Option<&Value>) -> String {
    let mut normalized: Vec<String> = Vec::new();
    for option in split_sip003_options(value) {
        match find_unescaped_separator(&option) {
            None => normalized.push(map_obfs_key(&option)),
            Some(index) => {
                let mapped = map_obfs_key(&option[..index]);
                normalized.push(format!("{mapped}={}", &option[index + 1..]));
            }
        }
    }
    normalized.join(";")
}

/// Python `_normalize_v2ray_plugin_options`: keep SIP003 v2ray-plugin mux
/// compatible with sing-box.
///
/// The standalone plugin commonly accepts boolean-looking values, but
/// sing-box's built-in implementation parses `mux` with `strconv.Atoi`.
/// Normalize the common boolean spellings and discard an otherwise invalid
/// mux option so one imported server cannot invalidate the whole runtime.
pub fn normalize_v2ray_plugin_options(value: Option<&Value>) -> String {
    let mut normalized: Vec<String> = Vec::new();
    for option in split_sip003_options(value) {
        let (key, raw_value) = match find_unescaped_separator(&option) {
            Some(index) => (
                option[..index].to_string(),
                option[index + 1..].trim().to_string(),
            ),
            None => (option.clone(), String::new()),
        };

        if key.trim().to_lowercase() != "mux" {
            normalized.push(option);
            continue;
        }

        let lowered = raw_value.to_lowercase();
        if ["", "true", "yes", "on"].contains(&lowered.as_str()) {
            normalized.push(format!("{key}=1"));
        } else if ["false", "no", "off"].contains(&lowered.as_str()) {
            normalized.push(format!("{key}=0"));
        } else if let Ok(number) = raw_value.parse::<i64>() {
            normalized.push(format!("{key}={number}"));
        } else {
            // Absence means the sing-box default (mux enabled with one
            // stream) and is safer than failing every outbound at startup.
            continue;
        }
    }
    normalized.join(";")
}

// ── Shadowsocks normalization ───────────────────────────────────────────────

/// Python `_shadowsocks_2022_key_sizes`.
pub fn shadowsocks_2022_key_sizes(password: &str) -> Result<Vec<i64>> {
    let mut sizes: Vec<i64> = Vec::new();
    for segment in password.split(':') {
        let mut clean = segment.trim().to_string();
        if clean.is_empty() {
            bail!("invalid Base64 key for Shadowsocks 2022");
        }
        let padding = (4 - clean.len() % 4) % 4;
        clean.push_str(&"=".repeat(padding));
        sizes.push(python_altchars_decode_len(&clean)? as i64);
    }
    Ok(sizes)
}

/// Python `_normalize_shadowsocks_outbound`.
pub fn normalize_shadowsocks_outbound(value: &mut Map<String, Value>) -> Result<()> {
    let raw_method = json_str_or(value.get("method"), "").trim().to_string();
    if raw_method.is_empty() {
        bail!("Shadowsocks node has no encryption method");
    }
    let mut method = raw_method.to_lowercase();
    if method == "chacha20-poly1305" {
        method = "chacha20-ietf-poly1305".to_string();
    }
    if !SUPPORTED_SHADOWSOCKS_METHODS.contains(&method.as_str()) {
        bail!("Unsupported Shadowsocks method `{raw_method}`");
    }

    let password = json_str_or(value.get("password"), "");
    if password.is_empty() && method != "none" {
        bail!("Shadowsocks node has no password");
    }

    if method.starts_with("2022-") {
        let expected: i64 = if method == "2022-blake3-aes-128-gcm" { 16 } else { 32 };
        let sizes = shadowsocks_2022_key_sizes(&password)?;
        let mut corrected: Option<&str> = None;
        if method == "2022-blake3-aes-128-gcm" && sizes.iter().all(|size| *size == 32) {
            corrected = Some("2022-blake3-aes-256-gcm");
        } else if method == "2022-blake3-aes-256-gcm" && sizes.iter().all(|size| *size == 16) {
            corrected = Some("2022-blake3-aes-128-gcm");
        }
        if !sizes.iter().all(|size| *size == expected) {
            match corrected {
                None => {
                    let rendered = sizes
                        .iter()
                        .map(|size| size.to_string())
                        .collect::<Vec<_>>()
                        .join("/");
                    bail!("invalid key length for {method}: required {expected}, got {rendered}");
                }
                Some(name) => method = name.to_string(),
            }
        }
    }

    let mut plugin = json_str_or(value.get("plugin"), "").trim().to_lowercase();
    plugin = match plugin.as_str() {
        "obfs" | "simple-obfs" => "obfs-local".to_string(),
        _ => plugin,
    };
    if !plugin.is_empty() {
        if !SUPPORTED_SHADOWSOCKS_PLUGINS.contains(&plugin.as_str()) {
            bail!("Unsupported Shadowsocks plugin `{plugin}`");
        }
        value.insert("plugin".into(), json!(plugin.clone()));
        let mut options = json_str_or(value.get("plugin_opts"), "").trim().to_string();
        if plugin == "obfs-local" && !options.is_empty() {
            options = normalize_obfs_options(Some(&json!(options)));
        } else if plugin == "v2ray-plugin" && !options.is_empty() {
            options = normalize_v2ray_plugin_options(Some(&json!(options)));
        }
        if !options.is_empty() {
            value.insert("plugin_opts".into(), json!(options));
        } else {
            value.shift_remove("plugin_opts");
        }
    } else {
        value.shift_remove("plugin");
        value.shift_remove("plugin_opts");
    }

    if udp_over_tcp_enabled(value.get("udp_over_tcp")) {
        value.shift_remove("multiplex");
    }
    value.insert("method".into(), json!(method));
    Ok(())
}

/// Python `_udp_over_tcp_enabled`.
pub fn udp_over_tcp_enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Object(map)) => {
            // `value.get("enabled") is not False`: only a literal `False`
            // disables it.
            !matches!(map.get("enabled"), Some(Value::Bool(false)))
        }
        Some(other) => !is_falsy_json(other),
        None => false,
    }
}

fn normalize_shadowsocks_methods_object(value: &mut Value) -> Result<()> {
    let Some(map) = value.as_object_mut() else {
        return Ok(());
    };
    if lower_trim(&json_str_or(map.get("type"), "")) != "shadowsocks" {
        return Ok(());
    }
    normalize_shadowsocks_outbound(map)
}

/// Python `_normalize_shadowsocks_methods`: translate the Xray/SIP002
/// ChaCha20 AEAD alias only for Shadowsocks.
///
/// Apply this to runtime copies, including every outbound in a full imported
/// config. A bad non-selected outbound otherwise aborts sing-box
/// initialization. Do not rewrite VMess security, legacy ChaCha20 stream
/// ciphers, SS2022 methods, passwords or similarly named VMess/Sudoku fields.
/// Unsupported values are rejected before the core can abort a
/// several-hundred-node runtime.
pub fn normalize_shadowsocks_methods(value: &mut Value) -> Result<()> {
    walk_values_mut(value, &mut normalize_shadowsocks_methods_object)
}

// ── VMess security ──────────────────────────────────────────────────────────

fn normalize_vmess_security_object(value: &mut Value) -> Result<()> {
    let Some(map) = value.as_object_mut() else {
        return Ok(());
    };
    if lower_trim(&json_str_or(map.get("type"), "")) != "vmess" {
        return Ok(());
    }
    let security = json_str_or(map.get("security"), "").trim().to_lowercase();
    if MISSING_VMESS_SECURITY.contains(&security.as_str()) {
        map.insert("security".into(), json!("auto"));
    } else if !SUPPORTED_VMESS_SECURITY.contains(&security.as_str()) {
        bail!("Unsupported VMess security type: {security}");
    } else {
        map.insert("security".into(), json!(security));
    }
    Ok(())
}

/// Python `_normalize_vmess_security`: normalize VMess ciphers before strict
/// sing-box config decoding.
///
/// Some subscription generators serialize an omitted cipher as the literal
/// string `null`. It means "use the client default", but sing-box treats it
/// as an unknown cipher and rejects the entire runtime, including otherwise
/// valid nodes in a large selector. Apply the repair recursively so native
/// full configs and dependency outbounds receive the same protection as
/// links.
pub fn normalize_vmess_security(value: &mut Value) -> Result<()> {
    walk_values_mut(value, &mut normalize_vmess_security_object)
}

// ── VLESS Vision ────────────────────────────────────────────────────────────

fn normalize_vless_vision_object(value: &mut Value) {
    let Some(map) = value.as_object_mut() else {
        return;
    };
    if lower_trim(&json_str_or(map.get("type"), "")) != "vless" {
        return;
    }
    let flow = json_str_or(map.get("flow"), "").trim().to_lowercase();
    if flow != "xtls-rprx-vision" {
        return;
    }
    let tls_enabled = map
        .get("tls")
        .and_then(Value::as_object)
        .map(|tls| matches!(tls.get("enabled"), Some(Value::Bool(true))))
        .unwrap_or(false);
    let encryption = json_str_or(map.get("encryption"), "")
        .trim()
        .to_lowercase();
    let encryption_enabled = !encryption.is_empty() && encryption != "none";
    // The extended core obtains the Vision base connection through a V2Ray
    // transport hook, so XHTTP/gRPC/WS are valid when TLS/Reality or VLESS
    // Encryption is actually present.
    if !tls_enabled && !encryption_enabled {
        map.shift_remove("flow");
    }
}

/// Python `_normalize_vless_vision`: remove Vision only when its TLS/Reality
/// or Encryption base is absent.
///
/// Lumen's extended sing-box core supports Vision over a V2Ray transport
/// (including XHTTP) through its transport connection hook.  Stripping flow
/// merely because a transport is present breaks otherwise valid nodes.  The
/// runtime warning is caused by a missing TLS/Reality and VLESS Encryption,
/// so only that genuinely invalid combination is normalized.
///
/// The recursive form also sanitizes imported full sing-box configurations,
/// not only links converted from Xray's schema.
pub fn normalize_vless_vision(value: &mut Value) {
    let _ = walk_values_mut(value, &mut |node: &mut Value| {
        normalize_vless_vision_object(node);
        Ok(())
    });
}

// ── TLS ─────────────────────────────────────────────────────────────────────

fn copy_alpn(tls: &mut Map<String, Value>, value: Option<&Value>) {
    let Some(alpn) = value.filter(|v| !is_falsy_json(v)) else {
        return;
    };
    let normalized = match alpn {
        Value::Array(items) => Value::Array(items.clone()),
        other => Value::Array(vec![Value::String(py_str(other))]),
    };
    tls.insert("alpn".into(), normalized);
}

/// Python `_apply_tls`.
pub fn apply_tls(sb: &mut Value, stream: &Value, server: &str) -> Result<()> {
    let security = lower_trim(&json_str_or(stream.get("security"), ""));
    if security != "tls" && security != "reality" {
        return Ok(());
    }

    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if let Some(tls_settings) = stream.get("tlsSettings").filter(|value| value.is_object()) {
        if !json_str_or(tls_settings.get("verifyPeerCertByName"), "")
            .trim()
            .is_empty()
        {
            setdefault_unsupported(sb, "TLS verifyPeerCertByName requires Xray");
        }
    }

    if security == "reality" {
        let empty = json!({});
        let reality = stream
            .get("realitySettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mldsa = or_str(&[reality.get("mldsa65Verify"), reality.get("mldsa65_verify")]);
        if !mldsa.trim().is_empty() {
            setdefault_unsupported(sb, "Reality ML-DSA verification requires Xray");
        }
        let spider_x = or_str(&[reality.get("spiderX"), reality.get("spider_x")])
            .trim()
            .to_string();
        if !spider_x.is_empty() && spider_x != "/" {
            setdefault_unsupported(sb, "Reality SpiderX requires Xray");
        }
        let server_name = match reality.get("serverName").filter(|v| !is_falsy_json(v)) {
            Some(value) => py_str(value),
            None => server.to_string(),
        };
        let server_name = server_name.trim().to_string();
        if !server_name.is_empty() {
            tls.insert("server_name".into(), json!(server_name));
        }
        copy_alpn(&mut tls, reality.get("alpn"));
        let fingerprint = normalize_utls_fingerprint(reality.get("fingerprint"))?;
        // The Reality client in the bundled core requires uTLS even when the
        // share link omitted fp=.
        let fingerprint = if fingerprint.is_empty() {
            "chrome".to_string()
        } else {
            fingerprint
        };
        tls.insert(
            "utls".into(),
            json!({"enabled": true, "fingerprint": fingerprint}),
        );
        let public_key = json_str_or(reality.get("publicKey"), "");
        let short_id = json_str_or(reality.get("shortId"), "");
        tls.insert(
            "reality".into(),
            json!({"enabled": true, "public_key": public_key, "short_id": short_id}),
        );
        tls.insert("insecure".into(), json!(false));
    } else {
        let empty = json!({});
        let tls_settings = stream
            .get("tlsSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let inferred = infer_transport_host(stream);
        let server_name = match tls_settings.get("serverName").filter(|v| !is_falsy_json(v)) {
            Some(value) => py_str(value),
            None => {
                if !inferred.is_empty() {
                    inferred
                } else {
                    server.to_string()
                }
            }
        };
        let server_name = server_name.trim().to_string();
        if !server_name.is_empty() {
            tls.insert("server_name".into(), json!(server_name));
        }
        copy_alpn(&mut tls, tls_settings.get("alpn"));
        let fingerprint = normalize_utls_fingerprint(tls_settings.get("fingerprint"))?;
        if !fingerprint.is_empty() {
            tls.insert(
                "utls".into(),
                json!({"enabled": true, "fingerprint": fingerprint}),
            );
        }
        let false_value = Value::Bool(false);
        if to_bool(tls_settings.get("allowInsecure").unwrap_or(&false_value)) {
            tls.insert("insecure".into(), json!(true));
        }
        let pins = [
            "pinSHA256",
            "pinnedPeerCertSha256",
            "certificatePublicKeySha256",
            "certificate_public_key_sha256",
        ]
        .iter()
        .filter_map(|key| tls_settings.get(key))
        .find(|v| !is_falsy_json(v))
        .cloned();
        if let Some(pins) = pins {
            let normalized = normalize_certificate_public_key_pins(&pins)?;
            tls.insert("certificate_public_key_sha256".into(), json!(normalized));
        }
        for (source_key, target_key) in [
            ("disableSNI", "disable_sni"),
            ("minVersion", "min_version"),
            ("maxVersion", "max_version"),
            ("cipherSuites", "cipher_suites"),
            ("curvePreferences", "curve_preferences"),
            ("handshakeTimeout", "handshake_timeout"),
        ] {
            if let Some(value) = tls_settings
                .get(source_key)
                .filter(|v| present_nonempty(Some(v)))
            {
                tls.insert(target_key.into(), value.clone());
            }
        }
        let ech_config = json_str_or(tls_settings.get("echConfigList"), "");
        if !ech_config.is_empty() {
            if ech_config.contains("://") {
                let (query_server, server_url) = match ech_config.find('+') {
                    Some(index) => (&ech_config[..index], &ech_config[index + 1..]),
                    None => (ech_config.as_str(), ""),
                };
                let mut ech = Map::new();
                ech.insert("enabled".into(), json!(true));
                if !query_server.is_empty() && !server_url.is_empty() {
                    ech.insert("query_server_name".into(), json!(query_server));
                }
                tls.insert("ech".into(), Value::Object(ech));
            } else {
                tls.insert(
                    "ech".into(),
                    json!({
                        "enabled": true,
                        "config": [format!("-----BEGIN ECH CONFIGS-----\n{ech_config}\n-----END ECH CONFIGS-----")],
                    }),
                );
            }
        }
    }

    set_key(sb, "tls", Value::Object(tls));
    Ok(())
}

/// Python `_infer_transport_host`.
pub fn infer_transport_host(stream: &Value) -> String {
    let network = json_str_or(stream.get("network"), "")
        .trim()
        .to_lowercase();
    if ["tcp", "raw", "ws"].contains(&network.as_str()) {
        let settings = if network == "ws" {
            stream.get("wsSettings")
        } else {
            stream.get("tcpSettings")
        };
        if let Some(settings) = settings.filter(|value| value.is_object()) {
            let headers = if network == "ws" {
                settings.get("headers").cloned()
            } else {
                settings
                    .get("header")
                    .filter(|value| value.is_object())
                    .and_then(|header| header.get("request"))
                    .filter(|value| value.is_object())
                    .and_then(|request| request.get("headers"))
                    .cloned()
            };
            if let Some(headers) = headers.filter(|value| value.is_object()) {
                let host = headers
                    .get("Host")
                    .filter(|v| !is_falsy_json(v))
                    .or_else(|| headers.get("host").filter(|v| !is_falsy_json(v)));
                if let Some(host) = host {
                    if let Value::Array(items) = host {
                        return items
                            .first()
                            .map(|v| py_str(v).trim().to_string())
                            .unwrap_or_default();
                    }
                    return py_str(host)
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                }
            }
        }
    }
    if network == "http" || network == "h2" {
        let empty = json!({});
        let settings = stream
            .get("httpSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let host = settings.get("host");
        if let Some(Value::Array(items)) = host {
            if let Some(first) = items.first() {
                return py_str(first).trim().to_string();
            }
        }
        if let Some(Value::String(text)) = host {
            return text.split(',').next().unwrap_or("").trim().to_string();
        }
        return String::new();
    }
    if network == "grpc" {
        let empty = json!({});
        let settings = stream
            .get("grpcSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        return json_str_or(settings.get("authority"), "").trim().to_string();
    }
    if network == "xhttp" || network == "httpupgrade" {
        let key = if network == "xhttp" {
            "xhttpSettings"
        } else {
            "httpupgradeSettings"
        };
        let empty = json!({});
        let settings = stream.get(key).filter(|value| value.is_object()).unwrap_or(&empty);
        return json_str_or(settings.get("host"), "")
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }
    String::new()
}

// ── Transports ──────────────────────────────────────────────────────────────

/// Python `_apply_transport`.
pub fn apply_transport(sb: &mut Value, stream: &Value) -> Result<()> {
    let network = lower_trim(&json_str_or(stream.get("network"), "tcp"));
    // Xray calls the plain TCP transport `raw`; sing-box represents the same
    // transport by omitting the V2Ray transport block and using network=tcp.
    if network == "tcp" || network == "raw" {
        return Ok(());
    }

    if network == "ws" {
        let empty = json!({});
        let ws = stream
            .get("wsSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mut transport = Map::new();
        transport.insert("type".into(), json!("ws"));
        let path = json_str_or(ws.get("path"), "");
        if !path.is_empty() {
            apply_websocket_path(&mut transport, &path, ws);
        }
        if let Some(headers) = ws.get("headers").filter(|value| value.is_object()) {
            if !headers.as_object().unwrap().is_empty() {
                transport.insert("headers".into(), headers.clone());
            }
        }
        // Python: ws_settings.get("maxEarlyData", ws_settings.get("ed"))
        let max_early_data = match ws.get("maxEarlyData") {
            Some(value) => Some(value.clone()),
            None => ws.get("ed").cloned(),
        };
        if let Some(value) = max_early_data {
            if present_nonempty(Some(&value)) {
                if let Some(number) = py_int(Some(&value)) {
                    transport.insert("max_early_data".into(), json!(number));
                }
            }
        }
        let early_data_header = json_str_or(ws.get("earlyDataHeaderName"), "")
            .trim()
            .to_string();
        if !early_data_header.is_empty() {
            transport.insert("early_data_header_name".into(), json!(early_data_header));
        }
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    if network == "http" || network == "h2" {
        let empty = json!({});
        let http = stream
            .get("httpSettings")
            .filter(|v| !is_falsy_json(v))
            .or_else(|| stream.get("h2Settings").filter(|v| !is_falsy_json(v)))
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mut transport = Map::new();
        transport.insert("type".into(), json!("http"));
        if let Some(host) = http.get("host").filter(|v| !is_falsy_json(v)) {
            let normalized = match host {
                Value::Array(items) => Value::Array(items.clone()),
                other => Value::Array(vec![Value::String(py_str(other))]),
            };
            transport.insert("host".into(), normalized);
        }
        let path = json_str_or(http.get("path"), "");
        if !path.is_empty() {
            transport.insert("path".into(), json!(path));
        }
        if let Some(headers) = http.get("headers").filter(|value| value.is_object()) {
            transport.insert("headers".into(), headers.clone());
        }
        let method = json_str_or(http.get("method"), "").trim().to_string();
        if !method.is_empty() {
            transport.insert("method".into(), json!(method));
        }
        copy_alias_value(http, &mut transport, &["idleTimeout", "idle_timeout"], "idle_timeout");
        copy_alias_value(http, &mut transport, &["pingTimeout", "ping_timeout"], "ping_timeout");
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    if network == "grpc" {
        let empty = json!({});
        let grpc = stream
            .get("grpcSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mut transport = Map::new();
        transport.insert("type".into(), json!("grpc"));
        let service_name = json_str_or(grpc.get("serviceName"), "");
        if !service_name.is_empty() {
            transport.insert("service_name".into(), json!(service_name));
        }
        let false_value = Value::Bool(false);
        let multi = grpc
            .get("multiMode")
            .or_else(|| grpc.get("multi_mode"))
            .unwrap_or(&false_value);
        if !json_str_or(grpc.get("authority"), "").trim().is_empty() || to_bool(multi) {
            setdefault_unsupported(sb, "gRPC authority/multi-mode requires Xray");
        }
        copy_alias_value(grpc, &mut transport, &["idleTimeout", "idle_timeout"], "idle_timeout");
        copy_alias_value(grpc, &mut transport, &["pingTimeout", "ping_timeout"], "ping_timeout");
        let permit = grpc
            .get("permitWithoutStream")
            .or_else(|| grpc.get("permit_without_stream"));
        if let Some(permit) = permit.filter(|v| present_nonempty(Some(v))) {
            transport.insert("permit_without_stream".into(), json!(to_bool(permit)));
        }
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    if network == "xhttp" {
        let empty = json!({});
        let xhttp = stream
            .get("xhttpSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mut transport = Map::new();
        transport.insert("type".into(), json!("xhttp"));
        let mode = json_str_or(xhttp.get("mode"), "").trim().to_string();
        if !["", "auto", "packet-up", "stream-up", "stream-one"].contains(&mode.as_str()) {
            setdefault_unsupported(sb, &format!("xhttp mode {mode}"));
            return Ok(());
        }
        if !mode.is_empty() && mode != "auto" {
            transport.insert("mode".into(), json!(mode));
        }
        let host = json_str_or(xhttp.get("host"), "").trim().to_string();
        if !host.is_empty() {
            transport.insert("host".into(), json!(host));
        }
        let path = json_str_or(xhttp.get("path"), "").trim().to_string();
        if !path.is_empty() {
            transport.insert("path".into(), json!(path));
        }
        if let Some(headers) = xhttp.get("headers").filter(|value| value.is_object()) {
            if !headers.as_object().unwrap().is_empty() {
                transport.insert("headers".into(), headers.clone());
            }
        }
        copy_xhttp_base_options(&as_map(xhttp), &mut transport, true);
        copy_xhttp_download_settings(xhttp, &mut transport)?;
        merge_xhttp_extra(xhttp.get("extra"), &mut transport)?;
        setdefault_key(&mut transport, "x_padding_bytes", json!("100-1000"));
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    if network == "quic" {
        setdefault_unsupported(sb, "QUIC transport options require Xray");
        return Ok(());
    }

    if network == "httpupgrade" {
        let empty = json!({});
        let settings = stream
            .get("httpupgradeSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let ed = json_str_or(settings.get("ed"), "").trim().to_string();
        let raw_path = json_str_or(settings.get("path"), "");
        if !ed.is_empty() || raw_path.to_lowercase().contains("?ed=") {
            setdefault_unsupported(sb, "HTTPUpgrade early-data requires Xray");
            return Ok(());
        }
        let mut transport = Map::new();
        transport.insert("type".into(), json!("httpupgrade"));
        let path = if raw_path.to_lowercase().contains("?ed=") {
            raw_path.splitn(2, '?').next().unwrap_or("").to_string()
        } else {
            raw_path.clone()
        };
        if !path.is_empty() {
            transport.insert("path".into(), json!(path));
        }
        let host = json_str_or(settings.get("host"), "");
        if !host.is_empty() {
            transport.insert("host".into(), json!(host));
        }
        if let Some(headers) = settings.get("headers").filter(|value| value.is_object()) {
            if !headers.as_object().unwrap().is_empty() {
                transport.insert("headers".into(), headers.clone());
            }
        }
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    if network == "kcp" || network == "mkcp" {
        let empty = json!({});
        let settings = stream
            .get("kcpSettings")
            .filter(|value| value.is_object())
            .unwrap_or(&empty);
        let mut transport = Map::new();
        transport.insert("type".into(), json!("mkcp"));
        for (source_key, target_key, caster) in [
            ("mtu", "mtu", XhttpValueCaster::Int),
            ("tti", "tti", XhttpValueCaster::Int),
            ("uplinkCapacity", "uplink_capacity", XhttpValueCaster::Int),
            ("downlinkCapacity", "downlink_capacity", XhttpValueCaster::Int),
            ("readBufferSize", "read_buffer_size", XhttpValueCaster::Int),
            ("writeBufferSize", "write_buffer_size", XhttpValueCaster::Int),
            ("seed", "seed", XhttpValueCaster::Str),
        ] {
            copy_xhttp_value(settings, &mut transport, source_key, target_key, caster);
        }
        if let Some(congestion) = settings
            .get("congestion")
            .filter(|v| present_nonempty(Some(v)))
        {
            transport.insert("congestion".into(), json!(to_bool(congestion)));
        }
        if let Some(header) = settings.get("header").filter(|value| value.is_object()) {
            let header_type = json_str_or(header.get("type"), "").trim().to_string();
            if !header_type.is_empty() {
                transport.insert("header_type".into(), json!(header_type));
            }
        }
        set_key(sb, "transport", Value::Object(transport));
        return Ok(());
    }

    setdefault_unsupported(sb, &network);
    Ok(())
}

// ── Semantic preservation of imported payloads ──────────────────────────────

fn preserve_or_reject_object(value: &mut Value) -> Result<()> {
    let Some(map) = value.as_object_mut() else {
        return Ok(());
    };

    if map.get("transport").and_then(Value::as_object).is_some() {
        let transport_type = map
            .get("transport")
            .and_then(|t| t.get("type"))
            .map(|v| lower_trim(&json_str_or(Some(v), "")))
            .unwrap_or_default();
        if transport_type == "grpc" {
            let false_value = Value::Bool(false);
            let multi = map
                .get("transport")
                .and_then(|t| {
                    if t.get("multiMode").is_some() {
                        t.get("multiMode")
                    } else {
                        t.get("multi_mode")
                    }
                })
                .unwrap_or(&false_value)
                .clone();
            if to_bool(&multi) {
                bail!("gRPC multi-mode requires the Xray runtime");
            }
            let authority = map
                .get("transport")
                .and_then(|t| t.get("authority"))
                .map(|v| json_str_or(Some(v), "").trim().to_string())
                .unwrap_or_default();
            if !authority.is_empty() {
                let tls_enabled = map
                    .get("tls")
                    .and_then(Value::as_object)
                    .map(|tls| matches!(tls.get("enabled"), Some(Value::Bool(true))))
                    .unwrap_or(false);
                if !tls_enabled {
                    bail!("gRPC authority without TLS requires the Xray runtime");
                }
                let existing = match map.get("tls").and_then(Value::as_object) {
                    Some(tls) => or_str(&[tls.get("server_name"), tls.get("serverName")]),
                    None => String::new(),
                };
                let existing = existing.trim().to_string();
                if !existing.is_empty() && existing.to_lowercase() != authority.to_lowercase() {
                    bail!("different gRPC authority and TLS server name require the Xray runtime");
                }
                if let Some(tls) = map.get_mut("tls").and_then(Value::as_object_mut) {
                    tls.insert("server_name".into(), json!(authority.clone()));
                }
                if let Some(transport) = map.get_mut("transport").and_then(Value::as_object_mut) {
                    transport.shift_remove("authority");
                }
            }
        } else if transport_type == "httpupgrade" {
            let path = map
                .get("transport")
                .and_then(|t| t.get("path"))
                .map(|v| json_str_or(Some(v), ""))
                .unwrap_or_default();
            let early = map
                .get("transport")
                .map(|t| or_str(&[t.get("ed"), t.get("max_early_data")]))
                .unwrap_or_default();
            if !early.trim().is_empty() || path.to_lowercase().contains("?ed=") {
                bail!("HTTPUpgrade early-data requires the Xray runtime");
            }
        } else if transport_type == "quic" {
            let has_semantic = map
                .get("transport")
                .and_then(Value::as_object)
                .map(|t| t.keys().any(|key| key != "type"))
                .unwrap_or(false);
            if has_semantic {
                bail!("QUIC transport options are not supported by the bundled sing-box schema");
            }
        } else if transport_type == "xhttp" {
            let legacy_detour = map
                .get("transport")
                .and_then(|t| t.get("download_detour"))
                .map(|v| json_str_or(Some(v), "").trim().to_string())
                .unwrap_or_default();
            if !legacy_detour.is_empty() {
                let download_is_dict = map
                    .get("transport")
                    .and_then(|t| t.get("download"))
                    .map(Value::is_object)
                    .unwrap_or(false);
                if download_is_dict {
                    if let Some(download) = map
                        .get_mut("transport")
                        .and_then(|t| t.get_mut("download"))
                        .and_then(Value::as_object_mut)
                    {
                        setdefault_key(download, "detour", json!(legacy_detour.clone()));
                    }
                } else {
                    let legacy_is_dict = map
                        .get("transport")
                        .and_then(|t| {
                            if t.get("download_settings").is_some() {
                                t.get("download_settings")
                            } else {
                                t.get("downloadSettings")
                            }
                        })
                        .map(Value::is_object)
                        .unwrap_or(false);
                    if legacy_is_dict {
                        if let Some(legacy) = map
                            .get_mut("transport")
                            .and_then(|t| {
                                if t.get("download_settings").is_some() {
                                    t.get_mut("download_settings")
                                } else {
                                    t.get_mut("downloadSettings")
                                }
                            })
                            .and_then(Value::as_object_mut)
                        {
                            setdefault_key(legacy, "detour", json!(legacy_detour.clone()));
                        }
                    } else {
                        bail!("XHTTP download_detour requires download settings");
                    }
                }
            }
        }
    }

    if let Some(reality) = map
        .get("tls")
        .and_then(Value::as_object)
        .and_then(|tls| tls.get("reality"))
        .and_then(Value::as_object)
    {
        let mldsa = or_str(&[reality.get("mldsa65Verify"), reality.get("mldsa65_verify")]);
        if !mldsa.trim().is_empty() {
            bail!("Reality ML-DSA verification requires the Xray runtime");
        }
        let spider_x = or_str(&[reality.get("spiderX"), reality.get("spider_x")])
            .trim()
            .to_string();
        if !spider_x.is_empty() && spider_x != "/" {
            bail!("Reality SpiderX requires the Xray runtime");
        }
    }

    Ok(())
}

/// Python `_preserve_or_reject_semantic_fields`: migrate losslessly or reject
/// fields the bundled schema cannot express.
///
/// Schema aliases may be renamed later, but connection semantics must never
/// be silently discarded merely to make strict JSON decoding succeed.
pub fn preserve_or_reject_semantic_fields(value: &mut Value) -> Result<()> {
    walk_values_mut(value, &mut preserve_or_reject_object)
}

// ── Removed/renamed schema fields ───────────────────────────────────────────

/// Casters accepted by `move_alias` (Python passed plain callables).
#[derive(Clone, Copy)]
pub enum AliasCaster {
    Bool,
    Int,
}

/// Python `_move_alias`.
pub fn move_alias(
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
    caster: Option<AliasCaster>,
) {
    let Some(value) = target.shift_remove(source_key) else {
        return;
    };
    if target.contains_key(target_key) || matches!(value, Value::Null) || value == json!("") {
        return;
    }
    let converted = match caster {
        Some(AliasCaster::Bool) => json!(to_bool(&value)),
        Some(AliasCaster::Int) => match py_int(Some(&value)) {
            // Python `int(value)` would raise on garbage; the raw value keeps
            // the payload inspectable instead of aborting the runtime copy.
            Some(number) => json!(number),
            None => value,
        },
        None => value,
    };
    target.insert(target_key.to_string(), converted);
}

/// Python `_normalize_native_transport`.
pub fn normalize_native_transport(transport: &mut Value) -> Result<()> {
    let Some(map) = transport.as_object_mut() else {
        return Ok(());
    };
    let transport_type = lower_trim(&json_str_or(map.get("type"), ""));
    map.shift_remove("authority");
    if transport_type == "grpc" {
        move_alias(map, "serviceName", "service_name", None);
        move_alias(map, "idleTimeout", "idle_timeout", None);
        move_alias(map, "pingTimeout", "ping_timeout", None);
        move_alias(map, "permitWithoutStream", "permit_without_stream", Some(AliasCaster::Bool));
        map.shift_remove("multiMode");
        map.shift_remove("multi_mode");
        return Ok(());
    }
    if transport_type == "ws" {
        move_alias(map, "maxEarlyData", "max_early_data", Some(AliasCaster::Int));
        move_alias(map, "earlyDataHeaderName", "early_data_header_name", None);
        let path = json_str_or(map.get("path"), "");
        if !path.is_empty() {
            let mut normalized = Map::new();
            // The constructed settings dict always carries both keys, so
            // `_apply_websocket_path` only re-derives early data from the
            // moved `max_early_data` field (Python semantics preserved).
            let settings = json!({
                "maxEarlyData": map.get("max_early_data").cloned().unwrap_or(Value::Null),
                "earlyDataHeaderName": map.get("early_data_header_name").cloned().unwrap_or(Value::Null),
            });
            apply_websocket_path(&mut normalized, &path, &settings);
            for (key, value) in normalized {
                map.insert(key, value);
            }
        }
        return Ok(());
    }
    if transport_type == "http" {
        move_alias(map, "idleTimeout", "idle_timeout", None);
        move_alias(map, "pingTimeout", "ping_timeout", None);
        return Ok(());
    }
    if transport_type == "httpupgrade" {
        map.shift_remove("ed");
        map.shift_remove("max_early_data");
        map.shift_remove("early_data_header_name");
        let path = json_str_or(map.get("path"), "");
        if path.to_lowercase().contains("?ed=") {
            let trimmed = path.splitn(2, '?').next().unwrap_or("").to_string();
            map.insert("path".into(), json!(trimmed));
        }
        return Ok(());
    }
    if transport_type == "quic" {
        let keys: Vec<String> = map
            .keys()
            .filter(|key| key.as_str() != "type")
            .cloned()
            .collect();
        for key in keys {
            map.shift_remove(&key);
        }
        return Ok(());
    }
    if transport_type != "xhttp" {
        return Ok(());
    }

    let mut legacy_download = map.shift_remove("download_settings");
    // Python: `pop(...) is None` covers both an absent key and a stored null.
    if legacy_download.as_ref().map(Value::is_null).unwrap_or(true) {
        legacy_download = map.shift_remove("downloadSettings");
    }
    if !map.contains_key("download") {
        if let Some(legacy) = legacy_download.filter(|value| value.is_object()) {
            let built = build_xhttp_download(&legacy)?;
            map.insert("download".into(), built);
        }
    }
    map.shift_remove("download_detour");
    let mut normalized = Map::new();
    copy_xhttp_base_options(map, &mut normalized, true);
    for (target_key, aliases) in XHTTP_BASE_ALIASES {
        for alias in *aliases {
            if *alias != *target_key {
                map.shift_remove(*alias);
            }
        }
    }
    for (key, value) in normalized {
        map.insert(key, value);
    }
    Ok(())
}

fn strip_removed_transport_object(value: &mut Value) -> Result<()> {
    let Some(map) = value.as_object_mut() else {
        return Ok(());
    };

    if map.get("transport").is_some() {
        let transport_type = map
            .get("transport")
            .and_then(|t| t.get("type"))
            .map(|v| lower_trim(&json_str_or(Some(v), "")))
            .unwrap_or_default();
        if transport_type == "tcp" || transport_type == "raw" {
            map.shift_remove("transport");
        } else if let Some(transport) = map.get_mut("transport") {
            normalize_native_transport(transport)?;
        }
    }

    if let Some(tls) = map.get_mut("tls").and_then(Value::as_object_mut) {
        move_alias(tls, "serverName", "server_name", None);
        move_alias(tls, "allowInsecure", "insecure", Some(AliasCaster::Bool));
        move_alias(tls, "disableSNI", "disable_sni", Some(AliasCaster::Bool));
        move_alias(tls, "minVersion", "min_version", None);
        move_alias(tls, "maxVersion", "max_version", None);
        move_alias(tls, "cipherSuites", "cipher_suites", None);
        move_alias(tls, "curvePreferences", "curve_preferences", None);
        move_alias(tls, "handshakeTimeout", "handshake_timeout", None);

        let existing_pins = tls.get("certificate_public_key_sha256").cloned();
        let alias_pins = [
            tls.shift_remove("certificatePublicKeySha256"),
            tls.shift_remove("pinnedPeerCertSha256"),
            tls.shift_remove("pinSHA256"),
        ];
        let pins = match existing_pins {
            Some(value) if !is_falsy_json(&value) => Some(value),
            _ => alias_pins
                .into_iter()
                .find(|item| match item {
                    None | Some(Value::Null) => false,
                    Some(Value::String(text)) => !text.is_empty(),
                    _ => true,
                })
                .flatten(),
        };
        if let Some(pins) = pins.filter(|v| !is_falsy_json(&v)) {
            let normalized = normalize_certificate_public_key_pins(&pins)?;
            tls.insert("certificate_public_key_sha256".into(), json!(normalized));
        }

        if let Some(utls) = tls.get_mut("utls").and_then(Value::as_object_mut) {
            let enabled_not_false = !matches!(utls.get("enabled"), Some(Value::Bool(false)));
            if enabled_not_false {
                let fingerprint = normalize_utls_fingerprint(utls.get("fingerprint"))?;
                if !fingerprint.is_empty() {
                    utls.insert("enabled".into(), json!(true));
                    utls.insert("fingerprint".into(), json!(fingerprint));
                }
            }
        }

        if let Some(reality) = tls.get_mut("reality").and_then(Value::as_object_mut) {
            move_alias(reality, "publicKey", "public_key", None);
            move_alias(reality, "shortId", "short_id", None);
            reality.shift_remove("spiderX");
            reality.shift_remove("mldsa65Verify");
        }
    }
    Ok(())
}

/// Python `_strip_removed_transport_fields`: migrate fields removed or
/// renamed in the bundled sing-box schema.
pub fn strip_removed_transport_fields(value: &mut Value) -> Result<()> {
    walk_values_mut(value, &mut strip_removed_transport_object)
}

// ── XHTTP shared options ────────────────────────────────────────────────────

/// Casters of Python `_copy_xhttp_value`.
#[derive(Clone, Copy)]
pub enum XhttpValueCaster {
    Int,
    Str,
}

/// Python `_copy_xhttp_value`.
pub fn copy_xhttp_value(
    source: &Value,
    target: &mut Map<String, Value>,
    source_key: &str,
    target_key: &str,
    caster: XhttpValueCaster,
) {
    let Some(value) = source
        .get(source_key)
        .filter(|v| present_nonempty(Some(v)))
    else {
        return;
    };
    let converted = match caster {
        XhttpValueCaster::Int => py_int(Some(value)).map(|number| json!(number)),
        XhttpValueCaster::Str => Some(json!(py_str(value))),
    };
    target.insert(
        target_key.to_string(),
        converted.unwrap_or_else(|| value.clone()),
    );
}

/// Python `_apply_websocket_path`.
pub fn apply_websocket_path(transport: &mut Map<String, Value>, raw_path: &str, settings: &Value) {
    let (path, found, query) = py_partition(raw_path, '?');
    transport.insert(
        "path".into(),
        json!(if found { path.to_string() } else { raw_path.to_string() }),
    );
    if !found {
        return;
    }
    let mut query_early_data = String::new();
    for item in query.split('&') {
        let (key, has_equals, value) = py_partition(item, '=');
        if has_equals && key.trim().to_lowercase() == "ed" {
            query_early_data = value.trim().to_string();
            break;
        }
    }
    // Python: settings.get("maxEarlyData", settings.get("ed", query_early_data))
    let configured = match settings.get("maxEarlyData") {
        Some(value) => Some(value.clone()),
        None => match settings.get("ed") {
            Some(value) => Some(value.clone()),
            None => Some(Value::String(query_early_data)),
        },
    };
    let Some(configured) = configured.filter(|v| present_nonempty(Some(v))) else {
        return;
    };
    let Some(early_data) = py_int(Some(&configured)) else {
        return;
    };
    if early_data <= 0 {
        return;
    }
    transport.insert("max_early_data".into(), json!(early_data));
    setdefault_key(transport, "early_data_header_name", json!("Sec-WebSocket-Protocol"));
}

/// Python `_copy_xhttp_download_settings`.
pub fn copy_xhttp_download_settings(source: &Value, target: &mut Map<String, Value>) -> Result<()> {
    let value = get_with_default(
        source,
        "downloadSettings",
        get_with_default(source, "download_settings", source.get("download")),
    );
    let Some(value) = value.filter(|v| v.is_object() && is_json_safe(v)) else {
        return Ok(());
    };
    target.insert("download".into(), build_xhttp_download(value)?);
    Ok(())
}

const XHTTP_EXTRA_ALLOWED: [&str; 45] = [
    "mode",
    "downloadSettings",
    "download_settings",
    "download",
    "host",
    "headers",
    "path",
    "scMaxEachPostBytes",
    "scMaxBufferedPosts",
    "scMinPostsIntervalMs",
    "xPaddingBytes",
    "padding",
    "noGRPCHeader",
    "no_grpc_header",
    "noSSEHeader",
    "no_sse_header",
    "scStreamUpServerSecs",
    "sc_stream_up_server_secs",
    "serverMaxHeaderBytes",
    "server_max_header_bytes",
    "trustedXForwardedFor",
    "trusted_x_forwarded_for",
    "xmux",
    "xPaddingObfsMode",
    "x_padding_obfs_mode",
    "xPaddingKey",
    "xPaddingHeader",
    "xPaddingPlacement",
    "xPaddingMethod",
    "uplinkHTTPMethod",
    "sessionPlacement",
    "sessionKey",
    "seqPlacement",
    "seqKey",
    "uplinkDataPlacement",
    "uplinkDataKey",
    "uplinkChunkSize",
    "sessionIDTable",
    "sessionIDLength",
    "congestionController",
    "cwnd",
    "sc_max_each_post_bytes",
    "sc_max_buffered_posts",
    "sc_min_posts_interval_ms",
    "x_padding_bytes",
];

/// Python `_merge_xhttp_extra`.
pub fn merge_xhttp_extra(extra: Option<&Value>, target: &mut Map<String, Value>) -> Result<()> {
    let Some(extra) = extra.and_then(Value::as_object) else {
        return Ok(());
    };
    for (key, value) in extra {
        if !XHTTP_EXTRA_ALLOWED.contains(&key.as_str()) || !is_json_safe(value) {
            continue;
        }
        if ["download", "downloadSettings", "download_settings"].contains(&key.as_str())
            && value.is_object()
        {
            target.insert("download".into(), build_xhttp_download(value)?);
            continue;
        }
        if key == "mode" && !target.contains_key("mode") {
            let mode = json_str_or(Some(value), "").trim().to_string();
            if ["packet-up", "stream-up", "stream-one"].contains(&mode.as_str()) {
                target.insert("mode".into(), json!(mode));
            }
        }
    }
    copy_xhttp_base_options(extra, target, false);
    Ok(())
}

/// Python `_normalize_xhttp_key`.
pub fn normalize_xhttp_key(key: &str) -> String {
    match key {
        "scMaxEachPostBytes" => "sc_max_each_post_bytes".to_string(),
        "scMaxBufferedPosts" => "sc_max_buffered_posts".to_string(),
        "scMinPostsIntervalMs" => "sc_min_posts_interval_ms".to_string(),
        "xPaddingBytes" | "padding" => "x_padding_bytes".to_string(),
        "noGRPCHeader" => "no_grpc_header".to_string(),
        "noSSEHeader" => "no_sse_header".to_string(),
        "scStreamUpServerSecs" => "sc_stream_up_server_secs".to_string(),
        "serverMaxHeaderBytes" => "server_max_header_bytes".to_string(),
        "trustedXForwardedFor" => "trusted_x_forwarded_for".to_string(),
        "xPaddingObfsMode" => "x_padding_obfs_mode".to_string(),
        "xPaddingKey" => "x_padding_key".to_string(),
        "xPaddingHeader" => "x_padding_header".to_string(),
        "xPaddingPlacement" => "x_padding_placement".to_string(),
        "xPaddingMethod" => "x_padding_method".to_string(),
        "uplinkHTTPMethod" | "uplinkHttpMethod" => "uplink_http_method".to_string(),
        "sessionPlacement" => "session_placement".to_string(),
        "sessionKey" => "session_key".to_string(),
        "seqPlacement" => "seq_placement".to_string(),
        "seqKey" => "seq_key".to_string(),
        "uplinkDataPlacement" => "uplink_data_placement".to_string(),
        "uplinkDataKey" => "uplink_data_key".to_string(),
        "uplinkChunkSize" => "uplink_chunk_size".to_string(),
        "sessionIDTable" | "sessionIdTable" => "session_id_table".to_string(),
        "sessionIDLength" | "sessionIdLength" => "session_id_length".to_string(),
        "congestionController" => "congestion_controller".to_string(),
        _ => key.to_string(),
    }
}

/// Python `_normalize_json_keys`.
pub fn normalize_json_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, item) in map {
                out.insert(normalize_xhttp_key(key), normalize_json_keys(item));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize_json_keys).collect()),
        other => other.clone(),
    }
}

/// Python `_XHTTP_BASE_ALIASES` (insertion order matters for the target keys).
pub const XHTTP_BASE_ALIASES: &[(&str, &[&str])] = &[
    ("host", &["host"]),
    ("path", &["path"]),
    ("headers", &["headers"]),
    ("domain_strategy", &["domain_strategy", "domainStrategy"]),
    ("x_padding_bytes", &["x_padding_bytes", "xPaddingBytes", "padding"]),
    ("no_grpc_header", &["no_grpc_header", "noGRPCHeader", "noGrpcHeader"]),
    ("no_sse_header", &["no_sse_header", "noSSEHeader", "noSseHeader"]),
    ("sc_max_each_post_bytes", &["sc_max_each_post_bytes", "scMaxEachPostBytes"]),
    ("sc_min_posts_interval_ms", &["sc_min_posts_interval_ms", "scMinPostsIntervalMs"]),
    ("sc_max_buffered_posts", &["sc_max_buffered_posts", "scMaxBufferedPosts"]),
    ("sc_stream_up_server_secs", &["sc_stream_up_server_secs", "scStreamUpServerSecs"]),
    ("server_max_header_bytes", &["server_max_header_bytes", "serverMaxHeaderBytes"]),
    ("trusted_x_forwarded_for", &["trusted_x_forwarded_for", "trustedXForwardedFor"]),
    ("xmux", &["xmux"]),
    ("x_padding_obfs_mode", &["x_padding_obfs_mode", "xPaddingObfsMode"]),
    ("x_padding_key", &["x_padding_key", "xPaddingKey"]),
    ("x_padding_header", &["x_padding_header", "xPaddingHeader"]),
    ("x_padding_placement", &["x_padding_placement", "xPaddingPlacement"]),
    ("x_padding_method", &["x_padding_method", "xPaddingMethod"]),
    ("uplink_http_method", &["uplink_http_method", "uplinkHTTPMethod", "uplinkHttpMethod"]),
    ("session_placement", &["session_placement", "sessionPlacement"]),
    ("session_key", &["session_key", "sessionKey"]),
    ("seq_placement", &["seq_placement", "seqPlacement"]),
    ("seq_key", &["seq_key", "seqKey"]),
    ("uplink_data_placement", &["uplink_data_placement", "uplinkDataPlacement"]),
    ("uplink_data_key", &["uplink_data_key", "uplinkDataKey"]),
    ("uplink_chunk_size", &["uplink_chunk_size", "uplinkChunkSize"]),
    ("session_id_table", &["session_id_table", "sessionIDTable", "sessionIdTable"]),
    ("session_id_length", &["session_id_length", "sessionIDLength", "sessionIdLength"]),
    ("congestion_controller", &["congestion_controller", "congestionController"]),
    ("cwnd", &["cwnd"]),
];

const XMUX_ALLOWED: [&str; 6] = [
    "max_concurrency",
    "max_connections",
    "c_max_reuse_times",
    "h_max_request_times",
    "h_max_reusable_secs",
    "h_keep_alive_period",
];

/// Python `_first_present`.
pub fn first_present(source: &Value, aliases: &[&str]) -> Option<Value> {
    aliases.iter().find_map(|key| source.get(*key).cloned())
}

/// Python `_copy_alias_value`.
pub fn copy_alias_value(
    source: &Value,
    target: &mut Map<String, Value>,
    aliases: &[&str],
    target_key: &str,
) {
    if let Some(value) = first_present(source, aliases) {
        if present_nonempty(Some(&value)) {
            target.insert(target_key.to_string(), value);
        }
    }
}

/// Python `_xhttp_padding_range`.
pub fn xhttp_padding_range(value: &Value) -> String {
    let low_owned: Value;
    let high_owned: Value;
    match value {
        Value::Object(map) => {
            low_owned = map.get("from").cloned().unwrap_or(Value::Null);
            high_owned = map.get("to").cloned().unwrap_or(Value::Null);
        }
        // Python checks bool before int because bool is an int subclass.
        Value::Bool(_) => return String::new(),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            low_owned = value.clone();
            high_owned = value.clone();
        }
        _ => {
            let text = or_str(&[Some(value)]).trim().to_string();
            let (low, _, high) = py_partition(&text, '-');
            let high = if high.is_empty() { low.to_string() } else { high.to_string() };
            low_owned = Value::String(low.to_string());
            high_owned = Value::String(high);
        }
    }
    let low_number = match py_str(&low_owned).trim().parse::<i64>() {
        Ok(number) => number,
        Err(_) => return String::new(),
    };
    let high_number = match py_str(&high_owned).trim().parse::<i64>() {
        Ok(number) => number,
        Err(_) => return String::new(),
    };
    // The fork also rejects a zero lower bound ("0-100").
    let low_number = low_number.max(1);
    if high_number < low_number {
        return String::new();
    }
    format!("{low_number}-{high_number}")
}

/// Python `_copy_xhttp_base_options`.
pub fn copy_xhttp_base_options(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    overwrite: bool,
) {
    for (target_key, aliases) in XHTTP_BASE_ALIASES {
        if !overwrite && target.contains_key(*target_key) {
            continue;
        }
        let value = aliases.iter().find_map(|key| source.get(*key).cloned());
        let Some(value) = value.filter(|v| present_nonempty(Some(v))) else {
            continue;
        };

        if *target_key == "headers" {
            if let Some(headers_value) = value.as_object() {
                let mut headers = Map::new();
                for (key, item) in headers_value {
                    if !item.is_null() {
                        headers.insert(key.clone(), Value::String(py_str(item)));
                    }
                }
                // The fork rejects Host inside headers; it has a dedicated
                // host option. Preserve the value by moving it there.
                for key in headers.keys().cloned().collect::<Vec<_>>() {
                    if key.to_lowercase() == "host" {
                        if let Some(host_value) = headers.shift_remove(&key) {
                            setdefault_key(target, "host", host_value);
                        }
                    }
                }
                if !headers.is_empty() {
                    target.insert((*target_key).to_string(), Value::Object(headers));
                }
                continue;
            }
        }
        if ["no_grpc_header", "no_sse_header", "x_padding_obfs_mode"].contains(target_key) {
            target.insert((*target_key).to_string(), json!(to_bool(&value)));
            continue;
        }
        if *target_key == "domain_strategy" {
            let mut normalized = py_str(&value).trim().to_lowercase().replace('-', "_");
            let compact = normalized.replace('_', "");
            normalized = match compact.as_str() {
                "preferipv4" => "prefer_ipv4".to_string(),
                "preferipv6" => "prefer_ipv6".to_string(),
                "ipv4only" => "ipv4_only".to_string(),
                "ipv6only" => "ipv6_only".to_string(),
                _ => normalized,
            };
            if ["prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"]
                .contains(&normalized.as_str())
            {
                target.insert((*target_key).to_string(), json!(normalized));
            }
            continue;
        }
        if *target_key == "x_padding_bytes" {
            // sing-box rejects a disabled range ("x_padding_bytes cannot be
            // disabled"); leave it unset so the regular default applies.
            let padding = xhttp_padding_range(&value);
            if !padding.is_empty() {
                target.insert((*target_key).to_string(), json!(padding));
            }
            continue;
        }
        if ["sc_max_buffered_posts", "server_max_header_bytes", "cwnd"].contains(target_key) {
            if let Some(number) = py_int(Some(&value)) {
                if number >= 0 {
                    target.insert((*target_key).to_string(), json!(number));
                }
            }
            continue;
        }
        if *target_key == "trusted_x_forwarded_for" {
            let normalized: Vec<String> = match &value {
                Value::Array(items) => items
                    .iter()
                    .map(py_str)
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
                other => py_str(other)
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
            };
            target.insert((*target_key).to_string(), json!(normalized));
            continue;
        }
        if *target_key == "xmux" {
            if let Some(xmux_value) = value.as_object() {
                let mut xmux = Map::new();
                for (key, item) in xmux_value {
                    let normalized = match key.as_str() {
                        "maxConcurrency" => "max_concurrency",
                        "maxConnections" => "max_connections",
                        "cMaxReuseTimes" => "c_max_reuse_times",
                        "hMaxRequestTimes" => "h_max_request_times",
                        "hMaxReusableSecs" => "h_max_reusable_secs",
                        "hKeepAlivePeriod" => "h_keep_alive_period",
                        other => other,
                    };
                    if !XMUX_ALLOWED.contains(&normalized) {
                        continue;
                    }
                    if normalized == "h_keep_alive_period" {
                        if let Some(number) = py_int(Some(item)) {
                            xmux.insert(normalized.to_string(), json!(number));
                        }
                        continue;
                    }
                    xmux.insert(normalized.to_string(), item.clone());
                }
                if !xmux.is_empty() {
                    target.insert((*target_key).to_string(), Value::Object(xmux));
                }
            }
            continue;
        }
        target.insert((*target_key).to_string(), value.clone());
    }
}

const XHTTP_TLS_ALLOWED: [&str; 28] = [
    "enabled",
    "engine",
    "disable_sni",
    "server_name",
    "insecure",
    "alpn",
    "min_version",
    "max_version",
    "cipher_suites",
    "curve_preferences",
    "certificate",
    "certificate_path",
    "certificate_public_key_sha256",
    "client_certificate",
    "client_certificate_path",
    "client_key",
    "client_key_path",
    "fragment",
    "fragment_fallback_delay",
    "record_fragment",
    "spoof",
    "spoof_method",
    "kernel_tx",
    "kernel_rx",
    "handshake_timeout",
    "ech",
    "utls",
    "reality",
];

/// Python `_build_xhttp_download`.
pub fn build_xhttp_download(source: &Value) -> Result<Value> {
    let stream = source
        .get("streamSettings")
        .filter(|value| value.is_object())
        .unwrap_or(source);
    let nested = get_with_default(stream, "xhttpSettings", stream.get("xhttp_settings"));
    let nested = if nested.map(Value::is_object).unwrap_or(false) {
        nested.unwrap()
    } else {
        get_with_default(source, "xhttpSettings", source.get("xhttp_settings"))
            .unwrap_or(&Value::Null)
    };
    let xhttp_is_source = !nested.is_object();
    let xhttp = if xhttp_is_source { source } else { nested };

    let server = or_str(&[
        source.get("server"),
        source.get("address"),
        stream.get("server"),
        stream.get("address"),
    ])
    .trim()
    .to_string();
    let inner = get_with_default(stream, "server_port", stream.get("port"));
    let middle = get_with_default(source, "port", inner);
    let raw_port = get_with_default(source, "server_port", middle);
    let port = py_int_or(raw_port, 0);
    if server.is_empty() || port <= 0 {
        bail!("XHTTP download settings require server/address and port/server_port");
    }

    let mut download = Map::new();
    download.insert("server".into(), json!(server.clone()));
    download.insert("server_port".into(), json!(port));
    copy_xhttp_base_options(&as_map(source), &mut download, true);
    if !xhttp_is_source {
        copy_xhttp_base_options(&as_map(xhttp), &mut download, true);
    }
    setdefault_key(&mut download, "x_padding_bytes", json!("100-1000"));

    let detour = or_str(&[source.get("detour"), stream.get("detour")])
        .trim()
        .to_string();
    if !detour.is_empty() {
        download.insert("detour".into(), json!(detour));
    }

    let native_tls = get_with_default(stream, "tls", source.get("tls"))
        .filter(|value| value.is_object())
        .cloned();
    if let Some(native_tls) = native_tls {
        let mut tls_out = Map::new();
        for (key, value) in native_tls.as_object().unwrap() {
            if XHTTP_TLS_ALLOWED.contains(&key.as_str()) {
                tls_out.insert(key.clone(), value.clone());
            }
        }
        setdefault_key(&mut tls_out, "enabled", json!(true));
        download.insert("tls".into(), Value::Object(tls_out));
    } else {
        let mut converted = json!({});
        apply_tls(&mut converted, stream, &server)?;
        if let Some(tls) = converted.get("tls").filter(|value| value.is_object()) {
            download.insert("tls".into(), tls.clone());
        }
    }
    Ok(Value::Object(download))
}

// ── uTLS fingerprints and certificate pins ──────────────────────────────────

/// Python `_normalize_utls_fingerprint`.
pub fn normalize_utls_fingerprint(value: Option<&Value>) -> Result<String> {
    let compact = json_str_or(value, "")
        .trim()
        .to_lowercase()
        .replace('_', "-");
    if compact.is_empty() {
        return Ok(String::new());
    }
    let aliases: [(&str, &str); 5] = [
        ("chrome-psk", "chrome_psk"),
        ("chrome-psk-shuffle", "chrome_psk_shuffle"),
        ("chrome-padding-psk-shuffle", "chrome_padding_psk_shuffle"),
        ("chrome-pq", "chrome_pq"),
        ("chrome-pq-psk", "chrome_pq_psk"),
    ];
    if let Some((_, mapped)) = aliases.iter().find(|(alias, _)| *alias == compact) {
        return Ok((*mapped).to_string());
    }
    for prefix in ["chrome", "firefox", "edge", "safari", "ios", "android", "qq", "360"] {
        if compact.starts_with(prefix) {
            return Ok(prefix.to_string());
        }
    }
    if compact.starts_with("randomized") {
        return Ok("randomized".to_string());
    }
    if compact.starts_with("random") {
        return Ok("random".to_string());
    }
    // A different ClientHello fingerprint can be required by the server.
    // Silently replacing an unknown value with Chrome produces a valid JSON
    // config that cannot connect, which is worse than an explicit fallback.
    bail!("Unsupported uTLS fingerprint `{}`", json_str_plain(value))
}

fn decode_hex_64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = Vec::with_capacity(32);
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
    }
    Some(out)
}

/// Python `_normalize_certificate_public_key_pins`.
pub fn normalize_certificate_public_key_pins(value: &Value) -> Result<Vec<String>> {
    let raw_values: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    let mut values: Vec<String> = Vec::new();
    for item in &raw_values {
        for part in py_str(item).split(',') {
            let part = part.trim();
            if !part.is_empty() {
                values.push(part.to_string());
            }
        }
    }
    let mut normalized: Vec<String> = Vec::new();
    for raw in values {
        // Python removeprefix chain: strip "sha256/", then "SHA256/".
        let mut compact = raw.as_str();
        if let Some(stripped) = compact.strip_prefix("sha256/") {
            compact = stripped;
        }
        if let Some(stripped) = compact.strip_prefix("SHA256/") {
            compact = stripped;
        }
        let hex_value = compact.replace(':', "").replace('-', "");
        let decoded = if hex_value.len() == 64
            && hex_value
                .chars()
                .all(|ch| "0123456789abcdefABCDEF".contains(ch))
        {
            decode_hex_64(&hex_value).ok_or_else(|| {
                anyhow::anyhow!("Certificate public-key SHA-256 must be Base64 or 64-digit hex")
            })?
        } else {
            // `-len(compact) % 4` counts characters, not UTF-8 bytes.
            let padding = (4 - compact.chars().count() % 4) % 4;
            let padded = format!("{compact}{}", "=".repeat(padding));
            match python_standard_decode_strict(&padded) {
                Ok(decoded) => decoded,
                Err(_) => match lumen_core::pybase64::py_a2b_base64(padded.as_bytes(), true) {
                    Ok(decoded) => decoded,
                    Err(_) => {
                        bail!("Certificate public-key SHA-256 must be Base64 or 64-digit hex")
                    }
                },
            }
        };
        if decoded.len() != 32 {
            bail!("Certificate public-key SHA-256 must decode to 32 bytes");
        }
        let canonical = STANDARD.encode(&decoded);
        if !normalized.contains(&canonical) {
            normalized.push(canonical);
        }
    }
    Ok(normalized)
}

// ── Scalar guards ───────────────────────────────────────────────────────────

/// Python `_is_json_safe`. `serde_json::Value` can only contain JSON-safe
/// data, so the guard is always true here; kept for call-site parity.
pub fn is_json_safe(value: &Value) -> bool {
    let _ = value;
    true
}

/// Python `_to_bool`.
pub fn to_bool(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        other => ["1", "true", "yes", "on"].contains(&lower_trim(&py_str(other)).as_str()),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_core::models::Node;

    fn node_with(outbound: Value) -> Node {
        let mut node = Node::new();
        node.outbound = outbound;
        node
    }

    #[test]
    fn vless_xray_outbound_becomes_native_singbox() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "vless",
                "settings": {"vnext": [{
                    "address": "example.com", "port": 443,
                    "users": [{"id": "u-1", "encryption": "none", "flow": "xtls-rprx-vision"}]
                }]},
                "streamSettings": {
                    "network": "tcp", "security": "reality",
                    "realitySettings": {
                        "serverName": "sni.example.com", "publicKey": "pk",
                        "shortId": "abcd", "fingerprint": "chrome"
                    }
                }
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["type"], json!("vless"));
        assert_eq!(outbound["server"], json!("example.com"));
        assert_eq!(outbound["server_port"], json!(443));
        assert_eq!(outbound["uuid"], json!("u-1"));
        assert_eq!(outbound["packet_encoding"], json!("xudp"));
        assert_eq!(outbound["flow"], json!("xtls-rprx-vision"));
        assert_eq!(outbound["tag"], json!("proxy"));
        assert_eq!(outbound["tls"]["enabled"], json!(true));
        assert_eq!(outbound["tls"]["server_name"], json!("sni.example.com"));
        assert_eq!(outbound["tls"]["reality"]["public_key"], json!("pk"));
        assert_eq!(outbound["tls"]["reality"]["short_id"], json!("abcd"));
        assert_eq!(outbound["tls"]["utls"]["fingerprint"], json!("chrome"));
        assert_eq!(outbound["tls"]["insecure"], json!(false));
    }

    #[test]
    fn raw_tcp_transport_is_omitted() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "vless",
                "settings": {"vnext": [{"address": "example.com", "port": 443, "users": [{"id": "u"}]}]},
                "streamSettings": {"network": "raw"}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert!(outbound.get("transport").is_none());
        assert_eq!(outbound["packet_encoding"], json!("xudp"));
    }

    #[test]
    fn unsupported_protocol_is_rejected() {
        let error =
            build_singbox_outbound(&node_with(json!({"protocol": "mtproto"})), "proxy", false, 8)
                .unwrap_err();
        assert!(error.to_string().contains("protocol `mtproto`"));
    }

    #[test]
    fn native_singbox_payload_is_normalized_and_tagged() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "trojan",
                "singbox": {"type": "trojan", "server": "example.com", "server_port": 443, "password": "pw"}
            })),
            "test-tag",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["type"], json!("trojan"));
        assert_eq!(outbound["tag"], json!("test-tag"));
    }

    #[test]
    fn native_wireguard_endpoint_is_migrated() {
        let key = format!("{}=", "A".repeat(43));
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "wireguard",
                "singbox": {
                    "type": "wireguard", "server": "1.2.3.4", "server_port": 51820,
                    "peer_public_key": key, "local_address": ["10.0.0.2/32"]
                }
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["type"], json!("wireguard"));
        assert_eq!(outbound["system"], json!(false));
        assert_eq!(outbound["address"], json!(["10.0.0.2/32"]));
    }

    #[test]
    fn websocket_early_data_is_normalized() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "trojan",
                "settings": {"servers": [{"address": "example.com", "port": 443, "password": "pw"}]},
                "streamSettings": {"network": "ws", "wsSettings": {"path": "/ws?ed=2048"}}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["transport"]["type"], json!("ws"));
        assert_eq!(outbound["transport"]["path"], json!("/ws"));
        assert_eq!(outbound["transport"]["max_early_data"], json!(2048));
        assert_eq!(
            outbound["transport"]["early_data_header_name"],
            json!("Sec-WebSocket-Protocol")
        );
    }

    #[test]
    fn xhttp_download_settings_build_a_detached_dial() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "vless",
                "settings": {"vnext": [{"address": "example.com", "port": 443, "users": [{"id": "u"}]}]},
                "streamSettings": {"network": "xhttp", "xhttpSettings": {
                    "path": "/x",
                    "downloadSettings": {"address": "dl.example.com", "port": 443,
                                         "xhttpSettings": {"path": "/d"}}
                }}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["transport"]["type"], json!("xhttp"));
        assert_eq!(outbound["transport"]["download"]["server"], json!("dl.example.com"));
        assert_eq!(outbound["transport"]["download"]["server_port"], json!(443));
        assert_eq!(
            outbound["transport"]["download"]["x_padding_bytes"],
            json!("100-1000")
        );
    }

    #[test]
    fn shadowsocks_method_alias_and_plugin_opts_are_normalized() {
        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "shadowsocks",
                "settings": {"servers": [{
                    "address": "1.2.3.4", "port": 8388,
                    "method": "chacha20-poly1305", "password": "pw",
                    "plugin": "obfs", "plugin_opts": "mode=http;host=cdn.example.com"
                }]}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["method"], json!("chacha20-ietf-poly1305"));
        assert_eq!(outbound["plugin"], json!("obfs-local"));
        assert_eq!(
            outbound["plugin_opts"],
            json!("obfs=http;obfs-host=cdn.example.com")
        );
    }

    #[test]
    fn shadowsocks_with_tls_is_unsupported_transport() {
        let error = build_singbox_outbound(
            &node_with(json!({
                "protocol": "shadowsocks",
                "settings": {"servers": [{"address": "1.2.3.4", "port": 8388, "method": "aes-256-gcm", "password": "pw"}]},
                "streamSettings": {"security": "tls", "tlsSettings": {}}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("transport `shadowsocks with TLS` не поддерживается"));
    }

    #[test]
    fn shadowsocks_2022_key_size_is_corrected() {
        let mut value = json!({
            "type": "shadowsocks",
            "method": "2022-blake3-aes-128-gcm",
            "password": "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY="
        });
        normalize_shadowsocks_methods(&mut value).unwrap();
        assert_eq!(value["method"], json!("2022-blake3-aes-256-gcm"));
    }

    #[test]
    fn shadowsocks_unsupported_method_is_rejected() {
        let mut value = json!({"type": "shadowsocks", "method": "rc4", "password": "pw"});
        assert!(normalize_shadowsocks_methods(&mut value).is_err());
    }

    #[test]
    fn vmess_security_null_becomes_auto_recursively() {
        let mut value = json!({"outbounds": [{"type": "vmess", "security": "null"}]});
        normalize_vmess_security(&mut value).unwrap();
        assert_eq!(value["outbounds"][0]["security"], json!("auto"));
    }

    #[test]
    fn vmess_unsupported_security_is_rejected() {
        let mut value = json!({"type": "vmess", "security": "aes-256-cfb"});
        assert!(normalize_vmess_security(&mut value).is_err());
    }

    #[test]
    fn vless_vision_requires_tls_base() {
        let mut value = json!({"type": "vless", "flow": "xtls-rprx-vision"});
        normalize_vless_vision(&mut value);
        assert!(value.get("flow").is_none());

        let mut value = json!({
            "type": "vless", "flow": "xtls-rprx-vision", "tls": {"enabled": true}
        });
        normalize_vless_vision(&mut value);
        assert_eq!(value["flow"], json!("xtls-rprx-vision"));
    }

    #[test]
    fn hysteria_defaults_are_supplied() {
        let outbound =
            build_singbox_outbound(&node_with(json!({"protocol": "hysteria"})), "proxy", false, 8)
                .unwrap();
        assert_eq!(outbound["up_mbps"], json!(50));
        assert_eq!(outbound["down_mbps"], json!(200));

        let outbound = build_singbox_outbound(
            &node_with(json!({
                "protocol": "hysteria",
                "singbox": {"type": "hysteria", "up_mbps": 10, "down_mbps": 20}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap();
        assert_eq!(outbound["up_mbps"], json!(10));
        assert_eq!(outbound["down_mbps"], json!(20));
    }

    #[test]
    fn grpc_authority_without_tls_is_rejected() {
        let mut value = json!({
            "type": "vless", "transport": {"type": "grpc", "authority": "srv.example.com"}
        });
        assert!(preserve_or_reject_semantic_fields(&mut value).is_err());

        let mut value = json!({
            "type": "vless",
            "transport": {"type": "grpc", "authority": "srv.example.com"},
            "tls": {"enabled": true}
        });
        preserve_or_reject_semantic_fields(&mut value).unwrap();
        assert_eq!(value["tls"]["server_name"], json!("srv.example.com"));
        assert!(value["transport"].get("authority").is_none());
    }

    #[test]
    fn removed_transport_fields_are_migrated() {
        let mut value = json!({
            "type": "vless",
            "transport": {"type": "tcp"},
            "tls": {
                "serverName": "srv", "allowInsecure": "true",
                "reality": {"publicKey": "pk", "shortId": "si"}
            }
        });
        strip_removed_transport_fields(&mut value).unwrap();
        assert!(value.get("transport").is_none());
        assert_eq!(value["tls"]["server_name"], json!("srv"));
        assert_eq!(value["tls"]["insecure"], json!(true));
        assert_eq!(value["tls"]["reality"]["public_key"], json!("pk"));
        assert_eq!(value["tls"]["reality"]["short_id"], json!("si"));
    }

    #[test]
    fn utls_fingerprints_are_normalized() {
        assert_eq!(
            normalize_utls_fingerprint(Some(&json!("Chrome_PSK"))).unwrap(),
            "chrome_psk"
        );
        assert_eq!(
            normalize_utls_fingerprint(Some(&json!("firefox-105"))).unwrap(),
            "firefox"
        );
        assert!(normalize_utls_fingerprint(Some(&json!("unknown-fp"))).is_err());
    }

    #[test]
    fn certificate_pins_are_canonicalized() {
        let hex_value = "a".repeat(64);
        let pins = normalize_certificate_public_key_pins(&json!(format!("sha256/{hex_value}")))
            .unwrap();
        let expected = format!("{}o=", "q".repeat(42));
        assert_eq!(pins, vec![expected]);

        let pins = normalize_certificate_public_key_pins(&json!([
            format!("sha256/{hex_value}"),
            format!("SHA256/{}", "0".repeat(64)),
            format!("SHA256/{}", "0".repeat(64)),
        ]))
        .unwrap();
        assert_eq!(pins.len(), 2);
    }

    #[test]
    fn sip003_options_keep_escapes() {
        assert_eq!(
            split_sip003_options(Some(&json!("a=1;b=2"))).join("|"),
            "a=1|b=2"
        );
        assert_eq!(
            split_sip003_options(Some(&json!("a\\;b=1;c=2"))).join("|"),
            "a\\;b=1|c=2"
        );
        assert_eq!(
            normalize_obfs_options(Some(&json!("mode=http;host=x"))),
            "obfs=http;obfs-host=x"
        );
        assert_eq!(
            normalize_v2ray_plugin_options(Some(&json!("mux=true;mode=websocket"))),
            "mux=1;mode=websocket"
        );
        assert_eq!(
            normalize_v2ray_plugin_options(Some(&json!("mux=not-a-number"))),
            ""
        );
    }

    #[test]
    fn openvpn_encrypted_key_needs_password() {
        let error = build_singbox_outbound(
            &node_with(json!({
                "protocol": "openvpn",
                "singbox": {"type": "openvpn-client", "tls": {
                    "key": "-----BEGIN ENCRYPTED PRIVATE KEY-----\nMIIB\n-----END ENCRYPTED PRIVATE KEY-----"
                }}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "OpenVPN private key is encrypted and requires a private key password"
        );
    }

    #[test]
    fn openvpn_user_auth_requires_credentials() {
        let error = build_singbox_outbound(
            &node_with(json!({
                "protocol": "openvpn",
                "singbox": {"type": "openvpn-client", "lumen_requires_user_auth": true}
            })),
            "proxy",
            false,
            8,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "OpenVPN node requires a username and password"
        );
    }

    #[test]
    fn xhttp_padding_ranges_and_httpupgrade_early_data() {
        assert_eq!(xhttp_padding_range(&json!("100-200")), "100-200");
        assert_eq!(xhttp_padding_range(&json!("0-100")), "1-100");
        assert_eq!(xhttp_padding_range(&json!({"from": 5, "to": 9})), "5-9");
        assert_eq!(xhttp_padding_range(&json!(true)), "");
        assert_eq!(xhttp_padding_range(&json!(300)), "300-300");

        let mut value = json!({
            "type": "vless",
            "transport": {"type": "httpupgrade", "path": "/p?ed=2048"}
        });
        assert!(preserve_or_reject_semantic_fields(&mut value).is_err());
    }

    #[test]
    fn normalize_json_keys_renames_xhttp_fields() {
        let value = json!({"scMaxEachPostBytes": 1, "nested": {"padding": 2}});
        let normalized = normalize_json_keys(&value);
        assert_eq!(normalized["sc_max_each_post_bytes"], json!(1));
        assert_eq!(normalized["nested"]["x_padding_bytes"], json!(2));
    }
}
