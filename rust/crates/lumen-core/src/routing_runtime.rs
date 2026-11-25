//! Port of `windows/xray_fluent/routing_runtime.py`.
//!
//! GUI routing rules for Xray (`{"type": "field", ..., "outboundTag"}`) and
//! sing-box route/DNS rules (`process_name`/`rule_set`/... plus sentinel-
//! delimited managed blocks for idempotent regeneration). All functions take
//! `serde_json::Value` inputs shaped like `RoutingSettings.to_dict()` /
//! `AppSettings.to_dict()`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};

use crate::constants::{
    base_dir, data_dir, ROUTING_DIRECT, ROUTING_GLOBAL,
    SUBSCRIPTION_FETCHER_EXE_NAME,
};
use crate::geodata_resources::is_singbox_binary_rule_set;
pub use crate::geodata_resources::{
    singbox_rule_set_dir as singbox_binary_rule_set_dir, singbox_rule_set_path,
};
use crate::models::{bool_field, obj_list, str_field, str_list, str_or};
use crate::presets::{process_preset_by_id, service_preset_by_id};
use crate::routing_presets::{
    custom_domain_rules, preset_domain_rules, ROUTING_PRESET_BLOCKED, ROUTING_PRESET_BLOCKED_CN,
};

struct RuleSetSource {
    domains: &'static [&'static str],
    domain_suffix: &'static [&'static str],
    ips: &'static [&'static str],
}

/// `_SINGBOX_RULE_SET_SOURCES`: locally generated rule-sets (zapret lists or
/// hardcoded suffixes). Paths are relative to the install root.
const SINGBOX_RULE_SET_SOURCES: &[(&str, RuleSetSource)] = &[
    (
        "geosite:lumen-exclude",
        RuleSetSource {
            domains: &["zapret/lists/list-exclude.txt"],
            domain_suffix: &[],
            ips: &[],
        },
    ),
    (
        "geoip:lumen-exclude",
        RuleSetSource {
            domains: &[],
            domain_suffix: &[],
            ips: &["zapret/lists/ipset-exclude.txt"],
        },
    ),
    (
        "geosite:category-ru",
        RuleSetSource {
            domains: &[],
            domain_suffix: &["ru", "su", "рф"],
            ips: &[],
        },
    ),
    (
        "geosite:category-media-ru-blocked",
        RuleSetSource {
            domains: &[
                "zapret/lists/discord.txt",
                "zapret/lists/google.txt",
                "zapret/lists/list-general.txt",
                "zapret/lists/list-google.txt",
                "zapret/lists/russia-discord.txt",
                "zapret/lists/russia-youtube.txt",
                "zapret/lists/youtube.txt",
            ],
            domain_suffix: &[],
            ips: &[],
        },
    ),
    (
        "geosite:ru-blocked",
        RuleSetSource {
            domains: &["zapret/lists/list-general.txt", "zapret/lists/list-google.txt"],
            domain_suffix: &[],
            ips: &[],
        },
    ),
    (
        "geoip:ru",
        RuleSetSource {
            domains: &[],
            domain_suffix: &[],
            ips: &["zapret/lists/ipset-ru.txt"],
        },
    ),
    (
        "geoip:ru-blocked",
        RuleSetSource {
            domains: &[],
            domain_suffix: &[],
            ips: &[
                "zapret/lists/ipset-all.txt",
                "zapret/lists/ipset-zapretkvn.txt",
                "zapret/lists/russia-discord-ipset.txt",
            ],
        },
    ),
    (
        "geoip:ru-blocked-community",
        RuleSetSource {
            domains: &[],
            domain_suffix: &[],
            ips: &[
                "zapret/lists/ipset-all.txt",
                "zapret/lists/ipset-zapretkvn.txt",
                "zapret/lists/russia-discord-ipset.txt",
            ],
        },
    ),
];

const BROWSER_DOH_DOMAIN_SUFFIXES: [&str; 13] = [
    "cloudflare-dns.com",
    "dns.adguard.com",
    "dns.alidns.com",
    "dns.cloudflare.com",
    "dns.google",
    "dns.nextdns.io",
    "dns.quad9.net",
    "doh.dns.sb",
    "doh.opendns.com",
    "doh.pub",
    "doh.sb",
    "mozilla.cloudflare-dns.com",
    "one.one.one.one",
];

const XRAY_UNSUPPORTED_GEOIP_CODES: [&str; 3] =
    ["geoip:lumen-exclude", "geoip:ru-blocked", "geoip:ru-blocked-community"];
const XRAY_UNSUPPORTED_GEOSITE_CODES: [&str; 1] = ["geosite:lumen-exclude"];

const ACTIONS: [&str; 3] = ["direct", "proxy", "block"];

// ── Rule-set key/tag bookkeeping ────────────────────────────────────────

fn is_known_rule_set_key(value: &str) -> bool {
    SINGBOX_RULE_SET_SOURCES.iter().any(|(key, _)| *key == value)
        || is_singbox_binary_rule_set(value)
}

fn is_managed_rule_set_tag(tag: &str) -> bool {
    managed_tag_to_key(tag).is_some()
}

/// `tag.replace(":", "-")` inverted over the known key set.
fn managed_tag_to_key(tag: &str) -> Option<String> {
    let key = if let Some(rest) = tag.strip_prefix("geosite-") {
        format!("geosite:{rest}")
    } else if let Some(rest) = tag.strip_prefix("geoip-") {
        format!("geoip:{rest}")
    } else {
        return None;
    };
    if is_known_rule_set_key(&key) {
        Some(key)
    } else {
        None
    }
}

fn rule_set_has_managed_tag(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => is_managed_rule_set_tag(s),
        Some(Value::Array(items)) => items
            .iter()
            .any(|item| is_managed_rule_set_tag(&value_str_or_empty(Some(item)))),
        _ => false,
    }
}

fn singbox_local_rule_set_dir() -> PathBuf {
    data_dir().join("runtime").join("sing-box-rule-sets")
}

// ── Small Value helpers (Python semantics) ──────────────────────────────

/// Python `str(x or "")` for scalars: `None`/`False`/`0` become empty.
fn value_str_or_empty(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => {
            if n.as_f64().map(|f| f == 0.0).unwrap_or(false) {
                String::new()
            } else {
                n.to_string()
            }
        }
        Some(Value::Bool(true)) => "True".to_string(),
        _ => String::new(),
    }
}

/// Python truthiness (`if rule.get("inbound"):`).
fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn ensure_object<'a>(value: &'a mut Value) -> &'a mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().unwrap()
}

// ── IP/network parsing (Python `ipaddress` equivalent, strict=False) ────

pub(crate) fn ip_network_strict_false(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.contains('/') {
        return value.parse::<std::net::IpAddr>().ok().map(|ip| ip.to_string());
    }
    let (address, prefix) = value.split_once('/')?;
    let ip = address.trim().parse().ok()?;
    let prefix = parse_ip_prefix(prefix, ip)?;
    let network = mask_ip(ip, prefix);
    Some(format!("{network}/{prefix}"))
}

/// Canonical network with an explicit prefix, matching Python's
/// `ip_network(value, strict=False)` for TUN route exclusions.
pub fn canonical_ip_network(value: &str) -> Option<String> {
    let (address, prefix) = match value.trim().split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (value.trim(), None),
    };
    let (address, scope) = match address.split_once('%') {
        Some((address, scope)) if !scope.is_empty() && !scope.contains('%') => (address, scope),
        Some(_) => return None,
        None => (address, ""),
    };
    let ip: std::net::IpAddr = address.parse().ok()?;
    if !scope.is_empty() && ip.is_ipv4() {
        return None;
    }
    let prefix = match prefix {
        Some(prefix) => parse_ip_prefix(prefix, ip)?,
        None if ip.is_ipv4() => 32,
        None => 128,
    };
    let network = mask_ip(ip, prefix);
    let mut address = match network {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some() => {
            let segments = v6.segments();
            format!("::ffff:{:x}:{:x}", segments[6], segments[7])
        }
        _ => network.to_string(),
    };
    if network == ip && !scope.is_empty() {
        address.push('%');
        address.push_str(scope);
    }
    Some(format!("{address}/{prefix}"))
}

fn mask_ip(ip: std::net::IpAddr, prefix: u32) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
            std::net::Ipv4Addr::from(u32::from(v4) & mask).into()
        }
        std::net::IpAddr::V6(v6) => {
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
            std::net::Ipv6Addr::from(u128::from(v6) & mask).into()
        }
    }
}

