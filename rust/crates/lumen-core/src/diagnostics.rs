//! Port of `windows/xray_fluent/diagnostics.py`: allowlisted, scrubbed
//! diagnostics with no implicit machine fingerprinting.
//!
//! Engine versions are probed through a caller-supplied callback because the
//! xray / sing-box managers live in `lumen-engines` (the engines themselves
//! stay external Go binaries).

use std::collections::{BTreeSet, HashSet};
use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc};
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::constants::{APP_VERSION, log_dir};
use crate::diagnostics_uploader::{get_upload_epoch, mark_bundle_generation};
use crate::models::{AppSettings, AppState, RoutingSettings};
use crate::paths::get_install_id;
use crate::pyjson;
use crate::secret_scrubber::{is_sensitive_key, safe_text, scrub_text, scrub_value};

pub const DOMAIN_LOG_FILES: [(&str, &str); 4] =
    [("errors", "errors.log"), ("core", "core.log"), ("app", "app.log"), ("traffic", "traffic.log")];
pub const LOG_ROTATIONS: usize = 3;
pub const DEFAULT_INCLUDE: [(&str, bool); 8] = [
    ("errors", true),
    ("core", true),
    ("app", true),
    ("traffic", true),
    ("state", true),
    ("recent", true),
    ("network", false),
    ("crash", false),
];
pub const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 3 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 48 * 1024 * 1024;
pub const MAX_MEMBERS: usize = 32;
const RECENT_LOG_LINES: usize = 2000;

static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+(?:\.\d+)?(?:[-+._](?:alpha|beta|rc|dev)\d*)?$").unwrap());
const ENGINE_VERSION_PATTERN: &str = r"\d{1,4}(?:\.\d{1,5}){1,3}(?:[-+][0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?";
static ENGINE_VERSION_SEARCH: LazyLock<Regex> = LazyLock::new(|| Regex::new(ENGINE_VERSION_PATTERN).unwrap());
static ENGINE_VERSION_FULL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("^(?:{ENGINE_VERSION_PATTERN})$")).unwrap());
static INSTALL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:[0-9a-f]{32}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$").unwrap()
});
static WINDOWS_VERSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,8}$").unwrap());

fn object_keys(value: Value) -> BTreeSet<String> {
    value.as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default()
}

static SETTING_FIELDS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    let mut fields = object_keys(AppSettings::default().to_dict());
    fields.insert("tun_strict_route_default_v2".into());
    fields
});
const PRIVATE_SETTING_FIELDS: [&str; 11] = [
    "ui_wallpaper",
    "subscription_include_regex",
    "subscription_exclude_regex",
    "subscription_user_agent",
    "node_filter_group",
    "selected_subscription_id",
    "proxy_chain_front_node_id",
    "proxy_chain_exit_node_id",
    "zapret_preset",
    "zapret_server_presets",
    "proxy_auth_username",
];
static ROUTING_FIELDS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    let mut fields = object_keys(RoutingSettings::default().to_dict());
    for extra in ["dns_bootstrap_type_explicit", "tun_default_outbound_user_selected_v2", "tun_default_outbound_explicit"] {
        fields.insert(extra.into());
    }
    fields
});
pub const ROUTING_COLLECTION_COUNTS: [(&str, &str); 8] = [
    ("direct_domains", "direct_domain_count"),
    ("proxy_domains", "proxy_domain_count"),
    ("block_domains", "block_domain_count"),
    ("process_rules", "process_rule_count"),
    ("service_routes", "service_route_count"),
    ("process_preset_routes", "process_preset_route_count"),
    ("dns_hosts", "dns_host_count"),
    ("tun_route_exclude_address", "route_exclusion_count"),
];
const PRIVATE_ROUTING_FIELDS: [&str; 10] = [
    "direct_domains",
    "proxy_domains",
    "block_domains",
    "process_rules",
    "dns_hosts",
    "tun_route_exclude_address",
    "dns_bootstrap_server",
    "dns_bootstrap_servers",
    "dns_proxy_server",
    "dns_proxy_servers",
];

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    /// Generation was cancelled or its consent generation was invalidated.
    #[error("Diagnostic generation cancelled")]
    Cancelled,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("invalid diagnostic archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid diagnostic JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// `include=None` keeps the defaults; otherwise only literal `true` opts in.
pub fn normalize_include(include: Option<&Value>) -> Vec<(&'static str, bool)> {
    match include {
        None => DEFAULT_INCLUDE.to_vec(),
        Some(include) => DEFAULT_INCLUDE
            .iter()
            .map(|(key, _)| (*key, matches!(include.get(*key), Some(Value::Bool(true)))))
            .collect(),
    }
}

fn flag(flags: &[(&str, bool)], key: &str) -> bool {
    flags.iter().any(|(name, selected)| *name == key && *selected)
}

pub fn safe_version(value: &Value) -> String {
    match value {
        Value::String(text) if VERSION.is_match(text) => text.clone(),
        _ => "unknown".into(),
    }
}

pub fn safe_install_id(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if INSTALL_ID.is_match(text) => Some(text.clone()),
        _ => None,
    }
}

