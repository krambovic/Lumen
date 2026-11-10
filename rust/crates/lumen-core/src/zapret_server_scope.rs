//! Restrict a Zapret preset to the traffic of a single VPN server.
//!
//! When a server is bound to a preset ("run together with server"), the bound
//! preset must only touch the connection to that server, while the preset the
//! user started by hand keeps working for everything else. winws2 supports
//! several profiles in one process (separated by `--new`, first match wins),
//! so both strategies run in a single winws2 instance:
//!
//! ```text
//! <merged global options> <server profiles> --new <manual profiles>
//! ```
//!
//! A server is bound either to a dedicated server strategy (`@server[:<id>]`:
//! a strategy from the winws2 catalog of its kind -- TCP, QUIC or WireGuard,
//! see [`crate::zapret_strategies`]) or to a preset. A preset becomes server
//! profiles by replacing its own targets (hostlists / ipsets / L7 filters)
//! with `--ipset-ip=<server IPs>` and the server port, so it never matches
//! other traffic.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::config::bootstrap_dns;
use crate::models::Node;
use crate::zapret_strategies::{
    is_server_strategy, kind_transport, node_server_kind, server_strategy_id, strategy_for,
    strategy_global_args, SERVER_STRATEGY_PRESET,
};

// Options that configure the winws2 process as a whole rather than a profile.
const GLOBAL_PREFIXES: [&str; 7] = [
    "--lua-init",
    "--lua-gc",
    "--blob",
    "--wf-",
    "--ctrack-",
    "--ipcache-",
    "--debug",
];
// WinDivert port filters: merged into one option per key.
const WF_PORT_KEYS: [&str; 4] =
    ["--wf-tcp-out", "--wf-udp-out", "--wf-tcp-in", "--wf-udp-in"];
// Profile target filters that are replaced by the server IP filter.
const TARGET_PREFIXES: [&str; 4] = ["--hostlist", "--ipset", "--filter-l7", "--name"];
const POSITIVE_TARGETS: [&str; 5] = [
    "--hostlist=",
    "--hostlist-domains=",
    "--ipset=",
    "--ipset-ip=",
    "--hostlist-auto=",
];

// Server profiles only touch connection handshakes: a desync applied to the
// data of a live VPN session ("--payload=all" in most presets) corrupts it.
const TCP_HANDSHAKE_PAYLOAD: &str = "tls_client_hello,http_req";
const UDP_HANDSHAKE_PAYLOAD: &str = "quic_initial,wireguard_initiation";

const UDP_PROTOCOLS: [&str; 8] =
    ["hysteria", "hysteria2", "hy2", "tuic", "wireguard", "wg", "warp", "awg"];
const UDP_NETWORKS: [&str; 3] = ["kcp", "mkcp", "quic"];

#[derive(Debug, Clone, PartialEq)]
pub struct ZapretServerScope {
    pub preset: String,
    pub ips: Vec<String>,
    pub port: i64,
    pub label: String,
    pub tcp: bool,
    pub udp: bool,
    pub node_id: String,
    /// Dedicated server strategy (preset == SERVER_STRATEGY_PRESET).
    pub kind: String,
    pub strategy: String,
    pub strategy_name: String,
    pub strategy_args: Vec<String>,
}

impl ZapretServerScope {
    pub fn uses_strategy(&self) -> bool {
        self.preset == SERVER_STRATEGY_PRESET
    }

    pub fn title(&self) -> String {
        if self.uses_strategy() {
            let name = if self.strategy_name.is_empty() { &self.strategy } else { &self.strategy_name };
            format!("стратегия {name}")
        } else {
            format!("пресет {}", self.preset)
        }
    }

    /// What winws2 does with the server (addresses excluded).
    pub fn signature(&self) -> (String, String, String, Vec<String>, i64, bool, bool) {
        (
            self.node_id.clone(),
            self.preset.clone(),
            self.strategy.clone(),
            self.strategy_args.clone(),
            self.port,
            self.tcp,
            self.udp,
        )
    }

    pub fn describe(&self) -> String {
        let target = self.ips.join(", ");
        let label = if self.label.is_empty() { &target } else { &self.label };
        format!("{label} ({target}:{})", self.port)
    }
}

