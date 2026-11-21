//! Xray sidecar config building (`_build_xray_sidecar_config`) and the local
//! OpenVPN outbound migration (`_normalize_openvpn_outbounds`).

use std::collections::HashSet;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::{
    ensure_list, is_falsy_json, json_str_field, json_str_or, lower_trim, py_int_or,
    SS_PROTECT_METHOD,
};
use lumen_core::config::multiplex::apply_xray_multiplex;
use lumen_core::config::proxy_chain::{apply_xray_proxy_chain, ProxyChain};
use lumen_core::config::xray_fragments::apply_xray_final_fragment;
use lumen_core::constants::PROXY_HOST;
use lumen_core::models::Node;
use lumen_core::openvpn_normalization::normalize_openvpn_outbound;

pub const APP_XRAY_SIDECAR_RELAY_INBOUND_TAG: &str = "__app_hybrid_relay_in";
pub const APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG: &str = "__app_hybrid_protect_out";

/// Python `_build_xray_sidecar_config`.
#[allow(clippy::too_many_arguments)]
pub fn build_xray_sidecar_config(
    node: &Node,
    relay_port: i64,
    relay_username: &str,
    relay_password: &str,
    protect_port: i64,
    protect_password: &str,
    enable_final_fragment: bool,
    fragment_packets: &str,
    fragment_length: &str,
    fragment_delay: &str,
    tail_fragment_enabled: bool,
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
    proxy_chain: Option<&mut ProxyChain>,
) -> Result<Value> {
    let outbound = &node.outbound;
    if !outbound.is_object() || outbound.as_object().is_some_and(|map| map.is_empty()) {
        bail!("Выбранный сервер не содержит outbound JSON для xray sidecar.");
    }
    if json_str_field(outbound, "protocol").trim().is_empty() {
        bail!("Выбранный сервер не содержит protocol для xray sidecar.");
    }
    if lower_trim(&json_str_field(outbound, "protocol")) == "xray_config" {
        let full_config = outbound
            .get("xray_config")
            .filter(|value| value.is_object())
            .ok_or_else(|| anyhow::anyhow!("AUTO-профиль Xray повреждён: отсутствует полный JSON-конфиг."))?;
        let mut config = full_config.clone();
        let outbounds_ok = config
            .get("outbounds")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty());
        if !outbounds_ok {
            bail!("AUTO-профиль Xray не содержит список outbounds.");
        }

        config["inbounds"] = json!([
            {
                "tag": APP_XRAY_SIDECAR_RELAY_INBOUND_TAG,
                "protocol": "socks",
                "listen": PROXY_HOST,
                "port": relay_port,
                "settings": {
                    "auth": "password",
                    "accounts": [{"user": relay_username, "pass": relay_password}],
                    "udp": true,
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls"],
                    "routeOnly": false,
                },
            }
        ]);
        let ignored_protocols = ["freedom", "blackhole", "dns"];
        {
            let outbounds = config.get_mut("outbounds").unwrap().as_array_mut().unwrap();
            for outbound in outbounds.iter_mut() {
                if !outbound.is_object() {
                    continue;
                }
                let protocol = lower_trim(&json_str_field(outbound, "protocol"));
                if ignored_protocols.contains(&protocol.as_str()) {
                    continue;
                }
                if !outbound.get("streamSettings").is_some_and(Value::is_object) {
                    outbound["streamSettings"] = json!({});
                }
                if !outbound["streamSettings"].get("sockopt").is_some_and(Value::is_object) {
                    outbound["streamSettings"]["sockopt"] = json!({});
                }
                outbound["streamSettings"]["sockopt"]["dialerProxy"] =
                    json!(APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG);
            }
        }
        let outbounds = config.get_mut("outbounds").unwrap().as_array_mut().unwrap();
        outbounds.push(json!({
            "tag": APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG,
            "protocol": "shadowsocks",
            "settings": {
                "servers": [
                    {
                        "address": PROXY_HOST,
                        "port": protect_port,
                        "method": SS_PROTECT_METHOD,
                        "password": protect_password,
                    }
                ]
            },
        }));
        return Ok(config);
    }

    let mut proxy_outbound = outbound.clone();
    proxy_outbound["tag"] = json!("proxy");
    apply_xray_multiplex(&mut proxy_outbound, multiplex_enabled, multiplex_concurrency);
    if !proxy_outbound.get("streamSettings").is_some_and(Value::is_object) {
        proxy_outbound["streamSettings"] = json!({});
    }
    if !proxy_outbound["streamSettings"].get("sockopt").is_some_and(Value::is_object) {
        proxy_outbound["streamSettings"]["sockopt"] = json!({});
    }
    proxy_outbound["streamSettings"]["sockopt"]["dialerProxy"] = json!(APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG);

    let mut config = json!({
        "log": {"loglevel": "warning"},
        "inbounds": [
            {
                "tag": APP_XRAY_SIDECAR_RELAY_INBOUND_TAG,
                "protocol": "socks",
                "listen": PROXY_HOST,
                "port": relay_port,
                "settings": {
                    "auth": "password",
                    "accounts": [{"user": relay_username, "pass": relay_password}],
                    "udp": true,
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls"],
                    "routeOnly": false,
                },
            }
        ],
        "outbounds": [
            proxy_outbound,
            {
                "tag": APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG,
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [
                        {
                            "address": PROXY_HOST,
                            "port": protect_port,
                            "method": SS_PROTECT_METHOD,
                            "password": protect_password,
                        }
                    ]
                },
            },
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": [
                {
                    "type": "field",
                    "inboundTag": [APP_XRAY_SIDECAR_RELAY_INBOUND_TAG],
                    "outboundTag": "proxy",
                }
            ],
        },
    });
    // Proxy chain lives in the sidecar: the front hop inherits the protect
    // dialer so it still leaves through sing-box `direct`.
    {
        let outbounds = config.get_mut("outbounds").unwrap().as_array_mut().unwrap();
        apply_xray_proxy_chain(outbounds, proxy_chain, "proxy");
    }
    if enable_final_fragment {
        apply_xray_final_fragment(
            &mut config,
            "proxy",
            fragment_packets,
            fragment_length,
            fragment_delay,
            tail_fragment_enabled,
        );
    }
    Ok(config)
}

