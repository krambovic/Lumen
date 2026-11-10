//! State model (`models.py`) — JSON keys and migration semantics are kept
//! byte-compatible with the Python client so `state.enc` profiles interoperate.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::constants::{ROUTING_DIRECT, ROUTING_RULE, STATE_SCHEMA_VERSION};

pub const DEFAULT_WINDOW_WIDTH: i64 = 1280;
pub const DEFAULT_WINDOW_HEIGHT: i64 = 720;
pub const DEFAULT_SUBSCRIPTION_HWID: &str = "00000000-0000-4000-8000-000000000000";
const LEGACY_WINDOW_DEFAULT: (i64, i64) = (1024, 768);

pub fn utc_now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

pub const CORE_PREFERENCES: [&str; 3] = ["auto", "xray", "singbox"];

pub fn normalize_core_preference(value: &Value) -> String {
    let text = str_field(value, "core_preference")
        .trim()
        .to_lowercase()
        .replace('_', "-");
    match text.as_str() {
        "sing-box" | "singbox" | "sing-box-extended" | "singbox-extended" => {
            "singbox".to_string()
        }
        t if CORE_PREFERENCES.contains(&t) => t.to_string(),
        _ => "auto".to_string(),
    }
}

// ── Value helpers (tolerant, mirroring Python `str()/int()` semantics) ──

pub fn str_field(data: &Value, key: &str) -> String {
    match data.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub fn str_or(data: &Value, key: &str, fallback: &str) -> String {
    let v = str_field(data, key);
    if v.is_empty() {
        fallback.to_string()
    } else {
        v
    }
}

pub fn opt_str(data: &Value, key: &str) -> Option<String> {
    match data.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) | None => None,
        Some(other) => Some(other.to_string().trim_matches('"').to_string()),
    }
}

pub fn int_field(data: &Value, key: &str, fallback: i64) -> i64 {
    match data.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_else(|| n.as_f64().map(|f| f as i64).unwrap_or(fallback)),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(fallback),
        Some(Value::Bool(_)) | Some(Value::Null) | None => fallback,
        _ => fallback,
    }
}

