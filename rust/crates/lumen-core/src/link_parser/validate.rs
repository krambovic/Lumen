//! Node outbound validation, runtime normalization and repair
//! (`validate_node_outbound`, `normalize_outbound_for_runtime`,
//! `repair_node_outbound_from_link` from link_parser.py).

use std::sync::OnceLock;

use regex::Regex;
use serde_json::{json, Value};

use super::clash::{is_clash_proxy_payload, parse_clash_proxy_payload};
use super::helpers::py_str_or;
use super::wireguard::{validate_awg3_header_key, validate_awg3_range};
use super::{parse_single, Node};

fn awg_uint_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+(?:\s*-\s*\d+)?").expect("static regex"))
}

fn awg_bytes_tag_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<b\s+0x([0-9A-Fa-f]*)>").expect("static regex"))
}

/// Python `is_happ_auto_placeholder(node)` — the dummy SS links used in
/// Dobry's URI subscription.
pub fn is_happ_auto_placeholder(node: &Node) -> bool {
    matches!(node.scheme.as_str(), "ss" | "shadowsocks")
        && node.server.to_lowercase() == "max.ru"
        && node.port == 1234
        && node.name.to_lowercase().contains("автовыбор")
}

/// Python `validate_node_outbound(node)` — `Some(message)` marks the node
/// as not launchable.
pub fn validate_node_outbound(node: &Node) -> Option<String> {
    if is_happ_auto_placeholder(node) {
        return Some(
            "Ссылка автовыбора — служебная заглушка Happ. Импортируйте или обновите подписку, чтобы загрузить полную AUTO-конфигурацию.".to_string(),
        );
    }
    let name_l = node.name.trim().to_lowercase();
    let scheme = node.scheme.trim().to_lowercase();
    if [
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
        "openvpn",
        "openvpn-client",
        "naive",
    ]
    .contains(&scheme.as_str())
        && (node.server.trim().is_empty() || node.port <= 0)
    {
        let label = if node.name.is_empty() {
            node.scheme.clone()
        } else {
            node.name.clone()
        };
        return Some(format!("Сервер {label} пропущен: нет адреса или порта."));
    }

    let server_trim = node.server.trim().to_string();
    if (server_trim == "0.0.0.0" || server_trim == "::") && node.port <= 1 {
        if name_l.contains("приложение не поддерживается")
            || name_l.contains("установите другое приложение")
            || name_l.contains("happ")
            || name_l.contains("unsupported")
        {
            return Some("Провайдер подписки отдал заглушку «приложение не поддерживается». Lumen попробует совместимые профили клиента; если ошибка повторяется, нужен формат sing-box/SFA или Clash.".to_string());
        }
        let label = node.name.trim();
        if !label.is_empty() {
            return Some(format!("Провайдер подписки вернул служебное сообщение: {label}"));
        }
        return Some("Сервер-заглушка 0.0.0.0:1 пропущен.".to_string());
    }

    let outbound = if node.outbound.is_object() {
        node.outbound.clone()
    } else {
        json!({})
    };
    let native_outbound = match outbound.get("singbox") {
        Some(v) if v.is_object() => v.clone(),
        _ => outbound.clone(),
    };
    let effective_scheme = {
        let a = scheme.clone();
        if !a.is_empty() {
            a
        } else {
            py_str_or(outbound.get("protocol"), "").trim().to_lowercase()
        }
    };
    if effective_scheme == "openvpn" {
        let servers = native_outbound.get("servers").and_then(Value::as_array);
        let Some(servers) = servers else {
            return Some("OpenVPN-профиль не содержит ни одного remote-сервера.".to_string());
        };
        if servers.is_empty() {
            return Some("OpenVPN-профиль не содержит ни одного remote-сервера.".to_string());
        }
        for server_options in servers {
            let Some(server_options) = server_options.as_object() else {
                return Some("OpenVPN servers должен быть списком объектов.".to_string());
            };
            let remote_port = match server_options.get("server_port") {
                Some(v) if !matches!(v, serde_json::Value::Null) => {
                    super::helpers::py_int_or(Some(v), 0).unwrap_or(0)
                }
                _ => 0,
            };
            if super::helpers::py_str_or(server_options.get("server"), "")
                .trim()
                .is_empty()
                || remote_port <= 0
            {
                return Some("OpenVPN remote должен содержать адрес и порт.".to_string());
            }
        }
        let proto = super::helpers::py_str_or(native_outbound.get("proto"), "")
            .trim()
            .to_lowercase();
        if proto != "udp" && proto != "tcp" {
            return Some("OpenVPN transport должен быть udp или tcp.".to_string());
        }
        let tls_openvpn = native_outbound
            .get("tls")
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let ca = {
            let a = super::helpers::py_str_or(tls_openvpn.get("ca"), "");
            if !a.is_empty() {
                a
            } else {
                super::helpers::py_str_or(tls_openvpn.get("ca_path"), "")
            }
        };
        if ca.trim().is_empty() {
            return Some("OpenVPN-профиль не содержит CA-сертификат.".to_string());
        }
        let has_tls_auth = super::helpers::py_truthy_opt(native_outbound.get("tls_auth"))
            || super::helpers::py_truthy_opt(native_outbound.get("tls_auth_path"));
        let has_tls_crypt = super::helpers::py_truthy_opt(native_outbound.get("tls_crypt"))
            || super::helpers::py_truthy_opt(native_outbound.get("tls_crypt_path"));
        if has_tls_auth && has_tls_crypt {
            return Some("OpenVPN TLS Auth и TLS Crypt нельзя включать одновременно.".to_string());
        }
        if super::helpers::py_truthy_opt(native_outbound.get("tls_crypt_v2")) && !has_tls_crypt {
            return Some("OpenVPN TLS Crypt v2 включён без ключа TLS Crypt.".to_string());
        }
    }
    if let Some(problem) = validate_wireguard_endpoint(node, &native_outbound) {
        return Some(problem);
    }
    let stream_settings = outbound.get("streamSettings").filter(|v| v.is_object());
    let Some(stream_settings) = stream_settings else {
        return None;
    };

    let security = super::helpers::py_str_or(stream_settings.get("security"), "")
        .trim()
        .to_lowercase();
    if security != "reality" {
        return None;
    }

    let reality_settings = stream_settings
        .get("realitySettings")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    let public_key = super::helpers::py_str_or(reality_settings.get("publicKey"), "")
        .trim()
        .to_string();
    let node_name = {
        let label = if node.name.is_empty() {
            node.server.clone()
        } else {
            node.name.clone()
        };
        let label = label.trim().to_string();
        if label.is_empty() {
            "безымянный сервер".to_string()
        } else {
            label
        }
    };
    if public_key.is_empty() {
        return Some(format!(
            "Сервер {node_name} не может быть запущен: для REALITY обязателен publicKey (параметр pbk в VLESS-ссылке), но в этой ссылке он пустой или отсутствует."
        ));
    }

    let short_id = super::helpers::py_str_or(reality_settings.get("shortId"), "")
        .trim()
        .to_string();
    if !short_id.is_empty()
        && (short_id.len() > 16
            || short_id.len() % 2 != 0
            || short_id
                .chars()
                .any(|ch| !"0123456789abcdefABCDEF".contains(ch)))
    {
        return Some(format!(
            "Сервер {node_name} не может быть запущен: shortId для REALITY должен быть hex-строкой четной длины до 16 символов. Проверьте параметр sid в ссылке."
        ));
    }
    None
}

