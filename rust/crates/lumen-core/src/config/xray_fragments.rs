//! TLS-fragmentation masks (`xray_fragments.py`).

use serde_json::{json, Value};

use crate::models::str_field;

fn clean_fragment_value(value: &str, fallback: &str) -> String {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

/// Python defaults: `packets="tlshello", length="50-100", delay="10-20"`.
pub fn build_fragment_mask(packets: &str, length: &str, delay: &str) -> Value {
    json!({
        "type": "fragment",
        "settings": {
            "packets": clean_fragment_value(packets, "tlshello"),
            "length": clean_fragment_value(length, "50-100"),
            "delay": clean_fragment_value(delay, "10-20"),
        },
    })
}

pub fn build_noise_mask() -> Value {
    json!({
        "type": "noise",
        "settings": {
            "length": "10-20",
            "delay": "10-16",
        },
    })
}

/// Python defaults: `packets="tlshello", length="50-100", delay="10-20",
/// tail_fragment=False`.
pub fn build_finalmask(packets: &str, length: &str, delay: &str, tail_fragment: bool) -> Value {
    let mut tcp = vec![build_fragment_mask(packets, length, delay)];
    if tail_fragment {
        tcp.push(build_fragment_mask("1-1", "1-3", "0-0"));
    }
    json!({"tcp": tcp, "udp": [build_noise_mask()]})
}

/// Python defaults: `packets="tlshello", length="50-100", delay="10-20",
/// tail_fragment=False`.
pub fn ensure_stream_finalmask(
    stream_settings: &mut Value,
    packets: &str,
    length: &str,
    delay: &str,
    tail_fragment: bool,
) -> bool {
    let mut changed = false;
    let has_finalmask = stream_settings
        .get("finalmask")
        .map(|v| v.is_object())
        .unwrap_or(false);
    if !has_finalmask {
        stream_settings["finalmask"] = json!({});
        changed = true;
    }

    let finalmask = stream_settings.get_mut("finalmask").unwrap();
    let need_tcp = match finalmask.get("tcp") {
        Some(Value::Array(items)) => items.is_empty(),
        Some(_) => true,
        None => true,
    };
    if need_tcp {
        finalmask["tcp"] = build_finalmask(packets, length, delay, tail_fragment)["tcp"].clone();
        changed = true;
    }

    let need_udp = match finalmask.get("udp") {
        Some(Value::Array(items)) => items.is_empty(),
        Some(_) => true,
        None => true,
    };
    if need_udp {
        finalmask["udp"] = json!([build_noise_mask()]);
        changed = true;
    }

    changed
}

/// Add Xray finalmask to TLS-like proxy outbounds, like v2rayN EnableFragment.
/// Python defaults: `packets="tlshello", length="50-100", delay="10-20",
/// tail_fragment=False`.
pub fn apply_xray_outbound_fragment(
    payload: &mut Value,
    packets: &str,
    length: &str,
    delay: &str,
    tail_fragment: bool,
) -> i64 {
    let Some(outbounds) = payload.get_mut("outbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };
    let mut patched = 0i64;
    for outbound in outbounds.iter_mut() {
        let Some(stream_settings) = outbound.get_mut("streamSettings") else {
            continue;
        };
        if !stream_settings.is_object() {
            continue;
        }
        if str_field(stream_settings, "security").trim().is_empty() {
            continue;
        }
        let dialer_proxy = stream_settings
            .get("sockopt")
            .and_then(|sockopt| sockopt.get("dialerProxy"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        if !dialer_proxy.trim().is_empty() {
            continue;
        }
        if ensure_stream_finalmask(stream_settings, packets, length, delay, tail_fragment) {
            patched += 1;
        }
    }
    patched
}

/// Insert a v2rayN-style freedom finalmask wrapper before proxy outbounds.
///
/// Xray sees this as:
///   inbound -> outbound tag `proxy` (freedom + finalmask)
///           -> dialerProxy `fragment-proxy` (the original proxy outbound)
///
/// That fragments the target site's TLS handshake without hardcoding any domain.
/// Python defaults: `tag_prefix="proxy", packets="tlshello", length="50-100",
/// delay="10-20", tail_fragment=False`.
pub fn apply_xray_final_fragment(
    payload: &mut Value,
    tag_prefix: &str,
    packets: &str,
    length: &str,
    delay: &str,
    tail_fragment: bool,
) -> i64 {
    let Some(outbounds) = payload.get_mut("outbounds").and_then(|v| v.as_array_mut()) else {
        return 0;
    };

    let mut existing_tags: std::collections::HashSet<String> = outbounds
        .iter()
        .filter(|outbound| outbound.is_object())
        .map(|outbound| str_field(outbound, "tag"))
        .collect();
    let mut patched = 0i64;

    let mut index = 0usize;
    while index < outbounds.len() {
        let outbound = &outbounds[index];
        if !outbound.is_object() {
            index += 1;
            continue;
        }

        let original_tag = str_field(outbound, "tag").trim().to_string();
        if !original_tag.starts_with(tag_prefix) {
            index += 1;
            continue;
        }
        if original_tag.starts_with("fragment-") {
            index += 1;
            continue;
        }

        let dialer_proxy = outbound
            .get("streamSettings")
            .filter(|s| s.is_object())
            .and_then(|stream| stream.get("sockopt"))
            .filter(|s| s.is_object())
            .and_then(|sockopt| sockopt.get("dialerProxy"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        if dialer_proxy.trim().starts_with("fragment-") {
            index += 1;
            continue;
        }

        let after_tag = format!("fragment-{original_tag}");
        if existing_tags.contains(&after_tag) {
            index += 1;
            continue;
        }

        let wrapper = json!({
            "tag": original_tag,
            "protocol": "freedom",
            "streamSettings": {
                "finalmask": build_finalmask(packets, length, delay, tail_fragment),
                "sockopt": {
                    "dialerProxy": after_tag,
                },
            },
        });
        if let Some(obj) = outbounds[index].as_object_mut() {
            obj.insert("tag".into(), json!(after_tag.clone()));
        }
        outbounds.insert(index, wrapper);
        existing_tags.insert(after_tag);
        patched += 1;
        index += 2;
    }

    patched
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finalmask_shape_with_tail() {
        let mask = build_finalmask("tlshello", "50-100", "10-20", true);
        assert_eq!(mask["tcp"].as_array().unwrap().len(), 2);
        assert_eq!(mask["tcp"][0]["settings"]["packets"], json!("tlshello"));
        assert_eq!(mask["tcp"][1]["settings"]["packets"], json!("1-1"));
        assert_eq!(mask["udp"][0]["type"], json!("noise"));

        let plain = build_finalmask("tlshello", "50-100", "10-20", false);
        assert_eq!(plain["tcp"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn fragment_values_fall_back_when_blank() {
        let mask = build_fragment_mask("  ", "", "5-9");
        assert_eq!(mask["settings"]["packets"], json!("tlshello"));
        assert_eq!(mask["settings"]["length"], json!("50-100"));
        assert_eq!(mask["settings"]["delay"], json!("5-9"));
    }

    #[test]
    fn outbound_fragment_patches_tls_only() {
        let mut payload = json!({"outbounds": [
            {"tag": "proxy", "protocol": "vless", "streamSettings": {"security": "tls"}},
            {"tag": "direct", "protocol": "freedom", "settings": {}},
            {"tag": "fragged", "protocol": "vless", "streamSettings": {
                "security": "tls",
                "sockopt": {"dialerProxy": "chain-front"},
            }},
        ]});
        let patched = apply_xray_outbound_fragment(&mut payload, "tlshello", "50-100", "10-20", false);
        assert_eq!(patched, 1);
        assert!(payload["outbounds"][0]["streamSettings"]["finalmask"]["tcp"].is_array());
        assert!(payload["outbounds"][1].get("streamSettings").is_none());
        assert!(payload["outbounds"][2]["streamSettings"].get("finalmask").is_none());
    }

    #[test]
    fn final_fragment_wraps_proxy_outbound() {
        let mut payload = json!({"outbounds": [
            {"tag": "proxy", "protocol": "vless", "streamSettings": {"security": "tls"}},
            {"tag": "direct", "protocol": "freedom", "settings": {}},
        ]});
        let patched = apply_xray_final_fragment(&mut payload, "proxy", "tlshello", "50-100", "10-20", false);
        assert_eq!(patched, 1);
        let outbounds = payload["outbounds"].as_array().unwrap();
        assert_eq!(outbounds[0]["protocol"], json!("freedom"));
        assert_eq!(outbounds[0]["tag"], json!("proxy"));
        assert_eq!(
            outbounds[0]["streamSettings"]["sockopt"]["dialerProxy"],
            json!("fragment-proxy")
        );
        assert_eq!(outbounds[1]["tag"], json!("fragment-proxy"));

        // Second pass is a no-op: the fragment- tag is skipped.
        let patched = apply_xray_final_fragment(&mut payload, "proxy", "tlshello", "50-100", "10-20", false);
        assert_eq!(patched, 0);
    }
}