pub fn sanitize_runtime(value: &Value) -> Value {
    let mut safe = Map::new();
    let Some(source) = value.as_object() else { return Value::Object(safe) };
    if let Some(Value::String(architecture)) = source.get("architecture") {
        if matches!(architecture.as_str(), "AMD64" | "ARM64" | "x86_64" | "aarch64" | "x86" | "i386" | "i686") {
            safe.insert("architecture".into(), json!(architecture));
        }
    }
    if let Some(Value::String(version)) = source.get("windows_version") {
        if WINDOWS_VERSION.is_match(version) {
            safe.insert("windows_version".into(), json!(version));
        }
    }
    if let Some(Value::Object(engines)) = source.get("engines") {
        let mut out = Map::new();
        for name in ["xray", "sing-box"] {
            let version = match engines.get(name) {
                Some(Value::String(v)) if v.chars().count() <= 80 && ENGINE_VERSION_FULL.is_match(v) => v.clone(),
                _ => "unknown".into(),
            };
            out.insert(name.into(), json!(version));
        }
        safe.insert("engines".into(), Value::Object(out));
    }
    Value::Object(safe)
}

/// Engine probe: `(engine_name, configured_path) -> raw version output`.
/// `engine_name` is `"xray"` or `"sing-box"`.
pub type EngineVersionProbe<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// Technical versions only; never include executable paths or host names.
pub fn collect_runtime_context(state: &AppState, probe: EngineVersionProbe<'_>) -> Value {
    let settings = state.settings.to_dict();
    let mut engines = Map::new();
    for (name, key) in [("xray", "xray_path"), ("sing-box", "singbox_path")] {
        let path = settings.get(key).and_then(Value::as_str).unwrap_or("");
        let version = probe(name, path)
            .and_then(|output| ENGINE_VERSION_SEARCH.find(&output).map(|m| m.as_str().to_string()))
            .unwrap_or_else(|| "unknown".into());
        engines.insert(name.into(), json!(version));
    }
    let mut context = json!({"architecture": lumen_platform::os_info::machine(), "engines": engines});
    if let Some((major, minor, build)) = lumen_platform::os_info::windows_version() {
        context["windows_version"] = json!(format!("{major}.{minor}.{build}"));
    }
    sanitize_runtime(&context)
}

/// Keep model fields and redact their values without reconstructing defaults.
fn sanitize_model_section(value: Option<&Value>, allowed: &BTreeSet<String>, private: &[&str]) -> Map<String, Value> {
    let mut safe = Map::new();
    let Some(Value::Object(section)) = value else { return safe };
    for (key, item) in section {
        if !allowed.contains(key) {
            continue;
        }
        if private.contains(&key.as_str()) || key.ends_with("_file") || (is_sensitive_key(key) && !item.is_boolean()) {
            safe.insert(key.clone(), json!("***"));
        } else {
            // Flags such as subscription_use_real_hwid describe behaviour,
            // whereas the string subscription_hwid is identifying data.
            safe.insert(key.clone(), scrub_value(item));
        }
    }
    safe
}