/// Python `_validate_wireguard_endpoint(node, endpoint)`.
pub(super) fn validate_wireguard_endpoint(node: &Node, endpoint: &Value) -> Option<String> {
    let endpoint_type = super::helpers::py_str_or(endpoint.get("type"), "")
        .trim()
        .to_lowercase();
    let amnezia = endpoint.get("amnezia").cloned();
    if endpoint_type != "wireguard" && endpoint_type != "awg" {
        if let Some(amnezia) = amnezia {
            if amnezia.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
                return validate_amnezia_settings(node, &amnezia);
            }
        }
        return None;
    }

    let node_label = {
        let label = if node.name.is_empty() {
            node.server.clone()
        } else {
            node.name.clone()
        };
        let label = label.trim().to_string();
        if label.is_empty() {
            "WireGuard".to_string()
        } else {
            label
        }
    };
    if super::helpers::py_str_or(endpoint.get("private_key"), "")
        .trim()
        .is_empty()
    {
        return Some(format!(
            "Сервер {node_label} не содержит private_key для WireGuard/AWG."
        ));
    }
    let addresses_ok = endpoint
        .get("address")
        .and_then(Value::as_array)
        .map(|items| items.iter().any(|item| !super::helpers::py_str_or(Some(item), "").trim().is_empty()))
        .unwrap_or(false);
    if !addresses_ok {
        return Some(format!(
            "Сервер {node_label} не содержит адрес интерфейса WireGuard/AWG."
        ));
    }
    let peers = endpoint.get("peers").and_then(Value::as_array);
    let Some(peers) = peers else {
        return Some(format!("Сервер {node_label} не содержит peers для WireGuard/AWG."));
    };
    if peers.is_empty() {
        return Some(format!("Сервер {node_label} не содержит peers для WireGuard/AWG."));
    }
    for peer in peers {
        let peer_ok = peer.as_object().map(|obj| {
            !super::helpers::py_str_or(obj.get("public_key"), "").trim().is_empty()
        });
        if peer_ok != Some(true) {
            return Some(format!(
                "Сервер {node_label}: у peer отсутствует public_key."
            ));
        }
        let peer_obj = peer.as_object().cloned().unwrap_or_default();
        let peer_address = super::helpers::py_str_or(peer_obj.get("address"), "")
            .trim()
            .to_string();
        let peer_port = super::helpers::py_int_or(peer_obj.get("port"), 0).unwrap_or(0);
        if peer_address.is_empty() || !(0 < peer_port && peer_port <= 65535) {
            return Some(format!(
                "Сервер {node_label}: у peer отсутствует корректный address или port."
            ));
        }
    }
    if let Some(amnezia) = amnezia {
        if amnezia.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
            return validate_amnezia_settings(node, &amnezia);
        }
    }
    None
}

