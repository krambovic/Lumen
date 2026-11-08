//! Local Xray inbound builders and runtime inbound reconciliation
//! (`xray_inbounds.py`).

use serde_json::{json, Value};

use crate::constants::PROXY_HOST;
use crate::models::str_field;

/// Python defaults: `route_only=False`.
pub fn build_xray_sniffing(route_only: bool) -> Value {
    json!({
        "enabled": true,
        "destOverride": ["http", "tls"],
        "routeOnly": route_only,
    })
}

/// Python defaults: `port=DEFAULT_SOCKS_PORT, tag="socks-in", route_only=False,
/// username="", password=""`.
pub fn build_xray_mixed_inbound(
    port: u16,
    tag: &str,
    route_only: bool,
    username: &str,
    password: &str,
) -> Value {
    let username = username.trim();
    let auth_enabled = !username.is_empty() && !password.is_empty();
    let mut settings = json!({
        "auth": if auth_enabled { "password" } else { "noauth" },
        "udp": true,
        "allowTransparent": false,
    });
    if auth_enabled {
        settings["accounts"] = json!([{"user": username, "pass": password}]);
    }
    json!({
        "tag": tag,
        "listen": PROXY_HOST,
        "port": port,
        "protocol": "mixed",
        "settings": settings,
        "sniffing": build_xray_sniffing(route_only),
    })
}

/// Python defaults: `port=DEFAULT_HTTP_PORT, tag="http-in", route_only=False,
/// username="", password=""`.
pub fn build_xray_http_compat_inbound(
    port: u16,
    tag: &str,
    route_only: bool,
    username: &str,
    password: &str,
) -> Value {
    let username = username.trim();
    let auth_enabled = !username.is_empty() && !password.is_empty();
    let settings = if auth_enabled {
        json!({"accounts": [{"user": username, "pass": password}]})
    } else {
        json!({})
    };
    json!({
        "tag": tag,
        "listen": PROXY_HOST,
        "port": port,
        "protocol": "http",
        "settings": settings,
        "sniffing": build_xray_sniffing(route_only),
    })
}

/// Force the expected sniffing block on every proxy-capable inbound.
/// Returns the number of changed fields, like the Python version.
pub fn normalize_xray_sniffing(payload: &mut Value, route_only: bool) -> i64 {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let mut changed = 0i64;
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() {
            continue;
        }
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        if !["mixed", "socks", "http", "tun"].contains(&protocol.as_str()) {
            continue;
        }
        let sniffing_is_object = inbound.get("sniffing").map(|v| v.is_object()).unwrap_or(false);
        if !sniffing_is_object {
            inbound["sniffing"] = build_xray_sniffing(route_only);
            changed += 1;
            continue;
        }
        let sniffing = inbound.get_mut("sniffing").unwrap();
        if sniffing.get("enabled") != Some(&Value::Bool(true)) {
            sniffing["enabled"] = json!(true);
            changed += 1;
        }
        if sniffing.get("destOverride") != Some(&json!(["http", "tls"])) {
            sniffing["destOverride"] = json!(["http", "tls"]);
            changed += 1;
        }
        // Python checks `sniffing.get("routeOnly") is not want_route_only`
        // (identity, not equality): anything but the exact boolean is replaced.
        if sniffing.get("routeOnly") != Some(&Value::Bool(route_only)) {
            sniffing["routeOnly"] = json!(route_only);
            changed += 1;
        }
    }
    changed
}