/// Saved counts describe the original collection, including masked items.
fn collection_count(source: &Map<String, Value>, key: &str, count_key: &str) -> u64 {
    if let Some(Value::Number(previous)) = source.get(count_key) {
        if let Some(previous) = previous.as_u64().filter(|_| previous.is_u64() || previous.is_i64()) {
            if previous <= 10_000_000 {
                return previous;
            }
        }
    }
    match source.get(key) {
        Some(Value::Array(items)) => items.len() as u64,
        Some(Value::Object(items)) => items.len() as u64,
        _ => 0,
    }
}

/// Useful app settings and routing with credentials and personal data
/// redacted. Idempotent on already-redacted state.
pub fn sanitize_state(state: &Value) -> Value {
    let Some(source) = state.as_object() else { return json!({}) };
    let mut safe = Map::new();
    safe.insert("node_count".into(), json!(collection_count(source, "nodes", "node_count")));
    safe.insert("subscription_count".into(), json!(collection_count(source, "subscriptions", "subscription_count")));
    safe.insert("group_count".into(), json!(collection_count(source, "manual_groups", "group_count")));
    safe.insert("routing_preset_count".into(), json!(collection_count(source, "routing_presets", "routing_preset_count")));
    safe.insert(
        "settings".into(),
        Value::Object(sanitize_model_section(source.get("settings"), &SETTING_FIELDS, &PRIVATE_SETTING_FIELDS)),
    );
    if let Some(Value::Number(schema)) = source.get("schema_version") {
        if schema.is_i64() || schema.is_u64() {
            safe.insert("schema_version".into(), Value::Number(schema.clone()));
        }
    }
    if let Some(Value::Object(routing_source)) = source.get("routing") {
        let mut routing = sanitize_model_section(source.get("routing"), &ROUTING_FIELDS, &PRIVATE_ROUTING_FIELDS);
        let mut rule_count = 0u64;
        for (key, count_key) in ROUTING_COLLECTION_COUNTS {
            let count = collection_count(routing_source, key, count_key);
            routing.insert(count_key.into(), json!(count));
            if key != "dns_hosts" && key != "tun_route_exclude_address" {
                rule_count += count;
            }
        }
        safe.insert("routing".into(), Value::Object(routing));
        safe.insert("routing_rule_count".into(), json!(rule_count));
    } else {
        safe.insert("routing_rule_count".into(), json!(collection_count(source, "routing_rules", "routing_rule_count")));
    }
    Value::Object(safe)
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).map(|meta| meta.file_type().is_file()).unwrap_or(false)
}

pub fn collect_log_files(log_dir: &Path, domains: &[&str]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for domain in domains {
        let Some((_, name)) = DOMAIN_LOG_FILES.iter().find(|(d, _)| d == domain) else { continue };
        let suffixes = std::iter::once(String::new()).chain((1..=LOG_ROTATIONS).map(|index| format!(".{index}")));
        for suffix in suffixes {
            let path = log_dir.join(format!("{name}{suffix}"));
            if is_regular_file(&path) {
                found.push(path);
            }
        }
    }
    found
}

/// Reachability of a fixed TCP endpoint, not a guarantee of Internet access.
fn probe_tcp(address: &str) -> bool {
    address
        .parse::<SocketAddr>()
        .ok()
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok())
}

fn system_proxy_status() -> (Option<bool>, Option<bool>) {
    if !cfg!(windows) {
        return (None, None);
    }
    match lumen_platform::inet_settings::read_internet_settings() {
        Some(settings) => (
            Some(settings.proxy_enable.unwrap_or(0) != 0),
            Some(settings.auto_config_url.as_deref().is_some_and(|url| !url.is_empty())),
        ),
        None => (None, None),
    }
}