pub fn option_key(arg: &str) -> &str {
    arg.split('=').next().unwrap_or(arg)
}

pub fn option_value(arg: &str) -> &str {
    match arg.split_once('=') {
        Some((_, value)) => value,
        None => "",
    }
}

pub fn is_global_option(arg: &str) -> bool {
    GLOBAL_PREFIXES.iter().any(|prefix| arg.starts_with(prefix))
}

/// Split winws2 argv into global options and `--new` separated profiles.
pub fn split_preset_args(args: &[String]) -> (Vec<String>, Vec<Vec<String>>) {
    let mut global_args: Vec<String> = Vec::new();
    let mut profiles: Vec<Vec<String>> = vec![Vec::new()];
    for raw in args {
        let arg = raw.trim();
        if arg.is_empty() {
            continue;
        }
        if arg == "--new" {
            profiles.push(Vec::new());
        } else if is_global_option(arg) {
            global_args.push(arg.to_string());
        } else {
            profiles.last_mut().unwrap().push(arg.to_string());
        }
    }
    profiles.retain(|profile| !profile.is_empty());
    (global_args, profiles)
}

fn port_ranges(spec: &str) -> Vec<(i64, i64)> {
    let mut ranges: Vec<(i64, i64)> = Vec::new();
    for item in spec.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (low, high) = match item.split_once('-') {
            Some((low, high)) => (low, Some(high)),
            None => (item, None),
        };
        let Ok(start) = low.trim().parse::<i64>() else { continue };
        let end = match high {
            Some(high) => match high.trim().parse::<i64>() {
                Ok(value) => value,
                Err(_) => continue,
            },
            None => start,
        };
        ranges.push((start.min(end), start.max(end)));
    }
    ranges
}

pub fn port_spec_covers(spec: &str, port: i64) -> bool {
    port_ranges(spec).iter().any(|(start, end)| *start <= port && port <= *end)
}

fn merge_port_specs(specs: &[&str]) -> String {
    let mut items: Vec<&str> = Vec::new();
    for spec in specs {
        for item in spec.split(',') {
            let item = item.trim();
            if !item.is_empty() && !items.contains(&item) {
                items.push(item);
            }
        }
    }
    items.join(",")
}

/// Merge the global options of several presets without duplicates.
///
/// `--blob` is deduplicated by name and single-value options keep their first
/// occurrence; WinDivert port filters are merged into one option and
/// `extra_ports` (e.g. the server port) is added to them.
pub fn merge_global_args(groups: &[&[String]], extra_ports: &[(String, i64)]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut blob_names: BTreeSet<String> = BTreeSet::new();
    let mut single_keys: BTreeSet<String> = BTreeSet::new();
    let mut wf_ports: HashMap<String, Vec<String>> = HashMap::new();
    let mut wf_positions: Vec<(String, usize)> = Vec::new();
    for group in groups {
        for arg in *group {
            let key = option_key(arg);
            if WF_PORT_KEYS.contains(&key) {
                if !wf_positions.iter().any(|(name, _)| name == key) {
                    wf_positions.push((key.to_string(), merged.len()));
                    merged.push(key.to_string());
                }
                wf_ports.entry(key.to_string()).or_default().push(option_value(arg).to_string());
                continue;
            }
            if seen.contains(arg) {
                continue;
            }
            if key == "--blob" {
                let name = option_value(arg).split(':').next().unwrap_or("").to_string();
                if blob_names.contains(&name) {
                    continue;
                }
                blob_names.insert(name);
            } else if key.starts_with("--ctrack-")
                || key.starts_with("--ipcache-")
                || key == "--lua-gc"
                || key == "--debug"
            {
                if single_keys.contains(key) {
                    continue;
                }
                single_keys.insert(key.to_string());
            }
            seen.insert(arg.clone());
            merged.push(arg.clone());
        }
    }
    for (key, port) in extra_ports {
        if !WF_PORT_KEYS.contains(&key.as_str()) || *port <= 0 {
            continue;
        }
        let specs = wf_ports.entry(key.clone()).or_default();
        if !specs.iter().any(|spec| port_spec_covers(spec, *port)) {
            specs.push(port.to_string());
        }
        if !wf_positions.iter().any(|(name, _)| name == key) {
            wf_positions.push((key.clone(), merged.len()));
            merged.push(key.clone());
        }
    }
    for (key, index) in &wf_positions {
        let empty: Vec<String> = Vec::new();
        let specs = wf_ports.get(key).unwrap_or(&empty);
        let refs: Vec<&str> = specs.iter().map(String::as_str).collect();
        merged[*index] = format!("{key}={}", merge_port_specs(&refs));
    }
    merged
}