fn parse_ip_prefix(prefix_part: &str, ip: std::net::IpAddr) -> Option<u32> {
    let max = match ip {
        std::net::IpAddr::V4(_) => 32u32,
        std::net::IpAddr::V6(_) => 128u32,
    };
    if !prefix_part.is_empty() && prefix_part.chars().all(|c| c.is_ascii_digit()) {
        let n: u32 = prefix_part.parse().ok()?;
        return if n <= max { Some(n) } else { None };
    }
    if matches!(ip, std::net::IpAddr::V4(_)) && prefix_part.contains('.') {
        let mask: std::net::Ipv4Addr = prefix_part.parse().ok()?;
        let bits = u32::from(mask);
        // Netmask ("255.255.255.0") or hostmask ("0.0.0.255") interpretation.
        let ones = bits.leading_ones();
        if ones <= 32 && bits == if ones == 0 { 0 } else { u32::MAX << (32 - ones) } {
            return Some(ones);
        }
        let trailing = bits.trailing_ones();
        if trailing == 0 {
            return if bits == 0 { Some(32) } else { None };
        }
        if trailing <= 32 && bits == u32::MAX >> (32 - trailing) {
            return Some(32 - trailing);
        }
    }
    None
}

// ── Zapret list readers ─────────────────────────────────────────────────

/// Zapret lists live at the install root (`DATA_DIR.parent` in Python).
fn zapret_base() -> &'static Path {
    base_dir()
}

fn read_domain_lists(paths: &[&str]) -> Vec<String> {
    read_domain_lists_in(zapret_base(), paths)
}

fn read_ip_lists(paths: &[&str]) -> Vec<String> {
    read_ip_lists_in(zapret_base(), paths)
}

fn read_domain_lists_in(base: &Path, paths: &[&str]) -> Vec<String> {
    let mut values: BTreeSet<String> = BTreeSet::new();
    for relative in paths {
        let path = base.join(relative);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read(&path) else {
            continue;
        };
        for line in String::from_utf8_lossy(&content).lines() {
            let value = normalize_list_entry(line);
            if value.is_empty() {
                continue;
            }
            if ip_network_strict_false(&value).is_some() {
                continue;
            }
            values.insert(value.trim_start_matches('.').to_lowercase());
        }
    }
    values.into_iter().collect()
}

fn read_ip_lists_in(base: &Path, paths: &[&str]) -> Vec<String> {
    let mut values: BTreeSet<String> = BTreeSet::new();
    for relative in paths {
        let path = base.join(relative);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read(&path) else {
            continue;
        };
        for line in String::from_utf8_lossy(&content).lines() {
            let value = normalize_list_entry(line);
            if value.is_empty() {
                continue;
            }
            if let Some(network) = canonical_ip_network(&value) {
                values.insert(network);
            }
        }
    }
    values.into_iter().collect()
}

/// `_normalize_list_entry`: strips comments, `domain:`-style prefixes,
/// adblock `||` markers and stray `|^,;` characters.
fn normalize_list_entry(line: &str) -> String {
    let mut value = line
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(line)
        .trim()
        .to_string();
    if value.is_empty() {
        return String::new();
    }
    for prefix in ["domain:", "full:", "keyword:", "regexp:", "ip:"] {
        if value.starts_with(prefix) {
            value = value[prefix.len()..].trim().to_string();
            break;
        }
    }
    if let Some(stripped) = value.strip_prefix("||") {
        value = stripped.to_string();
    }
    value
        .trim_matches(|c| c == '|' || c == '^' || c == ',' || c == ';' || c == ' ')
        .to_string()
}

// ── Xray expansion of unsupported geo codes ─────────────────────────────

fn expansion_cache() -> &'static Mutex<HashMap<String, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rule_set_source(value: &str) -> Option<&'static RuleSetSource> {
    SINGBOX_RULE_SET_SOURCES
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, source)| source)
}

/// `_expand_xray_unsupported_geoip` with the module-level memo cache.
fn expand_xray_unsupported_geoip(value: &str) -> Vec<String> {
    let mut cache = expansion_cache().lock();
    if let Some(hit) = cache.get(value) {
        return hit.clone();
    }
    let expanded = rule_set_source(value)
        .map(|source| read_ip_lists(source.ips))
        .unwrap_or_default();
    cache.insert(value.to_string(), expanded.clone());
    expanded
}

/// `_expand_xray_local_geosite` with the module-level memo cache.
fn expand_xray_local_geosite(value: &str) -> Vec<String> {
    let mut cache = expansion_cache().lock();
    if let Some(hit) = cache.get(value) {
        return hit.clone();
    }
    let expanded = match rule_set_source(value) {
        Some(source) => {
            let domains = read_domain_lists(source.domains);
            let suffixes: BTreeSet<String> = source
                .domain_suffix
                .iter()
                .map(|item| item.trim().trim_start_matches('.').to_lowercase())
                .filter(|item| !item.is_empty())
                .collect();
            let mut all: BTreeSet<String> = domains.into_iter().collect();
            all.extend(suffixes);
            all.into_iter().map(|item| format!("domain:{item}")).collect()
        }
        None => Vec::new(),
    };
    cache.insert(value.to_string(), expanded.clone());
    expanded
}

// ── Final outbound resolution ───────────────────────────────────────────

/// `_routing_final_outbound(routing, use_rule_default)`.
fn routing_final_outbound(routing: &Value, use_rule_default: bool) -> String {
    let mode = str_field(routing, "mode");
    if mode == ROUTING_GLOBAL {
        return "proxy".into();
    }
    if mode == ROUTING_DIRECT {
        return "direct".into();
    }
    if !use_rule_default {
        return "direct".into();
    }
    let default_outbound = str_field(routing, "tun_default_outbound").trim().to_lowercase();
    if default_outbound == "proxy" || default_outbound == "direct" {
        default_outbound
    } else {
        "proxy".into()
    }
}

/// Return an explicit service route or the route inherited from the preset.
pub fn effective_service_action(routing: &Value, service_id: &str) -> String {
    let explicit = value_str_or_empty(
        routing.get("service_routes").and_then(|routes| routes.get(service_id)),
    )
    .trim()
    .to_lowercase();
    if explicit == "proxy" || explicit == "direct" {
        return explicit;
    }
    let preset_id = str_field(routing, "preset_id").trim().to_lowercase();
    if preset_id == ROUTING_PRESET_BLOCKED || preset_id == ROUTING_PRESET_BLOCKED_CN {
        if let Some(preset) = service_preset_by_id(service_id) {
            if preset.default_action == "proxy" || preset.default_action == "direct" {
                return preset.default_action.into();
            }
        }
    }
    routing_final_outbound(routing, true)
}

/// Return the user-selected service override, not its resolved fallback.
///
/// The UI must distinguish an explicit service route from traffic that simply
/// inherits the selected preset/default route. Otherwise changing the
/// unmatched-traffic fallback makes every inherited service look as though an
/// individual rule was changed.
pub fn service_route_selection(routing: &Value, service_id: &str) -> String {
    let explicit = value_str_or_empty(
        routing.get("service_routes").and_then(|routes| routes.get(service_id)),
    )
    .trim()
    .to_lowercase();
    if explicit == "proxy" || explicit == "direct" {
        explicit
    } else {
        "default".into()
    }
}

// ── Xray rule helpers ───────────────────────────────────────────────────

/// `split_xray_domain_ip`: split mixed domain/IP items into Xray `domain`
/// and `ip` matcher lists, expanding unsupported local geo codes from the
/// zapret lists.
pub fn split_xray_domain_ip(items: &[String]) -> (Vec<String>, Vec<String>) {
    let mut domains: Vec<String> = Vec::new();
    let mut ips: Vec<String> = Vec::new();
    for raw in items {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let lowered = value.to_lowercase();
        if XRAY_UNSUPPORTED_GEOSITE_CODES.contains(&lowered.as_str()) {
            domains.extend(expand_xray_local_geosite(&lowered));
            continue;
        }
        if value.starts_with("domain:")
            || value.starts_with("full:")
            || value.starts_with("regexp:")
            || value.starts_with("keyword:")
            || value.starts_with("geosite:")
            || value.starts_with("ext:")
        {
            domains.push(value.to_string());
            continue;
        }
        if value.starts_with("geoip:") || value.starts_with("ip:") {
            if XRAY_UNSUPPORTED_GEOIP_CODES.contains(&lowered.as_str()) {
                ips.extend(expand_xray_unsupported_geoip(&lowered));
                continue;
            }
            ips.push(value.to_string());
            continue;
        }
        if ip_network_strict_false(value).is_some() {
            ips.push(value.to_string());
        } else {
            domains.push(format!("domain:{value}"));
        }
    }
    (domains, ips)
}