pub fn opt_int(data: &Value, key: &str) -> Option<i64> {
    match data.get(key) {
        Some(Value::Number(n)) => Some(n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn opt_f64(data: &Value, key: &str) -> Option<f64> {
    match data.get(key) {
        Some(Value::Number(n)) => Some(n.as_f64().unwrap_or(0.0)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn bool_field(data: &Value, key: &str, fallback: bool) -> bool {
    match data.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true") || s == "1",
        _ => fallback,
    }
}

pub fn str_list(data: &Value, key: &str) -> Vec<String> {
    match data.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn obj_list(data: &Value, key: &str) -> Vec<Value> {
    match data.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter(|v| v.is_object())
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

pub fn str_map(data: &Value, key: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(map)) = data.get(key) {
        for (k, v) in map {
            out.insert(k.clone(), match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => String::new(),
            });
        }
    }
    out
}

pub fn string_list_field(values: &Value, fallback: &str) -> Vec<String> {
    let raw_items: Vec<String> = match values {
        Value::String(s) => s
            .replace(';', "\n")
            .replace(',', "\n")
            .lines()
            .map(|l| l.to_string())
            .collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut result: Vec<String> = Vec::new();
    for item in raw_items {
        let value = item.trim().to_string();
        if !value.is_empty() && !result.contains(&value) {
            result.push(value);
        }
    }
    if result.is_empty() && !fallback.is_empty() {
        result.push(fallback.to_string());
    }
    result
}

pub fn normalize_dns_hosts(values: &Value) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    if let Some(map) = values.as_object() {
        for (raw_domain, raw_addresses) in map {
            let domain = raw_domain.trim().to_lowercase();
            let domain = domain.strip_suffix('.').unwrap_or(&domain).to_string();
            if domain.is_empty() {
                continue;
            }
            let addresses = string_list_field(raw_addresses, "");
            if !addresses.is_empty() {
                result.insert(domain, addresses);
            }
        }
    }
    result
}

fn normalize_window_size(width: &Value, height: &Value) -> (i64, i64) {
    let w = match width {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    };
    let h = match height {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    };
    let w = if w == 0 { DEFAULT_WINDOW_WIDTH } else { w };
    let h = if h == 0 { DEFAULT_WINDOW_HEIGHT } else { h };
    if (w, h) == LEGACY_WINDOW_DEFAULT || w < 640 || h < 360 {
        return (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT);
    }
    (w, h)
}

fn normalize_language(value: &Value) -> String {
    let language = str_field(value, "language").trim().to_lowercase();
    if ["ru", "en", "fa", "zh"].contains(&language.as_str()) {
        language
    } else {
        detect_system_language()
    }
}

pub fn detect_system_language() -> String {
    #[cfg(target_os = "windows")]
    {
        let lang = lumen_platform::system_language();
        if lang.starts_with("ru") {
            return "ru".into();
        }
        if lang.starts_with("fa") {
            return "fa".into();
        }
        if lang.starts_with("zh") {
            return "zh".into();
        }
        "en".into()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "en".into()
    }
}

fn normalize_tun_stack(value: &Value) -> String {
    let stack = str_field(value, "tun_stack").trim().to_lowercase();
    if ["system", "gvisor", "mixed"].contains(&stack.as_str()) {
        stack
    } else {
        "mixed".into()
    }
}

fn clamp_tun_mtu(value: &Value) -> i64 {
    let mtu = match value {
        Value::Number(n) => n.as_i64().unwrap_or(9000),
        Value::String(s) => s.trim().parse().unwrap_or(9000),
        _ => 9000,
    };
    mtu.clamp(1280, 65535)
}

fn normalize_local_port(value: i64, default: i64) -> i64 {
    // 10818 is reserved for droute Discord Voice.
    if value < 1025 || value > 65535 || value == 10818 {
        return default;
    }
    value
}

fn normalize_node_table_layout(value: &Value) -> Value {
    normalize_table_layout(value, &["manual", "name", "type", "transport", "address", "port", "group", "ping", "speed", "status", "last"])
}

fn normalize_zapret_table_layout(value: &Value) -> Value {
    normalize_table_layout(value, &["manual", "name", "description", "args", "date"])
}

fn normalize_table_layout(value: &Value, allowed: &[&str]) -> Value {
    let mut out = Map::new();
    if let Some(map) = value.as_object() {
        for (key, raw_value) in map {
            if !allowed.contains(&key.as_str()) {
                continue;
            }
            if key == "manual" {
                out.insert(key.clone(), Value::Bool(bool_field(&json!({"manual": raw_value}), "manual", false)));
                continue;
            }
            let numeric = match raw_value.as_f64() {
                Some(n) if n.is_finite() => n,
                _ => match raw_value.as_str().and_then(|s| s.trim().parse::<f64>().ok()) {
                    Some(n) if n.is_finite() => n,
                    _ => continue,
                },
            };
            out.insert(key.clone(), json!(numeric.clamp(48.0, 2000.0)));
        }
    }
    Value::Object(out)
}

fn normalize_zapret_server_presets(value: &Value) -> Value {
    let mut out = Map::new();
    if let Some(map) = value.as_object() {
        for (node_id, preset) in map {
            let key = node_id.trim().to_string();
            if key.is_empty() {
                continue;
            }
            out.insert(key, Value::String(match preset {
                Value::String(s) => s.trim().to_string(),
                other => other.to_string().trim_matches('"').to_string(),
            }));
        }
    }
    Value::Object(out)
}

// ── Node ──

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub scheme: String,
    pub server: String,
    pub port: i64,
    pub link: String,
    /// Xray-shaped or sing-box native outbound payload (arbitrary JSON).
    pub outbound: Value,
    pub group: String,
    pub tags: Vec<String>,
    pub ping_ms: Option<i64>,
    pub ping_kind: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub country_code: String,
    pub speed_mbps: Option<f64>,
    pub is_alive: Option<bool>,
    pub ping_history: Vec<(String, Option<i64>)>,
    pub speed_history: Vec<(String, Option<f64>)>,
    pub sort_order: i64,
    pub subscription_id: String,
    pub description: String,
    pub source_key: String,
}

impl Node {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            group: "Default".into(),
            created_at: utc_now_iso(),
            ..Default::default()
        }
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "scheme": self.scheme,
            "server": self.server,
            "port": self.port,
            "link": self.link,
            "outbound": self.outbound.clone(),
            "group": self.group,
            "tags": self.tags.clone(),
            "ping_ms": self.ping_ms,
            "ping_kind": self.ping_kind,
            "last_used_at": self.last_used_at,
            "created_at": self.created_at,
            "country_code": self.country_code,
            "speed_mbps": self.speed_mbps,
            "is_alive": self.is_alive,
            "ping_history": self.ping_history.iter().map(|(ts, ms)| match ms {
                Some(ms) => json!([ts, ms]),
                None => json!([ts, Value::Null]),
            }).collect::<Vec<_>>(),
            "speed_history": self.speed_history.iter().map(|(ts, mb)| match mb {
                Some(mb) => json!([ts, mb]),
                None => json!([ts, Value::Null]),
            }).collect::<Vec<_>>(),
            "sort_order": self.sort_order,
            "subscription_id": self.subscription_id,
            "description": self.description,
            "source_key": self.source_key,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        // Old endpoint pings were incorrectly persisted as profile health.
        let ping_kind = str_field(data, "ping_kind");
        let is_alive = if ping_kind == "proxy" {
            data.get("is_alive").and_then(|v| v.as_bool())
        } else {
            None
        };
        let ping_history = match data.get("ping_history").and_then(|v| v.as_array()) {
            Some(items) => items
                .iter()
                .filter_map(|item| {
                    let arr = item.as_array()?;
                    let ts = arr.first()?.as_str()?.to_string();
                    let ms = arr.get(1).and_then(|v| v.as_i64());
                    Some((ts, ms))
                })
                .collect(),
            None => Vec::new(),
        };
        let speed_history = match data.get("speed_history").and_then(|v| v.as_array()) {
            Some(items) => items
                .iter()
                .filter_map(|item| {
                    let arr = item.as_array()?;
                    let ts = arr.first()?.as_str()?.to_string();
                    let mb = arr.get(1).and_then(|v| v.as_f64());
                    Some((ts, mb))
                })
                .collect(),
            None => Vec::new(),
        };
        Self {
            id: str_or(data, "id", &Uuid::new_v4().to_string()),
            name: str_field(data, "name"),
            scheme: str_field(data, "scheme"),
            server: str_field(data, "server"),
            port: int_field(data, "port", 0),
            link: str_field(data, "link"),
            outbound: data.get("outbound").and_then(|v| v.as_object()).map(|m| Value::Object(m.clone())).unwrap_or_else(|| json!({})),
            group: str_or(data, "group", "Default"),
            tags: str_list(data, "tags"),
            ping_ms: opt_int(data, "ping_ms"),
            ping_kind,
            last_used_at: opt_str(data, "last_used_at"),
            created_at: str_or(data, "created_at", &utc_now_iso()),
            country_code: str_field(data, "country_code"),
            speed_mbps: opt_f64(data, "speed_mbps"),
            is_alive,
            ping_history,
            speed_history,
            sort_order: int_field(data, "sort_order", 0),
            subscription_id: str_field(data, "subscription_id"),
            description: str_field(data, "description"),
            source_key: str_field(data, "source_key"),
        }
    }
}

// ── RoutingSettings ──

#[derive(Debug, Clone, PartialEq)]
pub struct RoutingSettings {
    pub mode: String,
    pub preset_id: String,
    pub bypass_lan: bool,
    pub direct_domains: Vec<String>,
    pub proxy_domains: Vec<String>,
    pub block_domains: Vec<String>,
    pub dns_mode: String,
    pub dns_bootstrap_server: String,
    pub dns_bootstrap_servers: Vec<String>,
    pub dns_bootstrap_type: String,
    pub dns_bootstrap_strategy: String,
    pub dns_proxy_server: String,
    pub dns_proxy_servers: Vec<String>,
    pub dns_proxy_type: String,
    pub dns_proxy_strategy: String,
    pub dns_fake_enabled: bool,
    pub dns_hijack_enabled: bool,
    pub dns_parallel_query: bool,
    pub dns_optimistic_cache: bool,
    pub dns_geo_check: bool,
    pub dns_hosts: BTreeMap<String, Vec<String>>,
    pub tun_route_exclude_address: Vec<String>,
    pub process_rules: Vec<Value>,
    pub process_preset_routes: BTreeMap<String, String>,
    pub service_routes: BTreeMap<String, String>,
    pub tun_default_outbound: String,
    pub tun_default_outbound_user_selected: bool,
}

impl Default for RoutingSettings {
    fn default() -> Self {
        Self {
            mode: ROUTING_RULE.into(),
            preset_id: "blocked".into(),
            bypass_lan: true,
            direct_domains: Vec::new(),
            proxy_domains: Vec::new(),
            block_domains: Vec::new(),
            dns_mode: "builtin".into(),
            dns_bootstrap_server: "1.1.1.1".into(),
            dns_bootstrap_servers: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            dns_bootstrap_type: "https".into(),
            dns_bootstrap_strategy: "ipv4_only".into(),
            dns_proxy_server: "cloudflare-dns.com".into(),
            dns_proxy_servers: vec!["cloudflare-dns.com".into(), "dns.google".into()],
            dns_proxy_type: "https".into(),
            dns_proxy_strategy: "ipv4_only".into(),
            dns_fake_enabled: false,
            dns_hijack_enabled: true,
            dns_parallel_query: false,
            dns_optimistic_cache: false,
            dns_geo_check: true,
            dns_hosts: BTreeMap::new(),
            tun_route_exclude_address: Vec::new(),
            process_rules: Vec::new(),
            process_preset_routes: BTreeMap::new(),
            service_routes: BTreeMap::new(),
            tun_default_outbound: "direct".into(),
            tun_default_outbound_user_selected: false,
        }
    }
}

impl RoutingSettings {
    /// `__post_init__` normalization: fills server lists from primary entries.
    pub fn normalize(&mut self) {
        let bootstrap_defaults: Vec<String> = if self.dns_bootstrap_server == "1.1.1.1" {
            vec!["1.1.1.1".into(), "8.8.8.8".into()]
        } else {
            vec![self.dns_bootstrap_server.clone()]
        };
        let proxy_defaults: Vec<String> = if self.dns_proxy_server == "cloudflare-dns.com" {
            vec!["cloudflare-dns.com".into(), "dns.google".into()]
        } else {
            vec![self.dns_proxy_server.clone()]
        };
        self.dns_bootstrap_servers = if self.dns_bootstrap_servers.is_empty() {
            string_list_field(&Value::Array(bootstrap_defaults.iter().map(|s| json!(s)).collect()), "")
        } else {
            self.dns_bootstrap_servers.clone()
        };
        self.dns_proxy_servers = if self.dns_proxy_servers.is_empty() {
            string_list_field(
                &Value::Array(proxy_defaults.iter().map(|s| json!(s)).collect()),
                "cloudflare-dns.com",
            )
        } else {
            self.dns_proxy_servers.clone()
        };
        self.dns_bootstrap_server = self
            .dns_bootstrap_servers
            .first()
            .cloned()
            .unwrap_or_default();
        self.dns_proxy_server = self.dns_proxy_servers.first().cloned().unwrap_or_default();
        self.dns_hosts = self
            .dns_hosts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let default_outbound = self.tun_default_outbound.trim().to_lowercase();
        self.tun_default_outbound = if ["proxy", "direct"].contains(&default_outbound.as_str()) {
            default_outbound
        } else {
            "direct".into()
        };
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "mode": self.mode,
            "preset_id": self.preset_id,
            "bypass_lan": self.bypass_lan,
            "direct_domains": self.direct_domains,
            "proxy_domains": self.proxy_domains,
            "block_domains": self.block_domains,
            "dns_mode": self.dns_mode,
            "dns_bootstrap_server": self.dns_bootstrap_server,
            "dns_bootstrap_servers": self.dns_bootstrap_servers,
            "dns_bootstrap_type": self.dns_bootstrap_type,
            "dns_bootstrap_type_explicit": true,
            "dns_bootstrap_strategy": self.dns_bootstrap_strategy,
            "dns_proxy_server": self.dns_proxy_server,
            "dns_proxy_servers": self.dns_proxy_servers,
            "dns_proxy_type": self.dns_proxy_type,
            "dns_proxy_strategy": self.dns_proxy_strategy,
            "dns_fake_enabled": self.dns_fake_enabled,
            "dns_hijack_enabled": self.dns_hijack_enabled,
            "dns_parallel_query": self.dns_parallel_query,
            "dns_optimistic_cache": self.dns_optimistic_cache,
            "dns_geo_check": self.dns_geo_check,
            "dns_hosts": self.dns_hosts.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<BTreeMap<_, _>>(),
            "tun_route_exclude_address": self.tun_route_exclude_address,
            "process_rules": self.process_rules,
            "process_preset_routes": self.process_preset_routes,
            "service_routes": self.service_routes,
            "tun_default_outbound": self.tun_default_outbound,
            "tun_default_outbound_user_selected_v2": self.tun_default_outbound_user_selected,
            "tun_default_outbound_explicit": true,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        let mut preset_id = str_field(data, "preset_id").trim().to_lowercase();
        if preset_id.is_empty() {
            let direct: std::collections::HashSet<String> = str_list(data, "direct_domains")
                .iter()
                .map(|s| s.trim().to_lowercase())
                .collect();
            let proxy: std::collections::HashSet<String> = str_list(data, "proxy_domains")
                .iter()
                .map(|s| s.trim().to_lowercase())
                .collect();
            if direct.contains("geosite:category-ru") && direct.contains("geoip:ru") {
                preset_id = "except_ru".into();
            } else if ["geosite:ru-blocked", "geosite:category-media-ru-blocked", "geoip:ru-blocked", "geoip:ru-blocked-community"]
                .iter()
                .any(|k| proxy.contains(*k))
            {
                preset_id = "blocked".into();
            } else if str_field(data, "mode").to_lowercase() == "global" {
                preset_id = "global".into();
            } else {
                preset_id = "custom".into();
            }
        }
        let mut mode = str_field(data, "mode");
        if mode.trim().is_empty() {
            mode = ROUTING_RULE.into();
        }
        if mode.trim().to_lowercase() == "global" && ["blocked", "blocked_cn"].contains(&preset_id.as_str()) {
            // Old persisted states could infer the blocked-only preset from its
            // domain rules while retaining global mode; that combination makes
            // native sing-box runtime full-tunnel.
            mode = ROUTING_RULE.into();
        }

        let bootstrap_servers: Vec<String>;
        let mut bootstrap_server: String;
        if data.get("dns_bootstrap_servers").and_then(|v| v.as_array()).is_none() {
            bootstrap_server = str_or(data, "dns_bootstrap_server", "1.1.1.1");
            bootstrap_servers = Vec::new();
        } else {
            bootstrap_servers = str_list(data, "dns_bootstrap_servers")
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            bootstrap_server = bootstrap_servers.first().cloned().unwrap_or_default();
        }
        if bootstrap_server.is_empty() {
            bootstrap_server = "1.1.1.1".into();
        }

        let mut bootstrap_type = str_or(data, "dns_bootstrap_type", "https");
        let bootstrap_type_explicit = data
            .get("dns_bootstrap_type_explicit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !bootstrap_type_explicit
            && bootstrap_server.trim().to_lowercase() == "1.1.1.1"
            && bootstrap_type.trim().to_lowercase() == "udp"
        {
            // Migrate the legacy default only on first load; to_dict() writes
            // the marker so an intentional UDP choice survives later loads.
            bootstrap_type = "https".into();
        }

        let mut raw_default_outbound = str_field(data, "tun_default_outbound").trim().to_lowercase();
        let valid_default_outbound = ["proxy", "direct"].contains(&raw_default_outbound.as_str());
        let mut default_outbound_user_selected =
            data.get("tun_default_outbound_user_selected_v2").and_then(|v| v.as_bool()).unwrap_or(false);
        if default_outbound_user_selected && valid_default_outbound {
            // keep
        } else if ["blocked", "blocked_cn"].contains(&preset_id.as_str()) {
            raw_default_outbound = "direct".into();
            default_outbound_user_selected = false;
        } else if data.get("tun_default_outbound_explicit").and_then(|v| v.as_bool()).unwrap_or(false)
            && valid_default_outbound
        {
            default_outbound_user_selected = false;
        } else {
            raw_default_outbound = if mode.trim().to_lowercase() == ROUTING_DIRECT {
                "direct".into()
            } else {
                "proxy".into()
            };
            default_outbound_user_selected = false;
        }

        let mut settings = Self {
            mode,
            preset_id,
            bypass_lan: bool_field(data, "bypass_lan", true),
            direct_domains: str_list(data, "direct_domains"),
            proxy_domains: str_list(data, "proxy_domains"),
            block_domains: str_list(data, "block_domains"),
            dns_mode: str_or(data, "dns_mode", "builtin"),
            dns_bootstrap_server: bootstrap_server.clone(),
            dns_bootstrap_servers: bootstrap_servers,
            dns_bootstrap_type: bootstrap_type,
            dns_bootstrap_strategy: str_or(data, "dns_bootstrap_strategy", "ipv4_only"),
            dns_proxy_server: str_or(data, "dns_proxy_server", "cloudflare-dns.com"),
            dns_proxy_servers: str_list(data, "dns_proxy_servers"),
            dns_proxy_type: str_or(data, "dns_proxy_type", "https"),
            dns_proxy_strategy: str_or(data, "dns_proxy_strategy", "ipv4_only"),
            dns_fake_enabled: bool_field(data, "dns_fake_enabled", false),
            dns_hijack_enabled: bool_field(data, "dns_hijack_enabled", true),
            dns_parallel_query: bool_field(data, "dns_parallel_query", false),
            dns_optimistic_cache: bool_field(data, "dns_optimistic_cache", false),
            dns_geo_check: bool_field(data, "dns_geo_check", true),
            dns_hosts: normalize_dns_hosts(data.get("dns_hosts").unwrap_or(&Value::Null)),
            tun_route_exclude_address: str_list(data, "tun_route_exclude_address")
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            process_rules: obj_list(data, "process_rules"),
            process_preset_routes: str_map(data, "process_preset_routes"),
            service_routes: str_map(data, "service_routes"),
            tun_default_outbound: raw_default_outbound,
            tun_default_outbound_user_selected: default_outbound_user_selected,
        };
        settings.normalize();
        settings
    }
}

// ── SecuritySettings ──

#[derive(Debug, Clone, PartialEq)]
pub struct SecuritySettings {
    pub enabled: bool,
    pub password_hash: String,
    pub salt: String,
    pub auto_lock_minutes: i64,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            password_hash: String::new(),
            salt: String::new(),
            auto_lock_minutes: 15,
        }
    }
}

impl SecuritySettings {
    pub fn to_dict(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "password_hash": self.password_hash,
            "salt": self.salt,
            "auto_lock_minutes": self.auto_lock_minutes,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        Self {
            enabled: bool_field(data, "enabled", false),
            password_hash: str_field(data, "password_hash"),
            salt: str_field(data, "salt"),
            auto_lock_minutes: int_field(data, "auto_lock_minutes", 15),
        }
    }
}

// ── AppSettings ──

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub accent_color: String,
    pub interface_mode: String,
    pub auto_connect_last: bool,
    pub start_minimized: bool,
    pub enable_system_proxy: bool,
    pub system_proxy_bypass_lan: bool,
    pub firefox_proxy_integration: bool,
    pub launch_on_startup: bool,
    pub launch_in_tray_on_startup: bool,
    pub always_run_as_admin: bool,
    pub block_vpn_conflicts: bool,
    pub reconnect_on_network_change: bool,
    pub reconnect_after_sleep: bool,
    pub regional_preset: String,
    pub prefer_ipv6: bool,
    pub kill_switch: bool,
    pub resource_update_check: bool,
    pub xray_path: String,
    pub log_level: String,
    pub check_updates: bool,
    pub allow_updates: bool,
    pub app_auto_update: bool,
    pub release_channel: String,
    pub update_feed_url: String,
    pub xray_release_channel: String,
    pub xray_update_feed_url: String,
    pub xray_auto_update: bool,
    pub enable_xray_fragment: bool,
    pub enable_final_fragment: bool,
    pub fragment_packets: String,
    pub fragment_length: String,
    pub fragment_delay: String,
    pub tail_fragment_enabled: bool,
    pub multiplex_enabled: bool,
    pub multiplex_concurrency: i64,
    pub discord_proxy_enabled: bool,
    pub tun_mode: bool,
    pub core_preference: String,
    pub xray_config_file: String,
    pub xray_template_file: String,
    pub singbox_path: String,
    pub singbox_config_file: String,
    pub singbox_template_file: String,
    pub window_width: i64,
    pub window_height: i64,
    pub window_x: i64,
    pub window_y: i64,
    pub zapret_preset: String,
    pub zapret_autostart: bool,
    pub zapret_server_presets: Value,
    pub proxy_chain_enabled: bool,
    pub proxy_chain_front_node_id: String,
    pub proxy_chain_exit_node_id: String,
    pub auto_switch_enabled: bool,
    pub auto_switch_threshold_kbps: i64,
    pub auto_switch_delay_sec: i64,
    pub auto_switch_cooldown_sec: i64,
    pub auto_connect_on_import: bool,
    pub ping_method: String,
    pub speed_test_url: String,
    pub speed_test_concurrency: i64,
    pub subscription_auto_update_minutes: i64,
    pub subscription_include_regex: String,
    pub subscription_exclude_regex: String,
    pub subscription_user_agent: String,
    pub subscription_use_real_hwid: bool,
    pub subscription_hwid: String,
    pub subscription_use_proxy_tun: bool,
    pub subscription_converter_enabled: bool,
    pub subscription_converter_url: String,
    pub ui_density: String,
    pub ui_corner_radius: i64,
    pub ui_font_family: String,
    pub ui_font_scale: i64,
    pub ui_backdrop: String,
    pub ui_transparency_strength: i64,
    pub ui_theme_preset: String,
    pub ui_animations: bool,
    pub ui_base_tint: String,
    pub ui_base_tint_src: String,
    pub ui_wallpaper: String,
    pub ui_wallpaper_opacity: i64,
    pub ui_wallpaper_blur: i64,
    pub ui_wallpaper_brightness: i64,
    pub diagnostics_upload_enabled: bool,
    pub proxy_allow_lan: bool,
    pub tun_strict_route: bool,
    pub tun_stack: String,
    pub tun_mtu: i64,
    pub tun_endpoint_independent_nat: bool,
    pub tun_block_quic: bool,
    pub local_socks_port: i64,
    pub local_http_port: i64,
    pub proxy_auth_enabled: bool,
    pub proxy_auth_username: String,
    pub proxy_auth_password: String,
    pub sniff_route_only: bool,
    pub node_sort_key: String,
    pub node_sort_ascending: bool,
    pub node_filter_group: String,
    pub selected_subscription_id: String,
    pub node_table_layout: Value,
    pub zapret_table_layout: Value,
    pub onboarding_revision: i64,
    pub onboarding_version: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: detect_system_language(),
            accent_color: "#0078D4".into(),
            interface_mode: "full".into(),
            auto_connect_last: true,
            start_minimized: false,
            enable_system_proxy: true,
            system_proxy_bypass_lan: true,
            firefox_proxy_integration: false,
            launch_on_startup: false,
            launch_in_tray_on_startup: true,
            always_run_as_admin: false,
            block_vpn_conflicts: true,
            reconnect_on_network_change: true,
            reconnect_after_sleep: true,
            regional_preset: "russia".into(),
            prefer_ipv6: false,
            kill_switch: false,
            resource_update_check: false,
            xray_path: String::new(),
            log_level: "warning".into(),
            check_updates: true,
            allow_updates: true,
            app_auto_update: false,
            release_channel: "stable".into(),
            update_feed_url: String::new(),
            xray_release_channel: "beta".into(),
            xray_update_feed_url: String::new(),
            xray_auto_update: false,
            enable_xray_fragment: false,
            enable_final_fragment: false,
            fragment_packets: "tlshello".into(),
            fragment_length: "50-100".into(),
            fragment_delay: "10-20".into(),
            tail_fragment_enabled: false,
            multiplex_enabled: false,
            multiplex_concurrency: 8,
            discord_proxy_enabled: false,
            tun_mode: false,
            core_preference: "auto".into(),
            xray_config_file: String::new(),
            xray_template_file: String::new(),
            singbox_path: String::new(),
            singbox_config_file: String::new(),
            singbox_template_file: String::new(),
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            window_x: -1,
            window_y: -1,
            zapret_preset: String::new(),
            zapret_autostart: false,
            zapret_server_presets: json!({}),
            proxy_chain_enabled: false,
            proxy_chain_front_node_id: String::new(),
            proxy_chain_exit_node_id: String::new(),
            auto_switch_enabled: false,
            auto_switch_threshold_kbps: 50,
            auto_switch_delay_sec: 30,
            auto_switch_cooldown_sec: 60,
            auto_connect_on_import: false,
            ping_method: "http".into(),
            speed_test_url: String::new(),
            speed_test_concurrency: 0,
            subscription_auto_update_minutes: 240,
            subscription_include_regex: String::new(),
            subscription_exclude_regex: String::new(),
            subscription_user_agent: String::new(),
            subscription_use_real_hwid: true,
            subscription_hwid: DEFAULT_SUBSCRIPTION_HWID.into(),
            subscription_use_proxy_tun: false,
            subscription_converter_enabled: false,
            subscription_converter_url: String::new(),
            ui_density: "comfortable".into(),
            ui_corner_radius: 8,
            ui_font_family: String::new(),
            ui_font_scale: 100,
            ui_backdrop: "mica".into(),
            ui_transparency_strength: 50,
            ui_theme_preset: "default".into(),
            ui_animations: true,
            ui_base_tint: String::new(),
            ui_base_tint_src: String::new(),
            ui_wallpaper: String::new(),
            ui_wallpaper_opacity: 50,
            ui_wallpaper_blur: 10,
            ui_wallpaper_brightness: 50,
            diagnostics_upload_enabled: true,
            proxy_allow_lan: false,
            tun_strict_route: true,
            tun_stack: "mixed".into(),
            tun_mtu: 9000,
            tun_endpoint_independent_nat: false,
            tun_block_quic: true,
            local_socks_port: 10808,
            local_http_port: 10809,
            proxy_auth_enabled: false,
            proxy_auth_username: String::new(),
            proxy_auth_password: String::new(),
            sniff_route_only: false,
            node_sort_key: "manual".into(),
            node_sort_ascending: true,
            node_filter_group: String::new(),
            selected_subscription_id: String::new(),
            node_table_layout: json!({}),
            zapret_table_layout: json!({}),
            onboarding_revision: 0,
            onboarding_version: String::new(),
        }
    }
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.regional_preset = crate::routing_presets::normalize_regional_preset(&self.regional_preset);
        self.tun_stack = normalize_tun_stack(&json!({"tun_stack": self.tun_stack.clone()}));
        self.local_socks_port = normalize_local_port(self.local_socks_port, 10808);
        self.local_http_port = normalize_local_port(self.local_http_port, 10809);
        if self.local_http_port == self.local_socks_port {
            self.local_http_port = normalize_local_port(self.local_socks_port + 1, 10809);
        }
        self.tun_mtu = clamp_tun_mtu(&json!(self.tun_mtu));
        self.proxy_auth_username = self.proxy_auth_username.trim().chars().take(256).collect();
        if self.proxy_auth_password.len() > 256 {
            self.proxy_auth_password = self.proxy_auth_password.chars().take(256).collect();
        }
        if self.proxy_auth_enabled && (self.proxy_auth_username.is_empty() || self.proxy_auth_password.is_empty()) {
            self.proxy_auth_enabled = false;
        }
        if !["manual", "name", "group", "scheme", "transport", "ping", "speed", "last"].contains(&self.node_sort_key.as_str()) {
            self.node_sort_key = "manual".into();
        }
        self.node_filter_group = self.node_filter_group.trim().to_string();
        self.selected_subscription_id = self.selected_subscription_id.trim().to_string();
        self.node_table_layout = normalize_node_table_layout(&self.node_table_layout);
        self.zapret_table_layout = normalize_zapret_table_layout(&self.zapret_table_layout);
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "theme": self.theme,
            "language": self.language,
            "accent_color": self.accent_color,
            "interface_mode": self.interface_mode,
            "auto_connect_last": self.auto_connect_last,
            "start_minimized": self.start_minimized,
            "enable_system_proxy": self.enable_system_proxy,
            "system_proxy_bypass_lan": self.system_proxy_bypass_lan,
            "firefox_proxy_integration": self.firefox_proxy_integration,
            "launch_on_startup": self.launch_on_startup,
            "launch_in_tray_on_startup": self.launch_in_tray_on_startup,
            "always_run_as_admin": self.always_run_as_admin,
            "block_vpn_conflicts": self.block_vpn_conflicts,
            "reconnect_on_network_change": self.reconnect_on_network_change,
            "reconnect_after_sleep": self.reconnect_after_sleep,
            "regional_preset": self.regional_preset,
            "prefer_ipv6": self.prefer_ipv6,
            "kill_switch": self.kill_switch,
            "resource_update_check": self.resource_update_check,
            "xray_path": self.xray_path,
            "log_level": self.log_level,
            "check_updates": self.check_updates,
            "allow_updates": self.allow_updates,
            "app_auto_update": self.app_auto_update,
            "release_channel": self.release_channel,
            "update_feed_url": self.update_feed_url,
            "xray_release_channel": self.xray_release_channel,
            "xray_update_feed_url": self.xray_update_feed_url,
            "xray_auto_update": self.xray_auto_update,
            "enable_xray_fragment": self.enable_xray_fragment,
            "enable_final_fragment": self.enable_final_fragment,
            "fragment_packets": self.fragment_packets,
            "fragment_length": self.fragment_length,
            "fragment_delay": self.fragment_delay,
            "tail_fragment_enabled": self.tail_fragment_enabled,
            "multiplex_enabled": self.multiplex_enabled,
            "multiplex_concurrency": self.multiplex_concurrency,
            "discord_proxy_enabled": self.discord_proxy_enabled,
            "tun_mode": self.tun_mode,
            "core_preference": self.core_preference,
            "proxy_allow_lan": self.proxy_allow_lan,
            "tun_strict_route": self.tun_strict_route,
            "tun_strict_route_default_v2": true,
            "tun_stack": self.tun_stack,
            "tun_mtu": self.tun_mtu,
            "tun_endpoint_independent_nat": self.tun_endpoint_independent_nat,
            "tun_block_quic": self.tun_block_quic,
            "local_socks_port": self.local_socks_port,
            "local_http_port": self.local_http_port,
            "proxy_auth_enabled": self.proxy_auth_enabled,
            "proxy_auth_username": self.proxy_auth_username,
            "proxy_auth_password": self.proxy_auth_password,
            "sniff_route_only": self.sniff_route_only,
            "node_sort_key": self.node_sort_key,
            "node_sort_ascending": self.node_sort_ascending,
            "node_filter_group": self.node_filter_group,
            "selected_subscription_id": self.selected_subscription_id,
            "node_table_layout": self.node_table_layout.clone(),
            "zapret_table_layout": self.zapret_table_layout.clone(),
            "xray_config_file": self.xray_config_file,
            "xray_template_file": self.xray_template_file,
            "singbox_path": self.singbox_path,
            "singbox_config_file": self.singbox_config_file,
            "singbox_template_file": self.singbox_template_file,
            "window_width": self.window_width,
            "window_height": self.window_height,
            "window_x": self.window_x,
            "window_y": self.window_y,
            "zapret_preset": self.zapret_preset,
            "zapret_autostart": self.zapret_autostart,
            "zapret_server_presets": self.zapret_server_presets.clone(),
            "proxy_chain_enabled": self.proxy_chain_enabled,
            "proxy_chain_front_node_id": self.proxy_chain_front_node_id,
            "proxy_chain_exit_node_id": self.proxy_chain_exit_node_id,
            "auto_switch_enabled": self.auto_switch_enabled,
            "auto_switch_threshold_kbps": self.auto_switch_threshold_kbps,
            "auto_switch_delay_sec": self.auto_switch_delay_sec,
            "auto_switch_cooldown_sec": self.auto_switch_cooldown_sec,
            "auto_connect_on_import": self.auto_connect_on_import,
            "ping_method": self.ping_method,
            "speed_test_url": self.speed_test_url,
            "speed_test_concurrency": self.speed_test_concurrency,
            "subscription_auto_update_minutes": self.subscription_auto_update_minutes,
            "subscription_include_regex": self.subscription_include_regex,
            "subscription_exclude_regex": self.subscription_exclude_regex,
            "subscription_user_agent": self.subscription_user_agent,
            "subscription_use_real_hwid": self.subscription_use_real_hwid,
            "subscription_hwid": self.subscription_hwid,
            "subscription_use_proxy_tun": self.subscription_use_proxy_tun,
            "subscription_converter_enabled": self.subscription_converter_enabled,
            "subscription_converter_url": self.subscription_converter_url,
            "ui_density": self.ui_density,
            "ui_corner_radius": self.ui_corner_radius,
            "ui_font_family": self.ui_font_family,
            "ui_font_scale": self.ui_font_scale,
            "ui_backdrop": self.ui_backdrop,
            "ui_transparency_strength": self.ui_transparency_strength,
            "ui_theme_preset": self.ui_theme_preset,
            "ui_animations": self.ui_animations,
            "ui_base_tint": self.ui_base_tint,
            "ui_base_tint_src": self.ui_base_tint_src,
            "ui_wallpaper": self.ui_wallpaper,
            "ui_wallpaper_opacity": self.ui_wallpaper_opacity,
            "ui_wallpaper_blur": self.ui_wallpaper_blur,
            "ui_wallpaper_brightness": self.ui_wallpaper_brightness,
            "diagnostics_upload_enabled": self.diagnostics_upload_enabled,
            "onboarding_revision": self.onboarding_revision,
            "onboarding_version": self.onboarding_version,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        let mut xray_release_channel = str_or(data, "xray_release_channel", "beta").trim().to_lowercase();
        if !["stable", "beta"].contains(&xray_release_channel.as_str()) {
            xray_release_channel = "beta".into();
        }
        let (window_width, window_height) = normalize_window_size(data.get("window_width").unwrap_or(&Value::Null), data.get("window_height").unwrap_or(&Value::Null));
        let mut settings = Self {
            theme: str_or(data, "theme", "system"),
            language: normalize_language(data),
            accent_color: str_or(data, "accent_color", "#0078D4"),
            interface_mode: str_or(data, "interface_mode", "full"),
            auto_connect_last: bool_field(data, "auto_connect_last", true),
            start_minimized: bool_field(data, "start_minimized", false),
            enable_system_proxy: bool_field(data, "enable_system_proxy", true),
            system_proxy_bypass_lan: bool_field(data, "system_proxy_bypass_lan", true),
            firefox_proxy_integration: bool_field(data, "firefox_proxy_integration", false),
            launch_on_startup: bool_field(data, "launch_on_startup", false),
            launch_in_tray_on_startup: bool_field(data, "launch_in_tray_on_startup", true),
            always_run_as_admin: bool_field(data, "always_run_as_admin", false),
            block_vpn_conflicts: bool_field(data, "block_vpn_conflicts", true),
            reconnect_on_network_change: bool_field(data, "reconnect_on_network_change", true),
            reconnect_after_sleep: bool_field(data, "reconnect_after_sleep", true),
            regional_preset: str_or(data, "regional_preset", "russia"),
            prefer_ipv6: bool_field(data, "prefer_ipv6", false),
            kill_switch: bool_field(data, "kill_switch", false),
            resource_update_check: bool_field(data, "resource_update_check", false),
            xray_path: str_field(data, "xray_path"),
            log_level: str_or(data, "log_level", "warning"),
            check_updates: bool_field(data, "check_updates", true),
            allow_updates: bool_field(data, "allow_updates", true),
            app_auto_update: bool_field(data, "app_auto_update", false),
            release_channel: str_or(data, "release_channel", "stable"),
            update_feed_url: str_field(data, "update_feed_url"),
            xray_release_channel,
            xray_update_feed_url: str_field(data, "xray_update_feed_url"),
            xray_auto_update: bool_field(data, "xray_auto_update", false),
            enable_xray_fragment: bool_field(data, "enable_xray_fragment", false),
            enable_final_fragment: bool_field(data, "enable_final_fragment", false),
            fragment_packets: str_or(data, "fragment_packets", "tlshello"),
            fragment_length: str_or(data, "fragment_length", "50-100"),
            fragment_delay: str_or(data, "fragment_delay", "10-20"),
            tail_fragment_enabled: bool_field(data, "tail_fragment_enabled", false),
            multiplex_enabled: bool_field(data, "multiplex_enabled", false),
            multiplex_concurrency: int_field(data, "multiplex_concurrency", 8),
            discord_proxy_enabled: bool_field(data, "discord_proxy_enabled", false),
            tun_mode: bool_field(data, "tun_mode", false),
            core_preference: normalize_core_preference(data),
            proxy_allow_lan: bool_field(data, "proxy_allow_lan", false),
            tun_strict_route: if data.get("tun_strict_route_default_v2").and_then(|v| v.as_bool()).unwrap_or(false) {
                bool_field(data, "tun_strict_route", true)
            } else {
                true
            },
            tun_stack: normalize_tun_stack(data),
            tun_mtu: clamp_tun_mtu(data.get("tun_mtu").unwrap_or(&Value::Null)),
            tun_endpoint_independent_nat: bool_field(data, "tun_endpoint_independent_nat", false),
            tun_block_quic: bool_field(data, "tun_block_quic", true),
            local_socks_port: normalize_local_port(int_field(data, "local_socks_port", 10808), 10808),
            local_http_port: normalize_local_port(int_field(data, "local_http_port", 10809), 10809),
            proxy_auth_enabled: bool_field(data, "proxy_auth_enabled", false),
            proxy_auth_username: str_field(data, "proxy_auth_username"),
            proxy_auth_password: str_field(data, "proxy_auth_password"),
            sniff_route_only: bool_field(data, "sniff_route_only", false),
            node_sort_key: str_or(data, "node_sort_key", "manual"),
            node_sort_ascending: bool_field(data, "node_sort_ascending", true),
            node_filter_group: str_field(data, "node_filter_group"),
            selected_subscription_id: str_field(data, "selected_subscription_id"),
            node_table_layout: data
                .get("node_table_layout")
                .and_then(|v| v.as_object())
                .map(|m| Value::Object(m.clone()))
                .unwrap_or_else(|| json!({})),
            zapret_table_layout: data
                .get("zapret_table_layout")
                .and_then(|v| v.as_object())
                .map(|m| Value::Object(m.clone()))
                .unwrap_or_else(|| json!({})),
            xray_config_file: str_field(data, "xray_config_file"),
            xray_template_file: str_field(data, "xray_template_file"),
            singbox_path: str_field(data, "singbox_path"),
            singbox_config_file: str_field(data, "singbox_config_file"),
            singbox_template_file: str_field(data, "singbox_template_file"),
            window_width,
            window_height,
            window_x: int_field(data, "window_x", -1),
            window_y: int_field(data, "window_y", -1),
            zapret_preset: str_field(data, "zapret_preset"),
            zapret_autostart: bool_field(data, "zapret_autostart", false),
            zapret_server_presets: normalize_zapret_server_presets(data.get("zapret_server_presets").unwrap_or(&Value::Null)),
            proxy_chain_enabled: bool_field(data, "proxy_chain_enabled", false),
            proxy_chain_front_node_id: str_field(data, "proxy_chain_front_node_id").trim().to_string(),
            proxy_chain_exit_node_id: str_field(data, "proxy_chain_exit_node_id").trim().to_string(),
            auto_switch_enabled: bool_field(data, "auto_switch_enabled", false),
            auto_switch_threshold_kbps: int_field(data, "auto_switch_threshold_kbps", 50),
            auto_switch_delay_sec: int_field(data, "auto_switch_delay_sec", 30),
            auto_switch_cooldown_sec: int_field(data, "auto_switch_cooldown_sec", 60),
            auto_connect_on_import: bool_field(data, "auto_connect_on_import", false),
            ping_method: str_or(data, "ping_method", "http"),
            speed_test_url: str_field(data, "speed_test_url"),
            speed_test_concurrency: int_field(data, "speed_test_concurrency", 0),
            subscription_auto_update_minutes: match data.get("subscription_auto_update_minutes") {
                Some(Value::Null) | None => 240,
                Some(v) => int_field(&json!({"v": v}), "v", 240),
            },
            subscription_include_regex: str_field(data, "subscription_include_regex"),
            subscription_exclude_regex: str_field(data, "subscription_exclude_regex"),
            subscription_user_agent: str_field(data, "subscription_user_agent"),
            subscription_use_real_hwid: bool_field(data, "subscription_use_real_hwid", true),
            subscription_hwid: str_or(data, "subscription_hwid", DEFAULT_SUBSCRIPTION_HWID),
            subscription_use_proxy_tun: bool_field(data, "subscription_use_proxy_tun", false),
            subscription_converter_enabled: bool_field(data, "subscription_converter_enabled", false),
            subscription_converter_url: str_field(data, "subscription_converter_url"),
            ui_density: str_or(data, "ui_density", "comfortable"),
            ui_corner_radius: int_field(data, "ui_corner_radius", 8),
            ui_font_family: str_field(data, "ui_font_family"),
            ui_font_scale: int_field(data, "ui_font_scale", 100),
            ui_backdrop: str_or(data, "ui_backdrop", "mica"),
            ui_transparency_strength: int_field(data, "ui_transparency_strength", 50),
            ui_theme_preset: str_or(data, "ui_theme_preset", "default"),
            ui_animations: bool_field(data, "ui_animations", true),
            ui_base_tint: str_field(data, "ui_base_tint"),
            ui_base_tint_src: str_field(data, "ui_base_tint_src"),
            ui_wallpaper: str_field(data, "ui_wallpaper"),
            ui_wallpaper_opacity: int_field(data, "ui_wallpaper_opacity", 50),
            ui_wallpaper_blur: int_field(data, "ui_wallpaper_blur", 10),
            ui_wallpaper_brightness: int_field(data, "ui_wallpaper_brightness", 50),
            diagnostics_upload_enabled: bool_field(data, "diagnostics_upload_enabled", true),
            onboarding_revision: int_field(data, "onboarding_revision", 0).max(0),
            onboarding_version: normalize_onboarding_version(data),
        };
        settings.normalize();
        settings
    }
}

