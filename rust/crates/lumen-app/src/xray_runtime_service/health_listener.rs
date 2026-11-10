use std::io;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use lumen_core::constants::PROXY_HOST;
use rand::{RngCore, rngs::OsRng};
use serde_json::{Value, json};

pub(super) const PROXY_PROTOCOLS: &[&str] =
    &["vless", "vmess", "trojan", "shadowsocks", "socks", "http"];

/// Add a runtime-only authenticated HTTP inbound pinned to a selected outbound.
pub fn add_xray_health_listener(
    payload: &mut Value,
    port: u16,
    outbound_tag: &str,
) -> io::Result<String> {
    let Some(outbounds) = payload["outbounds"].as_array() else {
        return Ok(String::new());
    };
    let mut matching = outbounds
        .iter()
        .filter(|outbound| outbound["tag"] == outbound_tag);
    if port == 0
        || !matching
            .next()
            .and_then(|outbound| outbound["protocol"].as_str())
            .is_some_and(|protocol| PROXY_PROTOCOLS.contains(&protocol))
        || matching.next().is_some()
    {
        return Ok(String::new());
    }
    let Some(inbounds) = payload["inbounds"].as_array() else {
        return Ok(String::new());
    };
    let port_string = port.to_string();
    if !payload["routing"]["rules"].is_array()
        || inbounds
            .iter()
            .any(|inbound| inbound["port"] == port || inbound["port"] == port_string)
    {
        return Ok(String::new());
    }
    let mut random = [0u8; 44];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(io::Error::other)?;
    let tag: String = random[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let tag = format!("__app_health_{tag}");
    let username = "lumen-health";
    let password = URL_SAFE_NO_PAD.encode(&random[12..]);
    payload["inbounds"]
        .as_array_mut()
        .expect("checked inbounds")
        .push(json!({
            "tag": tag, "listen": PROXY_HOST, "port": port, "protocol": "http",
            "settings": {"accounts": [{"user": username, "pass": password}]},
            "sniffing": {"enabled": false},
        }));
    payload["routing"]["rules"]
        .as_array_mut()
        .expect("checked rules")
        .insert(
            0,
            json!({
                "type": "field", "inboundTag": [tag], "outboundTag": outbound_tag,
            }),
        );
    Ok(format!("http://{username}:{password}@{PROXY_HOST}:{port}"))
}
