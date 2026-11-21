//! TUN and runtime inbound contracts (`_ensure_singbox_tun_runtime_contract`,
//! `_ensure_singbox_tun_local_proxy_contract`, the Discord proxy contract, the
//! Clash API metrics contract and the managed TUN base rules).

use std::collections::HashSet;
use std::net::{IpAddr, ToSocketAddrs, UdpSocket};

use anyhow::Result;
#[cfg(test)]
use parking_lot::Mutex;
use serde_json::{json, Value};

use super::dns::{
    ensure_singbox_dns_runtime_contract, singbox_uses_builtin_dns, dns_strategy,
    BROWSER_DOH_DOMAIN_SUFFIXES,
};
use super::{
    ensure_dict, ensure_list, ensure_map_list, is_falsy_json, json_str_field, lower_trim,
    normalize_route_exclude_addresses, py_int_or,
};
use lumen_core::constants::{
    DEFAULT_DISCORD_SOCKS_PORT, DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT, PROXY_HOST,
    SINGBOX_CLASH_API_PORT, SINGBOX_TUN_INTERFACE_NAME,
};
use lumen_core::models::RoutingSettings;

const SINGBOX_TUN_IPV6_ADDRESS: &str = "fdfe:dcba:9876::1/126";
const SINGBOX_TUN_IPV4_CANDIDATES: [&str; 7] = [
    "172.18.0.1/30",
    "172.19.0.1/30",
    "198.18.0.1/30",
    "198.19.0.1/30",
    "100.64.0.1/30",
    "10.201.7.1/30",
    "192.0.2.1/30",
];

#[cfg(test)]
static TEST_LOCAL_ADDRS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
#[cfg(test)]
static TEST_GATEWAY_IN_USE: Mutex<Option<bool>> = Mutex::new(None);

/// Test hook mirroring Python `monkeypatch.setattr(planner, "_local_ipv4_addresses", ...)`.
#[cfg(test)]
pub(crate) fn set_test_tun_env(local_addrs: Option<HashSet<String>>, gateway_in_use: Option<bool>) {
    *TEST_LOCAL_ADDRS.lock() = local_addrs;
    *TEST_GATEWAY_IN_USE.lock() = gateway_in_use;
}

/// Python `_local_ipv4_addresses`.
pub fn local_ipv4_addresses() -> HashSet<String> {
    #[cfg(test)]
    if let Some(override_set) = TEST_LOCAL_ADDRS.lock().clone() {
        return override_set;
    }
    let mut found: HashSet<String> = HashSet::new();
    // Python: socket.getaddrinfo(socket.gethostname(), None, AF_INET).
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_default();
    if let Ok(addresses) = (hostname.as_str(), 0u16).to_socket_addrs() {
        for address in addresses {
            if let IpAddr::V4(v4) = address.ip() {
                found.insert(v4.to_string());
            }
        }
    }
    found
}

/// Python `_ipv4_gateway_in_use`: a gateway address already assigned to a local
/// adapter can be bound; a free one raises EADDRNOTAVAIL. This detects
/// Docker/WSL/Hyper-V clashes.
pub fn ipv4_gateway_in_use(gateway_ip: &str) -> bool {
    #[cfg(test)]
    if let Some(value) = *TEST_GATEWAY_IN_USE.lock() {
        return value;
    }
    UdpSocket::bind((gateway_ip, 0)).is_ok()
}

fn pick_singbox_tun_ipv4_with(known: &HashSet<String>, gateway_in_use: &dyn Fn(&str) -> bool) -> String {
    for candidate in SINGBOX_TUN_IPV4_CANDIDATES {
        let gateway = candidate.split('/').next().unwrap_or(candidate);
        if gateway_in_use(gateway) {
            continue;
        }
        let collides = known.iter().any(|address| {
            address
                .parse::<IpAddr>()
                .map(|ip| super::ip_in_cidr(ip, candidate))
                .unwrap_or(false)
        });
        if !collides {
            return candidate.to_string();
        }
    }
    SINGBOX_TUN_IPV4_CANDIDATES[0].to_string()
}

/// Python `_pick_singbox_tun_ipv4`.
pub fn pick_singbox_tun_ipv4() -> String {
    let known = local_ipv4_addresses();
    pick_singbox_tun_ipv4_with(&known, &|gateway| ipv4_gateway_in_use(gateway))
}

