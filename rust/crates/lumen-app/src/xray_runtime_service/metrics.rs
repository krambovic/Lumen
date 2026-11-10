use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use lumen_core::config::runtime_security::clamp_xray_local_inbounds;
use lumen_core::constants::{DEFAULT_DISCORD_SOCKS_PORT, PROXY_HOST};
use lumen_core::models::{AppSettings, str_field};
use serde_json::{Value, json};

use crate::runtime_introspection::{
    collect_xray_inbound_ports, ensure_dict, ensure_list, find_free_api_port,
    replace_or_append_tagged, runtime_port,
};

pub const APP_METRICS_API_TAG: &str = "__app_metrics_api";
pub const APP_METRICS_API_INBOUND_TAG: &str = "__app_metrics_api_in";
pub const APP_DISCORD_PROXY_INBOUND_TAG: &str = "discord-socks-in";

const PROTECTED_TAGS: &[&str] = &["block", "bypass", "direct", "dns", "dns-out", "proxy"];

/// Replace the imported control-plane API with a single loopback StatsService.
/// Inspection retains a preconfigured metrics port without probing the network.
pub fn ensure_xray_metrics_contract(
    payload: &mut Value,
    settings: &AppSettings,
    allocate_port: bool,
    mut log: impl FnMut(String),
) -> Result<(i64, Vec<String>)> {
    let config = payload.as_object_mut().context(super::INVALID_ROOT)?;
    ensure_dict(config, "stats");
    let system = ensure_dict(ensure_dict(config, "policy"), "system");
    for key in [
        "statsInboundUplink",
        "statsInboundDownlink",
        "statsOutboundUplink",
        "statsOutboundDownlink",
    ] {
        system.insert(key.into(), json!(true));
    }
    ensure_list(config, "inbounds");
    ensure_list(config, "outbounds");
    ensure_list(ensure_dict(config, "routing"), "rules");

    let imported_api_tag = str_field(&payload["api"], "tag");
    let imported_api_tag = imported_api_tag.trim();
    let legacy_tags = legacy_api_inbounds(payload, imported_api_tag);
    let mut api_tags = legacy_tags.clone();
    api_tags.insert(APP_METRICS_API_INBOUND_TAG.into());
    let inbounds = payload["inbounds"].as_array_mut().expect("array ensured");
    let preferred_api_port = inbounds
        .iter()
        .filter(|inbound| inbound["tag"] == APP_METRICS_API_INBOUND_TAG)
        .map(|inbound| runtime_port(inbound, "port"))
        .find(|port| *port > 0)
        .unwrap_or(0);
    inbounds.retain(|inbound| !api_tags.contains(str_field(inbound, "tag").trim()));

    let rules = payload["routing"]["rules"]
        .as_array_mut()
        .expect("array ensured");
    rules.retain_mut(|rule| keep_rule(rule, &api_tags));
    let legacy_outbound_unused = !legacy_tags.is_empty()
        && !rules
            .iter()
            .any(|rule| str_field(rule, "outboundTag").trim() == imported_api_tag);

    let outbounds = payload["outbounds"].as_array_mut().expect("array ensured");
    outbounds.retain(|outbound| {
        let tag = str_field(outbound, "tag");
        let is_api = tag.trim() == APP_METRICS_API_TAG
            || (legacy_outbound_unused && tag.trim() == imported_api_tag);
        !is_api
            || !matches!(
                str_field(outbound, "protocol")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "freedom" | "loopback"
            )
    });
    payload["api"] = json!({"tag": APP_METRICS_API_TAG, "services": ["StatsService"]});
    let mut existing_ports = collect_xray_inbound_ports(payload);
    let api_port = if preferred_api_port > 0 {
        preferred_api_port
    } else if allocate_port {
        i64::from(
            find_free_api_port(None, &existing_ports)
                .context("Не удалось выделить локальный порт для Xray metrics API.")?,
        )
    } else {
        0
    };
    existing_ports.insert(api_port);

    let inbounds = payload["inbounds"].as_array_mut().expect("array ensured");
    inbounds.push(json!({
        "tag": APP_METRICS_API_INBOUND_TAG, "listen": PROXY_HOST, "port": api_port,
        "protocol": "dokodemo-door", "settings": {"address": PROXY_HOST}
    }));
    if settings.discord_proxy_enabled
        && (!existing_ports.contains(&i64::from(DEFAULT_DISCORD_SOCKS_PORT))
            || inbounds
                .iter()
                .any(|inbound| inbound["tag"] == APP_DISCORD_PROXY_INBOUND_TAG))
    {
        replace_or_append_tagged(
            inbounds,
            APP_DISCORD_PROXY_INBOUND_TAG,
            json!({
                "tag": APP_DISCORD_PROXY_INBOUND_TAG, "listen": PROXY_HOST, "port": DEFAULT_DISCORD_SOCKS_PORT,
                "protocol": "socks", "settings": {"auth": "noauth", "udp": true},
                "sniffing": {"enabled": true, "destOverride": ["http", "tls"], "routeOnly": false},
            }),
        );
    }
    let inbound_tags = user_inbound_tags(inbounds);
    payload["outbounds"]
        .as_array_mut()
        .expect("array ensured")
        .push(json!({
            "tag": APP_METRICS_API_TAG, "protocol": "freedom", "settings": {}
        }));

    let rules = payload["routing"]["rules"]
        .as_array_mut()
        .expect("array ensured");
    rules.insert(0, json!({
        "type": "field", "inboundTag": [APP_METRICS_API_INBOUND_TAG], "outboundTag": APP_METRICS_API_TAG
    }));
    if settings.discord_proxy_enabled {
        let discord_rule = json!({
            "type": "field", "inboundTag": [APP_DISCORD_PROXY_INBOUND_TAG], "outboundTag": "proxy"
        });
        if let Some(existing) = rules.iter_mut().find(|rule| {
            rule["inboundTag"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|tag| tag == APP_DISCORD_PROXY_INBOUND_TAG))
        }) {
            *existing = discord_rule;
        } else {
            rules.insert(1, discord_rule);
        }
    }
    let changed = clamp_xray_local_inbounds(payload, settings.proxy_allow_lan);
    if changed != 0 {
        log(format!(
            "[xray] local inbounds locked to loopback ({changed} change(s))"
        ));
    }
    Ok((api_port, inbound_tags))
}