fn normalize_onboarding_version(data: &Value) -> String {
    use crate::release_notes::{legacy_profile_version, normalize_seen_version};
    // Profiles saved before 2.0 have no key at all: those users get the
    // "What's new" page; fresh installs start with an empty value.
    if data.get("onboarding_version").is_none() {
        return legacy_profile_version();
    }
    normalize_seen_version(data.get("onboarding_version").unwrap_or(&Value::Null))
}

// ── AppState ──

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub schema_version: i64,
    pub selected_node_id: Option<String>,
    pub nodes: Vec<Node>,
    pub routing: RoutingSettings,
    pub settings: AppSettings,
    pub security: SecuritySettings,
    pub subscriptions: Vec<Value>,
    pub manual_groups: Vec<String>,
    pub routing_presets: Vec<Value>,
    pub applied_migrations: BTreeMap<String, bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            selected_node_id: None,
            nodes: Vec::new(),
            routing: RoutingSettings::default(),
            settings: AppSettings::default(),
            security: SecuritySettings::default(),
            subscriptions: Vec::new(),
            manual_groups: Vec::new(),
            routing_presets: Vec::new(),
            applied_migrations: BTreeMap::new(),
        }
    }
}

impl AppState {
    pub fn to_dict(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "selected_node_id": self.selected_node_id.clone().map(Value::String).unwrap_or(Value::Null),
            "nodes": self.nodes.iter().map(|n| n.to_dict()).collect::<Vec<_>>(),
            "routing": self.routing.to_dict(),
            "settings": self.settings.to_dict(),
            "security": self.security.to_dict(),
            "subscriptions": self.subscriptions.clone(),
            "manual_groups": self.manual_groups,
            "routing_presets": self.routing_presets,
            "applied_migrations": self.applied_migrations,
        })
    }

    pub fn from_dict(data: &Value) -> Self {
        let nodes: Vec<Node> = obj_list(data, "nodes").iter().map(Node::from_dict).collect();
        let mut subscriptions: Vec<Value> = obj_list(data, "subscriptions")
            .into_iter()
            .map(|mut item| {
                if item.get("id").map(|v| v.as_str().map(|s| s.is_empty()).unwrap_or(true)).unwrap_or(true) {
                    item["id"] = json!(Uuid::new_v4().to_string());
                }
                item
            })
            .collect();
        // Re-link nodes to a uniquely matching subscription by group name.
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for subscription in &subscriptions {
            let id = str_field(subscription, "id");
            let group = str_field(subscription, "group").trim().to_string();
            if !group.is_empty() {
                groups.entry(group).or_default().push(id);
            }
        }
        let mut nodes = nodes;
        for node in &mut nodes {
            if !node.subscription_id.is_empty() {
                continue;
            }
            let matches = groups.get(node.group.trim());
            if let Some(m) = matches {
                if m.len() == 1 {
                    node.subscription_id = m[0].clone();
                }
            }
        }
        let _ = &mut subscriptions;
        Self {
            schema_version: int_field(data, "schema_version", STATE_SCHEMA_VERSION),
            selected_node_id: opt_str(data, "selected_node_id"),
            nodes,
            routing: RoutingSettings::from_dict(data.get("routing").unwrap_or(&json!({}))),
            settings: AppSettings::from_dict(data.get("settings").unwrap_or(&json!({}))),
            security: SecuritySettings::from_dict(data.get("security").unwrap_or(&json!({}))),
            subscriptions,
            manual_groups: str_list(data, "manual_groups")
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            routing_presets: obj_list(data, "routing_presets"),
            applied_migrations: {
                let mut m = BTreeMap::new();
                if let Some(Value::Object(map)) = data.get("applied_migrations") {
                    for (k, v) in map {
                        m.insert(k.clone(), v.as_bool().unwrap_or(false));
                    }
                }
                m
            },
        }
    }
}

/// Reference to a persisted subscription document (`storage.py`).
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionRecord {
    pub id: String,
    pub url: String,
    pub name: String,
    pub group: String,
    pub updated_at: Option<String>,
    pub node_count: i64,
}

pub fn parse_subscription_record(value: &Value) -> Option<SubscriptionRecord> {
    Some(SubscriptionRecord {
        id: str_or(value, "id", &Uuid::new_v4().to_string()),
        url: str_field(value, "url"),
        name: str_field(value, "name"),
        group: str_field(value, "group"),
        updated_at: opt_str(value, "updated_at"),
        node_count: int_field(value, "node_count", 0),
    })
}