fn profile_desyncs(profile: &[String]) -> Vec<String> {
    profile
        .iter()
        .filter(|arg| option_key(arg) == "--lua-desync")
        .map(|arg| option_value(arg).to_string())
        .collect()
}

fn is_pass_profile(profile: &[String]) -> bool {
    let desyncs = profile_desyncs(profile);
    desyncs.is_empty()
        || desyncs
            .iter()
            .all(|value| value.split(':').next().unwrap_or("").trim() == "pass")
}

/// Rewrite one preset profile so it only matches the server endpoint.
pub fn scope_profile(profile: &[String], scope: &ZapretServerScope, name: &str) -> Option<Vec<String>> {
    if is_pass_profile(profile) {
        return None;
    }
    let tcp_spec = profile
        .iter()
        .find(|arg| option_key(arg) == "--filter-tcp")
        .map(|arg| option_value(arg).to_string());
    let udp_spec = profile
        .iter()
        .find(|arg| option_key(arg) == "--filter-udp")
        .map(|arg| option_value(arg).to_string());
    let use_tcp = scope.tcp && (tcp_spec.is_some() || udp_spec.is_none());
    let use_udp = scope.udp && (udp_spec.is_some() || tcp_spec.is_none());
    if !use_tcp && !use_udp {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if use_tcp {
        parts.push(TCP_HANDSHAKE_PAYLOAD);
    }
    if use_udp {
        parts.push(UDP_HANDSHAKE_PAYLOAD);
    }
    let payload = parts.join(",");
    let mut body: Vec<String> = Vec::new();
    for arg in profile {
        let key = option_key(arg);
        if TARGET_PREFIXES.iter().any(|prefix| key.starts_with(prefix))
            || key == "--filter-tcp"
            || key == "--filter-udp"
            || key == "--payload"
        {
            continue;
        }
        body.push(handshake_only(arg, &payload));
    }
    let mut result = vec![format!("--name={name}")];
    if use_tcp {
        result.push(format!("--filter-tcp={}", scope.port));
    }
    if use_udp {
        result.push(format!("--filter-udp={}", scope.port));
    }
    result.push(format!("--ipset-ip={}", scope.ips.join(",")));
    result.push(format!("--payload={payload}"));
    result.extend(body);
    Some(result)
}

fn handshake_only(arg: &str, payload: &str) -> String {
    if option_key(arg) != "--lua-desync" {
        return arg.to_string();
    }
    static PAYLOAD_RE: OnceLock<Regex> = OnceLock::new();
    let re = PAYLOAD_RE.get_or_init(|| Regex::new(":payload=[^:]*").unwrap());
    re.replace_all(arg, format!(":payload={payload}")).into_owned()
}

fn profile_rank(profile: &[String], scope: &ZapretServerScope) -> (usize, usize) {
    let targeted = profile
        .iter()
        .any(|arg| POSITIVE_TARGETS.iter().any(|prefix| arg.starts_with(prefix)));
    let specs: Vec<&str> = profile
        .iter()
        .filter(|arg| option_key(arg) == "--filter-tcp" || option_key(arg) == "--filter-udp")
        .map(|arg| option_value(arg))
        .collect();
    let covered = specs.is_empty() || specs.iter().any(|spec| port_spec_covers(spec, scope.port));
    (if targeted { 1 } else { 0 }, if covered { 0 } else { 1 })
}

/// Global options and server-only profiles built from the bound preset.
///
/// Generic strategies ("all sites") go first, then service specific ones;
/// exclusion profiles (`pass`) are dropped because they would shadow the
/// strategy for the server.
pub fn build_server_profiles(
    server_args: &[String],
    scope: &ZapretServerScope,
) -> (Vec<String>, Vec<Vec<String>>) {
    let (global_args, profiles) = split_preset_args(server_args);
    let mut ranked: Vec<(usize, usize, usize, &Vec<String>)> = profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let (targeted, covered) = profile_rank(profile, scope);
            (targeted, covered, index, profile)
        })
        .collect();
    ranked.sort();
    let label = if scope.label.is_empty() {
        scope.ips.first().cloned().unwrap_or_default()
    } else {
        scope.label.clone()
    };
    let mut result: Vec<Vec<String>> = Vec::new();
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    for (_, _, _, profile) in &ranked {
        let Some(scoped) = scope_profile(profile, scope, &format!("Lumen server: {label}")) else {
            continue;
        };
        let marker: Vec<String> = scoped[1..].to_vec();
        if seen.contains(&marker) {
            continue;
        }
        seen.insert(marker);
        result.push(scoped);
    }
    for (index, profile) in result.iter_mut().enumerate() {
        profile[0] = format!("--name=Lumen server: {label} #{}", index + 1);
    }
    (global_args, result)
}

