//! Server bootstrap contracts (`_ensure_*_bootstrap_contract`), the endpoint
//! DNS cache/workers and the AWG 1.5/3.0 compatibility helpers.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, ToSocketAddrs};
#[cfg(any(test, feature = "test-support"))]
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde_json::{json, Value};

use super::{
    dial_value_mut, ensure_dict, ensure_list, endpoint_ip_cidr, is_domain_name, json_str_field,
    lower_trim, normalize_route_exclude_addresses, py_str_list_item, DialPath,
};
use super::APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG;

const ENDPOINT_DNS_CACHE_TTL: Duration = Duration::from_secs(300);
const ENDPOINT_DNS_FAILURE_TTL: Duration = Duration::from_secs(60);
const ENDPOINT_DNS_TIMEOUT: Duration = Duration::from_millis(1200);
// A subscription selector can hold hundreds of servers: they are resolved in
// parallel against one shared deadline instead of up to 1.2 s each in a row,
// which used to delay a TUN start by 20+ s once the cache had expired.
const ENDPOINT_DNS_PREFETCH: Duration = Duration::from_millis(1500);
const ENDPOINT_DNS_SLOTS: usize = 32;

// ── Endpoint DNS cache ──────────────────────────────────────────────────────

struct DnsEvent {
    done: Mutex<bool>,
    signal: Condvar,
}

impl DnsEvent {
    fn wait(&self, timeout: Duration) -> bool {
        let mut guard = self.done.lock();
        if *guard {
            return true;
        }
        let _result = self.signal.wait_for(&mut guard, timeout);
        *guard
    }

    fn set(&self) {
        *self.done.lock() = true;
        self.signal.notify_all();
    }
}

struct EndpointDnsState {
    cache: HashMap<String, (Instant, Vec<String>)>,
    // Names that did not resolve in time: a later plan skips them at once
    // instead of waiting again (a late answer still lands in the positive
    // cache).
    failed: HashMap<String, Instant>,
    pending: HashMap<String, Arc<DnsEvent>>,
}

static ENDPOINT_DNS: OnceLock<Mutex<EndpointDnsState>> = OnceLock::new();

fn endpoint_dns_state() -> &'static Mutex<EndpointDnsState> {
    ENDPOINT_DNS.get_or_init(|| {
        Mutex::new(EndpointDnsState {
            cache: HashMap::new(),
            failed: HashMap::new(),
            pending: HashMap::new(),
        })
    })
}

struct DnsSlots {
    available: Mutex<usize>,
    released: Condvar,
}

static DNS_SLOTS: OnceLock<DnsSlots> = OnceLock::new();

fn dns_slots() -> &'static DnsSlots {
    DNS_SLOTS.get_or_init(|| DnsSlots {
        available: Mutex::new(ENDPOINT_DNS_SLOTS),
        released: Condvar::new(),
    })
}

struct SlotGuard;

impl SlotGuard {
    fn acquire() -> Self {
        let slots = dns_slots();
        let mut available = slots.available.lock();
        while *available == 0 {
            slots.released.wait(&mut available);
        }
        *available -= 1;
        SlotGuard
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        let slots = dns_slots();
        *slots.available.lock() += 1;
        slots.released.notify_one();
    }
}

#[cfg(any(test, feature = "test-support"))]
type TestGetAddrInfo = dyn Fn(&str) -> std::io::Result<Vec<SocketAddr>> + Send + Sync;

#[cfg(any(test, feature = "test-support"))]
static TEST_GETADDRINFO: Mutex<Option<Arc<TestGetAddrInfo>>> = Mutex::new(None);

#[cfg(any(test, feature = "test-support"))]
pub fn set_test_getaddrinfo(resolver: Option<Arc<TestGetAddrInfo>>) {
    *TEST_GETADDRINFO.lock() = resolver;
}

#[cfg(test)]
pub(crate) fn clear_endpoint_dns_caches_for_tests() {
    let mut state = endpoint_dns_state().lock();
    state.cache.clear();
    state.failed.clear();
    state.pending.clear();
}