/// Python `_normalize_openvpn_outbounds`: move OpenVPN outbounds/endpoints into
/// `endpoints` with unique names, extracting `lumen_proxy` into detour
/// outbounds.
pub fn normalize_openvpn_outbounds(payload: &mut Value) -> Result<()> {
    if !payload.get("outbounds").is_some_and(Value::is_array) {
        payload["outbounds"] = json!([]);
    }
    let is_openvpn = |value: &Value| -> bool {
        value.is_object()
            && matches!(
                lower_trim(&json_str_field(value, "type")).as_str(),
                "openvpn" | "openvpn-client"
            )
    };
    let mut openvpn_candidates: Vec<Value> = Vec::new();
    let mut retained_outbounds: Vec<Value> = Vec::new();
    {
        let outbounds = payload.get_mut("outbounds").unwrap().as_array_mut().unwrap();
        for outbound in outbounds.drain(..) {
            if is_openvpn(&outbound) {
                openvpn_candidates.push(outbound);
            } else {
                retained_outbounds.push(outbound);
            }
        }
    }
    let endpoints = ensure_list(payload, "endpoints");
    let mut normalized_endpoints: Vec<Value> = Vec::new();
    for endpoint in endpoints.drain(..) {
        if is_openvpn(&endpoint) {
            openvpn_candidates.push(endpoint);
        } else {
            normalized_endpoints.push(endpoint);
        }
    }

    let mut used_names: HashSet<String> = HashSet::new();
    let mut detours: Vec<Value> = Vec::new();
    let mut openvpn_index: i64 = 0;
    for mut outbound in openvpn_candidates {
        normalize_openvpn_outbound(&mut outbound)?;
        outbound["system"] = json!(false);
        let requested_name = json_str_or(
            outbound.get("name"),
            &format!("openvpn{openvpn_index}"),
        )
        .trim()
        .to_string();
        let mut name_candidate = requested_name.clone();
        let mut suffix = 1;
        while used_names.contains(&name_candidate) {
            name_candidate = format!("{requested_name}-{suffix}");
            suffix += 1;
        }
        outbound["name"] = json!(name_candidate.clone());
        used_names.insert(name_candidate);
        let proxy = outbound
            .as_object_mut()
            .and_then(|map| map.shift_remove("lumen_proxy"));
        if let Some(proxy) = proxy.filter(|value| value.is_object()) {
            let proxy_type = lower_trim(&json_str_field(&proxy, "type"));
            if !matches!(proxy_type.as_str(), "http" | "socks") {
                let shown = if proxy_type.is_empty() { "unknown".to_string() } else { proxy_type.clone() };
                bail!("Unsupported OpenVPN proxy type `{shown}`");
            }
            let server = json_str_field(&proxy, "server").trim().to_string();
            let port = py_int_or(proxy.get("server_port").filter(|value| !is_falsy_json(value)), 0);
            if server.is_empty() || !(1..=65535).contains(&port) {
                bail!("OpenVPN proxy must contain a valid server and port");
            }
            let detour_tag = format!("ovpn-proxy-{openvpn_index}");
            outbound["detour"] = json!(detour_tag.clone());
            let mut detour = json!({
                "type": proxy_type,
                "tag": detour_tag,
                "server": server,
                "server_port": port,
            });
            for key in ["username", "password"] {
                let value = json_str_field(&proxy, key);
                if !value.is_empty() {
                    detour[key] = json!(value);
                }
            }
            detours.push(detour);
        }
        normalized_endpoints.push(outbound);
        openvpn_index += 1;
    }
    retained_outbounds.extend(detours);
    *payload.get_mut("outbounds").unwrap().as_array_mut().unwrap() = retained_outbounds;
    *ensure_list(payload, "endpoints") = normalized_endpoints;
    Ok(())
}
