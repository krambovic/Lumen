//! WireGuard endpoint normalization (`wireguard_normalization.py`).
//!
//! sing-box 1.13 removed the legacy WireGuard outbound.  The extended fork
//! follows the new endpoint schema, but still receives old outbound-shaped
//! profiles from providers.  Keep this conversion in one place so links,
//! native JSON documents and saved nodes all produce the same runtime shape.

use anyhow::{bail, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Map, Value};

// ── Python semantics helpers ──

fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn py_str_or_empty(value: &Value) -> String {
    if truthy(value) {
        py_str(value)
    } else {
        String::new()
    }
}

/// Python `int(value or 0)`: falsy values become 0; parse failures are None.
fn int_value_or(value: &Value) -> Option<i64> {
    if !truthy(value) {
        return Some(0);
    }
    match value {
        Value::Number(n) => Some(
            n.as_i64()
                .unwrap_or_else(|| n.as_f64().map(|f| f as i64).unwrap_or(0)),
        ),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        _ => None,
    }
}

fn int_value(value: &Value) -> Result<i64> {
    int_value_or(value).ok_or_else(|| anyhow::anyhow!("invalid integer value: {}", py_str(value)))
}

fn positive_int(value: &Value, fallback: i64) -> i64 {
    match int_value_or(value) {
        Some(parsed) if parsed > 0 => parsed,
        _ => fallback,
    }
}

fn take_key(map: &mut Map<String, Value>, key: &str) -> Value {
    map.remove(key).unwrap_or(Value::Null)
}

// ── IP prefix normalization ──

/// Return a sing-box compatible IP prefix, adding a host mask when absent.
pub fn normalize_ip_prefix(value: &Value) -> String {
    let text = py_str_or_empty(value)
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    if text.is_empty() {
        return String::new();
    }
    if let Some((addr_part, prefix_part)) = text.split_once('/') {
        if let Ok(address) = addr_part.trim().parse::<std::net::IpAddr>() {
            if let Ok(prefix) = prefix_part.trim().parse::<u32>() {
                let max = if address.is_ipv4() { 32 } else { 128 };
                if prefix <= max {
                    return format!("{address}/{prefix}");
                }
            }
        }
        return text;
    }
    match text.parse::<std::net::IpAddr>() {
        Ok(address) => {
            let bits = if address.is_ipv4() { 32 } else { 128 };
            format!("{address}/{bits}")
        }
        Err(_) => text,
    }
}

pub fn normalize_ip_prefixes(value: &Value) -> Vec<String> {
    let raw_items: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        other => py_str_or_empty(other)
            .split(',')
            .map(|part| Value::String(part.to_string()))
            .collect(),
    };
    let mut result: Vec<String> = Vec::new();
    for item in &raw_items {
        let normalized = normalize_ip_prefix(item);
        if !normalized.is_empty() && !result.contains(&normalized) {
            result.push(normalized);
        }
    }
    result
}

// ── Endpoint normalization ──