/// Explicitly selected probes; retain only booleans, never network addresses.
pub fn collect_network_context() -> Value {
    let route = lumen_platform::windows_network::query_windows_route_context();
    let (proxy_enabled, pac_present) = system_proxy_status();
    let mut env_proxies = Map::new();
    for scheme in ["http", "https", "all"] {
        let present = std::env::var_os(format!("{scheme}_proxy")).is_some()
            || std::env::var_os(format!("{}_PROXY", scheme.to_uppercase())).is_some();
        env_proxies.insert(scheme.into(), json!(present));
    }
    json!({
        "probe_performed": true,
        "probe_target": "cloudflare_tcp_443",
        "ipv4_internet": probe_tcp("1.1.1.1:443"),
        "ipv6_internet": probe_tcp("[2606:4700:4700::1111]:443"),
        "ipv6_supported": true,
        "system_dns": [],
        "tun_active": route.map(|r| Value::Bool(r.tun_active)).unwrap_or(Value::Null),
        "system_proxy_enabled": proxy_enabled,
        "system_pac_present": pac_present,
        "proxy_info": {
            "env_proxies": env_proxies,
            "windows_proxy_enable": null,
            "windows_proxy_server": null,
            "windows_auto_config_url": null,
        },
        "connected_adapters": [],
    })
}

pub fn sanitize_network(value: &Value) -> Value {
    let empty = Map::new();
    let source = value.as_object().unwrap_or(&empty);
    let mut safe = Map::new();
    for key in [
        "probe_performed",
        "ipv4_internet",
        "ipv6_internet",
        "ipv6_supported",
        "tun_active",
        "system_proxy_enabled",
        "system_pac_present",
    ] {
        let item = source.get(key).filter(|v| v.is_boolean()).cloned().unwrap_or(Value::Null);
        safe.insert(key.into(), item);
    }
    if source.get("probe_target").and_then(Value::as_str) == Some("cloudflare_tcp_443") {
        safe.insert("probe_target".into(), json!("cloudflare_tcp_443"));
    }
    let proxies = match source.get("proxy_info") {
        Some(Value::Object(info)) => info.get("env_proxies").cloned().unwrap_or_else(|| json!({})),
        _ => source.get("proxy_env_present").cloned().unwrap_or_else(|| json!({})),
    };
    let present = match proxies.as_object() {
        Some(proxies) => {
            let mut out = Map::new();
            for key in ["http", "https", "all"] {
                out.insert(key.into(), json!(matches!(proxies.get(key), Some(Value::Bool(true)))));
            }
            Value::Object(out)
        }
        None => json!({}),
    };
    safe.insert("proxy_env_present".into(), present);
    Value::Object(safe)
}

fn python_isoformat_time(naive: &NaiveDateTime) -> String {
    let mut out = naive.format("%Y-%m-%dT%H:%M:%S").to_string();
    let micros = naive.nanosecond() / 1000 % 1_000_000;
    if micros != 0 {
        out.push_str(&format!(".{:06}", micros));
    }
    out
}

fn python_offset(offset: &FixedOffset) -> String {
    let total = offset.local_minus_utc();
    let sign = if total < 0 { '-' } else { '+' };
    let total = total.abs();
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    if seconds != 0 {
        format!("{sign}{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{sign}{:02}:{:02}", hours, minutes)
    }
}