/// Python defaults: `socks_port=DEFAULT_SOCKS_PORT, http_port=DEFAULT_HTTP_PORT,
/// route_only=False, username="", password=""`.
pub fn ensure_xray_mixed_proxy_inbound(
    payload: &mut Value,
    socks_port: u16,
    http_port: u16,
    route_only: bool,
    username: &str,
    password: &str,
) -> i64 {
    let has_inbounds = payload.get("inbounds").map(|v| v.is_array()).unwrap_or(false);
    if !has_inbounds {
        payload["inbounds"] = json!([]);
    }
    let mut changed = normalize_xray_sniffing(payload, route_only);

    let mut main_index: i64 = -1;
    let mut http_index: i64 = -1;
    {
        let inbounds = payload["inbounds"].as_array().expect("inbounds is array");
        for (index, inbound) in inbounds.iter().enumerate() {
            if !inbound.is_object() {
                continue;
            }
            let tag = str_field(inbound, "tag").trim().to_string();
            let protocol = str_field(inbound, "protocol").trim().to_lowercase();
            let port = match inbound.get("port") {
                Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
                Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
                Some(Value::Bool(b)) => {
                    if *b {
                        1
                    } else {
                        0
                    }
                }
                _ => 0,
            };
            if tag == "socks-in"
                || tag == "mixed-in"
                || (port == socks_port as i64 && (protocol == "socks" || protocol == "mixed"))
            {
                main_index = index as i64;
            } else if tag == "http-in" || (port == http_port as i64 && protocol == "http") {
                http_index = index as i64;
            }
        }
    }

    let main_inbound = build_xray_mixed_inbound(socks_port, "socks-in", route_only, username, password);
    let inbounds = payload
        .get_mut("inbounds")
        .and_then(|v| v.as_array_mut())
        .expect("inbounds is array");
    if main_index >= 0 {
        let current = &mut inbounds[main_index as usize];
        if let Some(obj) = current.as_object_mut() {
            if let Some(replacement) = main_inbound.as_object() {
                for (key, value) in replacement {
                    if obj.get(key) != Some(value) {
                        obj.insert(key.clone(), value.clone());
                        changed += 1;
                    }
                }
            }
        }
    } else {
        inbounds.insert(0, main_inbound);
        changed += 1;
    }

    if http_port != socks_port {
        let http_inbound =
            build_xray_http_compat_inbound(http_port, "http-in", route_only, username, password);
        let inbounds = payload
            .get_mut("inbounds")
            .and_then(|v| v.as_array_mut())
            .expect("inbounds is array");
        // NOTE: Python parity — http_index was computed before the possible
        // main-inbound insert above, so a fresh main insert shifts the element
        // this index points at. Kept 1:1 on purpose.
        if http_index >= 0 {
            let current = &mut inbounds[http_index as usize];
            if let Some(obj) = current.as_object_mut() {
                if let Some(replacement) = http_inbound.as_object() {
                    for (key, value) in replacement {
                        if obj.get(key) != Some(value) {
                            obj.insert(key.clone(), value.clone());
                            changed += 1;
                        }
                    }
                }
            }
        } else {
            let insert_at = if !inbounds.is_empty() { 1 } else { 0 };
            inbounds.insert(insert_at, http_inbound);
            changed += 1;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};

    #[test]
    fn mixed_inbound_auth_shape() {
        let inbound = build_xray_mixed_inbound(DEFAULT_SOCKS_PORT, "socks-in", false, "u", "p");
        assert_eq!(inbound["protocol"], json!("mixed"));
        assert_eq!(inbound["listen"], json!(PROXY_HOST));
        assert_eq!(inbound["port"], json!(DEFAULT_SOCKS_PORT));
        assert_eq!(inbound["settings"]["auth"], json!("password"));
        assert_eq!(inbound["settings"]["accounts"][0]["user"], json!("u"));

        let anon = build_xray_mixed_inbound(DEFAULT_SOCKS_PORT, "socks-in", true, "", "");
        assert_eq!(anon["settings"]["auth"], json!("noauth"));
        assert_eq!(anon["sniffing"]["routeOnly"], json!(true));
    }

    #[test]
    fn http_compat_inbound_shape() {
        let inbound = build_xray_http_compat_inbound(DEFAULT_HTTP_PORT, "http-in", false, "", "");
        assert_eq!(inbound["protocol"], json!("http"));
        assert_eq!(inbound["settings"], json!({}));
        let authed = build_xray_http_compat_inbound(DEFAULT_HTTP_PORT, "http-in", false, "u", "p");
        assert_eq!(authed["settings"]["accounts"][0]["pass"], json!("p"));
    }

    #[test]
    fn ensure_inserts_both_inbounds() {
        let mut payload = json!({});
        let changed = ensure_xray_mixed_proxy_inbound(&mut payload, 10808, 10809, false, "", "");
        assert!(changed > 0);
        let inbounds = payload["inbounds"].as_array().unwrap();
        assert_eq!(inbounds[0]["tag"], json!("socks-in"));
        assert_eq!(inbounds[1]["tag"], json!("http-in"));

        // Second call is idempotent.
        let changed = ensure_xray_mixed_proxy_inbound(&mut payload, 10808, 10809, false, "", "");
        assert_eq!(changed, 0);
    }

    #[test]
    fn normalize_sniffing_counts_changes() {
        let mut payload = json!({"inbounds": [
            {"tag": "socks-in", "protocol": "mixed", "sniffing": {"enabled": false}},
            {"tag": "api", "protocol": "dokodemo-door"},
        ]});
        let changed = normalize_xray_sniffing(&mut payload, false);
        // enabled + destOverride + routeOnly on the first inbound.
        assert_eq!(changed, 3);
        assert_eq!(payload["inbounds"][0]["sniffing"]["routeOnly"], json!(false));
        // dokodemo-door is untouched.
        assert!(payload["inbounds"][1].get("sniffing").is_none());
    }
}
