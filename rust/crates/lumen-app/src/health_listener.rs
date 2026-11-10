use std::io;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use lumen_core::constants::PROXY_HOST;
use lumen_core::traffic_route_classifier::{RouteClass, RouteClassifier};
use rand::{RngCore, rngs::OsRng};
use serde_json::{Value, json};

use crate::runtime_introspection::{ensure_dict, ensure_list};

const USERNAME: &str = "lumen-health";

pub(crate) const PROXY_PROTOCOLS: &[&str] =
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
    let (tag, password) = probe_credentials()?;
    payload["inbounds"]
        .as_array_mut()
        .expect("checked inbounds")
        .push(json!({
            "tag": tag, "listen": PROXY_HOST, "port": port, "protocol": "http",
            "settings": {"accounts": [{"user": USERNAME, "pass": password}]},
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
    Ok(format!("http://{USERNAME}:{password}@{PROXY_HOST}:{port}"))
}

/// Pin a new sing-box health probe to a proven proxy outbound.
pub fn add_singbox_health_listener(
    payload: &mut Value,
    port: u16,
    outbound_tag: &str,
) -> io::Result<String> {
    if port == 0
        || RouteClassifier::for_health_probe(payload, outbound_tag).classify(&[outbound_tag])
            != RouteClass::Proxy
    {
        return Ok(String::new());
    }
    if payload.get("inbounds").is_some_and(|inbounds| {
        inbounds.as_array().is_none_or(|inbounds| {
            inbounds
                .iter()
                .any(|inbound| inbound["listen_port"] == port)
        })
    }) || payload.get("route").is_some_and(|route| {
        !route.is_object() || route.get("rules").is_some_and(|rules| !rules.is_array())
    }) {
        return Ok(String::new());
    }
    let (tag, password) = probe_credentials()?;
    let config = payload.as_object_mut().expect("verified runtime config");
    ensure_list(config, "inbounds").push(json!({
        "type": "http", "tag": tag, "listen": PROXY_HOST, "listen_port": port,
        "users": [{"username": USERNAME, "password": password}],
    }));
    ensure_list(ensure_dict(config, "route"), "rules").insert(
        0,
        json!({
            "inbound": [tag], "action": "route", "outbound": outbound_tag,
        }),
    );
    Ok(format!("http://{USERNAME}:{password}@{PROXY_HOST}:{port}"))
}

fn probe_credentials() -> io::Result<(String, String)> {
    let mut random = [0u8; 44];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(io::Error::other)?;
    let suffix: String = random[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((
        format!("__app_health_{suffix}"),
        URL_SAFE_NO_PAD.encode(&random[12..]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singbox_probe_uses_selector_default_and_precedes_existing_rules() -> io::Result<()> {
        let mut config = json!({
            "outbounds":[
                {"type":"selector","tag":"proxy","outbounds":["vpn","direct"],"default":"vpn"},
                {"type":"vless","tag":"vpn"}, {"type":"direct","tag":"direct"}
            ],
            "route":{"rules":[{"domain_suffix":["com"],"outbound":"direct"}]}
        });
        let url = add_singbox_health_listener(&mut config, 18080, "proxy")?;
        let inbound = &config["inbounds"][0];
        let password = inbound["users"][0]["password"].as_str().unwrap();
        assert_eq!(
            url,
            format!("http://lumen-health:{password}@127.0.0.1:18080")
        );
        assert_eq!(inbound["listen"], "127.0.0.1");
        assert_eq!(
            config["route"]["rules"][0],
            json!({
                "inbound":[inbound["tag"]], "action":"route","outbound":"proxy"
            })
        );
        assert!(add_singbox_health_listener(&mut config, 18080, "proxy")?.is_empty());
        config["outbounds"][0]["default"] = json!("direct");
        let before = config.clone();
        assert!(add_singbox_health_listener(&mut config, 18081, "proxy")?.is_empty());
        assert_eq!(config, before);
        Ok(())
    }
}