/// `datetime.fromisoformat(text).isoformat()`; `None` when unparsable.
pub fn reformat_isoformat(text: &str) -> Option<String> {
    let normalized = text.trim().replacen(' ', "T", 1);
    if let Ok(parsed) = DateTime::parse_from_rfc3339(&normalized) {
        let offset = *parsed.offset();
        return Some(python_isoformat_time(&parsed.naive_local()) + &python_offset(&offset));
    }
    for pattern in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&normalized, pattern) {
            return Some(python_isoformat_time(&naive));
        }
    }
    for pattern in ["%Y-%m-%dT%H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%dT%H:%M%:z"] {
        if let Ok(parsed) = DateTime::parse_from_str(&normalized, pattern) {
            let offset = *parsed.offset();
            return Some(python_isoformat_time(&parsed.naive_local()) + &python_offset(&offset));
        }
    }
    NaiveDate::parse_from_str(&normalized, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| python_isoformat_time(&naive))
}

/// UTC `datetime.now(timezone.utc).isoformat()`.
pub fn utc_now_isoformat() -> String {
    let now = Utc::now();
    python_isoformat_time(&now.naive_utc()) + "+00:00"
}

pub fn sanitize_meta(value: &Value, log_files: &[String]) -> Value {
    let empty = Map::new();
    let source = value.as_object().unwrap_or(&empty);
    let selected: Vec<&str> = match source.get("sections") {
        Some(Value::Array(sections)) => DEFAULT_INCLUDE
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| sections.iter().any(|s| s.as_str() == Some(key)))
            .collect(),
        None => Vec::new(),
        Some(_) => Vec::new(),
    };
    let timestamp = match source.get("timestamp") {
        Some(Value::String(text)) => reformat_isoformat(text).unwrap_or_default(),
        _ => String::new(),
    };
    let platform = match source.get("platform").and_then(Value::as_str) {
        Some(name @ ("win32" | "linux" | "darwin" | "Windows" | "Linux" | "Darwin")) => name.to_string(),
        _ => "unknown".into(),
    };
    let mut safe = Map::new();
    safe.insert("app_version".into(), json!(safe_version(source.get("app_version").unwrap_or(&Value::Null))));
    safe.insert("timestamp".into(), json!(timestamp));
    safe.insert("platform".into(), json!(platform));
    safe.insert("python".into(), json!(safe_version(source.get("python").unwrap_or(&Value::Null))));
    safe.insert("sections".into(), json!(selected));
    safe.insert("log_files".into(), json!(log_files));
    if let Some(install_id) = safe_install_id(source.get("install_id").unwrap_or(&Value::Null)) {
        safe.insert("install_id".into(), json!(install_id));
    }
    if let Some(runtime) = source.get("runtime") {
        safe.insert("runtime".into(), sanitize_runtime(runtime));
    }
    if selected.contains(&"network") {
        safe.insert("network".into(), sanitize_network(source.get("network").unwrap_or(&Value::Null)));
    }
    Value::Object(safe)
}

pub fn allowed_members(sections: &[&str]) -> HashSet<String> {
    let mut allowed: HashSet<String> = HashSet::from(["meta.json".to_string()]);
    for (domain, name) in DOMAIN_LOG_FILES {
        if sections.contains(&domain) {
            for suffix in ["", ".1", ".2", ".3"] {
                allowed.insert(format!("logs/{name}{suffix}"));
            }
        }
    }
    if sections.contains(&"state") {
        allowed.insert("state_redacted.json".into());
    }
    if sections.contains(&"recent") {
        allowed.insert("recent_logs.txt".into());
    }
    if sections.contains(&"crash") {
        allowed.insert("logs/faulthandler.log".into());
    }
    allowed
}

/// Total entries declared by the end-of-central-directory record; detects
/// duplicate names that a name-indexed reader would silently collapse.
fn declared_entry_count(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let lowest = data.len().saturating_sub(22 + 65_535);
    let mut index = data.len() - 22;
    loop {
        if data[index..index + 4] == [0x50, 0x4b, 0x05, 0x06] {
            let total = u16::from_le_bytes([data[index + 10], data[index + 11]]);
            return if total == 0xFFFF { None } else { Some(total as usize) };
        }
        if index == lowest {
            return None;
        }
        index -= 1;
    }
}