/// Python `_validate_amnezia_settings(node, amnezia)`.
fn validate_amnezia_settings(node: &Node, amnezia: &Value) -> Option<String> {
    let node_label = {
        let label = if node.name.is_empty() {
            node.server.clone()
        } else {
            node.name.clone()
        };
        let label = label.trim().to_string();
        if label.is_empty() {
            "AWG".to_string()
        } else {
            label
        }
    };
    let map = amnezia.as_object().cloned().unwrap_or_default();
    let get = |key: &str| map.get(key).cloned().unwrap_or(Value::Null);
    for key in ["jc", "jmin", "jmax", "s1", "s2", "s3", "s4", "itime"] {
        let value = get(key);
        if !matches!(value, Value::Null) {
            // Python `type(value) is not int` — bools and floats both fail.
            let is_int = super::helpers::is_py_int(&value);
            if !is_int {
                return Some(format!(
                    "Сервер {node_label}: параметр AWG `{key}` должен быть целым числом."
                ));
            }
        }
    }
    for key in ["h1", "h2", "h3", "h4"] {
        let value = get(key);
        if matches!(value, Value::Null) {
            continue;
        }
        if super::helpers::is_py_int(&value) {
            continue;
        }
        let ok = value
            .as_str()
            .map(|s| awg_uint_range_regex().find(s.trim()).map(|m| m.as_str()) == Some(s.trim()) && !s.trim().is_empty())
            .unwrap_or(false);
        if !ok {
            return Some(format!(
                "Сервер {node_label}: параметр AWG `{key}` должен быть числом или диапазоном."
            ));
        }
    }
    for key in ["i1", "i2", "i3", "i4", "i5", "j1", "j2", "j3"] {
        let value = get(key);
        if matches!(value, Value::Null) {
            continue;
        }
        if !value.is_string() {
            return Some(format!(
                "Сервер {node_label}: параметр AWG `{key}` должен быть строкой."
            ));
        }
        let text = value.as_str().unwrap_or("");
        for captures in awg_bytes_tag_regex().captures_iter(text) {
            let hex = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            if hex.len() % 2 != 0 {
                return Some(format!(
                    "AWG: нечётное число hex-символов в {}.",
                    key.to_uppercase()
                ));
            }
        }
    }
    let header_key = get("header_protection_key");
    if !matches!(header_key, Value::Null) {
        let text = super::helpers::py_str_or(Some(&header_key), "");
        if let Err(exc) = validate_awg3_header_key(&text) {
            return Some(format!("Сервер {node_label}: {exc}"));
        }
        for key in ["s1", "s2", "s3", "s4"] {
            let value = get(key);
            let is_int = super::helpers::is_py_int(&value);
            let as_int = if is_int {
                super::helpers::py_int_value(&value).unwrap_or(0)
            } else {
                -1
            };
            if !is_int || as_int < 12 {
                return Some(format!(
                    "Сервер {node_label}: AWG 3.0 с HeaderProtectionKey требует {} не меньше 12.",
                    key.to_uppercase()
                ));
            }
        }
    }
    for key in [
        "content_padding_addition",
        "rekey_after_time",
        "rekey_timeout",
        "reject_after_time",
        "keepalive_timeout",
        "max_handshake_attempts",
    ] {
        let value = get(key);
        if matches!(value, Value::Null) {
            continue;
        }
        // Python `isinstance(value, bool) or not isinstance(value, (int, str))`.
        let valid_type = value.is_string()
            || super::helpers::is_py_int(&value);
        if value.is_boolean() || !valid_type {
            return Some(format!(
                "Сервер {node_label}: параметр AWG `{key}` должен быть числом или диапазоном A-B."
            ));
        }
        let text = super::helpers::py_str_or(Some(&value), "");
        if let Err(exc) = validate_awg3_range(&text, key) {
            return Some(format!("Сервер {node_label}: {exc}"));
        }
    }
    let jmin = get("jmin");
    let jmax = get("jmax");
    let jmin_int = super::helpers::is_py_int(&jmin);
    let jmax_int = super::helpers::is_py_int(&jmax);
    if jmin_int && jmax_int {
        let jmin_value = super::helpers::py_int_value(&jmin).unwrap_or(0);
        let jmax_value = super::helpers::py_int_value(&jmax).unwrap_or(0);
        if jmin_value > jmax_value {
            return Some(format!(
                "Сервер {node_label}: параметр AWG `jmin` не может быть больше `jmax`."
            ));
        }
    }
    None
}

