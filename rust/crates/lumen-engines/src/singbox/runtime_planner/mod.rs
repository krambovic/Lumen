//! Sing-box runtime planner (`engines/singbox/runtime_planner.py`).
//!
//! Plans the runtime configuration from the loaded document and the selected
//! node: inbounds, outbounds, DNS and route contracts, TUN specifics, the
//! Clash API contract and migrations of imported configs.

mod bootstrap;
mod chain;
mod dns;
mod node_runtime;
mod planner;
mod sidecar;
#[cfg(test)]
mod tests;
mod tun;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use bootstrap::set_test_getaddrinfo;
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use tun::set_test_tun_env;

pub use bootstrap::{
    ensure_all_endpoint_server_bootstrap_contract, ensure_all_openvpn_server_bootstrap_contract,
    ensure_all_server_bootstrap_contracts, ensure_awg3_windows_bind_workaround,
    ensure_awg3_windows_bind_workarounds, ensure_direct_domain_route, ensure_direct_ip_route,
    ensure_endpoint_server_bootstrap_contract, ensure_hybrid_protect_route,
    ensure_openvpn_server_bootstrap_contract, ensure_proxy_server_bootstrap_contract,
    ensure_tun_route_exclude_addresses, endpoint_needs_windows_bind_workaround,
    has_tun_inbound, payload_server_hosts, prefetch_endpoint_addresses,
    prefetch_payload_endpoints, prime_endpoint_resolution, resolve_endpoint_addresses,
    strip_legacy_awg15_fields, AWG3_DIRECT_DETOUR_TAG,
};
pub use chain::{
    apply_singbox_proxy_chain, ensure_full_config_proxy_alias, ensure_singbox_hot_switch_selector,
    find_singbox_dial_by_tag, insert_singbox_dial, node_has_eager_singbox_runtime,
    singbox_node_source_signature, singbox_node_tag, ChainApplication,
};
pub use dns::{
    apply_imported_proxy_dns, build_dns_server, dns_strategy, ensure_singbox_dns_runtime_contract,
    ensure_singbox_hosts_dns_rule, ensure_singbox_https_dns_reject_rule,
    extract_dns_server_tag, is_default_direct_dns, is_default_proxy_dns,
    set_dns_server_dial_contract, singbox_uses_builtin_dns, validate_runtime_dns_contract,
    BROWSER_DOH_DOMAIN_SUFFIXES, DEFAULT_DIRECT_DNS_SERVER, DEFAULT_DIRECT_DNS_TYPE,
    DEFAULT_PROXY_DNS_SERVER, DEFAULT_PROXY_DNS_TYPE,
};
pub use node_runtime::{is_xray_exclusive_node, node_uses_xray_sidecar, xray_only_message};
pub use planner::{
    classify_node_for_singbox, node_is_full_singbox_config, node_should_use_xray_sidecar,
    plan_singbox_runtime, PlanRuntimeOptions, SingboxRuntimePlan, SingboxXraySidecarPlan,
};
pub use sidecar::{
    build_xray_sidecar_config, normalize_openvpn_outbounds, APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG,
    APP_XRAY_SIDECAR_RELAY_INBOUND_TAG,
};
pub use tun::{
    clamp_tun_mtu_for_proxy, configure_singbox_runtime_inbounds, ensure_singbox_discord_proxy_contract,
    ensure_singbox_metrics_contract, ensure_singbox_runtime_contract,
    ensure_singbox_tun_local_proxy_contract, ensure_singbox_tun_runtime_contract,
    ensure_tun_inbound, ipv4_gateway_in_use, local_ipv4_addresses, pick_singbox_tun_ipv4,
    remove_singbox_tls_fragment_rule, rename_singbox_rule_inbound_tags, singbox_tun_addresses,
};

use std::net::IpAddr;

use serde_json::{json, Map, Value};

/// Inbound tag of the sing-box shadowsocks protect listener of the hybrid
/// Xray sidecar (`_SS_PROTECT_METHOD` / `_APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG`).
pub const APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG: &str = "__app_hybrid_protect_in";
pub const APP_DISCORD_PROXY_INBOUND_TAG: &str = "discord-socks-in";
pub const APP_TUN_INBOUND_TAG: &str = "tun-in";
pub const APP_TUN_MIXED_INBOUND_TAG: &str = "socks-in";
pub const APP_TUN_HTTP_INBOUND_TAG: &str = "http-in";
/// `_SS_PROTECT_METHOD`: cipher of the hybrid sidecar protect inbound.
pub const SS_PROTECT_METHOD: &str = "chacha20-ietf-poly1305";
pub const SINGBOX_DHCP_AUTO_DNS: &str = "dhcp://auto";