/// Global options and the single server profile of a dedicated strategy.
pub fn build_strategy_profiles(scope: &ZapretServerScope) -> (Vec<String>, Vec<Vec<String>>) {
    let args = scope.strategy_args.clone();
    if args.is_empty() || scope.ips.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // A "pass" strategy is kept on purpose: the server's packets are then left
    // alone even when the manual preset would touch them.
    let transport = if scope.udp { "udp" } else { "tcp" };
    let label = if scope.label.is_empty() {
        scope.ips.first().cloned().unwrap_or_default()
    } else {
        scope.label.clone()
    };
    let mut profile = vec![
        format!("--name=Lumen server: {label}"),
        format!("--filter-{transport}={}", scope.port),
        format!("--ipset-ip={}", scope.ips.join(",")),
    ];
    profile.extend(args);
    (strategy_global_args(&scope.strategy_args), vec![profile])
}

/// winws2 argv: server-only profiles first, then the manual preset.
pub fn build_scoped_args(
    server_args: &[String],
    scope: &ZapretServerScope,
    manual_args: &[String],
) -> Vec<String> {
    let (server_globals, server_profiles) = if scope.uses_strategy() {
        build_strategy_profiles(scope)
    } else {
        build_server_profiles(server_args, scope)
    };
    let (manual_globals, manual_profiles) = split_preset_args(manual_args);
    if server_profiles.is_empty() {
        return Vec::new();
    }
    let mut extra_ports: Vec<(String, i64)> = Vec::new();
    if scope.tcp {
        extra_ports.push(("--wf-tcp-out".to_string(), scope.port));
    }
    if scope.udp {
        extra_ports.push(("--wf-udp-out".to_string(), scope.port));
    }
    let mut args = merge_global_args(&[&server_globals, &manual_globals], &extra_ports);
    let mut profiles = server_profiles;
    profiles.extend(manual_profiles);
    for (index, profile) in profiles.iter().enumerate() {
        if index > 0 {
            args.push("--new".to_string());
        }
        args.extend(profile.iter().cloned());
    }
    args
}