/// Python `normalize_node_outbound(node)` — in-place runtime normalization.
pub fn normalize_node_outbound(node: &mut Node) -> bool {
    if !node.outbound.is_object() {
        return false;
    }
    normalize_outbound_for_runtime(&mut node.outbound, &node.server)
}

/// Python `normalize_outbound_for_runtime(outbound, server)`.
pub fn normalize_outbound_for_runtime(outbound: &mut Value, server: &str) -> bool {
    let mut changed = false;
    if !outbound.is_object() {
        return false;
    }
    let stream_settings = outbound.get("streamSettings").cloned();
    let Some(mut stream_settings) = stream_settings.filter(|v| v.is_object()) else {
        return false;
    };

    if super::helpers::py_str_or(stream_settings.get("network"), "")
        .trim()
        .to_lowercase()
        == "xhttp"
    {
        let mux = outbound.get("mux").cloned();
        match mux {
            Some(mut mux) if mux.is_object() => {
                let obj = mux.as_object_mut().unwrap();
                if !matches!(obj.get("enabled"), Some(Value::Bool(false))) {
                    obj.insert("enabled".to_string(), Value::Bool(false));
                    changed = true;
                }
                if obj.get("concurrency") != Some(&json!(-1)) {
                    obj.insert("concurrency".to_string(), json!(-1));
                    changed = true;
                }
                if let Some(out_obj) = outbound.as_object_mut() {
                    out_obj.insert("mux".to_string(), mux);
                }
            }
            _ => {
                if let Some(out_obj) = outbound.as_object_mut() {
                    out_obj.insert("mux".to_string(), json!({ "enabled": false, "concurrency": -1 }));
                }
                changed = true;
            }
        }
    }

    let security = super::helpers::py_str_or(stream_settings.get("security"), "")
        .trim()
        .to_lowercase();
    if security == "tls" {
        let tls_settings = stream_settings.get("tlsSettings").cloned();
        let mut tls_settings = match tls_settings {
            Some(v) if v.is_object() => v,
            _ => {
                changed = true;
                json!({})
            }
        };
        let server_name = super::helpers::py_str_or(tls_settings.get("serverName"), "");
        if server_name.trim().is_empty() {
            let inferred = infer_transport_host(&stream_settings);
            let inferred = if inferred.is_empty() {
                server.to_string()
            } else {
                inferred
            };
            if !inferred.is_empty() {
                if let Some(obj) = tls_settings.as_object_mut() {
                    obj.insert("serverName".to_string(), Value::String(inferred));
                }
                changed = true;
            }
        }
        let has_ech = tls_settings
            .get("echConfigList")
            .map(super::helpers::py_truthy)
            .unwrap_or(false);
        let has_force = tls_settings
            .get("echForceQuery")
            .map(super::helpers::py_truthy)
            .unwrap_or(false);
        if has_ech && !has_force {
            if let Some(obj) = tls_settings.as_object_mut() {
                obj.insert("echForceQuery".to_string(), Value::String("full".into()));
            }
            changed = true;
        }
        if let Some(stream_obj) = stream_settings.as_object_mut() {
            stream_obj.insert("tlsSettings".to_string(), tls_settings);
        }
    } else if security == "reality" {
        let reality_settings = stream_settings.get("realitySettings").cloned();
        let mut reality_settings = match reality_settings {
            Some(v) if v.is_object() => v,
            _ => {
                changed = true;
                json!({})
            }
        };
        if !matches!(reality_settings.get("show"), Some(Value::Bool(false))) {
            if let Some(obj) = reality_settings.as_object_mut() {
                obj.insert("show".to_string(), Value::Bool(false));
            }
            changed = true;
        }
        if let Some(stream_obj) = stream_settings.as_object_mut() {
            stream_obj.insert("realitySettings".to_string(), reality_settings);
        }
    }
    if let Some(out_obj) = outbound.as_object_mut() {
        out_obj.insert("streamSettings".to_string(), stream_settings);
    }
    changed
}