fn deflated() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn state_json(value: &Value) -> String {
    pyjson::dumps(&sanitize_state(value), true, Some(2))
}

/// Rebuild old ZIPs from allowlisted selected entries; never extract files.
///
/// Rejects oversized/duplicate/encrypted archives. Drops comments/ZIP
/// extras, unknown paths, raw state, identifying metadata and unselected
/// sections.
pub fn sanitize_diagnostic_zip(data: &[u8], cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>, DiagnosticsError> {
    let check = || if cancelled() { Err(DiagnosticsError::Cancelled) } else { Ok(()) };
    check()?;
    if data.len() > MAX_BUNDLE_BYTES {
        return Err(DiagnosticsError::Invalid("Diagnostic archive exceeds size limit"));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(data))?;
    let count = archive.len();
    if count > MAX_MEMBERS || declared_entry_count(data).is_some_and(|declared| declared != count) {
        return Err(DiagnosticsError::Invalid("Invalid diagnostic archive members"));
    }
    let mut names = HashSet::new();
    let mut total: u64 = 0;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = archive.by_index_raw(index)?;
        if !names.insert(entry.name().to_string()) {
            return Err(DiagnosticsError::Invalid("Invalid diagnostic archive members"));
        }
        if entry.encrypted() || entry.size() > MAX_ENTRY_BYTES {
            return Err(DiagnosticsError::Invalid("Unsupported diagnostic archive member"));
        }
        total += entry.size();
        entries.push((entry.name().to_string(), entry.is_dir()));
    }
    if total > MAX_BUNDLE_BYTES as u64 {
        return Err(DiagnosticsError::Invalid("Diagnostic contents exceed size limit"));
    }
    let read_member = |archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str| -> Result<Vec<u8>, DiagnosticsError> {
        let mut file = archive.by_name(name)?;
        let mut raw = Vec::new();
        (&mut file).take(MAX_ENTRY_BYTES + 1).read_to_end(&mut raw)?;
        if raw.len() as u64 > MAX_ENTRY_BYTES {
            return Err(DiagnosticsError::Invalid("Unsupported diagnostic archive member"));
        }
        Ok(raw)
    };
    let metadata: Value = serde_json::from_slice(&read_member(&mut archive, "meta.json")?)?;
    let mut safe_meta = sanitize_meta(&metadata, &[]);
    let sections: Vec<String> = safe_meta["sections"]
        .as_array()
        .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let section_refs: Vec<&str> = sections.iter().map(String::as_str).collect();
    let allowed = allowed_members(&section_refs);
    let mut cleaned = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut files = Vec::new();
    for (name, is_dir) in entries {
        check()?;
        if !allowed.contains(&name) || name == "meta.json" || is_dir {
            continue;
        }
        let raw = String::from_utf8_lossy(&read_member(&mut archive, &name)?).into_owned();
        let text = if name == "state_redacted.json" {
            state_json(&serde_json::from_str::<Value>(&raw)?)
        } else {
            scrub_text(&Value::String(raw))
        };
        cleaned.start_file(name.as_str(), deflated())?;
        cleaned.write_all(text.as_bytes())?;
        if let Some(log_name) = name.strip_prefix("logs/") {
            files.push(log_name.to_string());
        }
    }
    safe_meta["log_files"] = json!(files);
    cleaned.start_file("meta.json", deflated())?;
    cleaned.write_all(pyjson::dumps(&safe_meta, true, Some(2)).as_bytes())?;
    let output = cleaned.finish()?.into_inner();
    check()?;
    Ok(output)
}

/// Reads from the beginning (never tails into a private-key body).
pub fn read_log(path: &Path) -> std::io::Result<String> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.take(MAX_LOG_BYTES as u64 + 1).read_to_end(&mut data)?;
    let truncated = data.len() > MAX_LOG_BYTES;
    data.truncate(MAX_LOG_BYTES);
    let text = scrub_text(&Value::String(String::from_utf8_lossy(&data).into_owned()));
    Ok(if truncated { text + "\n[TRUNCATED diagnostic log]" } else { text })
}