/// Return the current sing-box-extended WireGuard endpoint representation.
pub fn normalize_wireguard_endpoint(endpoint: &Value) -> Result<Value> {
    if !endpoint.is_object() {
        return Ok(endpoint.clone());
    }
    let mut result = endpoint.clone();
    let endpoint_type = py_str_or_empty(result.get("type").unwrap_or(&Value::Null))
        .trim()
        .to_lowercase();
    if endpoint_type == "warp" {
        result["system"] = json!(false);
        // sing-box-extended 2.5.1+ rejects an explicit "reserved" field on WARP
        // endpoints (`endpoints[0].reserved: json: unknown field "reserved"`);
        // the WARP profile derives the reserved bytes itself.
        if let Some(obj) = result.as_object_mut() {
            obj.remove("reserved");
        }
        return Ok(result);
    }
    if endpoint_type != "wireguard" && endpoint_type != "awg" {
        return Ok(result);
    }
    if endpoint_type == "awg" {
        result["type"] = json!("wireguard");
    }
    let obj = result.as_object_mut().unwrap();

    let legacy_server = py_str_or_empty(&take_key(obj, "server")).trim().to_string();
    let legacy_port = positive_int(&take_key(obj, "server_port"), 51820);
    let legacy_public_key = py_str_or_empty(&take_key(obj, "peer_public_key"))
        .trim()
        .to_string();
    let legacy_pre_shared_key = py_str_or_empty(&take_key(obj, "pre_shared_key"))
        .trim()
        .to_string();
    let legacy_allowed_ips = take_key(obj, "allowed_ips");
    let mut legacy_reserved = reserved_bytes(&take_key(obj, "reserved"));

    let local_address = take_key(obj, "local_address");
    let address_value = match obj.get("address") {
        Some(Value::String(s)) if s.is_empty() => local_address,
        Some(Value::Array(a)) if a.is_empty() => local_address,
        Some(v) => v.clone(),
        None => local_address,
    };
    obj.insert(
        "address".into(),
        Value::Array(
            normalize_ip_prefixes(&address_value)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    // Lumen is a Windows TUN client.  A system WireGuard interface routes its
    // own handshake back into the catch-all TUN route; userspace mode is the
    // supported sing-box endpoint mode for this topology.
    obj.insert("system".into(), json!(false));

    let peers: Vec<Value> = match obj.get("peers") {
        Some(Value::Array(items)) if !items.is_empty() => items.clone(),
        _ => {
            if !legacy_server.is_empty() || !legacy_public_key.is_empty() {
                vec![json!({
                    "address": legacy_server,
                    "port": legacy_port,
                    "public_key": legacy_public_key,
                    "pre_shared_key": legacy_pre_shared_key,
                    "allowed_ips": if truthy(&legacy_allowed_ips) {
                        legacy_allowed_ips.clone()
                    } else {
                        json!(["0.0.0.0/0", "::/0"])
                    },
                })]
            } else {
                Vec::new()
            }
        }
    };

    let peer_count = peers.len();
    let mut normalized_peers: Vec<Value> = Vec::new();
    for peer in &peers {
        if !peer.is_object() {
            normalized_peers.push(peer.clone());
            continue;
        }
        let mut normalized_peer = peer.clone();
        let pobj = normalized_peer.as_object_mut().unwrap();
        let peer_endpoint = py_str_or_empty(&take_key(pobj, "endpoint"))
            .trim()
            .to_string();
        let popped_server = take_key(pobj, "server");
        let mut peer_server = if truthy(&popped_server) {
            py_str(&popped_server).trim().to_string()
        } else {
            py_str_or_empty(pobj.get("address").unwrap_or(&Value::Null))
                .trim()
                .to_string()
        };
        let popped_port = take_key(pobj, "server_port");
        let port_source = if truthy(&popped_port) {
            popped_port
        } else {
            pobj.get("port").cloned().unwrap_or(Value::Null)
        };
        let mut peer_port = positive_int(&port_source, 51820);
        if !peer_endpoint.is_empty() {
            let (endpoint_host, endpoint_port) = split_endpoint(&peer_endpoint);
            if !endpoint_host.is_empty() {
                peer_server = endpoint_host;
            }
            if endpoint_port > 0 {
                peer_port = endpoint_port;
            }
        }
        pobj.insert("address".into(), json!(peer_server.clone()));
        pobj.insert("port".into(), json!(peer_port));
        if !truthy(pobj.get("public_key").unwrap_or(&Value::Null)) {
            let popped = py_str_or_empty(&take_key(pobj, "peer_public_key"))
                .trim()
                .to_string();
            pobj.insert("public_key".into(), json!(popped));
        }
        let allowed_existing = pobj.get("allowed_ips").cloned().unwrap_or(Value::Null);
        let allowed_source = if truthy(&allowed_existing) {
            allowed_existing
        } else {
            json!(["0.0.0.0/0", "::/0"])
        };
        pobj.insert(
            "allowed_ips".into(),
            Value::Array(
                normalize_ip_prefixes(&allowed_source)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        let peer_reserved = reserved_bytes(&take_key(pobj, "reserved"));
        if !peer_reserved.is_empty() {
            if peer_count == 1 && is_cloudflare_warp_peer(&peer_server) {
                legacy_reserved = peer_reserved;
            } else {
                bail!("sing-box extended 2.5.x supports reserved bytes only through a WARP endpoint");
            }
        }
        normalized_peers.push(normalized_peer);
    }
    obj.insert("peers".into(), Value::Array(normalized_peers));

    if !legacy_reserved.is_empty() {
        let first_peer = obj
            .get("peers")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let peer_server = {
            let address = py_str_or_empty(first_peer.get("address").unwrap_or(&Value::Null))
                .trim()
                .to_string();
            if address.is_empty() {
                legacy_server.clone()
            } else {
                address
            }
        };
        if !is_cloudflare_warp_peer(&peer_server) {
            bail!("sing-box extended 2.5.x supports reserved bytes only for Cloudflare WARP profiles");
        }
        let mut warp_endpoint = Map::new();
        warp_endpoint.insert("type".into(), json!("warp"));
        let tag = py_str_or_empty(obj.get("tag").unwrap_or(&Value::Null))
            .trim()
            .to_string();
        let tag_value = if tag.is_empty() { "proxy".to_string() } else { tag };
        warp_endpoint.insert("tag".into(), json!(tag_value));
        warp_endpoint.insert("system".into(), json!(false));
        let udp_timeout = py_str_or_empty(obj.get("udp_timeout").unwrap_or(&Value::Null));
        let udp_value = if udp_timeout.is_empty() { "5m0s".to_string() } else { udp_timeout };
        warp_endpoint.insert("udp_timeout".into(), json!(udp_value));
        warp_endpoint.insert(
            "profile".into(),
            json!({
                "detour": "direct",
                "private_key": py_str_or_empty(obj.get("private_key").unwrap_or(&Value::Null)),
            }),
        );
        let keepalive = first_peer
            .get("persistent_keepalive_interval")
            .cloned()
            .unwrap_or(Value::Null);
        let keepalive_set = match &keepalive {
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            _ => true,
        };
        if keepalive_set {
            let interval = int_value(&keepalive)?;
            warp_endpoint.insert("persistent_keepalive_interval".into(), json!(interval));
        }
        for key in [
            "listen_port",
            "workers",
            "preallocated_buffers_per_pool",
            "disable_pauses",
            "amnezia",
            "domain_resolver",
            "detour",
        ] {
            let value = obj.get(key).cloned().unwrap_or(Value::Null);
            let skip = match &value {
                Value::Null => true,
                Value::String(s) => s.is_empty(),
                Value::Array(a) => a.is_empty(),
                Value::Object(o) => o.is_empty(),
                _ => false,
            };
            if !skip {
                warp_endpoint.insert(key.into(), value);
            }
        }
        return Ok(Value::Object(warp_endpoint));
    }
    Ok(result)
}

/// Normalize endpoints and migrate removed WireGuard outbounds in-place.
pub fn normalize_singbox_wireguard_endpoints(payload: &mut Value) -> Result<()> {
    if !payload.is_object() {
        return Ok(());
    }
    let had_endpoint_list = payload.get("endpoints").map(Value::is_array).unwrap_or(false);
    let endpoints: Vec<Value> = if had_endpoint_list {
        payload
            .get("endpoints")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut normalized_endpoints: Vec<Value> = Vec::new();
    for endpoint in &endpoints {
        if endpoint.is_object() {
            normalized_endpoints.push(normalize_wireguard_endpoint(endpoint)?);
        } else {
            normalized_endpoints.push(endpoint.clone());
        }
    }

    let mut retained_outbounds: Vec<Value> = Vec::new();
    if let Some(outbounds) = payload.get("outbounds").cloned() {
        if outbounds.is_array() {
            for outbound in outbounds.as_array().unwrap() {
                let outbound_type = if outbound.is_object() {
                    py_str_or_empty(outbound.get("type").unwrap_or(&Value::Null))
                        .trim()
                        .to_lowercase()
                } else {
                    String::new()
                };
                if ["wireguard", "awg", "warp"].contains(&outbound_type.as_str()) {
                    let migrated = normalize_wireguard_endpoint(outbound)?;
                    let tag = py_str_or_empty(migrated.get("tag").unwrap_or(&Value::Null))
                        .trim()
                        .to_string();
                    if !tag.is_empty() {
                        normalized_endpoints.retain(|item| {
                            !(item.is_object()
                                && py_str_or_empty(item.get("tag").unwrap_or(&Value::Null))
                                    .trim()
                                    == tag)
                        });
                    }
                    normalized_endpoints.push(migrated);
                } else {
                    retained_outbounds.push(outbound.clone());
                }
            }
            payload["outbounds"] = Value::Array(retained_outbounds);
        }
    }

    if had_endpoint_list || !normalized_endpoints.is_empty() {
        payload["endpoints"] = Value::Array(normalized_endpoints);
    }
    Ok(())
}

// ── Private helpers ──

fn split_endpoint(value: &str) -> (String, i64) {
    let text = value.trim();
    if text.is_empty() {
        return (String::new(), 0);
    }
    if text.starts_with('[') {
        if let Some(closing) = text.find(']') {
            if closing > 0 {
                let host = text[1..closing].to_string();
                let remainder = &text[closing + 1..];
                let port = match remainder.strip_prefix(':') {
                    Some(rest) => positive_int(&Value::String(rest.to_string()), 0),
                    None => 0,
                };
                return (host, port);
            }
        }
    }
    if let Some((host, port_text)) = text.rsplit_once(':') {
        if !host.is_empty()
            && !port_text.is_empty()
            && port_text.chars().all(|c| c.is_ascii_digit())
        {
            let port = port_text.parse::<i64>().unwrap_or(0);
            return (host.to_string(), port);
        }
    }
    (text.to_string(), 0)
}

fn reserved_bytes(value: &Value) -> Vec<i64> {
    if !truthy(value) {
        return Vec::new();
    }
    let parts: Vec<String> = match value {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Number(n) => n
                    .as_i64()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| n.to_string()),
                other => py_str(other),
            })
            .collect(),
        other => {
            let text = py_str(other)
                .trim()
                .trim_matches(|c| c == '[' || c == ']')
                .to_string();
            split_reserved_text(&text)
        }
    };
    if parts.len() == 1 && !matches!(value, Value::Array(_)) {
        let encoded = parts[0].replace('-', "+").replace('_', "/");
        let padding = "=".repeat((4 - encoded.len() % 4) % 4);
        if let Ok(decoded) = BASE64.decode(format!("{encoded}{padding}").as_bytes()) {
            if decoded.len() == 3 {
                return decoded.iter().map(|b| *b as i64).collect();
            }
        }
    }
    let mut parsed: Vec<i64> = Vec::with_capacity(parts.len());
    for part in &parts {
        match part.trim().parse::<i64>() {
            Ok(number) => parsed.push(number),
            Err(_) => return Vec::new(),
        }
    }
    if parsed.len() == 3 && parsed.iter().all(|item| (0..=255).contains(item)) {
        parsed
    } else {
        Vec::new()
    }
}

fn split_reserved_text(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_whitespace() || c == ',' || c == ';' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn is_cloudflare_warp_peer(value: &str) -> bool {
    let host = value.trim().to_lowercase();
    let host = host.trim_matches('.');
    if host == "engage.cloudflareclient.com" || host.ends_with(".cloudflareclient.com") {
        return true;
    }
    let Ok(address) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    match address {
        std::net::IpAddr::V4(ip) => {
            let v = u32::from_be_bytes(ip.octets());
            const MASK24: u32 = 0xFFFF_FF00;
            const NET_192: u32 = (162 << 24) | (159 << 16) | (192 << 8);
            const NET_193: u32 = (162 << 24) | (159 << 16) | (193 << 8);
            const NET_195: u32 = (162 << 24) | (159 << 16) | (195 << 8);
            const NET_188_114_96_20: u32 =
                ((188 << 24) | (114 << 16) | (96 << 8)) & 0xFFFF_F000;
            const MASK20: u32 = 0xFFFF_F000;
            (v & MASK24) == NET_192
                || (v & MASK24) == NET_193
                || (v & MASK24) == NET_195
                || (v & MASK20) == NET_188_114_96_20
        }
        std::net::IpAddr::V6(ip) => {
            let prefix = u128::from(ip) >> 80; // top 48 bits
            prefix == 0x2606_4700_00d0 || prefix == 0x2606_4700_00d1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_ip_prefixes() {
        assert_eq!(
            normalize_ip_prefixes(&json!("10.0.0.2, fd00::1 , 10.0.0.2")),
            vec!["10.0.0.2/32", "fd00::1/128"]
        );
        assert_eq!(
            normalize_ip_prefixes(&json!(["10.0.0.2/8", "fd00::1/64"])),
            vec!["10.0.0.2/8", "fd00::1/64"]
        );
        // Invalid entries are preserved verbatim.
        assert_eq!(normalize_ip_prefixes(&json!("not-an-ip")), vec!["not-an-ip"]);
        assert_eq!(normalize_ip_prefixes(&json!("10.0.0.2/33")), vec!["10.0.0.2/33"]);
        // Quoted values are stripped.
        assert_eq!(normalize_ip_prefixes(&json!("\"10.0.0.2\"")), vec!["10.0.0.2/32"]);
        assert!(normalize_ip_prefixes(&Value::Null).is_empty());
    }

    #[test]
    fn test_flat_fields_folded_into_peers() {
        let endpoint = json!({
            "type": "wireguard",
            "server": "peer.example.com",
            "server_port": 51821,
            "peer_public_key": "PUBKEY=",
            "pre_shared_key": "PSK=",
            "address": "10.0.0.2/32",
            "private_key": "PRIV=",
        });
        let normalized = normalize_wireguard_endpoint(&endpoint).unwrap();
        assert_eq!(normalized["type"], "wireguard");
        assert_eq!(normalized["system"], false);
        assert!(normalized.get("server").is_none());
        let peers = normalized["peers"].as_array().unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0]["address"], "peer.example.com");
        assert_eq!(peers[0]["port"], 51821);
        assert_eq!(peers[0]["public_key"], "PUBKEY=");
        assert_eq!(peers[0]["pre_shared_key"], "PSK=");
        assert_eq!(
            peers[0]["allowed_ips"],
            json!(["0.0.0.0/0", "::/0"])
        );
        // local_address fallback when address is missing.
        let normalized = normalize_wireguard_endpoint(&json!({
            "type": "wireguard",
            "server": "s",
            "peer_public_key": "K",
            "local_address": ["10.0.0.3/32"],
            "private_key": "P",
        }))
        .unwrap();
        assert_eq!(normalized["address"], json!(["10.0.0.3/32"]));
    }

    #[test]
    fn test_reserved_bytes_parsing() {
        assert_eq!(reserved_bytes(&json!([1, 2, 3])), vec![1, 2, 3]);
        assert_eq!(reserved_bytes(&json!("1,2,3")), vec![1, 2, 3]);
        assert_eq!(reserved_bytes(&json!("[1; 2 3]")), vec![1, 2, 3]);
        // base64 of the three bytes 0x01 0x02 0x03.
        assert_eq!(reserved_bytes(&json!("AQID")), vec![1, 2, 3]);
        // "+vv8" encodes 0xFA 0xFB 0xFC; its url-safe form uses "-" for "+".
        assert_eq!(reserved_bytes(&json!("+vv8")), vec![250, 251, 252]);
        assert_eq!(reserved_bytes(&json!("----")), vec![251, 239, 190]);
        assert!(reserved_bytes(&json!("bad-value")).is_empty());
        assert!(reserved_bytes(&json!([1, 2])).is_empty());
        assert!(reserved_bytes(&json!([1, 2, 300])).is_empty());
        assert!(reserved_bytes(&Value::Null).is_empty());
        // A peer-level reserved value on a single WARP peer promotes the
        // endpoint to a WARP profile.
        let normalized = normalize_wireguard_endpoint(&json!({
            "type": "wireguard",
            "private_key": "PRIV=",
            "peers": [{
                "address": "engage.cloudflareclient.com",
                "port": 2408,
                "public_key": "PUB=",
                "reserved": "AQID",
            }],
        }))
        .unwrap();
        assert_eq!(normalized["type"], "warp");
        assert_eq!(normalized["tag"], "proxy");
        assert_eq!(normalized["system"], false);
        assert_eq!(normalized["udp_timeout"], "5m0s");
        assert_eq!(normalized["profile"]["detour"], "direct");
        assert_eq!(normalized["profile"]["private_key"], "PRIV=");
    }

    #[test]
    fn test_warp_peer_detection() {
        for host in [
            "engage.cloudflareclient.com",
            "foo.cloudflareclient.com",
            "162.159.192.1",
            "162.159.193.9",
            "162.159.195.254",
            "188.114.96.1",
            "188.114.111.254",
            "2606:4700:d0::1",
            "2606:4700:d1:abcd::",
        ] {
            assert!(is_cloudflare_warp_peer(host), "{host} should match");
        }
        for host in [
            "example.com",
            "1.2.3.4",
            "162.159.194.1",
            "188.114.112.0",
            "188.114.95.255",
            "2606:4700:d2::1",
            "engage.cloudflare.com",
        ] {
            assert!(!is_cloudflare_warp_peer(host), "{host} should not match");
        }
    }

    #[test]
    fn test_split_endpoint_ipv6_aware() {
        assert_eq!(
            split_endpoint("[2606:4700::1]:51820"),
            ("2606:4700::1".to_string(), 51820)
        );
        assert_eq!(split_endpoint("[2606:4700::1]"), ("2606:4700::1".to_string(), 0));
        assert_eq!(
            split_endpoint("peer.example.com:10000"),
            ("peer.example.com".to_string(), 10000)
        );
        assert_eq!(split_endpoint("peer.example.com"), ("peer.example.com".to_string(), 0));
        assert_eq!(split_endpoint(""), (String::new(), 0));
    }

    #[test]
    fn test_reserved_outside_warp_rejected() {
        let err = normalize_wireguard_endpoint(&json!({
            "type": "wireguard",
            "private_key": "PRIV=",
            "peers": [{"address": "vpn.example.com", "port": 51820, "public_key": "K", "reserved": [1, 2, 3]}],
        }))
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            "sing-box extended 2.5.x supports reserved bytes only through a WARP endpoint"
        );
    }

    #[test]
    fn test_warp_endpoint_short_circuit_and_singbox_migration() {
        // WARP endpoints keep their shape, drop reserved and force system=false.
        let normalized = normalize_wireguard_endpoint(&json!({
            "type": "warp",
            "tag": "my-warp",
            "system": true,
            "reserved": [9, 9, 9],
            "profile": {"detour": "direct", "private_key": "P"},
        }))
        .unwrap();
        assert_eq!(normalized["system"], false);
        assert!(normalized.get("reserved").is_none());
        assert_eq!(normalized["tag"], "my-warp");

        // Legacy `awg` outbounds migrate into endpoints as wireguard entries.
        let mut payload = json!({
            "outbounds": [
                {"type": "awg", "tag": "awg0", "server": "s1", "server_port": 443, "peer_public_key": "K", "private_key": "P", "local_address": "10.0.0.2/32"},
                {"type": "direct", "tag": "direct"}
            ]
        });
        normalize_singbox_wireguard_endpoints(&mut payload).unwrap();
        let outbounds = payload["outbounds"].as_array().unwrap();
        assert_eq!(outbounds.len(), 1);
        assert_eq!(outbounds[0]["type"], "direct");
        let endpoints = payload["endpoints"].as_array().unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], "wireguard");
        assert_eq!(endpoints[0]["tag"], "awg0");
        assert_eq!(endpoints[0]["peers"][0]["address"], "s1");
        assert_eq!(endpoints[0]["peers"][0]["port"], 443);

        // Removed `warp` outbounds stay WARP endpoints without reserved.
        let mut payload = json!({
            "endpoints": [],
            "outbounds": [
                {"type": "warp", "tag": "w0", "private_key": "P", "reserved": [1, 2, 3]}
            ]
        });
        normalize_singbox_wireguard_endpoints(&mut payload).unwrap();
        let endpoints = payload["endpoints"].as_array().unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0]["type"], "warp");
        assert!(endpoints[0].get("reserved").is_none());
    }
}