// ── Python value semantics helpers ──────────────────────────────────────────

/// Python `str(value or "")` for JSON scalars: falsy values become `""`.
pub fn json_str(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(true)) => "True".to_string(),
        Some(Value::Bool(false)) => String::new(),
        _ => String::new(),
    }
}

/// Python `str(data.get(key) or "")`.
pub fn json_str_field(data: &Value, key: &str) -> String {
    json_str(data.get(key))
}

/// Python `str(value or fallback)` for JSON scalars.
pub fn json_str_or(value: Option<&Value>, fallback: &str) -> String {
    match value.filter(|item| !is_falsy_json(item)) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(true)) => "True".to_string(),
        _ => fallback.to_string(),
    }
}

/// Python `value.strip().lower()`.
pub fn lower_trim(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Python `str(item)` for list items (strings stay raw, numbers and booleans
/// render like Python).
pub fn py_str_list_item(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

/// Python truthiness for JSON values (`None`/`False`/`0`/`""`/empty containers
/// are falsy).
pub fn is_falsy_json(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(flag) => !flag,
        Value::Number(number) => number.as_f64().map(|f| f == 0.0).unwrap_or(false),
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
    }
}

/// Python `int(value)` returning `None` on `TypeError`/`ValueError`.
pub fn py_int(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                Some(int)
            } else {
                number.as_f64().map(|float| float.trunc() as i64)
            }
        }
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Python `int(value)` with the caught-exception fallback.
pub fn py_int_or(value: Option<&Value>, fallback: i64) -> i64 {
    py_int(value).unwrap_or(fallback)
}

// ── Shared JSON shape helpers ───────────────────────────────────────────────

/// Python `_ensure_dict(parent, key)`: return the child object or replace it.
pub fn ensure_map_dict<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    if !parent.get(key).is_some_and(Value::is_object) {
        parent.insert(key.to_string(), json!({}));
    }
    parent.get_mut(key).unwrap().as_object_mut().unwrap()
}

/// Python `_ensure_list(parent, key)`: return the child array or replace it.
pub fn ensure_map_list<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    if !parent.get(key).is_some_and(Value::is_array) {
        parent.insert(key.to_string(), json!([]));
    }
    parent.get_mut(key).unwrap().as_array_mut().unwrap()
}

pub fn ensure_dict<'a>(parent: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    ensure_map_dict(parent.as_object_mut().expect("parent must be an object"), key)
}

pub fn ensure_list<'a>(parent: &'a mut Value, key: &str) -> &'a mut Vec<Value> {
    ensure_map_list(parent.as_object_mut().expect("parent must be an object"), key)
}

/// Python `_replace_or_append_tagged(items, tag, payload)`; returns the final
/// position of the payload (needed by `_insert_singbox_dial` identity check).
pub fn replace_or_append_tagged(items: &mut Vec<Value>, tag: &str, payload: &Value) -> usize {
    for (index, item) in items.iter_mut().enumerate() {
        if item.is_object() && json_str_field(item, "tag") == tag {
            *item = payload.clone();
            return index;
        }
    }
    items.push(payload.clone());
    items.len() - 1
}

/// Python `_find_proxy_outbound_index(outbounds)`.
pub fn find_proxy_outbound_index(outbounds: Option<&Vec<Value>>) -> Option<usize> {
    let outbounds = outbounds?;
    outbounds
        .iter()
        .position(|outbound| outbound.is_object() && json_str_field(outbound, "tag") == "proxy")
}

/// Python `_config_has_proxy_outbound(payload)`.
pub fn config_has_proxy_outbound(payload: &Value) -> bool {
    find_proxy_outbound_index(payload.get("outbounds").and_then(Value::as_array)).is_some()
}

/// Python `_is_singbox_endpoint(payload)`.
pub fn is_singbox_endpoint(payload: &Value) -> bool {
    if !payload.is_object() {
        return false;
    }
    matches!(
        lower_trim(&json_str_field(payload, "type")).as_str(),
        "warp" | "wireguard" | "openvpn-client"
    )
}

// ── IP helpers ──────────────────────────────────────────────────────────────

/// Python `_is_domain_name(value)`.
pub fn is_domain_name(value: &str) -> bool {
    let host = value.trim();
    if host.is_empty() || is_dhcp_auto_dns(host) {
        return false;
    }
    host.parse::<IpAddr>().is_err()
}