/// `(tcp, udp)` used by the node to reach its server.
pub fn node_transports(node: &Node) -> (bool, bool) {
    let outbound = &node.outbound;
    let mut protocol = if outbound.is_object() {
        outbound
            .get("protocol")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .or_else(|| outbound.get("type").and_then(Value::as_str))
            .unwrap_or("")
            .trim()
            .to_lowercase()
    } else {
        String::new()
    };
    if protocol.is_empty() {
        protocol = node.scheme.trim().to_lowercase();
    }
    let network = outbound
        .get("streamSettings")
        .filter(|stream| stream.is_object())
        .and_then(|stream| stream.get("network"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if UDP_PROTOCOLS.contains(&protocol.as_str()) || UDP_NETWORKS.contains(&network.as_str()) {
        return (false, true);
    }
    (true, false)
}

/// Scope for a bound node; None without an address or a usable strategy.
pub fn make_server_scope(node: &Node, preset: &str, ips: &[String]) -> Option<ZapretServerScope> {
    make_server_scope_in(node, preset, ips, None)
}

pub fn make_server_scope_in(
    node: &Node,
    preset: &str,
    ips: &[String],
    catalog_dir: Option<&std::path::Path>,
) -> Option<ZapretServerScope> {
    let (host, port) = node_endpoint(node);
    let ip_list: Vec<String> = ips.to_vec();
    if port == 0 || ip_list.is_empty() {
        return None;
    }
    let node_id = node.id.clone();
    let label = if !node.name.is_empty() {
        node.name.clone()
    } else if !host.is_empty() {
        host
    } else {
        node_id.clone()
    };
    if is_server_strategy(preset) {
        let kind = node_server_kind(node);
        let strategy = strategy_for(&kind, &server_strategy_id(preset), catalog_dir)?;
        let udp = kind_transport(&kind) == "udp";
        return Some(ZapretServerScope {
            preset: SERVER_STRATEGY_PRESET.to_string(),
            ips: ip_list,
            port,
            label,
            tcp: !udp,
            udp,
            node_id,
            kind,
            strategy: strategy.strategy_id.clone(),
            strategy_name: strategy.name.clone(),
            strategy_args: strategy.args.clone(),
        });
    }
    let (tcp, udp) = node_transports(node);
    Some(ZapretServerScope {
        preset: preset.to_string(),
        ips: ip_list,
        port,
        label,
        tcp,
        udp,
        node_id,
        kind: String::new(),
        strategy: String::new(),
        strategy_name: String::new(),
        strategy_args: Vec::new(),
    })
}

pub fn node_endpoint(node: &Node) -> (String, i64) {
    let host = node.server.trim().trim_matches(|c| c == '[' || c == ']').to_string();
    let port = node.port;
    let port = if port > 0 && port < 65536 { port } else { 0 };
    (host, port)
}

fn usable_ip(value: &str) -> bool {
    bootstrap_dns::usable_address(value)
}

static RESOLVE_CACHE: OnceLock<parking_lot::Mutex<HashMap<String, (Instant, Vec<String>)>>> =
    OnceLock::new();

fn resolve_cache() -> &'static parking_lot::Mutex<HashMap<String, (Instant, Vec<String>)>> {
    RESOLVE_CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

const RESOLVE_CACHE_TTL: Duration = Duration::from_secs(24 * 3600);

fn trusted_doh(host: &str) -> Result<BTreeSet<String>, bootstrap_dns::BootstrapDnsError> {
    bootstrap_dns::resolve_doh(host, 2.5, &bootstrap_dns::DOH_SERVERS)
}

fn system_lookup(host: &str) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    let iter = (host, 0u16)
        .to_socket_addrs()
        .map_err(|err| err.to_string())?;
    for info in iter {
        let mut address = info.ip().to_string();
        if let Some(pos) = address.find('%') {
            address.truncate(pos);
        }
        if usable_ip(&address) {
            if let Ok(ip) = address.parse::<std::net::IpAddr>() {
                found.insert(ip.to_string());
            }
        }
    }
    Ok(found)
}

fn ordered(addresses: BTreeSet<String>) -> Vec<String> {
    let mut list: Vec<String> = addresses.into_iter().collect();
    // Python sorts by (":" in a, a): IPv4 first, then IPv6, each lexicographic.
    list.sort_by(|a, b| {
        let a6 = a.contains(':');
        let b6 = b.contains(':');
        a6.cmp(&b6).then_with(|| a.cmp(b))
    });
    list
}

/// IP literal or the last real answer for the host, without any lookup.
pub fn cached_server_ips(host: &str) -> Vec<String> {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']').trim_end_matches('.');
    if host.is_empty() {
        return Vec::new();
    }
    if usable_ip(host) {
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return vec![ip.to_string()];
        }
    }
    let cache = resolve_cache().lock();
    if let Some((stamp, ips)) = cache.get(&host.to_lowercase()) {
        if Instant::now().duration_since(*stamp) < RESOLVE_CACHE_TTL {
            return ips.clone();
        }
    }
    Vec::new()
}