/// `append_xray_domain_ip_rule`.
pub fn append_xray_domain_ip_rule(rules: &mut Vec<Value>, items: &[String], outbound_tag: &str) {
    let (domains, ips) = split_xray_domain_ip(items);
    if !domains.is_empty() {
        rules.push(json!({
            "type": "field",
            "domain": sorted_unique(&domains),
            "outboundTag": outbound_tag,
        }));
    }
    if !ips.is_empty() {
        rules.push(json!({
            "type": "field",
            "ip": sorted_unique(&ips),
            "outboundTag": outbound_tag,
        }));
    }
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    values.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

/// `resolve_xray_process_name`: Xray only matches process file names, so
/// path-shaped entries collapse to `ntpath.basename`.
pub fn resolve_xray_process_name(rule: &Value) -> String {
    let value = str_field(rule, "process").trim().to_string();
    if value.is_empty() {
        return String::new();
    }
    let match_mode = str_field(rule, "match").trim().to_lowercase();
    if match_mode == "path_regex" {
        return String::new();
    }
    let looks_like_path = match_mode == "path"
        || value.contains('\\')
        || value.contains('/')
        || (value.len() > 1 && value.as_bytes()[1] == b':');
    if looks_like_path {
        nt_basename(&value)
    } else {
        value
    }
}

/// `ntpath.basename`: both separators split; `C:` alone has an empty
/// basename while `C:foo` is drive-relative.
fn nt_basename(value: &str) -> String {
    if let Some(idx) = value.rfind(|c| c == '\\' || c == '/') {
        return value[idx + 1..].to_string();
    }
    if value.len() > 1 && value.as_bytes()[1] == b':' {
        return value[2..].to_string();
    }
    value.to_string()
}

/// `append_xray_process_rule`.
pub fn append_xray_process_rule(rules: &mut Vec<Value>, processes: &[String], action: &str) {
    let names: Vec<String> = processes
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if names.is_empty() {
        return;
    }
    let outbound = if ACTIONS.contains(&action) { action } else { "direct" };
    rules.push(json!({
        "type": "field",
        "process": names,
        "network": "tcp,udp",
        "outboundTag": outbound,
    }));
}

/// `collect_service_route_domains`: per-service domain lists grouped by
/// action. A missing per-service override inherits the built-in blocked
/// preset's catalog action. In every other preset it remains unmatched and
/// uses route.final instead of becoming a synthetic explicit service rule.
pub fn collect_service_route_domains(routing: &Value) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut service_direct: Vec<String> = Vec::new();
    let mut service_proxy: Vec<String> = Vec::new();
    let service_block: Vec<String> = Vec::new();
    let preset_id = str_field(routing, "preset_id").trim().to_lowercase();
    let blocked_preset =
        preset_id == ROUTING_PRESET_BLOCKED || preset_id == ROUTING_PRESET_BLOCKED_CN;
    let service_routes = routing.get("service_routes");
    for preset in crate::presets::SERVICE_PRESETS {
        let raw_action = service_routes.and_then(|routes| routes.get(preset.id));
        let mut action = value_str_or_empty(raw_action).trim().to_lowercase();
        if raw_action.map(Value::is_null).unwrap_or(true) && blocked_preset {
            action = preset.default_action.to_string();
        }
        if action != "direct" && action != "proxy" {
            continue;
        }
        if action == "direct" {
            service_direct.extend(preset.domains.iter().map(|d| d.to_string()));
        } else {
            service_proxy.extend(preset.domains.iter().map(|d| d.to_string()));
        }
    }
    (service_direct, service_proxy, service_block)
}

fn action_bucket(action: &str) -> Option<usize> {
    ACTIONS.iter().position(|a| *a == action)
}

/// `build_xray_gui_routing_rules`.
pub fn build_xray_gui_routing_rules(routing: &Value, settings: &Value) -> Vec<Value> {
    let mut rules: Vec<Value> = Vec::new();
    let tun_mode = bool_field(settings, "tun_mode", false);
    // Auto/sing-box TUN routes applications in sing-box; Xray sees them itself
    // in system-proxy mode and when it owns the tunnel (main core Xray).
    if !tun_mode || str_field(settings, "core_preference") == "xray" {
        let mut preset_processes: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        if let Some(routes) = routing.get("process_preset_routes").and_then(Value::as_object) {
            for (preset_id, action_value) in routes {
                let action = value_str_or_empty(Some(action_value));
                if let Some(preset) = process_preset_by_id(preset_id) {
                    if let Some(bucket) = action_bucket(&action) {
                        preset_processes[bucket]
                            .extend(preset.processes.iter().map(|p| p.to_string()));
                    }
                }
            }
        }
        for (bucket, action) in preset_processes.iter().zip(ACTIONS.iter()) {
            append_xray_process_rule(&mut rules, bucket, action);
        }

        let mut manual_processes: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for pr in obj_list(routing, "process_rules") {
            let name = resolve_xray_process_name(&pr);
            let action = str_or(&pr, "action", "direct");
            if !name.is_empty() {
                if let Some(bucket) = action_bucket(&action) {
                    manual_processes[bucket].push(name);
                }
            }
        }
        for (bucket, action) in manual_processes.iter().zip(ACTIONS.iter()) {
            append_xray_process_rule(&mut rules, bucket, action);
        }
    }

    let (service_direct, service_proxy, service_block) = collect_service_route_domains(routing);
    let (preset_direct, preset_proxy) = preset_domain_rules(&str_field(routing, "preset_id"));

    append_xray_domain_ip_rule(&mut rules, &service_direct, "direct");
    append_xray_domain_ip_rule(
        &mut rules,
        &custom_domain_rules(&str_list(routing, "direct_domains")),
        "direct",
    );
    append_xray_domain_ip_rule(&mut rules, &service_block, "block");
    append_xray_domain_ip_rule(
        &mut rules,
        &custom_domain_rules(&str_list(routing, "block_domains")),
        "block",
    );
    append_xray_domain_ip_rule(&mut rules, &service_proxy, "proxy");
    append_xray_domain_ip_rule(
        &mut rules,
        &custom_domain_rules(&str_list(routing, "proxy_domains")),
        "proxy",
    );
    append_xray_domain_ip_rule(&mut rules, &preset_direct, "direct");
    append_xray_domain_ip_rule(&mut rules, &preset_proxy, "proxy");

    // Match sing-box priority: explicit process/domain/service choices win over
    // the optional LAN bypass, and the final catch-all remains last.
    if bool_field(routing, "bypass_lan", true) {
        rules.push(json!({
            "type": "field",
            "ip": ["geoip:private"],
            "outboundTag": "direct",
        }));
        rules.push(json!({
            "type": "field",
            "domain": ["geosite:private"],
            "outboundTag": "direct",
        }));
    }

    rules.push(json!({
        "type": "field",
        "network": "tcp,udp",
        "outboundTag": routing_final_outbound(routing, tun_mode),
    }));
    rules
}

/// `apply_xray_gui_routing`: splice the GUI block into `payload.routing.rules`
/// after stripping rules left by older builds.
pub fn apply_xray_gui_routing(payload: &mut Value, routing: &Value, settings: &Value) {
    if !payload.is_object() {
        *payload = json!({});
    }
    let local_tags = xray_local_proxy_inbound_tags(payload);

    let payload_obj = payload.as_object_mut().unwrap();
    if !payload_obj.get("routing").map(Value::is_object).unwrap_or(false) {
        payload_obj.insert("routing".into(), json!({}));
    }
    let route = ensure_object(payload_obj.get_mut("routing").unwrap());
    if !route.get("rules").map(Value::is_array).unwrap_or(false) {
        route.insert("rules".into(), json!([]));
    }
    let rules = route.get_mut("rules").unwrap().as_array_mut().unwrap();
    rules.retain(|rule| !is_legacy_lumen_xray_route_rule(rule));
    let insert_at = xray_runtime_rule_insert_index(rules);
    let mut gui_rules = build_xray_gui_routing_rules(routing, settings);
    if !local_tags.is_empty() && !gui_rules.is_empty() {
        // Same contract as sing-box: an app that chose the local proxy wants
        // the proxy, once the explicit rules (RU direct, services, LAN, ...)
        // did not match.  The catch-all below then covers TUN traffic only.
        let position = gui_rules.len() - 1;
        gui_rules.insert(
            position,
            json!({"type": "field", "inboundTag": local_tags, "outboundTag": "proxy"}),
        );
    }
    let tail = rules.split_off(insert_at);
    rules.extend(gui_rules);
    rules.extend(tail);
}

fn xray_local_proxy_inbound_tags(payload: &Value) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let empty: Vec<Value> = Vec::new();
    for inbound in payload.get("inbounds").and_then(Value::as_array).unwrap_or(&empty) {
        let tag_value = value_str_or_empty(inbound.get("tag"));
        let tag = tag_value.trim();
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        if tag.is_empty()
            || tag.starts_with("__app_")
            || tag == "api"
            || tag == "discord-socks-in"
        {
            continue;
        }
        if protocol == "mixed" || protocol == "socks" || protocol == "http" {
            tags.push(tag.to_string());
        }
    }
    tags
}