/// Python `_is_dhcp_auto_dns(value)`.
pub fn is_dhcp_auto_dns(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case(SINGBOX_DHCP_AUTO_DNS)
}

/// Python `_endpoint_ip_cidr(value)`: canonical `/32`/`/128` CIDR of a plain
/// address, or `""` when the value is not an IP.
pub fn endpoint_ip_cidr(value: &str) -> String {
    let Ok(address) = value.trim().parse::<IpAddr>() else {
        return String::new();
    };
    format!("{address}/{}", if address.is_ipv6() { 128 } else { 32 })
}

/// Approximation of Python `ip_address(...).is_private` for the profile DNS
/// hint check (`_apply_imported_proxy_dns`).
pub fn ip_is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => {
            // fec0::/10 (deprecated site-local) completes the Python set.
            let segments = v6.segments();
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || (segments[0] & 0xffc0) == 0xfec0
        }
    }
}

/// Membership test of an address inside a `a.b.c.d/prefix` network string
/// (Python `ip_address(addr) in ip_network(network, strict=False)`).
pub fn ip_in_cidr(address: IpAddr, network: &str) -> bool {
    let Some((network_address, prefix)) = network.trim().split_once('/') else {
        return false;
    };
    let Ok(network_address) = network_address.parse::<IpAddr>() else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match (address, network_address) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            if prefix > 32 {
                return false;
            }
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
            (u32::from(a) & mask) == (u32::from(b) & mask)
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            if prefix > 128 {
                return false;
            }
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
            (u128::from(a) & mask) == (u128::from(b) & mask)
        }
        _ => false,
    }
}

/// Python `_normalize_route_exclude_addresses(values)`: canonical CIDR list,
/// deduplicated and sorted.
pub fn normalize_route_exclude_addresses(values: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for raw in values {
        let value = raw.trim();
        if value.is_empty() || value.contains('%') {
            continue;
        }
        // sing-box accepts CIDR entries here; plain IPs are normalized to /32
        // or /128. `canonical_ip_network` mirrors ip_network(strict=False).
        if let Some(canonical) = lumen_core::routing_runtime::canonical_ip_network(value) {
            normalized.push(canonical);
        }
    }
    normalized.sort();
    normalized.dedup();
    normalized
}

// ── Ports and passwords ─────────────────────────────────────────────────────

/// Python `_find_free_port`: bind a TCP socket on the loopback proxy host.
pub fn find_free_port(
    preferred: i64,
    port_range: Option<std::ops::Range<i64>>,
    excluded: &[i64],
) -> anyhow::Result<i64> {
    let mut candidates: Vec<i64> = Vec::new();
    if preferred > 0 {
        candidates.push(preferred);
    }
    let range = port_range.unwrap_or(preferred..preferred + 100);
    for port in range {
        if !candidates.contains(&port) {
            candidates.push(port);
        }
    }
    for port in candidates {
        if port <= 0 || excluded.contains(&port) {
            continue;
        }
        if let Ok(listener) = std::net::TcpListener::bind((lumen_core::constants::PROXY_HOST, port as u16)) {
            drop(listener);
            return Ok(port);
        }
    }
    anyhow::bail!("No free TCP port available near {preferred}")
}

/// Python `_generate_ss_password`: random protect inbound password.
pub fn generate_ss_password(length: usize) -> String {
    lumen_core::config::runtime_security::generate_local_proxy_credentials("protect", length).1
}

// ── Dial paths ──────────────────────────────────────────────────────────────

/// Location of an outbound/endpoints entry inside the runtime payload. Python
/// mutates those dicts by reference; the Rust planner addresses them by path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialPath {
    Outbound(usize),
    Endpoint(usize),
}

impl DialPath {
    pub fn section(&self) -> &'static str {
        match self {
            DialPath::Outbound(_) => "outbounds",
            DialPath::Endpoint(_) => "endpoints",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            DialPath::Outbound(index) | DialPath::Endpoint(index) => *index,
        }
    }

    pub fn from_section(section: &str, index: usize) -> DialPath {
        if section == "endpoints" {
            DialPath::Endpoint(index)
        } else {
            DialPath::Outbound(index)
        }
    }
}

/// Mutable reference to the dial at `path` (the sections exist by then).
pub fn dial_value_mut<'a>(payload: &'a mut Value, path: DialPath) -> &'a mut Value {
    let section = match path {
        DialPath::Outbound(_) => "outbounds",
        DialPath::Endpoint(_) => "endpoints",
    };
    &mut payload[section][path.index()]
}