fn legacy_api_inbounds(payload: &Value, imported_tag: &str) -> HashSet<String> {
    if imported_tag.is_empty() || PROTECTED_TAGS.contains(&imported_tag) {
        return HashSet::new();
    }
    let inbounds: HashMap<_, _> = payload["inbounds"]
        .as_array()
        .expect("array ensured")
        .iter()
        .filter_map(|inbound| {
            inbound["tag"]
                .as_str()
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(|tag| (tag, inbound))
        })
        .collect();
    payload["routing"]["rules"]
        .as_array()
        .expect("array ensured")
        .iter()
        .filter(|rule| str_field(rule, "outboundTag").trim() == imported_tag)
        .flat_map(|rule| {
            let tags = &rule["inboundTag"];
            tags.as_array()
                .map_or_else(|| std::slice::from_ref(tags), Vec::as_slice)
        })
        .filter_map(|tag| tag.as_str().map(str::trim))
        .filter(|tag| {
            inbounds.get(tag).is_some_and(|inbound| {
                str_field(inbound, "protocol")
                    .trim()
                    .eq_ignore_ascii_case("dokodemo-door")
            })
        })
        .map(str::to_owned)
        .collect()
}

fn keep_rule(rule: &mut Value, api_tags: &HashSet<String>) -> bool {
    if str_field(rule, "outboundTag").trim() == APP_METRICS_API_TAG {
        return false;
    }
    if let Some(tags) = rule.get_mut("inboundTag").and_then(Value::as_array_mut) {
        if tags.is_empty() {
            return true;
        }
        tags.retain(|tag| !api_tags.contains(tag.as_str().unwrap_or_default().trim()));
        return !tags.is_empty();
    }
    !api_tags.contains(str_field(rule, "inboundTag").trim())
}

fn user_inbound_tags(inbounds: &mut [Value]) -> Vec<String> {
    let mut tags = Vec::new();
    for (index, inbound) in inbounds
        .iter_mut()
        .enumerate()
        .filter(|(_, inbound)| inbound.is_object())
    {
        let raw_tag = str_field(inbound, "tag");
        let tag = raw_tag.trim();
        if tag == APP_METRICS_API_INBOUND_TAG {
            continue;
        }
        let tag = if tag.is_empty() {
            let generated = format!("__app_user_inbound_{index}");
            inbound["tag"] = json!(generated);
            generated
        } else {
            tag.to_owned()
        };
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}