/// Real IP addresses of the server (blocking; call from a worker).
///
/// Trusted DoH by literal resolver IP first (ISP DNS may lie about blocked
/// names), then the system resolver (fake-ip answers of TUN mode are
/// ignored), then the last address learned over DoH (persistent, up to
/// 7 days) and finally the last answer of this session.
pub fn resolve_server_ips(host: &str) -> Vec<String> {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']').trim_end_matches('.');
    if host.is_empty() {
        return Vec::new();
    }
    if usable_ip(host) {
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return vec![ip.to_string()];
        }
    }
    let key = host.to_lowercase();
    let resolution = bootstrap_dns::resolve_bootstrap_with(host, 3.0, Some(&trusted_doh), &system_lookup);
    let ips = match resolution {
        Ok(resolution) => ordered(resolution.addresses),
        Err(err) => {
            // Trusted resolvers denied the name: nothing at all. Other
            // resolver failures fall back to the session cache below.
            if err
                .downcast_ref::<bootstrap_dns::BootstrapDnsError>()
                .is_some_and(|e| matches!(e, bootstrap_dns::BootstrapDnsError::NameNotFound(_)))
            {
                return Vec::new();
            }
            Vec::new()
        }
    };
    let now = Instant::now();
    let mut cache = resolve_cache().lock();
    if !ips.is_empty() {
        cache.insert(key, (now, ips.clone()));
        return ips;
    }
    if let Some((stamp, ips)) = cache.get(&key) {
        if now.duration_since(*stamp) < RESOLVE_CACHE_TTL {
            return ips.clone();
        }
    }
    Vec::new()
}