/// Python `_endpoint_dns_key`: the lookup key of a host name. The Python tuple
/// also carried `id(socket.getaddrinfo)` (process identity); the Rust cache is
/// keyed on the lower-cased name alone.
fn endpoint_dns_key(host: &str) -> String {
    host.trim().to_lowercase()
}

fn lookup_addresses(host: &str) -> Vec<IpAddr> {
    #[cfg(any(test, feature = "test-support"))]
    {
        // Clone the hook out of the lock so a slow resolver never blocks a
        // test resetting the override.
        let injected = TEST_GETADDRINFO.lock().as_ref().cloned();
        if let Some(resolver) = injected {
            return resolver(host)
                .map(|addresses| addresses.iter().map(|address| address.ip()).collect())
                .unwrap_or_default();
        }
    }
    match (host, 0u16).to_socket_addrs() {
        Ok(addresses) => addresses.map(|address| address.ip()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Python `_resolved_addresses`: deduplicated, IPv4 addresses before IPv6,
/// each group sorted lexicographically.
fn resolved_addresses(addresses: Vec<IpAddr>) -> Vec<String> {
    let mut unique: Vec<IpAddr> = Vec::new();
    for address in addresses {
        if !unique.contains(&address) {
            unique.push(address);
        }
    }
    unique.sort_by(|a, b| {
        let key_a = (if a.is_ipv6() { 6 } else { 4 }, a.to_string());
        let key_b = (if b.is_ipv6() { 6 } else { 4 }, b.to_string());
        key_a.cmp(&key_b)
    });
    unique.into_iter().map(|address| address.to_string()).collect()
}

/// Python `_start_endpoint_resolution`: spawn the resolver thread unless the
/// answer is already known; return the pending event otherwise.
fn start_endpoint_resolution(key: &str) -> Option<Arc<DnsEvent>> {
    let event = {
        let mut state = endpoint_dns_state().lock();
        if let Some((cached_at, _)) = state.cache.get(key) {
            if cached_at.elapsed() < ENDPOINT_DNS_CACHE_TTL {
                return None;
            }
        }
        if let Some(failed_at) = state.failed.get(key) {
            if failed_at.elapsed() < ENDPOINT_DNS_FAILURE_TTL {
                return None;
            }
        }
        if let Some(pending) = state.pending.get(key) {
            return Some(pending.clone());
        }
        let event = Arc::new(DnsEvent {
            done: Mutex::new(false),
            signal: Condvar::new(),
        });
        state.pending.insert(key.to_string(), event.clone());
        event
    };
    let key_owned = key.to_string();
    let spawned = std::thread::Builder::new()
        .name("tun-endpoint-resolver".to_string())
        .spawn(move || {
            let addresses = {
                let _slot = SlotGuard::acquire();
                lookup_addresses(&key_owned)
            };
            let resolved = resolved_addresses(addresses);
            let mut state = endpoint_dns_state().lock();
            if !resolved.is_empty() {
                state.cache.insert(key_owned.clone(), (Instant::now(), resolved));
                state.failed.remove(&key_owned);
            } else {
                state.failed.insert(key_owned.clone(), Instant::now());
            }
            if let Some(event) = state.pending.remove(&key_owned) {
                drop(state);
                event.set();
            }
        });
    if spawned.is_err() {
        // Mirror a failed daemon-thread start: drop the pending entry so a
        // later plan retries instead of waiting on a dead event.
        endpoint_dns_state().lock().pending.remove(key);
        return None;
    }
    Some(event)
}

fn known_endpoint_addresses(key: &str) -> Vec<String> {
    let state = endpoint_dns_state().lock();
    if let Some((cached_at, addresses)) = state.cache.get(key) {
        if cached_at.elapsed() < ENDPOINT_DNS_CACHE_TTL {
            return addresses.clone();
        }
    }
    Vec::new()
}

fn mark_endpoint_timeout(key: &str) {
    let mut state = endpoint_dns_state().lock();
    if !state.cache.contains_key(key) {
        state.failed.insert(key.to_string(), Instant::now());
    }
}

/// Python `_resolve_endpoint_addresses`.
pub fn resolve_endpoint_addresses(host: &str) -> Vec<String> {
    if !is_domain_name(host) {
        return Vec::new();
    }
    let key = endpoint_dns_key(host);
    if let Some(event) = start_endpoint_resolution(&key) {
        if !event.wait(ENDPOINT_DNS_TIMEOUT) {
            mark_endpoint_timeout(&key);
        }
    }
    known_endpoint_addresses(&key)
}

/// Python `_prefetch_endpoint_addresses`: resolve many server names at once so
/// the per-server passes hit the cache.
pub fn prefetch_endpoint_addresses(hosts: &[String], timeout: Duration) {
    let mut pending: Vec<(String, Arc<DnsEvent>)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for host in hosts {
        if !is_domain_name(host) {
            continue;
        }
        let key = endpoint_dns_key(host);
        if !seen.insert(key.clone()) {
            continue;
        }
        if let Some(event) = start_endpoint_resolution(&key) {
            pending.push((key, event));
        }
    }
    let deadline = Instant::now() + timeout;
    for (key, event) in pending {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !event.wait(remaining) {
            mark_endpoint_timeout(&key);
        }
    }
}

/// Python `prime_endpoint_resolution`: warm endpoint DNS without delaying
/// application or connection startup.
pub fn prime_endpoint_resolution(host: &str) {
    if !is_domain_name(host) {
        return;
    }
    let host = host.to_string();
    let _ = std::thread::Builder::new()
        .name("tun-endpoint-prewarm".to_string())
        .spawn(move || {
            resolve_endpoint_addresses(&host);
        });
}

// ── Payload helpers ─────────────────────────────────────────────────────────

/// Python `_has_tun_inbound(payload)`.
pub fn has_tun_inbound(payload: &Value) -> bool {
    payload
        .get("inbounds")
        .and_then(Value::as_array)
        .is_some_and(|inbounds| {
            inbounds
                .iter()
                .any(|inbound| inbound.is_object() && lower_trim(&json_str_field(inbound, "type")) == "tun")
        })
}

/// Python `_payload_server_hosts(items)`.
pub fn payload_server_hosts(items: &[Value]) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for item in items {
        if !item.is_object() {
            continue;
        }
        let item_type = lower_trim(&json_str_field(item, "type"));
        let server = json_str_field(item, "server").trim().to_string();
        if !server.is_empty() {
            hosts.push(server);
        }
        if item_type == "wireguard" {
            for peer in item.get("peers").and_then(Value::as_array).into_iter().flatten() {
                if peer.is_object() {
                    hosts.push(json_str_field(peer, "address").trim().to_string());
                }
            }
        } else if item_type == "warp" {
            hosts.push("engage.cloudflareclient.com".to_string());
        }
        if let Some(servers) = item.get("servers").and_then(Value::as_array) {
            for server_options in servers {
                if server_options.is_object() {
                    hosts.push(json_str_field(server_options, "server").trim().to_string());
                }
            }
        }
    }
    hosts
}

/// Python `_prefetch_payload_endpoints(payload, extra=())`: TUN only, proxy
/// mode never resolves server names while planning.
pub fn prefetch_payload_endpoints(payload: &Value, extra: &[Value]) {
    if !has_tun_inbound(payload) {
        return;
    }
    let mut items: Vec<Value> = Vec::new();
    items.extend(payload.get("outbounds").and_then(Value::as_array).cloned().unwrap_or_default());
    items.extend(payload.get("endpoints").and_then(Value::as_array).cloned().unwrap_or_default());
    items.extend_from_slice(extra);
    let hosts = payload_server_hosts(&items);
    prefetch_endpoint_addresses(&hosts, ENDPOINT_DNS_PREFETCH);
}

// ── Route rules ─────────────────────────────────────────────────────────────

/// Python `_ensure_tun_route_exclude_addresses`.
pub fn ensure_tun_route_exclude_addresses(payload: &mut Value, addresses: &[String]) {
    let normalized = normalize_route_exclude_addresses(addresses);
    if normalized.is_empty() {
        return;
    }
    let Some(inbounds) = payload.get_mut("inbounds").and_then(Value::as_array_mut) else {
        return;
    };
    for inbound in inbounds.iter_mut() {
        if !inbound.is_object() || lower_trim(&json_str_field(inbound, "type")) != "tun" {
            continue;
        }
        let existing: Vec<String> = inbound
            .get("route_exclude_address")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(py_str_list_item).collect())
            .unwrap_or_default();
        let mut combined = existing;
        combined.extend(normalized.iter().cloned());
        inbound["route_exclude_address"] = json!(normalize_route_exclude_addresses(&combined));
    }
}

/// Leading sniff/dns rule skip of Python `_ensure_direct_domain_route` and
/// `_ensure_direct_ip_route`.
fn direct_route_insert_index(rules: &[Value]) -> usize {
    let mut insert_index = 0usize;
    for (index, rule) in rules.iter().enumerate() {
        if !rule.is_object() {
            continue;
        }
        if json_str_field(rule, "action") == "sniff" || json_str_field(rule, "protocol") == "dns" {
            insert_index = index + 1;
            continue;
        }
        break;
    }
    insert_index
}

/// Python `_ensure_direct_domain_route`.
pub fn ensure_direct_domain_route(payload: &mut Value, server: &str) {
    let server = server.trim();
    if server.is_empty() {
        return;
    }
    let direct_rule = json!({"domain": [server], "action": "route", "outbound": "direct"});
    let route = ensure_dict(payload, "route");
    let rules = super::ensure_map_list(route, "rules");
    for rule in rules.iter_mut() {
        if !rule.is_object() {
            continue;
        }
        if let Some(Value::Array(domain_value)) = rule.get("domain") {
            if domain_value.iter().any(|item| py_str_list_item(item) == server) {
                *rule = direct_rule.clone();
                return;
            }
        }
    }
    let insert_index = direct_route_insert_index(rules);
    rules.insert(insert_index, direct_rule);
}

/// Python `_ensure_direct_ip_route`.
pub fn ensure_direct_ip_route(payload: &mut Value, cidr: &str) {
    let cidr = cidr.trim();
    if cidr.is_empty() {
        return;
    }
    let direct_rule = json!({"ip_cidr": [cidr], "action": "route", "outbound": "direct"});
    let route = ensure_dict(payload, "route");
    let rules = super::ensure_map_list(route, "rules");
    for rule in rules.iter_mut() {
        if !rule.is_object() {
            continue;
        }
        if let Some(Value::Array(ip_value)) = rule.get("ip_cidr") {
            if ip_value.iter().any(|item| py_str_list_item(item) == cidr) {
                *rule = direct_rule.clone();
                return;
            }
        }
    }
    let insert_index = direct_route_insert_index(rules);
    rules.insert(insert_index, direct_rule);
}

/// Python `_ensure_hybrid_protect_route`.
pub fn ensure_hybrid_protect_route(payload: &mut Value) {
    let protect_rule = json!({
        "inbound": [APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG],
        "action": "route",
        "outbound": "direct",
    });
    let route = ensure_dict(payload, "route");
    let rules = super::ensure_map_list(route, "rules");
    for rule in rules.iter_mut() {
        if !rule.is_object() {
            continue;
        }
        if let Some(Value::Array(inbound_value)) = rule.get("inbound") {
            if inbound_value
                .iter()
                .any(|item| py_str_list_item(item) == APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG)
            {
                *rule = protect_rule;
                return;
            }
        }
    }
    rules.insert(0, protect_rule);
}

// ── Bootstrap contracts ─────────────────────────────────────────────────────

/// Python `_ensure_proxy_server_bootstrap_contract`.
pub fn ensure_proxy_server_bootstrap_contract(payload: &mut Value, path: DialPath, preferred_server: &str) {
    ensure_proxy_server_bootstrap_contract_with_resolver(
        payload,
        path,
        preferred_server,
        &resolve_endpoint_addresses,
    );
}

pub fn ensure_proxy_server_bootstrap_contract_with_resolver(
    payload: &mut Value,
    path: DialPath,
    preferred_server: &str,
    resolver: &dyn Fn(&str) -> Vec<String>,
) {
    let server = {
        let outbound = dial_value_mut(payload, path);
        let preferred = if preferred_server.trim().is_empty() {
            json_str_field(outbound, "server")
        } else {
            preferred_server.to_string()
        };
        preferred.trim().to_string()
    };
    if server.is_empty() {
        return;
    }
    let endpoint_cidr = endpoint_ip_cidr(&server);
    if !endpoint_cidr.is_empty() {
        ensure_tun_route_exclude_addresses(payload, &[endpoint_cidr.clone()]);
        ensure_direct_ip_route(payload, &endpoint_cidr);
        return;
    }

    // Domain-based proxy servers must resolve through bootstrap-dns, otherwise
    // proxy-dns can recurse into the proxy outbound before the tunnel is ready.
    {
        let outbound = dial_value_mut(payload, path);
        outbound["domain_resolver"] = json!("bootstrap-dns");
    }
    // Proxy mode has no virtual adapter to exclude endpoint IPs from.  A
    // selector with hundreds of subscription nodes used to resolve every
    // candidate synchronously here, delaying each proxy start by seconds.
    // The direct domain rule and bootstrap resolver are sufficient without TUN.
    if !has_tun_inbound(payload) {
        ensure_direct_domain_route(payload, &server);
        return;
    }
    let endpoint_addresses = resolver(&server);
    let endpoint_cidrs: Vec<String> = endpoint_addresses.iter().map(|address| endpoint_ip_cidr(address)).collect();
    if !endpoint_cidrs.is_empty() {
        // v2rayN parity: keep the domain in the outbound so sing-box re-resolves
        // it on every redial
        ensure_tun_route_exclude_addresses(payload, &endpoint_cidrs);
        for cidr in &endpoint_cidrs {
            ensure_direct_ip_route(payload, cidr);
        }
    }
    ensure_direct_domain_route(payload, &server);
}

/// Python `_resolve_endpoint_ip_cidrs(host)`.
fn resolve_endpoint_ip_cidrs_with(host: &str, resolver: &dyn Fn(&str) -> Vec<String>) -> Vec<String> {
    resolver(host).iter().map(|address| endpoint_ip_cidr(address)).collect()
}

/// Python `_ensure_endpoint_server_bootstrap_contract`.
pub fn ensure_endpoint_server_bootstrap_contract(payload: &mut Value, path: DialPath) {
    ensure_endpoint_server_bootstrap_contract_with_resolver(payload, path, &resolve_endpoint_addresses);
}

pub fn ensure_endpoint_server_bootstrap_contract_with_resolver(
    payload: &mut Value,
    path: DialPath,
    resolver: &dyn Fn(&str) -> Vec<String>,
) {
    let (endpoint_type, detour, peer_hosts) = {
        let endpoint = dial_value_mut(payload, path);
        let endpoint_type = lower_trim(&json_str_field(endpoint, "type"));
        let detour = json_str_field(endpoint, "detour").trim().to_string();
        let mut peer_hosts: Vec<String> = Vec::new();
        if endpoint_type == "wireguard" {
            for peer in endpoint.get("peers").and_then(Value::as_array).into_iter().flatten() {
                if peer.is_object() {
                    peer_hosts.push(json_str_field(peer, "address").trim().to_string());
                }
            }
        }
        (endpoint_type, detour, peer_hosts)
    };
    if matches!(endpoint_type.as_str(), "wireguard" | "warp") && detour.is_empty() {
        // sing-box on Windows fails to bind the WireGuard endpoint socket when
        // route.auto_detect_interface is enabled and the endpoint has no detour
        // ("unable to update bind: An invalid argument was supplied", see
        // SagerNet/sing-box#2900). The endpoint then reports "connected" but
        // passes no traffic. Pin the endpoint dialer to the direct outbound,
        // mirroring what masque/openvpn outbounds already do.
        dial_value_mut(payload, path)["detour"] = json!("direct");
    }
    let mut hosts: Vec<String> = Vec::new();
    if endpoint_type == "wireguard" {
        dial_value_mut(payload, path)["domain_resolver"] = json!("bootstrap-dns");
        hosts.extend(peer_hosts);
    } else if endpoint_type == "warp" {
        dial_value_mut(payload, path)["domain_resolver"] = json!("bootstrap-dns");
        hosts.push("engage.cloudflareclient.com".to_string());
    }

    let mut resolved_by_host: HashMap<String, Vec<String>> = HashMap::new();
    let mut endpoint_cidrs: Vec<String> = Vec::new();
    for host in &hosts {
        let cidr = endpoint_ip_cidr(host);
        if !cidr.is_empty() {
            endpoint_cidrs.push(cidr);
        } else if is_domain_name(host) {
            let resolved = resolve_endpoint_ip_cidrs_with(host, resolver);
            resolved_by_host.insert(host.clone(), resolved.clone());
            endpoint_cidrs.extend(resolved);
        }
    }
    let non_empty: Vec<String> = endpoint_cidrs.iter().filter(|cidr| !cidr.is_empty()).cloned().collect();
    ensure_tun_route_exclude_addresses(payload, &non_empty);
    for host in &hosts {
        let endpoint_cidr = endpoint_ip_cidr(host);
        if !endpoint_cidr.is_empty() {
            ensure_direct_ip_route(payload, &endpoint_cidr);
        } else if is_domain_name(host) {
            for cidr in resolved_by_host.get(host).map(Vec::as_slice).unwrap_or(&[]) {
                ensure_direct_ip_route(payload, cidr);
            }
            ensure_direct_domain_route(payload, host);
        }
    }
}

/// Python `_ensure_all_endpoint_server_bootstrap_contract`.
pub fn ensure_all_endpoint_server_bootstrap_contract(payload: &mut Value) {
    let Some(endpoints) = payload.get("endpoints").and_then(Value::as_array) else {
        return;
    };
    let count = endpoints.len();
    for index in 0..count {
        let endpoint_type = payload
            .get("endpoints")
            .and_then(Value::as_array)
            .and_then(|items| items.get(index))
            .map(|endpoint| lower_trim(&json_str_field(endpoint, "type")))
            .unwrap_or_default();
        if !matches!(endpoint_type.as_str(), "wireguard" | "warp") {
            continue;
        }
        ensure_endpoint_server_bootstrap_contract(payload, DialPath::Endpoint(index));
    }
}

/// Python `_ensure_openvpn_server_bootstrap_contract`.
pub fn ensure_openvpn_server_bootstrap_contract(payload: &mut Value, path: DialPath) {
    let servers: Vec<String> = {
        let outbound = dial_value_mut(payload, path);
        let mut servers: Vec<String> = Vec::new();
        if let Some(items) = outbound.get("servers").and_then(Value::as_array) {
            for server_options in items {
                if server_options.is_object() {
                    let server = json_str_field(server_options, "server").trim().to_string();
                    if !server.is_empty() {
                        servers.push(server);
                    }
                }
            }
        }
        servers
    };
    for server in servers {
        ensure_proxy_server_bootstrap_contract(payload, path, &server);
    }
}

/// Python `_ensure_all_openvpn_server_bootstrap_contract`.
pub fn ensure_all_openvpn_server_bootstrap_contract(payload: &mut Value) {
    let Some(endpoints) = payload.get("endpoints").and_then(Value::as_array) else {
        return;
    };
    let count = endpoints.len();
    for index in 0..count {
        let endpoint_type = payload
            .get("endpoints")
            .and_then(Value::as_array)
            .and_then(|items| items.get(index))
            .map(|endpoint| lower_trim(&json_str_field(endpoint, "type")))
            .unwrap_or_default();
        if endpoint_type != "openvpn-client" {
            continue;
        }
        ensure_openvpn_server_bootstrap_contract(payload, DialPath::Endpoint(index));
    }
}

/// Python `_ensure_all_server_bootstrap_contracts`.
pub fn ensure_all_server_bootstrap_contracts(payload: &mut Value) {
    prefetch_payload_endpoints(payload, &[]);
    ensure_all_endpoint_server_bootstrap_contract(payload);
    ensure_all_openvpn_server_bootstrap_contract(payload);
}

// ── AWG compatibility ───────────────────────────────────────────────────────

pub const AWG3_DIRECT_DETOUR_TAG: &str = "awg3-direct";

/// Python `_strip_legacy_awg15_fields`: remove AWG 1.5-only fields before
/// handing data to extended 2.6.x.
///
/// Older imports remain readable/editable (and can still be exported), while
/// the current strict sing-box schema never receives the removed J1-J3/Itime
/// members.  The operation is deliberately limited to the runtime copy.
pub fn strip_legacy_awg15_fields(payload: &mut Value) -> bool {
    let mut changed = false;
    if payload.get("endpoints").is_some_and(Value::is_array) {
        let endpoints = payload.get_mut("endpoints").unwrap().as_array_mut().unwrap();
        for endpoint in endpoints.iter_mut() {
            changed |= strip_legacy_awg15_fields_from_dial(endpoint);
        }
    } else {
        changed = strip_legacy_awg15_fields_from_dial(payload);
    }
    changed
}

fn strip_legacy_awg15_fields_from_dial(endpoint: &mut Value) -> bool {
    if !endpoint.is_object() {
        return false;
    }
    let Some(amnezia) = endpoint.get_mut("amnezia").filter(|value| value.is_object()) else {
        return false;
    };
    let mut changed = false;
    for key in ["j1", "j2", "j3", "itime"] {
        if amnezia.get(key).is_some() {
            amnezia.as_object_mut().unwrap().shift_remove(key);
            changed = true;
        }
    }
    changed
}

/// Python `endpoint_needs_windows_bind_workaround`: whether an AWG 3.0
/// endpoint needs a non-default Windows bind.
pub fn endpoint_needs_windows_bind_workaround(endpoint: &Value) -> bool {
    endpoint
        .get("amnezia")
        .filter(|value| value.is_object())
        .map(|amnezia| !json_str_field(amnezia, "header_protection_key").trim().is_empty())
        .unwrap_or(false)
}

fn ensure_awg3_detour_outbound(runtime_config: &mut Value) {
    let outbounds = ensure_list(runtime_config, "outbounds");
    if !outbounds
        .iter()
        .any(|item| item.is_object() && json_str_field(item, "tag") == AWG3_DIRECT_DETOUR_TAG)
    {
        outbounds.push(json!({"type": "direct", "tag": AWG3_DIRECT_DETOUR_TAG}));
    }
}

/// Python `ensure_awg3_windows_bind_workaround`: route AWG 3.0 through a
/// direct detour to avoid the Windows bind bug.
///
/// The extended core's default Windows WireGuard listener rewrites bytes that
/// are part of AWG 3.0 header-protection's nonce.  A detour selects the
/// userspace ClientBind path and leaves those bytes intact.  Explicit user
/// detours are preserved.
pub fn ensure_awg3_windows_bind_workaround(runtime_config: &mut Value, endpoint: &mut Value) -> bool {
    if !endpoint_needs_windows_bind_workaround(endpoint) {
        return false;
    }
    if !json_str_field(endpoint, "detour").trim().is_empty() {
        return false;
    }
    ensure_awg3_detour_outbound(runtime_config);
    endpoint["detour"] = json!(AWG3_DIRECT_DETOUR_TAG);
    true
}

/// Python `_ensure_awg3_windows_bind_workarounds`: apply the AWG 3.0 Windows
/// bind workaround to imported endpoints.
///
/// Selected-node plans call `ensure_awg3_windows_bind_workaround` directly.  A
/// full sing-box document can contain several endpoints and does not have a
/// selected `Node` to pass through that path, so inspect all top-level
/// endpoints without changing explicit detours.
pub fn ensure_awg3_windows_bind_workarounds(runtime_config: &mut Value) -> bool {
    let Some(endpoints) = runtime_config.get("endpoints").and_then(Value::as_array) else {
        return false;
    };
    let targets: Vec<usize> = endpoints
        .iter()
        .enumerate()
        .filter(|(_, endpoint)| {
            endpoint.is_object()
                && endpoint_needs_windows_bind_workaround(endpoint)
                && json_str_field(endpoint, "detour").trim().is_empty()
        })
        .map(|(index, _)| index)
        .collect();
    if targets.is_empty() {
        return false;
    }
    ensure_awg3_detour_outbound(runtime_config);
    let endpoints = runtime_config.get_mut("endpoints").unwrap().as_array_mut().unwrap();
    for index in targets {
        endpoints[index]["detour"] = json!(AWG3_DIRECT_DETOUR_TAG);
    }
    true
}
