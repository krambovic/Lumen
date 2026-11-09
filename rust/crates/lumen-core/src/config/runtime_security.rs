//! Clamp/strip/generate credential helpers used around config builds
//! (`application/runtime_security.py`).

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::constants::PROXY_HOST;
use crate::models::str_field;

const PROXY_INBOUND_TYPES: [&str; 3] = ["socks", "http", "mixed"];
const LOCAL_LISTEN_HOSTS: [&str; 3] = [PROXY_HOST, "localhost", "::1"];
const XRAY_LOCAL_INBOUND_TAGS: [&str; 3] = ["api", "__app_metrics_api_in", "discord-socks-in"];
const SINGBOX_LOCAL_INBOUND_TAGS: [&str; 3] =
    ["__app_hybrid_protect_in", "__app_hybrid_relay_in", "discord-socks-in"];

/// Python defaults: `prefix="local", password_length=24`.
pub fn generate_local_proxy_credentials(prefix: &str, password_length: usize) -> (String, String) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let suffix: String = (0..3)
        .map(|_| format!("{:02x}", rng.r#gen::<u8>()))
        .collect();
    let username = format!("{prefix}-{suffix}");
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let password: String = (0..password_length)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect();
    (username, password)
}

fn keep_tag_set<'a>(keep_tags: &'a [&'a str]) -> HashSet<&'a str> {
    keep_tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn inbound_type_str(inbound: &Value, type_key: &str) -> String {
    // Python: str(inbound.get(type_key) or inbound.get("protocol") or "").strip().lower()
    let mut value = str_field(inbound, type_key);
    if value.trim().is_empty() {
        value = str_field(inbound, "protocol");
    }
    value.trim().to_lowercase()
}

fn is_loopback_listen(value: Option<&Value>) -> bool {
    let text = match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(_)) | Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    LOCAL_LISTEN_HOSTS.contains(&text.trim().to_lowercase().as_str())
}

pub fn strip_xray_proxy_inbounds(payload: &mut Value, keep_tags: &[&str]) -> i64 {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let keep = keep_tag_set(keep_tags);
    let mut removed = 0i64;
    inbounds.retain(|inbound| {
        if !inbound.is_object() {
            return true;
        }
        let tag = str_field(inbound, "tag").trim().to_string();
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        if keep.contains(tag.as_str()) || !PROXY_INBOUND_TYPES.contains(&protocol.as_str()) {
            return true;
        }
        removed += 1;
        false
    });
    removed
}

pub fn strip_singbox_proxy_inbounds(payload: &mut Value, keep_tags: &[&str]) -> i64 {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let keep = keep_tag_set(keep_tags);
    let mut removed = 0i64;
    inbounds.retain(|inbound| {
        if !inbound.is_object() {
            return true;
        }
        let tag = str_field(inbound, "tag").trim().to_string();
        let inbound_type = inbound_type_str(inbound, "type");
        if keep.contains(tag.as_str()) || !PROXY_INBOUND_TYPES.contains(&inbound_type.as_str()) {
            return true;
        }
        removed += 1;
        false
    });
    removed
}

/// Keep local proxy/control inbounds on loopback only.
///
/// The public system proxy must remain no-auth for WinINET/Necko compatibility,
/// so the meaningful safety boundary is never exposing those inbounds to LAN.
/// With allow_lan the user explicitly opts in to LAN clients (v2rayN-style).
pub fn clamp_xray_local_inbounds(payload: &mut Value, allow_lan: bool) -> i64 {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let mut changed = 0i64;
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() {
            continue;
        }
        let tag = str_field(inbound, "tag").trim().to_string();
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        if allow_lan && PROXY_INBOUND_TYPES.contains(&protocol.as_str()) && tag != "api" {
            continue;
        }
        let should_clamp =
            PROXY_INBOUND_TYPES.contains(&protocol.as_str()) || XRAY_LOCAL_INBOUND_TAGS.contains(&tag.as_str());
        if !should_clamp {
            continue;
        }
        if !is_loopback_listen(inbound.get("listen")) {
            inbound["listen"] = json!(PROXY_HOST);
            changed += 1;
        }
        if tag == "api" || tag == "__app_metrics_api_in" || protocol == "dokodemo-door" {
            if let Some(settings) = inbound.get_mut("settings") {
                if settings.is_object() && !is_loopback_listen(settings.get("address")) {
                    settings["address"] = json!(PROXY_HOST);
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Keep sing-box local proxy/control inbounds on loopback only.
pub fn clamp_singbox_local_inbounds(payload: &mut Value) -> i64 {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let mut changed = 0i64;
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() {
            continue;
        }
        let tag = str_field(inbound, "tag").trim().to_string();
        let inbound_type = inbound_type_str(inbound, "type");
        let should_clamp =
            PROXY_INBOUND_TYPES.contains(&inbound_type.as_str()) || SINGBOX_LOCAL_INBOUND_TAGS.contains(&tag.as_str());
        if !should_clamp {
            continue;
        }
        if !is_loopback_listen(inbound.get("listen")) {
            inbound["listen"] = json!(PROXY_HOST);
            changed += 1;
        }
    }
    changed
}

/// Python `tag=None` maps to `None` here; pass `Some(tag)` to target one inbound.
pub fn set_xray_socks_inbound_auth(
    payload: &mut Value,
    username: &str,
    password: &str,
    tag: Option<&str>,
) -> bool {
    let Some(inbounds) = payload.get_mut("inbounds").and_then(|v| v.as_array_mut()) else {
        return false;
    };
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() {
            continue;
        }
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        if protocol != "socks" && protocol != "mixed" {
            continue;
        }
        let inbound_tag = str_field(inbound, "tag").trim().to_string();
        if let Some(tag) = tag {
            if inbound_tag != tag {
                continue;
            }
        }
        if !inbound.get("settings").map(|v| v.is_object()).unwrap_or(false) {
            inbound["settings"] = json!({});
        }
        let settings = inbound.get_mut("settings").unwrap();
        settings["auth"] = json!("password");
        settings["accounts"] = json!([{"user": username, "pass": password}]);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credentials_shape() {
        let (username, password) = generate_local_proxy_credentials("local", 24);
        assert!(username.starts_with("local-"));
        assert_eq!(username.len(), "local-".len() + 6);
        assert_eq!(password.len(), 24);
        assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn strip_removes_proxy_inbounds_but_keeps_keep_tags() {
        let mut payload = json!({"inbounds": [
            {"tag": "socks-in", "protocol": "mixed"},
            {"tag": "http-in", "protocol": "http"},
            {"tag": "api", "protocol": "dokodemo-door"},
            "junk",
        ]});
        let removed = strip_xray_proxy_inbounds(&mut payload, &["socks-in"]);
        assert_eq!(removed, 1);
        let inbounds = payload["inbounds"].as_array().unwrap();
        assert_eq!(inbounds.len(), 3);
        assert_eq!(inbounds[0]["tag"], json!("socks-in"));
        assert_eq!(inbounds[2], json!("junk"));
    }

    #[test]
    fn strip_singbox_uses_type_field() {
        let mut payload = json!({"inbounds": [
            {"tag": "mixed-in", "type": "mixed"},
            {"tag": "legacy", "protocol": "socks"},
            {"tag": "tun-in", "type": "tun"},
        ]});
        let removed = strip_singbox_proxy_inbounds(&mut payload, &[]);
        assert_eq!(removed, 2);
    }

    #[test]
    fn clamp_xray_pins_loopback() {
        let mut payload = json!({"inbounds": [
            {"tag": "socks-in", "protocol": "mixed", "listen": "0.0.0.0"},
            {"tag": "api", "protocol": "dokodemo-door", "listen": "0.0.0.0",
             "settings": {"address": "0.0.0.0"}},
            {"tag": "other", "protocol": "freedom", "listen": "0.0.0.0"},
        ]});
        let changed = clamp_xray_local_inbounds(&mut payload, false);
        assert_eq!(changed, 3);
        assert_eq!(payload["inbounds"][0]["listen"], json!(PROXY_HOST));
        assert_eq!(payload["inbounds"][1]["listen"], json!(PROXY_HOST));
        assert_eq!(payload["inbounds"][1]["settings"]["address"], json!(PROXY_HOST));
        // Unrelated protocol stays untouched.
        assert_eq!(payload["inbounds"][2]["listen"], json!("0.0.0.0"));

        // allow_lan keeps user-facing proxy inbounds as-is.
        let mut lan = json!({"inbounds": [{"tag": "socks-in", "protocol": "mixed", "listen": "0.0.0.0"}]});
        assert_eq!(clamp_xray_local_inbounds(&mut lan, true), 0);
    }

    #[test]
    fn clamp_singbox_pins_loopback() {
        let mut payload = json!({"inbounds": [
            {"tag": "mixed-in", "type": "mixed", "listen": "0.0.0.0"},
            {"tag": "__app_hybrid_protect_in", "type": "socks", "listen": "0.0.0.0"},
            {"tag": "tun-in", "type": "tun", "listen": "0.0.0.0"},
        ]});
        let changed = clamp_singbox_local_inbounds(&mut payload);
        assert_eq!(changed, 2);
        assert_eq!(payload["inbounds"][0]["listen"], json!(PROXY_HOST));
        assert_eq!(payload["inbounds"][1]["listen"], json!(PROXY_HOST));
        assert_eq!(payload["inbounds"][2]["listen"], json!("0.0.0.0"));
    }

    #[test]
    fn set_socks_auth_targets_first_match() {
        let mut payload = json!({"inbounds": [
            {"tag": "api", "protocol": "dokodemo-door"},
            {"tag": "socks-in", "protocol": "mixed", "settings": {}},
        ]});
        assert!(set_xray_socks_inbound_auth(&mut payload, "u", "p", None));
        let settings = &payload["inbounds"][1]["settings"];
        assert_eq!(settings["auth"], json!("password"));
        assert_eq!(settings["accounts"][0]["user"], json!("u"));

        let mut mismatch = json!({"inbounds": [{"tag": "other", "protocol": "mixed"}]});
        assert!(!set_xray_socks_inbound_auth(&mut mismatch, "u", "p", Some("socks-in")));
    }
}