fn is_legacy_lumen_xray_route_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    let outbound = value_str_or_empty(obj.get("outboundTag"));
    let domains: BTreeSet<String> = obj
        .get("domain")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|v| value_str_or_empty(Some(v))).collect())
        .unwrap_or_default();
    let ips: BTreeSet<String> = obj
        .get("ip")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|v| value_str_or_empty(Some(v))).collect())
        .unwrap_or_default();
    let processes: BTreeSet<String> = obj
        .get("process")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|v| value_str_or_empty(Some(v)).to_lowercase()).collect())
        .unwrap_or_default();
    let network = value_str_or_empty(obj.get("network"));
    let port = value_str_or_empty(obj.get("port"));

    if outbound == "block" && network == "udp" && port == "443" {
        return true;
    }
    if outbound == "direct"
        && (ips == BTreeSet::from(["geoip:private".to_string()])
            || domains == BTreeSet::from(["geosite:private".to_string()]))
    {
        return true;
    }
    let regional_domains: BTreeSet<&str> = BTreeSet::from([
        "geosite:category-ru",
        "geosite:ru-available-only-inside",
        "geosite:ru-blocked",
        "geosite:cn",
        "geosite:gfw",
        "geosite:greatfire",
        "geosite:google",
        "geosite:ir",
    ]);
    let regional_ips: BTreeSet<&str> = BTreeSet::from([
        "geoip:ru",
        "geoip:ru-blocked",
        "geoip:ru-blocked-community",
        "geoip:cn",
        "geoip:facebook",
        "geoip:fastly",
        "geoip:google",
        "geoip:netflix",
        "geoip:telegram",
        "geoip:twitter",
        "geoip:ir",
    ]);
    let domains_regional = !domains.is_empty() && domains.iter().all(|d| regional_domains.contains(d.as_str()));
    let ips_regional = !ips.is_empty() && ips.iter().all(|i| regional_ips.contains(i.as_str()));
    if (outbound == "direct" || outbound == "proxy") && (domains_regional || ips_regional) {
        return true;
    }
    if outbound == "proxy"
        && network == "tcp,udp"
        && domains.is_empty()
        && ips.is_empty()
        && processes.is_empty()
    {
        return true;
    }
    let legacy_domains: BTreeSet<&str> = BTreeSet::from([
        r"regexp:(^|\\.)facebook\.com$",
        r"regexp:(^|\\.)fbcdn\.net$",
        r"regexp:(^|\\.)instagram\.com$",
        "full:ntc.party",
        "full:rutracker.org",
        "full:static.rutracker.cc",
    ]);
    if outbound == "proxy"
        && domains.len() == legacy_domains.len()
        && domains.iter().all(|d| legacy_domains.contains(d.as_str()))
    {
        return true;
    }
    let legacy_proxy_processes: BTreeSet<&str> = BTreeSet::from([
        "telegram.exe",
        "ayugram.exe",
        "discord.exe",
        "vesktop.exe",
        "spotify.exe",
        "chrome.exe",
        "firefox.exe",
        "waterfox.exe",
        "librewolf.exe",
        "msedge.exe",
        "opera.exe",
        "brave.exe",
        "vivaldi.exe",
        "browser.exe",
    ]);
    outbound == "proxy"
        && !processes.is_empty()
        && processes.iter().all(|p| legacy_proxy_processes.contains(p.as_str()))
}

fn xray_runtime_rule_insert_index(rules: &[Value]) -> usize {
    let mut index = 0;
    while index < rules.len() {
        let rule = &rules[index];
        let Some(inbound) = rule.get("inboundTag").and_then(Value::as_array) else {
            break;
        };
        let inbound_text: BTreeSet<String> =
            inbound.iter().map(|item| value_str_or_empty(Some(item))).collect();
        if inbound_text
            .iter()
            .any(|tag| tag.starts_with("__app_") || tag == "api" || tag == "discord-socks-in")
        {
            index += 1;
            continue;
        }
        break;
    }
    index
}

// ── sing-box sentinels ──────────────────────────────────────────────────

fn subscription_fetcher_direct_rule() -> Value {
    json!({
        "process_name": [SUBSCRIPTION_FETCHER_EXE_NAME],
        "action": "route",
        "outbound": "direct",
    })
}

/// Valid but inert sentinel delimiting the app-owned GUI route block.
fn lumen_singbox_route_end_rule() -> Value {
    json!({
        "process_name": ["__lumen_gui_route_end__.exe"],
        "action": "route",
        "outbound": "direct",
    })
}

/// Valid but inert sentinel delimiting the app-owned GUI DNS block.
fn lumen_singbox_dns_start_rule() -> Value {
    json!({"domain": ["__lumen_gui_dns_start__.invalid"], "action": "reject"})
}

fn lumen_singbox_dns_end_rule() -> Value {
    json!({"domain": ["__lumen_gui_dns_end__.invalid"], "action": "reject"})
}

/// Remove complete app-owned blocks while preserving imported look-alike rules.
fn strip_marked_singbox_rule_block(
    rules: &[Value],
    start: &dyn Fn(&Value) -> bool,
    end: &dyn Fn(&Value) -> bool,
) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::new();
    let mut index = 0;
    while index < rules.len() {
        if !start(&rules[index]) {
            result.push(rules[index].clone());
            index += 1;
            continue;
        }
        let mut end_index = index + 1;
        while end_index < rules.len() && !end(&rules[end_index]) {
            end_index += 1;
        }
        if end_index >= rules.len() {
            // Legacy configs have no end sentinel. Remove only the known start
            // marker; imported rules following it must remain untouched.
            index += 1;
            continue;
        }
        index = end_index + 1;
    }
    result
}

fn is_subscription_fetcher_direct_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    let names: BTreeSet<String> = obj
        .get("process_name")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|value| value_str_or_empty(Some(value)).trim().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    names.len() == 1
        && names.contains(SUBSCRIPTION_FETCHER_EXE_NAME.to_lowercase().as_str())
        && obj.get("action") == Some(&json!("route"))
        && obj.get("outbound") == Some(&json!("direct"))
}

fn is_lumen_singbox_route_end_rule(rule: &Value) -> bool {
    *rule == lumen_singbox_route_end_rule()
}

fn is_lumen_singbox_dns_start_rule(rule: &Value) -> bool {
    *rule == lumen_singbox_dns_start_rule()
}

fn is_lumen_singbox_dns_end_rule(rule: &Value) -> bool {
    *rule == lumen_singbox_dns_end_rule()
}

fn is_legacy_lumen_singbox_route_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    let is_udp = match obj.get("network") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| value_str_or_empty(Some(item)).to_lowercase())
            .collect::<BTreeSet<_>>()
            == BTreeSet::from(["udp".to_string()]),
        other => value_str_or_empty(other).to_lowercase() == "udp",
    };
    let is_443 = match obj.get("port") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| value_str_or_empty(Some(item)))
            .collect::<BTreeSet<_>>()
            == BTreeSet::from(["443".to_string()]),
        other => value_str_or_empty(other) == "443",
    };
    let outbound = value_str_or_empty(obj.get("outbound"));
    if is_udp && is_443 && outbound == "block" {
        return true;
    }
    if outbound != "direct" && outbound != "proxy" && outbound != "block" {
        return false;
    }
    if rule_set_has_managed_tag(obj.get("rule_set")) {
        return true;
    }
    obj.get("ip_is_private") == Some(&json!(true)) && outbound == "direct"
}

fn is_lumen_singbox_dns_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    if is_https_dns_reject_rule(rule) {
        return true;
    }
    if is_browser_doh_dns_reject_rule(rule) {
        return true;
    }
    if value_str_or_empty(obj.get("server")) == "fake-dns" {
        let query_types: BTreeSet<String> = obj
            .get("query_type")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|item| value_str_or_empty(Some(item))).collect())
            .unwrap_or_default();
        if query_types == BTreeSet::from(["A".to_string(), "AAAA".to_string()])
            && obj
                .keys()
                .all(|key| ["query_type", "server", "rewrite_ttl", "action"].contains(&key.as_str()))
        {
            return true;
        }
    }
    let action = value_str_or_empty(obj.get("action"));
    if action != "route" && action != "reject" {
        return false;
    }
    let server = value_str_or_empty(obj.get("server"));
    if action == "route" && server != "bootstrap-dns" && server != "proxy-dns" {
        return false;
    }
    rule_set_has_managed_tag(obj.get("rule_set"))
}

fn is_browser_doh_dns_reject_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    if obj.get("action") != Some(&json!("reject")) {
        return false;
    }
    let Some(suffixes) = obj.get("domain_suffix").and_then(Value::as_array) else {
        return false;
    };
    let suffix_list: Vec<String> = suffixes.iter().map(|item| value_str_or_empty(Some(item))).collect();
    suffix_list == BROWSER_DOH_DOMAIN_SUFFIXES
}