use serde_json::Value;
use std::net::ToSocketAddrs;

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_catalog_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../windows/zapret/strategy_catalogs/winws2")
    }

    fn server_preset() -> Vec<String> {
        [
            "--lua-init=@lua/zapret-lib.lua",
            "--wf-tcp-out=80,443",
            "--wf-udp-out=443",
            "--blob=tls1:@bin/tls_1.bin",
            "--name=exclude",
            "--filter-tcp=80,443",
            "--hostlist=lists/netrogat.txt",
            "--lua-desync=pass",
            "--new",
            "--name=youtube",
            "--filter-tcp=443",
            "--hostlist=lists/youtube.txt",
            "--lua-desync=fake:blob=tls1",
            "--new",
            "--name=all",
            "--filter-tcp=80,443-65535",
            "--ipset-exclude=lists/ipset-ru.txt",
            "--payload=tls_client_hello",
            "--lua-desync=multisplit:pos=1",
            "--new",
            "--name=quic",
            "--filter-udp=443",
            "--filter-l7=quic",
            "--lua-desync=fake:blob=quic1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn manual_preset() -> Vec<String> {
        [
            "--lua-init=@lua/zapret-lib.lua",
            "--lua-init=@lua/custom.lua",
            "--wf-tcp-out=443,2053",
            "--blob=tls1:@bin/other.bin",
            "--name=discord",
            "--filter-tcp=443",
            "--hostlist=lists/discord.txt",
            "--lua-desync=split",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn scope() -> ZapretServerScope {
        ZapretServerScope {
            preset: "srv".to_string(),
            ips: vec!["203.0.113.10".to_string()],
            port: 8443,
            label: "NL".to_string(),
            tcp: true,
            udp: false,
            node_id: String::new(),
            kind: String::new(),
            strategy: String::new(),
            strategy_name: String::new(),
            strategy_args: Vec::new(),
        }
    }

    #[test]
    fn server_profiles_only_match_the_server() {
        let args = build_scoped_args(&server_preset(), &scope(), &[]);
        let (_, profiles) = split_preset_args(&args);

        assert_eq!(profiles.len(), 2);
        for profile in &profiles {
            assert!(profile[0].starts_with("--name=Lumen server: NL"));
            assert!(profile.contains(&"--filter-tcp=8443".to_string()));
            assert!(profile.contains(&"--ipset-ip=203.0.113.10".to_string()));
            assert!(!profile
                .iter()
                .any(|a| a.starts_with("--hostlist")
                    || a.starts_with("--ipset=")
                    || a.starts_with("--ipset-exclude")
                    || a.starts_with("--filter-l7")));
        }
        // Generic "all sites" strategy first, exclusion (pass) profile dropped.
        assert!(profiles[0].contains(&"--lua-desync=multisplit:pos=1".to_string()));
        assert!(profiles[0].contains(&"--payload=tls_client_hello,http_req".to_string()));
        assert!(profiles[1].contains(&"--lua-desync=fake:blob=tls1".to_string()));
        assert!(!profiles.iter().any(|p| p.iter().any(|a| a.contains("--lua-desync=pass"))));
    }

    #[test]
    fn manual_profiles_follow_unchanged_after_new() {
        let args = build_scoped_args(&server_preset(), &scope(), &manual_preset());
        let (globals, profiles) = split_preset_args(&args);

        assert_eq!(
            profiles.last().unwrap(),
            &[
                "--name=discord".to_string(),
                "--filter-tcp=443".to_string(),
                "--hostlist=lists/discord.txt".to_string(),
                "--lua-desync=split".to_string(),
            ]
        );
        assert_eq!(args.iter().filter(|a| a.as_str() == "--new").count(), profiles.len() - 1);
        assert_eq!(
            globals.iter().filter(|a| a.as_str() == "--lua-init=@lua/zapret-lib.lua").count(),
            1
        );
        assert!(globals.contains(&"--lua-init=@lua/custom.lua".to_string()));
        assert_eq!(
            globals.iter().filter(|a| a.starts_with("--blob=tls1:")).cloned().collect::<Vec<_>>(),
            vec!["--blob=tls1:@bin/tls_1.bin".to_string()]
        );
    }

    #[test]
    fn windivert_filter_includes_server_port() {
        let args = build_scoped_args(&server_preset(), &scope(), &manual_preset());
        let wf_tcp: Vec<&String> = args.iter().filter(|a| a.starts_with("--wf-tcp-out=")).collect();
        assert_eq!(wf_tcp, [&"--wf-tcp-out=80,443,2053,8443".to_string()]);

        let merged = merge_global_args(
            &[&["--wf-tcp-out=80,443-65535".to_string()]],
            &[("--wf-tcp-out".to_string(), 8443)],
        );
        assert_eq!(merged, ["--wf-tcp-out=80,443-65535".to_string()]);
    }

    #[test]
    fn udp_server_uses_udp_profiles() {
        let mut udp_scope = scope();
        udp_scope.tcp = false;
        udp_scope.udp = true;
        udp_scope.port = 4433;
        let args = build_scoped_args(&server_preset(), &udp_scope, &[]);
        let (_, profiles) = split_preset_args(&args);

        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].contains(&"--filter-udp=4433".to_string()));
        assert!(profiles[0].contains(&"--payload=quic_initial,wireguard_initiation".to_string()));
        assert!(profiles[0].contains(&"--lua-desync=fake:blob=quic1".to_string()));
        assert!(args.iter().any(|a| a.starts_with("--wf-udp-out=") && a.ends_with(",4433")));
    }

    #[test]
    fn server_profiles_never_touch_session_data() {
        let preset: Vec<String> = [
            "--filter-tcp=443",
            "--payload=all",
            "--lua-desync=fake:blob=tls1:repeats=6:payload=all",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let scoped = scope_profile(&preset, &scope(), "Lumen server: NL").unwrap();
        assert!(scoped.contains(&"--payload=tls_client_hello,http_req".to_string()));
        assert!(!scoped.iter().any(|a| a.contains("payload=all")));
    }

    #[test]
    fn port_spec_tools() {
        assert!(port_spec_covers("80,443-65535", 8443));
        assert!(!port_spec_covers("80,443", 8443));
        assert_eq!(merge_port_specs(&["80,443", "443,2053"]), "80,443,2053");
    }

    #[test]
    fn split_preset_args_splits_on_new() {
        let args: Vec<String> = ["--wf-tcp-out=80", "--new", "--name=x", "--new"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (globals, profiles) = split_preset_args(&args);
        assert_eq!(globals, ["--wf-tcp-out=80".to_string()]);
        assert_eq!(profiles, [vec!["--name=x".to_string()]]);
    }

    #[test]
    fn node_transport_kinds() {
        let tcp_node = Node {
            scheme: "vless".to_string(),
            outbound: serde_json::json!({"protocol": "vless", "streamSettings": {"network": "ws"}}),
            ..Default::default()
        };
        let udp_node = Node {
            scheme: "hysteria2".to_string(),
            outbound: serde_json::json!({"type": "hysteria2"}),
            ..Default::default()
        };
        let kcp_node = Node {
            scheme: "vmess".to_string(),
            outbound: serde_json::json!({"protocol": "vmess", "streamSettings": {"network": "kcp"}}),
            ..Default::default()
        };
        assert_eq!(node_transports(&tcp_node), (true, false));
        assert_eq!(node_transports(&udp_node), (false, true));
        assert_eq!(node_transports(&kcp_node), (false, true));
    }

    #[test]
    fn strategy_scope_per_server_kind() {
        let dir = shipped_catalog_dir();
        let node = Node {
            id: "n1".to_string(),
            name: "NL".to_string(),
            server: "vpn.example.com".to_string(),
            port: 8443,
            scheme: "vless".to_string(),
            outbound: serde_json::json!({"protocol": "vless"}),
            ..Default::default()
        };
        let tcp = make_server_scope_in(&node, SERVER_STRATEGY_PRESET, &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        assert!(tcp.uses_strategy() && tcp.kind == "tcp" && tcp.tcp && !tcp.udp);
        assert_eq!(tcp.strategy, crate::zapret_strategies::default_strategy("tcp"));
        assert!(tcp.title().contains("стратегия"));

        let quic_node = Node {
            id: "n1".to_string(),
            name: "NL".to_string(),
            server: "vpn.example.com".to_string(),
            port: 8443,
            scheme: "hysteria2".to_string(),
            outbound: serde_json::json!({"type": "hysteria2"}),
            ..Default::default()
        };
        let quic = make_server_scope_in(&quic_node, SERVER_STRATEGY_PRESET, &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        assert_eq!(quic.kind, "quic");
        assert!(quic.udp && !quic.tcp);

        let vanished =
            make_server_scope_in(&quic_node, "@server:no-such", &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        assert_eq!(vanished.strategy, crate::zapret_strategies::default_strategy("quic"));
    }

    #[test]
    fn strategy_scope_args_are_server_only() {
        let dir = shipped_catalog_dir();
        let node = Node {
            id: "n1".to_string(),
            name: "NL".to_string(),
            server: "vpn.example.com".to_string(),
            port: 51820,
            scheme: "wireguard".to_string(),
            outbound: serde_json::json!({"type": "wireguard"}),
            ..Default::default()
        };
        let wire = make_server_scope_in(&node, SERVER_STRATEGY_PRESET, &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        let args = build_scoped_args(&[], &wire, &manual_preset());
        let (globals, profiles) = split_preset_args(&args);

        assert_eq!(
            &profiles[0][..3],
            [
                "--name=Lumen server: NL".to_string(),
                "--filter-udp=51820".to_string(),
                "--ipset-ip=203.0.113.10".to_string(),
            ]
        );
        assert_eq!(profiles[0][3..], wire.strategy_args);
        assert_eq!(profiles.last().unwrap()[0], "--name=discord");
        assert!(globals.iter().any(|a| a.starts_with("--wf-udp-out=") && a.ends_with("51820")));
        assert_eq!(
            globals.iter().filter(|a| a.as_str() == "--lua-init=@lua/zapret-lib.lua").count(),
            1
        );
    }

    #[test]
    fn signature_tracks_strategy_not_ips() {
        let dir = shipped_catalog_dir();
        let node = Node {
            id: "n1".to_string(),
            name: "NL".to_string(),
            server: "vpn.example.com".to_string(),
            port: 8443,
            scheme: "vless".to_string(),
            outbound: serde_json::json!({"protocol": "vless"}),
            ..Default::default()
        };
        let first = make_server_scope_in(&node, SERVER_STRATEGY_PRESET, &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        let moved = make_server_scope_in(&node, SERVER_STRATEGY_PRESET, &["203.0.113.11".to_string()], Some(&dir)).unwrap();
        assert_eq!(first.signature(), moved.signature());
        let preset_scope = make_server_scope_in(&node, "general.txt", &["203.0.113.10".to_string()], Some(&dir)).unwrap();
        assert_ne!(preset_scope.signature(), first.signature());
    }

    #[test]
    fn literal_ips_resolve_without_network() {
        assert_eq!(resolve_server_ips("203.0.113.4"), ["203.0.113.4".to_string()]);
        assert_eq!(resolve_server_ips(""), Vec::<String>::new());
        assert_eq!(cached_server_ips("203.0.113.4"), ["203.0.113.4".to_string()]);
    }
}