/// Python `_infer_transport_host(stream_settings)`.
fn infer_transport_host(stream_settings: &Value) -> String {
    let network = super::helpers::py_str_or(stream_settings.get("network"), "")
        .trim()
        .to_lowercase();
    if network == "ws" {
        let headers = stream_settings
            .get("wsSettings")
            .filter(|v| v.is_object())
            .and_then(|ws| ws.get("headers"))
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        if headers.is_object() {
            let host = {
                let a = super::helpers::py_str_or(headers.get("Host"), "");
                if !a.is_empty() {
                    a
                } else {
                    super::helpers::py_str_or(headers.get("host"), "")
                }
            };
            return host.split(',').next().unwrap_or("").trim().to_string();
        }
    }
    if network == "http" || network == "h2" {
        if let Some(http_settings) = stream_settings.get("httpSettings").filter(|v| v.is_object()) {
            if let Some(hosts) = http_settings.get("host") {
                if let Some(items) = hosts.as_array() {
                    if !items.is_empty() {
                        return super::helpers::py_str_or(Some(&items[0]), "")
                            .trim()
                            .to_string();
                    }
                } else if let Some(text) = hosts.as_str() {
                    return text.split(',').next().unwrap_or("").trim().to_string();
                }
            }
        }
    }
    if network == "grpc" {
        if let Some(grpc_settings) = stream_settings.get("grpcSettings").filter(|v| v.is_object()) {
            return super::helpers::py_str_or(grpc_settings.get("authority"), "")
                .trim()
                .to_string();
        }
    }
    String::new()
}

/// Python `repair_node_outbound_from_link(node)`.
pub fn repair_node_outbound_from_link(node: &mut Node) -> bool {
    let link = node.link.trim().to_string();
    if link.is_empty() {
        return repair_legacy_direct_masque_shape(node);
    }
    let mut clash_payload: Option<Value> = None;
    if link.starts_with('{') {
        if let Ok(decoded) = serde_json::from_str::<Value>(&link) {
            if is_clash_proxy_payload(&decoded) {
                clash_payload = Some(decoded);
            }
        }
    }
    let reparsed = if clash_payload.is_some() {
        parse_clash_proxy_payload(clash_payload.as_ref().unwrap())
    } else {
        parse_single(&link)
    };
    let reparsed = match reparsed {
        Ok(node) => node,
        Err(_) => return repair_legacy_direct_masque_shape(node),
    };
    let mut changed = false;
    if clash_payload.is_some() {
        let reparsed_changed = reparsed.outbound != node.outbound
            || reparsed.link != node.link
            || reparsed.scheme != node.scheme;
        if reparsed_changed {
            node.outbound = reparsed.outbound.clone();
            node.link = reparsed.link.clone();
            node.scheme = reparsed.scheme.clone();
            changed = true;
        }
    } else if reparsed.outbound != node.outbound {
        node.outbound = reparsed.outbound.clone();
        changed = true;
    }
    if node.scheme.is_empty() {
        node.scheme = reparsed.scheme.clone();
        changed = true;
    }
    if node.server.is_empty() {
        node.server = reparsed.server.clone();
        changed = true;
    }
    if node.port <= 0 {
        node.port = reparsed.port;
        changed = true;
    }
    repair_legacy_direct_masque_shape(node) || changed
}