fn is_https_dns_reject_rule(rule: &Value) -> bool {
    let Some(obj) = rule.as_object() else {
        return false;
    };
    if obj.get("action") != Some(&json!("reject")) {
        return false;
    }
    let Some(query_type) = obj.get("query_type").and_then(Value::as_array) else {
        return false;
    };
    query_type
        .iter()
        .map(|item| value_str_or_empty(Some(item)).trim().to_uppercase())
        .collect::<BTreeSet<_>>()
        == BTreeSet::from(["HTTPS".to_string(), "SVCB".to_string()])
}

fn is_singbox_hosts_dns_rule(rule: &Value) -> bool {
    rule.as_object()
        .map(|obj| {
            obj.get("ip_accept_any") == Some(&json!(true))
                && value_str_or_empty(obj.get("server")) == "hosts-dns"
        })
        .unwrap_or(false)
}

/// `_singbox_dns_strategy`.
fn singbox_dns_strategy(value: &str) -> String {
    let strategy = value.trim().to_lowercase().replace('-', "_");
    if ["prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"].contains(&strategy.as_str()) {
        strategy
    } else {
        "prefer_ipv4".into()
    }
}

/// `routing_with_ip_preference`: pin both DNS strategies to the preferred
/// family unless the user picked an *only* mode.
pub fn routing_with_ip_preference(routing: &Value, prefer_ipv6: bool) -> Value {
    let mut effective = routing.clone();
    let preferred = if prefer_ipv6 { "prefer_ipv6" } else { "prefer_ipv4" };
    for key in ["dns_bootstrap_strategy", "dns_proxy_strategy"] {
        let current = singbox_dns_strategy(&str_field(&effective, key));
        if current == "prefer_ipv4" || current == "prefer_ipv6" {
            if let Some(obj) = effective.as_object_mut() {
                obj.insert(key.into(), json!(preferred));
            }
        }
    }
    effective
}

// ── sing-box rule builders ──────────────────────────────────────────────

fn append_singbox_process_rule(
    rules: &mut Vec<Value>,
    values: &[String],
    action: &str,
    key: &str,
) {
    let names: Vec<String> = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if names.is_empty() {
        return;
    }
    let outbound = if ACTIONS.contains(&action) { action } else { "direct" };
    rules.push(json!({ (key): names, "action": "route", "outbound": outbound }));
}

/// `build_singbox_gui_route_rules`: route rules plus the managed rule-set
/// tags they reference.
pub fn build_singbox_gui_route_rules(routing: &Value) -> (Vec<Value>, BTreeSet<String>) {
    let mut rules: Vec<Value> = Vec::new();
    let mut rule_sets: BTreeSet<String> = BTreeSet::new();

    let mut preset_processes: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    if let Some(routes) = routing.get("process_preset_routes").and_then(Value::as_object) {
        for (preset_id, action_value) in routes {
            let action = value_str_or_empty(Some(action_value));
            if let Some(preset) = process_preset_by_id(preset_id) {
                if let Some(bucket) = action_bucket(&action) {
                    preset_processes[bucket].extend(preset.processes.iter().map(|p| p.to_string()));
                }
            }
        }
    }
    for (bucket, action) in preset_processes.iter().zip(ACTIONS.iter()) {
        append_singbox_process_rule(&mut rules, bucket, action, "process_name");
    }

    let mut manual_names: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut manual_paths: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut manual_regex: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for pr in obj_list(routing, "process_rules") {
        let action = str_or(&pr, "action", "direct");
        let Some(bucket) = action_bucket(&action) else {
            continue;
        };
        let value = str_field(&pr, "process").trim().to_string();
        if value.is_empty() {
            continue;
        }
        let match_mode = str_field(&pr, "match").trim().to_lowercase();
        if match_mode == "path_regex" {
            manual_regex[bucket].push(value);
        } else if match_mode == "path"
            || value.contains('\\')
            || value.contains('/')
            || (value.len() > 1 && value.as_bytes()[1] == b':')
        {
            manual_paths[bucket].push(value);
        } else {
            manual_names[bucket].push(value);
        }
    }
    for bucket in 0..3 {
        append_singbox_process_rule(&mut rules, &manual_names[bucket], ACTIONS[bucket], "process_name");
        append_singbox_process_rule(&mut rules, &manual_paths[bucket], ACTIONS[bucket], "process_path");
        append_singbox_process_rule(
            &mut rules,
            &manual_regex[bucket],
            ACTIONS[bucket],
            "process_path_regex",
        );
    }

    let (service_direct, service_proxy, service_block) = collect_service_route_domains(routing);
    let (preset_direct, preset_proxy) = preset_domain_rules(&str_field(routing, "preset_id"));

    let groups: Vec<(Vec<String>, &str)> = vec![
        (service_direct, "direct"),
        (custom_domain_rules(&str_list(routing, "direct_domains")), "direct"),
        (service_block, "block"),
        (custom_domain_rules(&str_list(routing, "block_domains")), "block"),
        (service_proxy, "proxy"),
        (custom_domain_rules(&str_list(routing, "proxy_domains")), "proxy"),
        (preset_direct, "direct"),
        (preset_proxy, "proxy"),
    ];
    for (items, outbound) in groups {
        if let Some((rule, used_sets)) = singbox_domain_ip_rule(&items, outbound) {
            rules.push(rule);
            rule_sets.extend(used_sets);
        }
    }

    if bool_field(routing, "dns_fake_enabled", false) {
        rules.push(json!({
            "ip_cidr": ["198.18.0.0/15", "fc00::/18"],
            "action": "route",
            "outbound": routing_final_outbound(routing, true),
        }));
    }

    if bool_field(routing, "bypass_lan", true) {
        rules.push(json!({"ip_is_private": true, "action": "route", "outbound": "direct"}));
    }
    (rules, rule_sets)
}

/// `build_singbox_gui_dns_rules`: DNS rules plus the managed rule-set tags
/// they reference.
pub fn build_singbox_gui_dns_rules(routing: &Value) -> (Vec<Value>, BTreeSet<String>) {
    let mut rules: Vec<Value> = Vec::new();
    let mut rule_sets: BTreeSet<String> = BTreeSet::new();

    let (service_direct, service_proxy, service_block) = collect_service_route_domains(routing);
    let (preset_direct, preset_proxy) = if bool_field(routing, "dns_geo_check", true) {
        preset_domain_rules(&str_field(routing, "preset_id"))
    } else {
        (Vec::new(), Vec::new())
    };

    let direct_dns_action = if str_list(routing, "dns_bootstrap_servers").is_empty() {
        "proxy-dns"
    } else {
        "bootstrap-dns"
    };
    let groups: Vec<(Vec<String>, &str)> = vec![
        (service_direct, direct_dns_action),
        (custom_domain_rules(&str_list(routing, "direct_domains")), direct_dns_action),
        (service_block, "reject"),
        (custom_domain_rules(&str_list(routing, "block_domains")), "reject"),
        (service_proxy, "proxy-dns"),
        (custom_domain_rules(&str_list(routing, "proxy_domains")), "proxy-dns"),
        (preset_direct, direct_dns_action),
        (preset_proxy, "proxy-dns"),
    ];
    for (items, dns_action) in groups {
        if let Some((rule, used_sets)) = singbox_domain_dns_rule(&items, dns_action) {
            rules.push(rule);
            rule_sets.extend(used_sets);
        }
    }
    (rules, rule_sets)
}

/// `_singbox_rule_set_tag`: known managed code → dashed tag.
fn singbox_rule_set_tag(value: &str) -> String {
    if !is_known_rule_set_key(value) {
        return String::new();
    }
    value.replace(':', "-")
}

/// `_singbox_dns_rule_set_tag`: only domain-based rule-sets apply to DNS.
fn singbox_dns_rule_set_tag(value: &str) -> String {
    if is_singbox_binary_rule_set(value) {
        return if value.starts_with("geosite:") {
            value.replace(':', "-")
        } else {
            String::new()
        };
    }
    let Some(source) = rule_set_source(value) else {
        return String::new();
    };
    if !source.domains.is_empty() || !source.domain_suffix.is_empty() {
        value.replace(':', "-")
    } else {
        String::new()
    }
}