fn temporary_path(dir: &Path) -> PathBuf {
    dir.join(format!(".lumen-diagnostic-{}.tmp", uuid::Uuid::new_v4().simple()))
}

struct TempGuard(Option<PathBuf>);

impl Drop for TempGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Atomic selected export. `Some({})` is metadata-only; `None` keeps the six
/// default sections. Network capabilities and crash logs require explicit
/// `true`. Consent-generation changes and `cancelled` abort the export.
pub fn export_diagnostics(
    zip_path: &Path,
    state: &AppState,
    logs: &[String],
    include: Option<&Value>,
    cancelled: Option<&dyn Fn() -> bool>,
    engine_probe: EngineVersionProbe<'_>,
) -> Result<PathBuf, DiagnosticsError> {
    export_diagnostics_in(&log_dir(), zip_path, state, logs, include, cancelled, engine_probe)
}

/// [`export_diagnostics`] with an explicit log directory.
pub fn export_diagnostics_in(
    logs_dir: &Path,
    zip_path: &Path,
    state: &AppState,
    logs: &[String],
    include: Option<&Value>,
    cancelled: Option<&dyn Fn() -> bool>,
    engine_probe: EngineVersionProbe<'_>,
) -> Result<PathBuf, DiagnosticsError> {
    let epoch = get_upload_epoch();
    mark_bundle_generation(zip_path, epoch);
    let check = || {
        if get_upload_epoch() != epoch || cancelled.is_some_and(|cancelled| cancelled()) {
            Err(DiagnosticsError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    let flags = normalize_include(include);
    let domains: Vec<&str> = DOMAIN_LOG_FILES.iter().map(|(d, _)| *d).filter(|d| flag(&flags, d)).collect();
    let mut files = collect_log_files(logs_dir, &domains);
    let crash = logs_dir.join("faulthandler.log");
    if flag(&flags, "crash") && is_regular_file(&crash) {
        files.push(crash);
    }
    let sections: Vec<&str> = flags.iter().filter(|(_, selected)| *selected).map(|(key, _)| *key).collect();
    let mut metadata = json!({
        "app_version": APP_VERSION,
        "install_id": get_install_id(),
        "timestamp": utc_now_isoformat(),
        "platform": lumen_platform::os_info::sys_platform(),
        // No Python runtime in the Rust build; sanitized to "unknown".
        "python": null,
        "sections": sections,
    });
    metadata["runtime"] = collect_runtime_context(state, engine_probe);
    if flag(&flags, "network") {
        metadata["network"] = collect_network_context();
    }
    let parent = zip_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = temporary_path(parent);
    let mut guard = TempGuard(Some(temporary.clone()));
    let stream = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let mut archive = zip::ZipWriter::new(stream);
    let mut written_files = Vec::new();
    check()?;
    if flag(&flags, "state") {
        archive.start_file("state_redacted.json", deflated())?;
        archive.write_all(state_json(&state.to_dict()).as_bytes())?;
    }
    if flag(&flags, "recent") {
        let start = logs.len().saturating_sub(RECENT_LOG_LINES);
        let joined = logs[start..].iter().map(|line| safe_text(&Value::String(line.clone()))).collect::<Vec<_>>().join("\n");
        archive.start_file("recent_logs.txt", deflated())?;
        archive.write_all(scrub_text(&Value::String(joined)).as_bytes())?;
    }
    for path in &files {
        check()?;
        let Ok(text) = read_log(path) else { continue };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        archive.start_file(format!("logs/{name}"), deflated())?;
        archive.write_all(text.as_bytes())?;
        written_files.push(name);
    }
    archive.start_file("meta.json", deflated())?;
    archive.write_all(pyjson::dumps(&sanitize_meta(&metadata, &written_files), true, Some(2)).as_bytes())?;
    archive.finish()?.sync_all()?;
    check()?;
    std::fs::rename(&temporary, zip_path)?;
    guard.0 = None;
    Ok(zip_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_probe(_: &str, _: &str) -> Option<String> {
        Some("Xray 25.10.15 (Xray, Penetrates Everything.)".into())
    }

    #[test]
    fn include_and_versions() {
        assert_eq!(normalize_include(None), DEFAULT_INCLUDE.to_vec());
        let picked = normalize_include(Some(&json!({"network": true, "errors": 1})));
        assert!(flag(&picked, "network") && !flag(&picked, "errors"));
        assert_eq!(safe_version(&json!("2.1.4")), "2.1.4");
        assert_eq!(safe_version(&json!("2.1.4-beta2")), "2.1.4-beta2");
        assert_eq!(safe_version(&json!("2.1.4 evil")), "unknown");
        assert!(safe_install_id(&json!("0123456789ABCDEF0123456789abcdef")).is_some());
        assert!(safe_install_id(&json!("nope")).is_none());
        let runtime = sanitize_runtime(&json!({"architecture": "AMD64", "windows_version": "10.0.26100",
            "engines": {"xray": "25.10.15", "sing-box": "C:/path"}, "host": "pc"}));
        assert_eq!(runtime, json!({"architecture": "AMD64", "windows_version": "10.0.26100",
            "engines": {"xray": "25.10.15", "sing-box": "unknown"}}));
    }

    #[test]
    fn state_is_redacted_and_idempotent() {
        let mut state = AppState::default();
        state.routing.direct_domains = vec!["secret.example".into(), "b.example".into()];
        let first = sanitize_state(&state.to_dict());
        assert_eq!(first["routing"]["direct_domains"], json!("***"));
        assert_eq!(first["routing"]["direct_domain_count"], json!(2));
        let second = sanitize_state(&first);
        assert_eq!(first, second);
        assert!(!first.to_string().contains("secret.example"));
    }

    #[test]
    fn isoformat_roundtrip() {
        assert_eq!(reformat_isoformat("2026-11-09T18:54:14.252000+00:00").unwrap(), "2026-11-09T18:54:14.252000+00:00");
        assert_eq!(reformat_isoformat("2026-11-09").unwrap(), "2026-11-09T00:00:00");
        assert!(reformat_isoformat("garbage").is_none());
    }

    #[test]
    fn export_then_sanitize_zip() {
        let _epoch_guard = crate::diagnostics_uploader::TEST_EPOCH_LOCK.lock();
        let dir = std::env::temp_dir().join(format!("lumen-diag-{}", uuid::Uuid::new_v4().simple()));
        let logs = dir.join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("app.log"), "hello password=hunter2\n").unwrap();
        let target = dir.join("out").join("diag.zip");
        let state = AppState::default();
        let path = export_diagnostics_in(&logs, &target, &state, &["line".into()], None, None, &no_probe).unwrap();
        let data = std::fs::read(&path).unwrap();
        let cleaned = sanitize_diagnostic_zip(&data, &|| false).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(cleaned.as_slice())).unwrap();
        let mut meta = String::new();
        archive.by_name("meta.json").unwrap().read_to_string(&mut meta).unwrap();
        let meta: Value = serde_json::from_str(&meta).unwrap();
        assert_eq!(meta["log_files"], json!(["app.log"]));
        assert_eq!(meta["runtime"]["engines"]["xray"], json!("25.10.15"));
        assert_eq!(meta["python"], json!("unknown"));
        assert!(archive.by_name("state_redacted.json").is_ok());
        let mut log = String::new();
        archive.by_name("logs/app.log").unwrap().read_to_string(&mut log).unwrap();
        assert!(!log.contains("hunter2"));
        assert!(matches!(sanitize_diagnostic_zip(&data, &|| true), Err(DiagnosticsError::Cancelled)));
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("out")).unwrap().filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