/// Python `_repair_legacy_direct_masque_shape(node)` — move old Clash MASQUE
/// credentials out of the registration profile.
pub fn repair_legacy_direct_masque_shape(node: &mut Node) -> bool {
    if !node.outbound.is_object() {
        return false;
    }
    let outbound = node.outbound.as_object().cloned().unwrap_or_default();
    let Some(native_value) = outbound.get("singbox").filter(|v| v.is_object()).cloned() else {
        return false;
    };
    let mut native = native_value.as_object().cloned().unwrap_or_default();
    let protocol = {
        let a = py_str_or(outbound.get("protocol"), "");
        let a = if !a.is_empty() {
            a
        } else if !node.scheme.is_empty() {
            node.scheme.clone()
        } else {
            py_str_or(native.get("type"), "")
        };
        a.trim().to_lowercase()
    };
    if protocol != "masque"
        && py_str_or(native.get("type"), "").trim().to_lowercase() != "masque"
    {
        return false;
    }

    let profile = native
        .get("profile")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let profile_obj = profile.as_object().cloned().unwrap_or_default();
    let profile_private_key = py_str_or(profile_obj.get("private_key"), "")
        .trim()
        .to_string();
    let private_key = {
        let a = py_str_or(native.get("private_key"), "");
        if !a.is_empty() {
            a
        } else {
            profile_private_key.clone()
        }
    }
    .trim()
    .to_string();
    let public_key = py_str_or(native.get("public_key"), "").trim().to_string();
    let mut changed = false;
    let mut address = native.get("tunnel_address").cloned();
    let legacy_address = native.get("address").cloned();
    let address_missing = match &address {
        None | Some(Value::Null) => true,
        Some(Value::Array(items)) => items.is_empty(),
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::Bool(b)) => !*b,
        _ => false,
    };
    if address_missing && legacy_address.as_ref().map(|v| v.is_array()).unwrap_or(false) {
        address = legacy_address.clone();
        native.insert("tunnel_address".to_string(), address.clone().unwrap_or(Value::Null));
        native.shift_remove("address");
        changed = true;
    }
    let server = {
        let a = py_str_or(native.get("server"), "");
        if !a.is_empty() {
            a
        } else {
            node.server.clone()
        }
    }
    .trim()
    .to_string();
    let has_address = match &address {
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    };
    if private_key.is_empty() || public_key.is_empty() || !has_address || server.is_empty() {
        // Python mutated `native` (the live singbox dict) before this check,
        // so the tunnel_address migration persists even on the failure path.
        if changed {
            if let Some(obj) = node.outbound.as_object_mut() {
                obj.insert("singbox".to_string(), Value::Object(native));
            }
        }
        return false;
    }

    if native.get("private_key").cloned().unwrap_or(Value::Null) != Value::String(private_key.clone()) {
        native.insert("private_key".to_string(), Value::String(private_key.clone()));
        changed = true;
    }
    if py_str_or(native.get("server"), "").trim() != server {
        native.insert("server".to_string(), Value::String(server.clone()));
        changed = true;
    }
    let has_server_port = native
        .get("server_port")
        .map(super::helpers::py_truthy)
        .unwrap_or(false);
    if !has_server_port && node.port > 0 {
        native.insert("server_port".to_string(), json!(node.port));
        changed = true;
    }

    let mut normalized_profile = profile_obj.clone();
    if normalized_profile.shift_remove("private_key").is_some() {
        changed = true;
    }
    let detour = py_str_or(normalized_profile.get("detour"), "");
    if detour != "direct" {
        normalized_profile.insert("detour".to_string(), Value::String("direct".into()));
        changed = true;
    }
    if native.get("profile").cloned().unwrap_or(Value::Null) != Value::Object(normalized_profile.clone()) {
        native.insert("profile".to_string(), Value::Object(normalized_profile));
        changed = true;
    }

    if let Some(obj) = node.outbound.as_object_mut() {
        obj.insert("singbox".to_string(), Value::Object(native));
    }
    changed
}