/// `_singbox_domain_ip_rule`.
fn singbox_domain_ip_rule(items: &[String], outbound: &str) -> Option<(Value, BTreeSet<String>)> {
    let mut domain_suffix: Vec<String> = Vec::new();
    let mut domain: Vec<String> = Vec::new();
    let mut domain_keyword: Vec<String> = Vec::new();
    let mut domain_regex: Vec<String> = Vec::new();
    let mut ip_cidr: Vec<String> = Vec::new();
    let mut rule_set: Vec<String> = Vec::new();

    for raw in items {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let mapped = singbox_rule_set_tag(value);
        if !mapped.is_empty() {
            rule_set.push(mapped);
            continue;
        }
        if let Some(rest) = value.strip_prefix("domain:") {
            domain_suffix.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("full:") {
            domain.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("keyword:") {
            domain_keyword.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("regexp:") {
            domain_regex.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("ip:") {
            ip_cidr.push(rest.to_string());
        } else if ip_network_strict_false(value).is_some() {
            ip_cidr.push(value.to_string());
        } else {
            domain_suffix.push(value.to_string());
        }
    }

    let mut rule = Map::new();
    if !domain_suffix.is_empty() {
        rule.insert("domain_suffix".into(), json!(sorted_unique(&domain_suffix)));
    }
    if !domain.is_empty() {
        rule.insert("domain".into(), json!(sorted_unique(&domain)));
    }
    if !domain_keyword.is_empty() {
        rule.insert("domain_keyword".into(), json!(sorted_unique(&domain_keyword)));
    }
    if !domain_regex.is_empty() {
        rule.insert("domain_regex".into(), json!(sorted_unique(&domain_regex)));
    }
    if !ip_cidr.is_empty() {
        rule.insert("ip_cidr".into(), json!(sorted_unique(&ip_cidr)));
    }
    if !rule_set.is_empty() {
        let sets = sorted_unique(&rule_set);
        rule.insert("rule_set".into(), json!(sets));
    }
    if rule.is_empty() {
        return None;
    }
    rule.insert("action".into(), json!("route"));
    rule.insert("outbound".into(), json!(outbound));
    Some((Value::Object(rule), rule_set.into_iter().collect()))
}

/// `_singbox_domain_dns_rule`.
fn singbox_domain_dns_rule(items: &[String], dns_action: &str) -> Option<(Value, BTreeSet<String>)> {
    let mut domain_suffix: Vec<String> = Vec::new();
    let mut domain: Vec<String> = Vec::new();
    let mut domain_keyword: Vec<String> = Vec::new();
    let mut domain_regex: Vec<String> = Vec::new();
    let mut rule_set: Vec<String> = Vec::new();

    for raw in items {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let mapped = singbox_dns_rule_set_tag(value);
        if !mapped.is_empty() {
            rule_set.push(mapped);
            continue;
        }
        if let Some(rest) = value.strip_prefix("domain:") {
            domain_suffix.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("full:") {
            domain.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("keyword:") {
            domain_keyword.push(rest.to_string());
        } else if let Some(rest) = value.strip_prefix("regexp:") {
            domain_regex.push(rest.to_string());
        } else if value.starts_with("ip:") || value.starts_with("geoip:") {
            continue;
        } else if ip_network_strict_false(value).is_some() {
            continue;
        } else {
            domain_suffix.push(value.to_string());
        }
    }

    let mut rule = Map::new();
    if !domain_suffix.is_empty() {
        rule.insert("domain_suffix".into(), json!(sorted_unique(&domain_suffix)));
    }
    if !domain.is_empty() {
        rule.insert("domain".into(), json!(sorted_unique(&domain)));
    }
    if !domain_keyword.is_empty() {
        rule.insert("domain_keyword".into(), json!(sorted_unique(&domain_keyword)));
    }
    if !domain_regex.is_empty() {
        rule.insert("domain_regex".into(), json!(sorted_unique(&domain_regex)));
    }
    if !rule_set.is_empty() {
        let sets = sorted_unique(&rule_set);
        rule.insert("rule_set".into(), json!(sets));
    }
    if rule.is_empty() {
        return None;
    }
    if dns_action == "reject" {
        rule.insert("action".into(), json!("reject"));
    } else {
        rule.insert("action".into(), json!("route"));
        rule.insert("server".into(), json!(dns_action));
    }
    Some((Value::Object(rule), rule_set.into_iter().collect()))
}

/// `_ensure_singbox_rule_sets`: drop managed entries and re-add the ones the
/// current rules reference, resolving binary `.srs` or local JSON sources.
fn ensure_singbox_rule_sets(route: &mut Map<String, Value>, tags: &BTreeSet<String>) -> Result<()> {
    if !route.get("rule_set").map(Value::is_array).unwrap_or(false) {
        route.insert("rule_set".into(), json!([]));
    }
    let existing = route.get_mut("rule_set").unwrap().as_array_mut().unwrap();
    existing.retain(|item| {
        !(item.is_object() && is_managed_rule_set_tag(&value_str_or_empty(item.get("tag"))))
    });
    if tags.is_empty() {
        return Ok(());
    }

    let mut entries: Vec<Value> = Vec::new();
    for tag in tags {
        let Some(key) = managed_tag_to_key(tag) else {
            continue;
        };
        let binary_path = singbox_rule_set_path(&key);
        let (path, rule_format) = if binary_path.as_ref().map(|path| path.is_file()).unwrap_or(false) {
            (binary_path.unwrap(), "binary")
        } else if rule_set_source(&key).is_some() {
            (ensure_singbox_local_rule_set(&key)?, "source")
        } else {
            anyhow::bail!(
                "Не найден региональный rule-set {key}. Обновите GeoIP/GeoSite в настройках обновлений."
            );
        };
        entries.push(json!({
            "type": "local",
            "tag": tag,
            "format": rule_format,
            "path": path.to_string_lossy(),
        }));
    }
    existing.extend(entries);
    Ok(())
}

/// `_ensure_singbox_local_rule_set`: assemble and persist the local JSON
/// rule-set for a managed key (only when its content changed).
fn ensure_singbox_local_rule_set(key: &str) -> Result<PathBuf> {
    let Some(source) = rule_set_source(key) else {
        anyhow::bail!(
            "Не найден региональный rule-set {key}. Обновите GeoIP/GeoSite в настройках обновлений."
        );
    };
    let tag = key.replace(':', "-");
    let dir = singbox_local_rule_set_dir();
    let path = dir.join(format!("{tag}.json"));
    let payload = local_rule_set_payload(source, &tag, base_dir());
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string(&payload)?;
    let needs_write = match std::fs::read_to_string(&path) {
        Ok(existing) => existing != text,
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(&path, text.as_bytes())?;
    }
    Ok(path)
}

/// Pure rule-set payload assembly (`{"version": 3, "rules": [...]}`) used by
/// `ensure_singbox_local_rule_set`; `base` roots the zapret list paths.
fn local_rule_set_payload(source: &RuleSetSource, tag: &str, base: &Path) -> Value {
    let domains = read_domain_lists_in(base, source.domains);
    let domain_suffix: BTreeSet<String> = source
        .domain_suffix
        .iter()
        .map(|value| value.trim().trim_start_matches('.').to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    let ips = read_ip_lists_in(base, source.ips);

    let mut rules: Vec<Value> = Vec::new();
    if !domains.is_empty() {
        rules.push(json!({"domain_suffix": domains}));
    }
    if !domain_suffix.is_empty() {
        rules.push(json!({"domain_suffix": domain_suffix.into_iter().collect::<Vec<_>>()}));
    }
    if !ips.is_empty() {
        rules.push(json!({"ip_cidr": ips}));
    }
    if rules.is_empty() {
        rules.push(json!({"domain_suffix": [tag]}));
    }

    json!({"version": 3, "rules": rules})
}

fn singbox_runtime_rule_insert_index(rules: &[Value]) -> usize {
    let mut index = 0;
    while index < rules.len() {
        let rule = &rules[index];
        if !rule.is_object() {
            break;
        }
        let action = value_str_or_empty(rule.get("action"));
        if action == "sniff" || action == "hijack-dns" || action == "route-options" {
            index += 1;
            continue;
        }
        if is_truthy(rule.get("inbound")) {
            index += 1;
            continue;
        }
        break;
    }
    index
}

/// `apply_singbox_gui_routing`: rebuild the managed route + DNS blocks inside
/// `payload` (sentinel-delimited, so repeated application is idempotent).
pub fn apply_singbox_gui_routing(payload: &mut Value, routing: &Value) -> Result<()> {
    if !payload.is_object() {
        *payload = json!({});
    }

    let (mut gui_rules, mut route_rule_sets) = build_singbox_gui_route_rules(routing);
    let (dns_rules, dns_rule_sets) = build_singbox_gui_dns_rules(routing);
    route_rule_sets.extend(dns_rule_sets);

    let dns_is_object = payload.get("dns").map(Value::is_object).unwrap_or(false);
    let dns_tags: BTreeSet<String> = if dns_is_object {
        payload
            .get("dns")
            .and_then(|dns| dns.get("servers"))
            .and_then(Value::as_array)
            .map(|servers| {
                servers
                    .iter()
                    .filter(|server| server.is_object())
                    .map(|server| value_str_or_empty(server.get("tag")).trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };

    let payload_obj = payload.as_object_mut().unwrap();
    if !payload_obj.get("route").map(Value::is_object).unwrap_or(false) {
        payload_obj.insert("route".into(), json!({}));
    }
    let route = ensure_object(payload_obj.get_mut("route").unwrap());
    if !route.get("rules").map(Value::is_array).unwrap_or(false) {
        route.insert("rules".into(), json!([]));
    }
    {
        let rules = route.get_mut("rules").unwrap().as_array_mut().unwrap();
        *rules = strip_marked_singbox_rule_block(
            rules,
            &|rule| is_subscription_fetcher_direct_rule(rule),
            &|rule| is_lumen_singbox_route_end_rule(rule),
        );
        rules.retain(|rule| {
            !is_legacy_lumen_singbox_route_rule(rule)
                && !is_subscription_fetcher_direct_rule(rule)
                && !is_lumen_singbox_route_end_rule(rule)
        });
    }
    ensure_singbox_rule_sets(route, &route_rule_sets)?;
    {
        let rules = route.get_mut("rules").unwrap().as_array_mut().unwrap();
        let insert_at = singbox_runtime_rule_insert_index(rules);
        let mut block = Vec::with_capacity(gui_rules.len() + 2);
        block.push(subscription_fetcher_direct_rule());
        block.append(&mut gui_rules);
        block.push(lumen_singbox_route_end_rule());
        let tail = rules.split_off(insert_at);
        rules.extend(block);
        rules.extend(tail);
    }
    route.insert("final".into(), json!(routing_final_outbound(routing, true)));

    // DNS privacy is independent from the traffic fallback.  Unmatched
    // names always use the resolver whose dialer is the proxy; explicit
    // direct-domain rules may still select bootstrap-dns below.
    if dns_is_object {
        let target_dns = "proxy-dns";
        let target_strategy = singbox_dns_strategy(&str_field(routing, "dns_proxy_strategy"));
        if dns_tags.contains(target_dns) {
            route.insert("default_domain_resolver".into(), json!({
                "server": target_dns,
                "strategy": target_strategy,
            }));
        }
        let dns = payload.get_mut("dns").unwrap();
        if dns_tags.contains(target_dns) {
            dns["final"] = json!(target_dns);
        }
        if !dns.get("rules").map(Value::is_array).unwrap_or(false) {
            dns.as_object_mut().unwrap().insert("rules".into(), json!([]));
        }
        let existing = dns.get_mut("rules").unwrap().as_array_mut().unwrap();
        *existing = strip_marked_singbox_rule_block(
            existing,
            &|rule| is_lumen_singbox_dns_start_rule(rule),
            &|rule| is_lumen_singbox_dns_end_rule(rule),
        );
        existing.retain(|rule| {
            !is_lumen_singbox_dns_rule(rule)
                && !is_lumen_singbox_dns_start_rule(rule)
                && !is_lumen_singbox_dns_end_rule(rule)
        });
        if dns_tags.contains("fake-dns") {
            existing.insert(
                0,
                json!({
                    "query_type": ["A", "AAAA"],
                    "server": "fake-dns",
                    "rewrite_ttl": 1,
                }),
            );
        }
        if !dns_rules.is_empty() {
            let mut dns_block = Vec::with_capacity(dns_rules.len() + 2);
            dns_block.push(lumen_singbox_dns_start_rule());
            dns_block.extend(dns_rules);
            dns_block.push(lumen_singbox_dns_end_rule());
            let tail = existing.split_off(0);
            existing.extend(dns_block);
            existing.extend(tail);
        }
        // Keep the HTTPS/SVCB (type 64/65) reject ahead of every domain rule so
        // ECH config and HTTP/3 hints are never answered for routed domains.
        existing.retain(|rule| !is_https_dns_reject_rule(rule));
        existing.insert(0, json!({"query_type": ["HTTPS", "SVCB"], "action": "reject"}));
        existing.insert(
            1,
            json!({
                "domain_suffix": BROWSER_DOH_DOMAIN_SUFFIXES,
                "action": "reject",
            }),
        );
        // DNS rules are first-match: the user hosts override has to stay ahead of
        // the FakeIP A/AAAA rule, which would otherwise answer every query first.
        let hosts_rules: Vec<Value> = existing
            .iter()
            .filter(|rule| is_singbox_hosts_dns_rule(rule))
            .cloned()
            .collect();
        if !hosts_rules.is_empty() {
            existing.retain(|rule| !is_singbox_hosts_dns_rule(rule));
            let tail = existing.split_off(2);
            existing.extend(hosts_rules);
            existing.extend(tail);
        }
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked_routing() -> Value {
        json!({
            "mode": "rule",
            "preset_id": "blocked",
            "bypass_lan": false,
            "direct_domains": [],
            "proxy_domains": [],
            "block_domains": [],
            "process_rules": [],
            "process_preset_routes": {},
            "service_routes": {},
            "dns_bootstrap_servers": [],
            "tun_default_outbound": "direct",
        })
    }

    #[test]
    fn xray_blocked_preset_proxies_ru_blocked() {
        let routing = blocked_routing();
        let settings = json!({"tun_mode": false, "core_preference": "auto"});
        let rules = build_xray_gui_routing_rules(&routing, &settings);
        let proxy_rule = rules
            .iter()
            .find(|r| r["outboundTag"] == json!("proxy") && r["domain"] == json!(["geosite:ru-blocked"]))
            .expect("geosite:ru-blocked must route to proxy");
        assert_eq!(proxy_rule["type"], json!("field"));
        // tun_mode=false → use_rule_default=false → the catch-all is direct.
        let last = rules.last().unwrap();
        assert_eq!(last["network"], json!("tcp,udp"));
        assert_eq!(last["outboundTag"], json!("direct"));
        assert_eq!(rules.first().unwrap()["type"], json!("field"));
    }

    #[test]
    fn xray_process_rules_grouped_by_action() {
        let routing = json!({
            "mode": "rule",
            "preset_id": "global",
            "bypass_lan": false,
            "process_preset_routes": {"telegram": "proxy", "torrents": "direct"},
            "process_rules": [
                {"process": "C:\\Games\\game.exe", "action": "block", "match": "path"},
                {"process": " MyApp.exe ", "action": "proxy"},
            ],
            "service_routes": {},
            "direct_domains": [],
            "proxy_domains": [],
            "block_domains": [],
            "tun_default_outbound": "proxy",
        });
        let settings = json!({"tun_mode": false, "core_preference": "auto"});
        let rules = build_xray_gui_routing_rules(&routing, &settings);
        let direct = rules
            .iter()
            .find(|r| r["outboundTag"] == json!("direct") && r["process"].is_array())
            .unwrap();
        let direct_names: Vec<&str> = direct["process"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(direct_names.contains(&"qbittorrent.exe"));
        assert_eq!(direct["network"], json!("tcp,udp"));
        let proxy = rules
            .iter()
            .find(|r| r["outboundTag"] == json!("proxy") && r["process"].is_array())
            .unwrap();
        let proxy_names: Vec<&str> = proxy["process"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(proxy_names.contains(&"Telegram.exe"));
        // The manual proxy bucket produces its own rule next to the preset one.
        assert!(rules.iter().any(|r| {
            r["outboundTag"] == json!("proxy")
                && r["process"]
                    .as_array()
                    .map(|names| names.contains(&json!("MyApp.exe")))
                    .unwrap_or(false)
        }));
        let block = rules
            .iter()
            .find(|r| r["outboundTag"] == json!("block") && r["process"].is_array())
            .unwrap();
        assert_eq!(block["process"], json!(["game.exe"]));
    }

    #[test]
    fn xray_gui_routing_deterministic_and_splices_local_proxy() {
        let routing = blocked_routing();
        let settings = json!({"tun_mode": false, "core_preference": "auto"});
        let mut first = json!({"inbounds": [{"tag": "socks-in", "protocol": "mixed"}]});
        apply_xray_gui_routing(&mut first, &routing, &settings);
        let mut second = json!({"inbounds": [{"tag": "socks-in", "protocol": "mixed"}]});
        apply_xray_gui_routing(&mut second, &routing, &settings);
        assert_eq!(first["routing"]["rules"], second["routing"]["rules"]);
        let rules = second["routing"]["rules"].as_array().unwrap();
        // The local-proxy rule lands right before the final catch-all.
        let last = rules.last().unwrap();
        assert_eq!(last["network"], json!("tcp,udp"));
        let before_last = &rules[rules.len() - 2];
        assert_eq!(before_last["inboundTag"], json!(["socks-in"]));
        assert_eq!(before_last["outboundTag"], json!("proxy"));
    }

    #[test]
    fn singbox_process_rules_sentinel_block_is_idempotent() {
        let routing = json!({
            "mode": "rule",
            "preset_id": "global",
            "bypass_lan": true,
            "direct_domains": [],
            "proxy_domains": [],
            "block_domains": [],
            "process_rules": [{"process": "myapp.exe", "action": "direct"}],
            "process_preset_routes": {"telegram": "proxy"},
            "service_routes": {"youtube": "direct"},
            "dns_fake_enabled": false,
            "dns_geo_check": true,
            "tun_default_outbound": "proxy",
        });
        let mut payload = json!({});
        apply_singbox_gui_routing(&mut payload, &routing).unwrap();
        let mut again = payload.clone();
        apply_singbox_gui_routing(&mut again, &routing).unwrap();
        assert_eq!(payload["route"]["rules"], again["route"]["rules"]);

        let rules = again["route"]["rules"].as_array().unwrap();
        let end_markers = rules
            .iter()
            .filter(|r| r["process_name"] == json!(["__lumen_gui_route_end__.exe"]))
            .count();
        assert_eq!(end_markers, 1);
        assert!(is_lumen_singbox_route_end_rule(rules.last().unwrap()));
        assert!(is_subscription_fetcher_direct_rule(&rules[0]));
        assert!(
            rules.iter().any(|r| {
                r["process_name"]
                    .as_array()
                    .map(|names| names.contains(&json!("Telegram.exe")))
                    .unwrap_or(false)
                    && r["outbound"] == json!("proxy")
            }),
            "preset process rule missing"
        );
        assert!(
            rules.iter().any(|r| r["process_name"] == json!(["myapp.exe"])
                && r["outbound"] == json!("direct")),
            "manual process rule missing"
        );
        assert!(rules.iter().any(|r| r["ip_is_private"] == json!(true)));
        assert_eq!(again["route"]["final"], json!("proxy"));
    }

    #[test]
    fn singbox_dns_sentinels_fakeip_and_rejects_are_idempotent() {
        let routing = json!({
            "mode": "rule",
            "preset_id": "global",
            "bypass_lan": false,
            "direct_domains": [],
            "proxy_domains": [],
            "block_domains": [],
            "process_rules": [],
            "process_preset_routes": {},
            "service_routes": {"youtube": "direct"},
            "dns_geo_check": false,
            "dns_fake_enabled": true,
            "dns_bootstrap_servers": ["1.1.1.1"],
            "dns_proxy_strategy": "prefer_ipv4",
            "tun_default_outbound": "direct",
        });
        let mut payload = json!({
            "dns": {
                "servers": [{"tag": "proxy-dns"}, {"tag": "fake-dns"}, {"tag": "bootstrap-dns"}],
                "rules": [{"domain_suffix": ["keep.example"], "server": "hosts-dns",
                           "ip_accept_any": true}],
            },
        });
        apply_singbox_gui_routing(&mut payload, &routing).unwrap();
        let mut again = payload.clone();
        apply_singbox_gui_routing(&mut again, &routing).unwrap();
        assert_eq!(payload["dns"]["rules"], again["dns"]["rules"]);

        let dns_rules = again["dns"]["rules"].as_array().unwrap();
        assert_eq!(dns_rules[0], json!({"query_type": ["HTTPS", "SVCB"], "action": "reject"}));
        assert_eq!(dns_rules[1]["action"], json!("reject"));
        assert!(dns_rules[1]["domain_suffix"].as_array().unwrap().contains(&json!("dns.google")));
        // hosts override moves ahead of the FakeIP rule (index 2).
        assert_eq!(dns_rules[2]["server"], json!("hosts-dns"));
        assert!(dns_rules.iter().any(|r| r["server"] == json!("fake-dns")
            && r["rewrite_ttl"] == json!(1)
            && r["query_type"] == json!(["A", "AAAA"])));
        assert_eq!(
            dns_rules
                .iter()
                .filter(|r| r["domain"] == json!(["__lumen_gui_dns_start__.invalid"]))
                .count(),
            1
        );
        assert_eq!(again["dns"]["final"], json!("proxy-dns"));
        assert_eq!(again["route"]["default_domain_resolver"]["server"], json!("proxy-dns"));
        assert_eq!(again["route"]["default_domain_resolver"]["strategy"], json!("prefer_ipv4"));
        // FakeIP route rule for FakeIP CIDRs.
        assert!(again["route"]["rules"].as_array().unwrap().iter().any(|r| r["ip_cidr"]
            == json!(["198.18.0.0/15", "fc00::/18"])));
    }

    #[test]
    fn singbox_rule_set_generated_from_temp_list_files() {
        let base = std::env::temp_dir().join(format!("lumen-rr-test-{}", std::process::id()));
        let lists = base.join("zapret").join("lists");
        std::fs::create_dir_all(&lists).unwrap();
        std::fs::write(
            lists.join("list-general.txt"),
            "||example-blocked.com^\n# comment\n10.0.0.5/24\ndomain:foo.org\n|bar.example^|\n",
        )
        .unwrap();
        std::fs::write(lists.join("ipset-ru.txt"), "1.2.3.0/24\n10.0.0.5/24\nnot-an-ip\n").unwrap();

        let geosite = local_rule_set_payload(
            &rule_set_source("geosite:ru-blocked").unwrap(),
            "geosite-ru-blocked",
            &base,
        );
        assert_eq!(geosite["version"], json!(3));
        let geosite_suffixes: Vec<&str> = geosite["rules"][0]["domain_suffix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(geosite_suffixes, vec!["bar.example", "example-blocked.com", "foo.org"]);

        let geoip = local_rule_set_payload(
            &rule_set_source("geoip:ru").unwrap(),
            "geoip-ru",
            &base,
        );
        assert_eq!(geoip["rules"][0]["ip_cidr"], json!(["1.2.3.0/24", "10.0.0.0/24"]));

        // The route builder maps preset geo codes to managed rule-set tags.
        let routing = json!({
            "mode": "rule",
            "preset_id": "blocked",
            "bypass_lan": false,
            "direct_domains": [],
            "proxy_domains": [],
            "block_domains": [],
            "process_rules": [],
            "process_preset_routes": {},
            "service_routes": {},
            "tun_default_outbound": "direct",
        });
        let (route_rules, tags) = build_singbox_gui_route_rules(&routing);
        assert!(tags.contains("geosite-ru-blocked"));
        assert!(tags.contains("geoip-ru-blocked"));
        let rule_with_sets = route_rules
            .iter()
            .find(|r| {
                r.get("rule_set")
                    .and_then(Value::as_array)
                    .map(|sets| sets.contains(&json!("geosite-ru-blocked")))
                    .unwrap_or(false)
            })
            .expect("managed rule-set tags in route rule");
        assert_eq!(
            rule_with_sets["rule_set"],
            json!(["geoip-ru-blocked", "geoip-ru-blocked-community", "geosite-ru-blocked"])
        );
        assert_eq!(rule_with_sets["action"], json!("route"));
        assert_eq!(rule_with_sets["outbound"], json!("proxy"));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ip_network_normalizes_host_bits() {
        assert_eq!(ip_network_strict_false("10.0.0.5/24").as_deref(), Some("10.0.0.0/24"));
        assert_eq!(ip_network_strict_false("1.2.3.4").as_deref(), Some("1.2.3.4"));
        assert_eq!(ip_network_strict_false("2001:DB8::1").as_deref(), Some("2001:db8::1"));
        assert_eq!(ip_network_strict_false("10.0.0.0/33"), None);
        assert_eq!(ip_network_strict_false("10.0.0.300"), None);
        assert_eq!(ip_network_strict_false("1.2.3.4/255.255.255.0").as_deref(), Some("1.2.3.0/24"));
    }

    #[test]
    fn routing_with_ip_preference_pins_families() {
        let routing = json!({
            "dns_bootstrap_strategy": "ipv4_only",
            "dns_proxy_strategy": "bogus",
        });
        let effective = routing_with_ip_preference(&routing, true);
        assert_eq!(effective["dns_bootstrap_strategy"], json!("ipv4_only"));
        assert_eq!(effective["dns_proxy_strategy"], json!("prefer_ipv6"));
        let effective = routing_with_ip_preference(&routing, false);
        assert_eq!(effective["dns_proxy_strategy"], json!("prefer_ipv4"));
    }

    #[test]
    fn service_route_helpers_resolve_presets() {
        let routing = json!({
            "mode": "rule",
            "preset_id": "blocked",
            "service_routes": {"youtube": "direct"},
            "tun_default_outbound": "direct",
        });
        assert_eq!(effective_service_action(&routing, "youtube"), "direct");
        // Missing override inherits the blocked preset's catalog action.
        assert_eq!(effective_service_action(&routing, "steam"), "direct");
        assert_eq!(effective_service_action(&routing, "telegram"), "proxy");
        assert_eq!(service_route_selection(&routing, "youtube"), "direct");
        assert_eq!(service_route_selection(&routing, "steam"), "default");
    }
}
