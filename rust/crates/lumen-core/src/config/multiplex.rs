//! Connection multiplexing toggles (`multiplex.py`).

use serde_json::{json, Value};

use crate::models::str_field;

const SUPPORTED_PROXY_TYPES: [&str; 4] = ["vless", "vmess", "trojan", "shadowsocks"];

/// Python `int(value)` semantics with a fallback of 8 for garbage, clamped to 1..=32.
pub fn normalize_mux_concurrency(value: &Value) -> i64 {
    let concurrency = match value {
        Value::Number(n) => n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().map(|f| f as i64).unwrap_or(8)),
        Value::String(s) => s.trim().parse::<i64>().unwrap_or(8),
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        _ => 8,
    };
    concurrency.clamp(1, 32)
}

pub fn apply_xray_multiplex(outbound: &mut Value, enabled: bool, concurrency: i64) -> bool {
    let protocol = str_field(outbound, "protocol").trim().to_lowercase();
    if !SUPPORTED_PROXY_TYPES.contains(&protocol.as_str()) || uses_xtls_vision(outbound) {
        return false;
    }

    if !enabled {
        outbound["mux"] = json!({"enabled": false, "concurrency": -1});
        return false;
    }

    let streams = normalize_mux_concurrency(&json!(concurrency));
    outbound["mux"] = json!({"enabled": true, "concurrency": streams});
    true
}

pub fn apply_singbox_multiplex(outbound: &mut Value, enabled: bool, concurrency: i64) -> bool {
    let outbound_type = str_field(outbound, "type").trim().to_lowercase();
    if !SUPPORTED_PROXY_TYPES.contains(&outbound_type.as_str())
        || uses_singbox_xtls_vision(outbound)
        || (outbound_type == "shadowsocks" && udp_over_tcp_enabled(outbound.get("udp_over_tcp")))
    {
        if let Some(obj) = outbound.as_object_mut() {
            obj.remove("multiplex");
        }
        return false;
    }

    if !enabled {
        if let Some(obj) = outbound.as_object_mut() {
            obj.remove("multiplex");
        }
        return false;
    }

    let streams = normalize_mux_concurrency(&json!(concurrency));
    outbound["multiplex"] = json!({
        "enabled": true,
        "protocol": "smux",
        "max_connections": 4,
        "min_streams": 4,
        "max_streams": streams,
    });
    true
}

fn udp_over_tcp_enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Object(obj)) => obj.get("enabled") != Some(&Value::Bool(false)),
        Some(Value::Null) | None => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn uses_xtls_vision(outbound: &Value) -> bool {
    let Some(users) = outbound
        .get("settings")
        .and_then(|s| s.get("vnext"))
        .and_then(|v| v.as_array())
    else {
        return false;
    };
    let Some(first) = users.first() else {
        return false;
    };
    let Some(users) = first.get("users").and_then(|u| u.as_array()) else {
        return false;
    };
    let Some(user) = users.first() else {
        return false;
    };
    crate::models::str_field(user, "flow").trim().to_lowercase() == "xtls-rprx-vision"
}

fn uses_singbox_xtls_vision(outbound: &Value) -> bool {
    str_field(outbound, "flow").trim().to_lowercase() == "xtls-rprx-vision"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mux_inserted_when_enabled() {
        let mut outbound = json!({"protocol": "vless", "settings": {}});
        assert!(apply_xray_multiplex(&mut outbound, true, 8));
        assert_eq!(outbound["mux"]["enabled"], json!(true));
        assert_eq!(outbound["mux"]["concurrency"], json!(8));
    }

    #[test]
    fn mux_disabled_marker_when_off() {
        let mut outbound = json!({"protocol": "trojan", "settings": {}});
        assert!(!apply_xray_multiplex(&mut outbound, false, 8));
        assert_eq!(outbound["mux"]["concurrency"], json!(-1));
    }

    #[test]
    fn vision_flow_blocks_mux() {
        let mut outbound = json!({
            "protocol": "vless",
            "settings": {"vnext": [{"users": [{"flow": "xtls-rprx-vision"}]}]},
        });
        assert!(!apply_xray_multiplex(&mut outbound, true, 8));
        assert!(outbound.get("mux").is_none());
    }

    #[test]
    fn concurrency_clamped() {
        assert_eq!(normalize_mux_concurrency(&json!(0)), 1);
        assert_eq!(normalize_mux_concurrency(&json!(99)), 32);
        assert_eq!(normalize_mux_concurrency(&json!("junk")), 8);
    }

    #[test]
    fn singbox_multiplex_shape() {
        let mut outbound = json!({"type": "vless", "flow": ""});
        assert!(apply_singbox_multiplex(&mut outbound, true, 12));
        assert_eq!(outbound["multiplex"]["protocol"], json!("smux"));
        assert_eq!(outbound["multiplex"]["max_streams"], json!(12));

        let mut ss = json!({"type": "shadowsocks", "udp_over_tcp": {"enabled": true}});
        assert!(!apply_singbox_multiplex(&mut ss, true, 8));
        assert!(ss.get("multiplex").is_none());
    }
}