/// Python `_singbox_tun_addresses`.
pub fn singbox_tun_addresses() -> Vec<String> {
    vec![pick_singbox_tun_ipv4(), SINGBOX_TUN_IPV6_ADDRESS.to_string()]
}

/// Python `_ensure_tun_inbound`.
pub fn ensure_tun_inbound(config: &mut Value) {
    let inbounds = ensure_list(config, "inbounds");
    if inbounds
        .iter()
        .any(|inbound| inbound.is_object() && lower_trim(&json_str_field(inbound, "type")) == "tun")
    {
        return;
    }
    inbounds.insert(
        0,
        json!({
            "type": "tun",
            "tag": super::APP_TUN_INBOUND_TAG,
            "interface_name": SINGBOX_TUN_INTERFACE_NAME,
        }),
    );
}

/// Python `_rename_singbox_rule_inbound_tags`.
pub fn rename_singbox_rule_inbound_tags(payload: &mut Value, old_tags: &HashSet<String>, new_tag: &str) {
    let Some(rules) = payload
        .get_mut("route")
        .filter(|route| route.is_object())
        .and_then(|route| route.get_mut("rules"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for rule in rules.iter_mut() {
        if !rule.is_object() {
            continue;
        }
        match rule.get("inbound").cloned() {
            Some(Value::String(inbound)) => {
                if old_tags.contains(inbound.as_str()) {
                    rule["inbound"] = json!(new_tag);
                }
            }
            Some(Value::Array(items)) => {
                // Python: list(dict.fromkeys(new_tag if str(item) in old_tags else item ...)).
                let mut unique: Vec<Value> = Vec::new();
                for item in &items {
                    let entry = if old_tags.contains(&super::py_str_list_item(item)) {
                        json!(new_tag)
                    } else {
                        item.clone()
                    };
                    if !unique.contains(&entry) {
                        unique.push(entry);
                    }
                }
                rule["inbound"] = Value::Array(unique);
            }
            _ => {}
        }
    }
}

/// Python `_configure_singbox_runtime_inbounds`.
pub fn configure_singbox_runtime_inbounds(
    config: &mut Value,
    tun_mode: bool,
    local_socks_port: i64,
    local_http_port: i64,
    proxy_auth_username: &str,
    proxy_auth_password: &str,
) {
    if tun_mode {
        ensure_tun_inbound(config);
        return;
    }
    {
        let inbounds = ensure_list(config, "inbounds");
        inbounds.retain(|inbound| {
            !(inbound.is_object() && lower_trim(&json_str_field(inbound, "type")) == "tun")
        });
    }
    ensure_singbox_tun_local_proxy_contract(
        config,
        local_socks_port,
        local_http_port,
        proxy_auth_username,
        proxy_auth_password,
    );
}

/// Python `_ensure_singbox_discord_proxy_contract`.
pub fn ensure_singbox_discord_proxy_contract(payload: &mut Value, enabled: bool) -> Result<()> {
    {
        let inbounds = ensure_list(payload, "inbounds");
        inbounds.retain(|inbound| {
            !(inbound.is_object() && json_str_field(inbound, "tag") == super::APP_DISCORD_PROXY_INBOUND_TAG)
        });
    }
    {
        let route = ensure_dict(payload, "route");
        let rules = ensure_map_list(route, "rules");
        rules.retain(|rule| !is_singbox_discord_proxy_rule(rule));
    }

    if !enabled {
        return Ok(());
    }

    {
        let inbounds = payload.get("inbounds").and_then(Value::as_array);
        if let Some(inbounds) = inbounds {
            for inbound in inbounds {
                if !inbound.is_object() {
                    continue;
                }
                let listen_port = inbound.get("listen_port").filter(|value| !is_falsy_json(value));
                let port_value = match listen_port {
                    Some(value) => Some(value),
                    None => inbound.get("port").filter(|value| !is_falsy_json(value)),
                };
                let port = py_int_or(port_value, 0);
                if port == DEFAULT_DISCORD_SOCKS_PORT as i64 {
                    let mut tag = json_str_field(inbound, "tag").trim().to_string();
                    if tag.is_empty() {
                        tag = "<no tag>".to_string();
                    }
                    anyhow::bail!(
                        "sing-box inbound `{tag}` already uses Discord proxy port {}.",
                        DEFAULT_DISCORD_SOCKS_PORT
                    );
                }
            }
        }
    }

    {
        let inbounds = ensure_list(payload, "inbounds");
        inbounds.push(json!({
            "type": "socks",
            "tag": super::APP_DISCORD_PROXY_INBOUND_TAG,
            "listen": PROXY_HOST,
            "listen_port": DEFAULT_DISCORD_SOCKS_PORT as i64,
        }));
    }
    {
        let route = ensure_dict(payload, "route");
        let rules = ensure_map_list(route, "rules");
        let index = singbox_discord_rule_insert_index(rules);
        rules.insert(
            index,
            json!({
                "inbound": [super::APP_DISCORD_PROXY_INBOUND_TAG],
                "action": "route",
                "outbound": "proxy",
            }),
        );
    }
    Ok(())
}

fn singbox_discord_rule_insert_index(rules: &[Value]) -> usize {
    let mut index = 0usize;
    while index < rules.len() {
        let rule = &rules[index];
        if !rule.is_object() {
            break;
        }
        if rule.get("inbound").map(|value| !is_falsy_json(value)).unwrap_or(false) {
            index += 1;
            continue;
        }
        let action = json_str_field(rule, "action");
        if matches!(action.as_str(), "sniff" | "hijack-dns" | "route-options") {
            index += 1;
            continue;
        }
        break;
    }
    index
}

fn is_singbox_discord_proxy_rule(rule: &Value) -> bool {
    if !rule.is_object() {
        return false;
    }
    match rule.get("inbound") {
        Some(Value::String(inbound)) => inbound == super::APP_DISCORD_PROXY_INBOUND_TAG,
        Some(Value::Array(items)) => items
            .iter()
            .any(|item| super::py_str_list_item(item) == super::APP_DISCORD_PROXY_INBOUND_TAG),
        _ => false,
    }
}

/// Python `_ensure_singbox_tun_local_proxy_contract`: keep v2rayN-style local
/// proxy ports alive while TUN is running.
///
/// Some browsers, Necko profiles, extensions and helper apps keep using
/// 127.0.0.1:10808/10809 even after the Windows system proxy is disabled for
/// TUN. v2rayN keeps local proxy inbounds available in TUN mode, so do the same
/// here instead of letting those requests time out outside sing-box.
pub fn ensure_singbox_tun_local_proxy_contract(
    payload: &mut Value,
    socks_port: i64,
    http_port: i64,
    username: &str,
    password: &str,
) {
    let safe_port = |value: i64, default: i64| -> i64 {
        if value <= 0 || value > 65535 || value == DEFAULT_DISCORD_SOCKS_PORT as i64 {
            // keep droute port free
            default
        } else {
            value
        }
    };
    let socks_value = safe_port(socks_port, DEFAULT_SOCKS_PORT as i64);
    let mut http_value = safe_port(http_port, DEFAULT_HTTP_PORT as i64);
    if http_value == socks_value {
        http_value = if socks_value != DEFAULT_HTTP_PORT as i64 {
            DEFAULT_HTTP_PORT as i64
        } else {
            DEFAULT_HTTP_PORT as i64 + 1
        };
    }
    {
        let inbounds = ensure_list(payload, "inbounds");
        inbounds.retain(|inbound| {
            let managed = inbound.is_object()
                && (json_str_field(inbound, "tag").trim() == super::APP_TUN_MIXED_INBOUND_TAG
                    || json_str_field(inbound, "tag").trim() == super::APP_TUN_HTTP_INBOUND_TAG);
            !managed
        });
    }
    {
        let route = ensure_dict(payload, "route");
        let rules = ensure_map_list(route, "rules");
        rules.retain(|rule| !is_singbox_tun_local_proxy_rule(rule));
    }

    let auth_users: Vec<Value> = if !username.trim().is_empty() && !password.is_empty() {
        vec![json!({"username": username.trim(), "password": password})]
    } else {
        Vec::new()
    };
    let mut mixed_inbound = json!({
        "type": "mixed",
        "tag": super::APP_TUN_MIXED_INBOUND_TAG,
        "listen": PROXY_HOST,
        "listen_port": socks_value,
    });
    let mut http_inbound = json!({
        "type": "http",
        "tag": super::APP_TUN_HTTP_INBOUND_TAG,
        "listen": PROXY_HOST,
        "listen_port": http_value,
    });
    if !auth_users.is_empty() {
        mixed_inbound["users"] = json!(auth_users);
        http_inbound["users"] = json!(auth_users);
    }
    {
        let inbounds = ensure_list(payload, "inbounds");
        inbounds.push(mixed_inbound);
        inbounds.push(http_inbound);
    }
    {
        let route = ensure_dict(payload, "route");
        let rules = ensure_map_list(route, "rules");
        let index = singbox_local_proxy_rule_insert_index(rules);
        rules.insert(
            index,
            json!({
                "inbound": [super::APP_TUN_MIXED_INBOUND_TAG, super::APP_TUN_HTTP_INBOUND_TAG],
                "action": "route",
                "outbound": "proxy",
            }),
        );
    }
}

fn is_singbox_tun_local_proxy_rule(rule: &Value) -> bool {
    if !rule.is_object() {
        return false;
    }
    let tags: HashSet<String> = match rule.get("inbound") {
        Some(Value::String(inbound)) => HashSet::from([inbound.clone()]),
        Some(Value::Array(items)) => items.iter().map(super::py_str_list_item).collect(),
        _ => return false,
    };
    (tags.contains(super::APP_TUN_MIXED_INBOUND_TAG) || tags.contains(super::APP_TUN_HTTP_INBOUND_TAG))
        && json_str_field(rule, "outbound") == "proxy"
}

fn singbox_local_proxy_rule_insert_index(rules: &[Value]) -> usize {
    let mut index = 0usize;
    while index < rules.len() {
        let rule = &rules[index];
        if !rule.is_object() {
            break;
        }
        if rule.get("inbound").map(|value| !is_falsy_json(value)).unwrap_or(false) {
            index += 1;
            continue;
        }
        let action = json_str_field(rule, "action");
        if matches!(action.as_str(), "sniff" | "route-options") {
            index += 1;
            continue;
        }
        break;
    }
    index
}

/// Python `secrets.token_urlsafe(nbytes)`.
fn token_urlsafe(nbytes: usize) -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut buffer = vec![0u8; nbytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

/// Python `_ensure_singbox_metrics_contract`; returns the rotating Clash API
/// secret.
pub fn ensure_singbox_metrics_contract(payload: &mut Value) -> String {
    {
        let log = ensure_dict(payload, "log");
        let level = lower_trim(&super::json_str(log.get("level")));
        if matches!(level.as_str(), "" | "warn" | "warning" | "error" | "fatal" | "panic") {
            log.insert("level".to_string(), json!("info"));
        }
        log.entry("timestamp".to_string()).or_insert(json!(true));
    }
    {
        let experimental = ensure_dict(payload, "experimental");
        let cache_file = super::ensure_map_dict(experimental, "cache_file");
        let cache_path = lumen_core::constants::init_data_dir()
            .join("runtime")
            .join("sing-box-cache.db");
        let _ = std::fs::create_dir_all(cache_path.parent().unwrap_or(std::path::Path::new(".")));
        cache_file.insert("enabled".to_string(), json!(true));
        cache_file.insert("path".to_string(), json!(cache_path.to_string_lossy()));
        cache_file.insert("store_fakeip".to_string(), json!(true));
        cache_file.insert("store_warp_config".to_string(), json!(true));
    }
    let clash_api_secret = token_urlsafe(32);
    {
        let experimental = payload.get_mut("experimental").unwrap();
        if let Some(map) = experimental.as_object_mut() {
            map.insert(
                "clash_api".to_string(),
                json!({
                    "external_controller": format!("127.0.0.1:{SINGBOX_CLASH_API_PORT}"),
                    "secret": clash_api_secret,
                }),
            );
        }
    }
    clash_api_secret
}

/// Python `_clamp_tun_mtu_for_proxy`: do not advertise jumbo TUN packets to
/// MTU-limited VPN endpoints.
pub fn clamp_tun_mtu_for_proxy(payload: &mut Value, proxy: &Value, enabled: bool) {
    if !enabled {
        return;
    }
    let proxy_type = lower_trim(&json_str_field(proxy, "type"));
    if !matches!(
        proxy_type.as_str(),
        "wireguard" | "warp" | "masque" | "openvpn" | "openvpn-client"
    ) {
        return;
    }
    let fallback = if matches!(proxy_type.as_str(), "openvpn" | "openvpn-client") {
        1500
    } else {
        1280
    };
    let endpoint_mtu = match proxy.get("mtu").filter(|value| !is_falsy_json(value)) {
        Some(value) => super::py_int(Some(value)).unwrap_or(1280),
        None => fallback,
    };
    let endpoint_mtu = endpoint_mtu.clamp(576, 9000);
    let Some(inbounds) = payload.get_mut("inbounds").and_then(Value::as_array_mut) else {
        return;
    };
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() || lower_trim(&json_str_field(inbound, "type")) != "tun" {
            continue;
        }
        let current_mtu = match inbound.get("mtu").filter(|value| !is_falsy_json(value)) {
            Some(value) => super::py_int(Some(value)).unwrap_or(endpoint_mtu),
            None => endpoint_mtu,
        };
        inbound["mtu"] = json!(current_mtu.min(endpoint_mtu));
    }
}

/// Python `_ensure_singbox_tun_runtime_contract`: patch app-owned runtime
/// fields for raw sing-box configs.
///
/// Keep the Windows adapter identity stable so route and DNS registration can
/// be reused across reconnects. MTU, stack and strict_route follow user
/// settings (v2rayN parity) with safe defaults.
#[allow(clippy::too_many_arguments)]
pub fn ensure_singbox_tun_runtime_contract(
    payload: &mut Value,
    routing: Option<&RoutingSettings>,
    enable_final_fragment: bool,
    system_dns_servers: &[String],
    tun_strict_route: bool,
    tun_stack: &str,
    tun_mtu: i64,
    tun_endpoint_independent_nat: bool,
    tun_block_quic: bool,
    local_socks_port: i64,
    local_http_port: i64,
    proxy_auth_username: &str,
    proxy_auth_password: &str,
) {
    let mtu_value = tun_mtu.clamp(1280, 65535);
    let mut stack_value = lower_trim(tun_stack);
    if !matches!(stack_value.as_str(), "system" | "gvisor" | "mixed") {
        stack_value = "mixed".to_string();
    }
    let mut has_tun = false;
    let mut renamed_tun_tags: HashSet<String> = HashSet::new();
    if let Some(inbounds) = payload.get_mut("inbounds").and_then(Value::as_array_mut) {
        for inbound in inbounds.iter_mut() {
            if !inbound.is_object() {
                continue;
            }
            if lower_trim(&json_str_field(inbound, "type")) != "tun" {
                continue;
            }
            has_tun = true;
            let previous_tag = json_str_field(inbound, "tag").trim().to_string();
            if !previous_tag.is_empty() && previous_tag != super::APP_TUN_INBOUND_TAG {
                renamed_tun_tags.insert(previous_tag);
            }
            inbound["tag"] = json!(super::APP_TUN_INBOUND_TAG);
            // Preserve an explicit name from the selected template/imported
            // config. Only omitted names receive Lumen's stable default.
            let interface_name = json_str_field(inbound, "interface_name").trim().to_string();
            inbound["interface_name"] = json!(if interface_name.is_empty() {
                SINGBOX_TUN_INTERFACE_NAME.to_string()
            } else {
                interface_name
            });
            inbound["address"] = json!(singbox_tun_addresses());
            inbound["mtu"] = json!(mtu_value);
            inbound["auto_route"] = json!(true);
            if let Some(map) = inbound.as_object_mut() {
                map.shift_remove("route_address");
            }
            // On Windows, strict_route installs the WFP protection needed to
            // stop multihomed DNS from leaving through a physical interface.
            // The route-level DNS hijack below only sees packets already in TUN.
            inbound["strict_route"] = json!(tun_strict_route);
            inbound["stack"] = json!(stack_value);
            if tun_endpoint_independent_nat && stack_value != "system" {
                // full-cone NAT, needs gvisor/mixed UDP stack
                inbound["endpoint_independent_nat"] = json!(true);
            } else if let Some(map) = inbound.as_object_mut() {
                map.shift_remove("endpoint_independent_nat");
            }
            let excludes = normalize_route_exclude_addresses(
                routing.map(|routing| routing.tun_route_exclude_address.as_slice()).unwrap_or(&[]),
            );
            if !excludes.is_empty() {
                inbound["route_exclude_address"] = json!(excludes);
            } else if let Some(map) = inbound.as_object_mut() {
                map.shift_remove("route_exclude_address");
            }
            if let Some(map) = inbound.as_object_mut() {
                map.shift_remove("sniff");
            }
        }
    }
    if !has_tun {
        return;
    }
    if !renamed_tun_tags.is_empty() {
        rename_singbox_rule_inbound_tags(payload, &renamed_tun_tags, super::APP_TUN_INBOUND_TAG);
    }

    {
        let route = ensure_dict(payload, "route");
        route.insert("auto_detect_interface".to_string(), json!(true));
    }
    let use_builtin_dns = singbox_uses_builtin_dns(routing);
    let direct_strategy = dns_strategy(routing.map(|routing| routing.dns_bootstrap_strategy.as_str()).unwrap_or(""));
    {
        let route = ensure_dict(payload, "route");
        route.insert(
            "default_domain_resolver".to_string(),
            json!({"server": "bootstrap-dns", "strategy": direct_strategy}),
        );
    }
    ensure_singbox_dns_runtime_contract(payload, routing, system_dns_servers);
    {
        let route = ensure_dict(payload, "route");
        let rules = ensure_map_list(route, "rules");
        ensure_singbox_tun_base_rules(
            rules,
            enable_final_fragment,
            use_builtin_dns,
            routing.map(|routing| routing.dns_hijack_enabled).unwrap_or(true),
            tun_block_quic,
        );
    }
    ensure_singbox_tun_local_proxy_contract(
        payload,
        local_socks_port,
        local_http_port,
        proxy_auth_username,
        proxy_auth_password,
    );
}

/// Python `_ensure_singbox_runtime_contract`.
#[allow(clippy::too_many_arguments)]
pub fn ensure_singbox_runtime_contract(
    payload: &mut Value,
    tun_mode: bool,
    routing: Option<&RoutingSettings>,
    enable_final_fragment: bool,
    system_dns_servers: &[String],
    tun_strict_route: bool,
    tun_stack: &str,
    tun_mtu: i64,
    tun_endpoint_independent_nat: bool,
    tun_block_quic: bool,
    local_socks_port: i64,
    local_http_port: i64,
    proxy_auth_username: &str,
    proxy_auth_password: &str,
) {
    if tun_mode {
        ensure_singbox_tun_runtime_contract(
            payload,
            routing,
            enable_final_fragment,
            system_dns_servers,
            tun_strict_route,
            tun_stack,
            tun_mtu,
            tun_endpoint_independent_nat,
            tun_block_quic,
            local_socks_port,
            local_http_port,
            proxy_auth_username,
            proxy_auth_password,
        );
        return;
    }

    {
        let route = ensure_dict(payload, "route");
        route.insert("auto_detect_interface".to_string(), json!(true));
    }
    let direct_strategy = dns_strategy(routing.map(|routing| routing.dns_bootstrap_strategy.as_str()).unwrap_or(""));
    {
        let route = ensure_dict(payload, "route");
        route.insert(
            "default_domain_resolver".to_string(),
            json!({"server": "bootstrap-dns", "strategy": direct_strategy}),
        );
    }
    ensure_singbox_dns_runtime_contract(payload, routing, system_dns_servers);
    ensure_singbox_tun_local_proxy_contract(
        payload,
        local_socks_port,
        local_http_port,
        proxy_auth_username,
        proxy_auth_password,
    );
}

/// Python `_ensure_singbox_tun_base_rules`.
pub fn ensure_singbox_tun_base_rules(
    rules: &mut Vec<Value>,
    enable_final_fragment: bool,
    builtin_dns: bool,
    dns_hijack_all: bool,
    block_quic: bool,
) {
    // `builtin_dns` and `dns_hijack_all` are accepted for signature parity and
    // unused, exactly like the Python body.
    let _ = (builtin_dns, dns_hijack_all);
    let noisy_local_dns_rejects = [
        json!({
            "network": "udp",
            "port": [135, 137, 138, 139, 5353, 5355],
            "action": "reject",
        }),
        json!({
            "ip_cidr": ["224.0.0.0/3", "ff00::/8"],
            "action": "reject",
        }),
    ];
    // DNS hijack must be early and broad: some Windows/Chromium paths are
    // detected by sing-box as protocol=dns before a plain port rule matches.
    let dns_hijack_rule = json!({
        "type": "logical",
        "mode": "or",
        "rules": [{"port": 53}, {"protocol": "dns"}],
        "action": "hijack-dns",
    });
    let sniff_rule = json!({
        "action": "sniff",
        "timeout": "1s",
    });
    // Drop QUIC/HTTP3 coming from the local TUN so browsers fall back to TCP
    // HTTP/2. Scoped to tun-in only, so proxy UDP endpoints (WARP/AWG/Hysteria2)
    // that dial out from the outbound side are untouched.
    let quic_reject_rule = json!({
        "inbound": [super::APP_TUN_INBOUND_TAG],
        "protocol": "quic",
        "action": "reject",
    });
    let browser_doh_reject_rule = json!({
        "inbound": [super::APP_TUN_INBOUND_TAG],
        "domain_suffix": BROWSER_DOH_DOMAIN_SUFFIXES,
        "action": "reject",
    });
    let mut base_rules: Vec<Value> = vec![sniff_rule];
    if enable_final_fragment {
        // `route` is terminal, `route-options` is not: the fragment rule has to
        // precede the GUI route rules or only fall-through traffic gets it.
        base_rules.push(json!({
            "protocol": ["tls"],
            "action": "route-options",
            "tls_fragment": true,
            "tls_fragment_fallback_delay": "500ms",
        }));
    }
    if block_quic {
        base_rules.push(quic_reject_rule);
    }
    base_rules.push(browser_doh_reject_rule);
    base_rules.push(dns_hijack_rule);
    base_rules.extend(noisy_local_dns_rejects);
    rules.retain(|rule| !is_singbox_tun_base_rule(rule));
    let mut prepended = base_rules;
    prepended.extend(rules.drain(..));
    *rules = prepended;
}

/// Python `_remove_singbox_tls_fragment_rule`.
pub fn remove_singbox_tls_fragment_rule(payload: &mut Value) {
    let Some(rules) = payload
        .get_mut("route")
        .filter(|route| route.is_object())
        .and_then(|route| route.get_mut("rules"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    rules.retain(|rule| !is_app_singbox_tls_fragment_rule(rule));
}

/// Python `_is_singbox_tun_base_rule`.
fn is_singbox_tun_base_rule(rule: &Value) -> bool {
    if !rule.is_object() {
        return false;
    }
    let action = json_str_field(rule, "action");
    if action == "sniff" {
        return true;
    }
    if action == "hijack-dns" {
        return true;
    }
    if action == "reject"
        && rule.get("inbound") == Some(&json!([super::APP_TUN_INBOUND_TAG]))
        && json_str_field(rule, "protocol") == "quic"
    {
        return true;
    }
    if is_app_singbox_tls_fragment_rule(rule) {
        return true;
    }
    if action == "reject"
        && rule.get("inbound") == Some(&json!([super::APP_TUN_INBOUND_TAG]))
        && rule.get("domain_suffix") == Some(&json!(BROWSER_DOH_DOMAIN_SUFFIXES))
    {
        return true;
    }
    let protocol = rule.get("protocol");
    if protocol == Some(&json!("dns")) || protocol == Some(&json!(["dns"])) {
        return true;
    }
    let port = rule.get("port");
    if (port == Some(&json!(53)) || port == Some(&json!([53])))
        && matches!(action.as_str(), "hijack-dns" | "")
    {
        return true;
    }
    if action == "reject" && json_str_field(rule, "network") == "udp" && port == Some(&json!(443)) {
        return true;
    }
    if action == "reject"
        && json_str_field(rule, "network") == "udp"
        && (port == Some(&json!([135, 137, 138, 139, 5353]))
            || port == Some(&json!([135, 137, 138, 139, 5353, 5355])))
    {
        return true;
    }
    action == "reject" && rule.get("ip_cidr") == Some(&json!(["224.0.0.0/3", "ff00::/8"]))
}

/// Python `_is_app_singbox_tls_fragment_rule`.
fn is_app_singbox_tls_fragment_rule(rule: &Value) -> bool {
    if !rule.is_object() {
        return false;
    }
    if json_str_field(rule, "action") != "route-options" {
        return false;
    }
    if rule.get("protocol") != Some(&json!(["tls"])) {
        return false;
    }
    let fragment = rule.get("tls_record_fragment") == Some(&Value::Bool(true))
        || rule.get("tls_fragment") == Some(&Value::Bool(true));
    if !fragment {
        return false;
    }
    const ALLOWED: [&str; 5] = [
        "protocol",
        "action",
        "tls_record_fragment",
        "tls_fragment",
        "tls_fragment_fallback_delay",
    ];
    rule.as_object()
        .unwrap()
        .keys()
        .all(|key| ALLOWED.contains(&key.as_str()))
}
