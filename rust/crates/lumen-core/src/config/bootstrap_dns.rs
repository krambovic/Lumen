//! Trusted resolution of a VPN server name before the tunnel is up.
//!
//! Ported from ZapretKVN. The system resolver of many ISPs answers NXDOMAIN or
//! a stub address for blocked names and plain DNS to public resolvers is
//! intercepted, so the server name is first asked over DoH from several
//! resolvers addressed by literal IP (no SNI is sent for an IP, the
//! certificate is checked against the IP SAN). Then the system resolver is
//! tried, and as a last resort the last address obtained over DoH (kept for up
//! to seven days; system answers are never stored because they may be forged).
//!
//! All functions here are blocking (worker threads only), matching the Python
//! module; call them from `spawn_blocking` in async contexts.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Resolvers whose certificates carry the IP in SAN (HTTP/1.1 DoH).
pub const DOH_SERVERS: [&str; 5] = ["8.8.8.8", "1.1.1.1", "94.140.14.14", "8.8.4.4", "1.0.0.1"];
pub const DOH_PATH: &str = "/dns-query";
pub const CACHE_MAX_AGE_SEC: f64 = 7.0 * 24.0 * 3600.0;

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
const RCODE_NXDOMAIN: u16 = 3;

/// No trusted resolver answered.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BootstrapDnsError {
    /// Trusted resolvers confirmed that the name does not exist.
    #[error("{0}")]
    NameNotFound(String),
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapResolution {
    pub addresses: BTreeSet<String>,
    /// One of "literal" | "doh" | "system" | "cache".
    pub source: &'static str,
}

pub fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .trim_end_matches('.')
        .trim()
        .to_lowercase()
}

/// Wire-format DNS question with ID 0 as RFC 8484 recommends (cache friendly).
pub fn build_query(name: &str, qtype: u16) -> anyhow::Result<Vec<u8>> {
    let ascii = idna::domain_to_ascii(&normalize_host(name))
        .map_err(|e| anyhow::anyhow!("idna encode failed: {e}"))?;
    let mut question = Vec::new();
    for label in ascii.split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        anyhow::ensure!(bytes.len() <= 63, "label too long: {label}");
        question.push(bytes.len() as u8);
        question.extend_from_slice(bytes);
    }
    question.push(0u8);

    let mut out = Vec::with_capacity(12 + question.len() + 4);
    // struct.pack("!HHHHHH", 0, 0x0100, 1, 0, 0, 0)
    out.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&question);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    Ok(out)
}

fn skip_name(payload: &[u8], mut offset: usize) -> anyhow::Result<usize> {
    loop {
        let length = *payload
            .get(offset)
            .ok_or_else(|| anyhow::anyhow!("malformed DNS response"))?;
        if length & 0xC0 == 0xC0 {
            return Ok(offset + 2);
        }
        if length == 0 {
            return Ok(offset + 1);
        }
        offset += 1 + length as usize;
    }
}

/// `(rcode, addresses)` of a DNS answer; error for garbage.
pub fn parse_response(payload: &[u8]) -> anyhow::Result<(u16, Vec<String>)> {
    if payload.len() < 12 {
        anyhow::bail!("malformed DNS response");
    }
    let be16 = |b: &[u8]| u16::from_be_bytes([b[0], b[1]]);
    let flags = be16(&payload[2..4]);
    let questions = be16(&payload[4..6]);
    let answers = be16(&payload[6..8]);

    let mut offset = 12usize;
    for _ in 0..questions {
        offset = skip_name(payload, offset)? + 4;
    }
    let mut addresses: Vec<String> = Vec::new();
    for _ in 0..answers {
        offset = skip_name(payload, offset)?;
        if offset + 10 > payload.len() {
            anyhow::bail!("malformed DNS response");
        }
        let rtype = be16(&payload[offset..offset + 2]);
        let rdlength = be16(&payload[offset + 8..offset + 10]) as usize;
        offset += 10;
        if offset + rdlength > payload.len() {
            anyhow::bail!("truncated record");
        }
        let rdata = &payload[offset..offset + rdlength];
        offset += rdlength;
        if rtype == TYPE_A && rdlength == 4 {
            addresses.push(std::net::Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]).to_string());
        } else if rtype == TYPE_AAAA && rdlength == 16 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(rdata);
            addresses.push(std::net::Ipv6Addr::from(octets).to_string());
        }
    }
    Ok((flags & 0x000F, addresses))
}

fn usable_ip(ip: IpAddr) -> bool {
    if ip.is_unspecified() || ip.is_multicast() || ip.is_loopback() {
        return false;
    }
    // sing-box fake-ip pool: a TUN answer, never the real server.
    if let IpAddr::V4(v4) = ip {
        let octets = v4.octets();
        if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
            return false;
        }
    }
    true
}

pub fn usable_address(address: &str) -> bool {
    address.trim().parse::<IpAddr>().map(usable_ip).unwrap_or(false)
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn value_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A and AAAA over one TLS connection to one resolver.
pub fn doh_lookup(server: &str, host: &str, timeout: f64) -> Result<BTreeSet<String>, BootstrapDnsError> {
    // Zapret resolves the server from the controller's async loop.
    crate::blocking::outside_runtime(|| doh_lookup_blocking(server, host, timeout))
}

fn doh_lookup_blocking(server: &str, host: &str, timeout: f64) -> Result<BTreeSet<String>, BootstrapDnsError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs_f64(timeout.max(0.1)))
        .build()
        .map_err(|e| BootstrapDnsError::Failed(format!("DoH {server}: {e}")))?;
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut name_missing = false;
    for qtype in [TYPE_A, TYPE_AAAA] {
        let body = build_query(host, qtype)
            .map_err(|e| BootstrapDnsError::Failed(format!("DoH {server}: {e}")))?;
        let response = client
            .post(format!("https://{server}:443{DOH_PATH}"))
            .header("Content-Type", "application/dns-message")
            .header("Accept", "application/dns-message")
            .body(body)
            .send()
            .map_err(|e| BootstrapDnsError::Failed(format!("DoH {server}: {e}")))?;
        let status = response.status().as_u16();
        // Python: body = response.read(65535)
        let payload = response
            .bytes()
            .map_err(|e| BootstrapDnsError::Failed(format!("DoH {server}: {e}")))?;
        let payload = &payload[..payload.len().min(65535)];
        if status != 200 {
            return Err(BootstrapDnsError::Failed(format!("DoH {server}: HTTP {status}")));
        }
        let (rcode, addresses) = parse_response(payload)
            .map_err(|e| BootstrapDnsError::Failed(format!("DoH {server}: {e}")))?;
        if rcode == RCODE_NXDOMAIN {
            name_missing = true;
            break;
        }
        if rcode != 0 {
            return Err(BootstrapDnsError::Failed(format!("DoH {server}: rcode {rcode}")));
        }
        found.extend(addresses.into_iter().filter(|a| usable_address(a)));
    }
    if name_missing && found.is_empty() {
        return Err(BootstrapDnsError::NameNotFound(host.to_string()));
    }
    Ok(found)
}

/// First non-empty answer of any resolver; all of them are asked at once.
pub fn resolve_doh(
    host: &str,
    timeout: f64,
    servers: &[&str],
) -> Result<BTreeSet<String>, BootstrapDnsError> {
    resolve_doh_with_lookup(host, timeout, servers, &doh_lookup)
}

fn resolve_doh_with_lookup<F>(
    host: &str,
    timeout: f64,
    servers: &[&str],
    lookup: F,
) -> Result<BTreeSet<String>, BootstrapDnsError>
where
    F: Fn(&str, &str, f64) -> Result<BTreeSet<String>, BootstrapDnsError> + Send + Sync + 'static,
{
    let targets: Vec<&str> = servers.to_vec();
    if targets.is_empty() {
        return Err(BootstrapDnsError::Failed("no trusted resolver answered: None".to_string()));
    }
    let lookup = Arc::new(lookup);
    let (tx, rx) = mpsc::channel::<(usize, Result<BTreeSet<String>, BootstrapDnsError>)>();
    for (index, server) in targets.iter().enumerate() {
        let lookup = Arc::clone(&lookup);
        let server = (*server).to_string();
        let host = host.to_string();
        let tx = tx.clone();
        thread::spawn(move || {
            let result = lookup(&server, &host, timeout);
            let _ = tx.send((index, result));
        });
    }
    drop(tx);

    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0) + 1.0);
    let mut missing = 0usize;
    let mut last_error: Option<String> = None;
    let mut outstanding = targets.len();
    while outstanding > 0 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok((_index, result)) => {
                outstanding -= 1;
                match result {
                    Ok(addresses) if !addresses.is_empty() => return Ok(addresses),
                    Ok(_) => {}
                    Err(BootstrapDnsError::NameNotFound(_)) => {
                        missing += 1;
                        last_error.get_or_insert_with(|| "name not found".to_string());
                    }
                    Err(err) => last_error = Some(err.to_string()),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if missing > 0 && missing == targets.len() {
        return Err(BootstrapDnsError::NameNotFound(host.to_string()));
    }
    Err(BootstrapDnsError::Failed(format!(
        "no trusted resolver answered: {}",
        last_error.as_deref().unwrap_or("None")
    )))
}

/// Last server addresses obtained over the trusted path.
pub struct BootstrapCache {
    path: PathBuf,
    lock: parking_lot::Mutex<()>,
}

impl BootstrapCache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: parking_lot::Mutex::new(()),
        }
    }

    fn load_entries(&self) -> BTreeMap<String, Value> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return BTreeMap::new();
        };
        let Ok(payload) = serde_json::from_str::<Value>(&text) else {
            return BTreeMap::new();
        };
        payload
            .get("hosts")
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    fn parse_saved_at(entry: Option<&Value>) -> Option<f64> {
        // Python: float(entry.get("saved_at", 0) or 0); TypeError/ValueError -> not fresh.
        let entry = entry?.as_object()?;
        let saved_at = match entry.get("saved_at") {
            None | Some(Value::Null) => 0.0,
            Some(Value::Number(n)) => n.as_f64()?,
            Some(Value::Bool(b)) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Some(Value::String(s)) => {
                if s.trim().is_empty() {
                    0.0
                } else {
                    s.trim().parse::<f64>().ok()?
                }
            }
            _ => return None,
        };
        Some(saved_at)
    }

    fn fresh(entry: Option<&Value>, now: f64) -> bool {
        let Some(saved_at) = Self::parse_saved_at(entry) else {
            return false;
        };
        let age = now - saved_at;
        (0.0..=CACHE_MAX_AGE_SEC).contains(&age)
    }

    pub fn lookup(&self, host: &str) -> Vec<String> {
        let host = normalize_host(host);
        let now = now_secs();
        let entry = {
            let _guard = self.lock.lock();
            self.load_entries().get(&host).cloned()
        };
        if !Self::fresh(entry.as_ref(), now) {
            return Vec::new();
        }
        let mut result: Vec<String> = Vec::new();
        if let Some(addresses) = entry
            .as_ref()
            .and_then(|entry| entry.get("addresses"))
            .and_then(|v| v.as_array())
        {
            for raw in addresses {
                let address = value_str(raw);
                let Ok(ip) = address.trim().parse::<IpAddr>() else {
                    continue;
                };
                if !usable_ip(ip) {
                    continue;
                }
                let canonical = ip.to_string();
                if !result.contains(&canonical) {
                    result.push(canonical);
                }
            }
        }
        result
    }

    pub fn remember(&self, host: &str, addresses: impl IntoIterator<Item = String>) {
        let host = normalize_host(host);
        let mut cleaned: Vec<String> = addresses
            .into_iter()
            .filter_map(|a| a.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string()))
            .collect();
        cleaned.sort();
        cleaned.dedup();
        // Python sort key: (":" in a, a) — IPv4 first, then lexical.
        cleaned.sort_by(|a, b| {
            (a.contains(':'), a.as_str()).cmp(&(b.contains(':'), b.as_str()))
        });
        if host.is_empty() || cleaned.is_empty() {
            return;
        }
        let now = now_secs();
        let payload = {
            let _guard = self.lock.lock();
            let mut entries: BTreeMap<String, Value> = self
                .load_entries()
                .into_iter()
                .filter(|(_, entry)| Self::fresh(Some(entry), now))
                .collect();
            let same = entries.get(&host).and_then(|entry| entry.get("addresses")).map(|a| {
                a.as_array()
                    .map(|arr| arr.iter().map(value_str).collect::<Vec<_>>() == cleaned)
                    .unwrap_or(false)
            });
            if same == Some(true) {
                if let Some(saved_at) = Self::parse_saved_at(entries.get(&host)) {
                    if now - saved_at < 3600.0 {
                        return;
                    }
                }
            }
            entries.insert(host.clone(), json!({"addresses": cleaned, "saved_at": now}));
            json!({"version": 1, "hosts": entries})
        };
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(serde_json::to_string_pretty(&payload).unwrap_or_default().as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        };
        // The cache is a safety net only.
        let _ = write();
    }
}

static CACHE: parking_lot::Mutex<Option<Arc<BootstrapCache>>> = parking_lot::Mutex::new(None);

/// Enable the persistent last-known-good cache (app) or disable it (tests).
pub fn configure_cache(path: Option<PathBuf>) {
    *CACHE.lock() = path.map(|path| Arc::new(BootstrapCache::new(path)));
}

fn current_cache() -> Option<Arc<BootstrapCache>> {
    CACHE.lock().clone()
}

/// Default LKG cache location: `DATA_DIR/bootstrap-lkg.json`.
pub fn default_cache_path() -> PathBuf {
    crate::constants::data_dir().join("bootstrap-lkg.json")
}

fn system_lookup(host: &str) -> Result<BTreeSet<String>, String> {
    let mut resolved: BTreeSet<String> = BTreeSet::new();
    let iter = (host, 0u16)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?;
    for info in iter {
        let mut address = info.ip().to_string();
        if let Some(pos) = address.find('%') {
            address.truncate(pos);
        }
        let Ok(ip) = address.parse::<IpAddr>() else {
            continue;
        };
        if usable_ip(ip) {
            resolved.insert(ip.to_string());
        }
    }
    Ok(resolved)
}

/// Server addresses: trusted DoH -> system resolver -> remembered address.
///
/// Blocking, worker threads only. Errors when none of the three sources knows
/// the name. Python defaults: `timeout=3.0`.
pub fn resolve_bootstrap(host: &str, timeout: f64) -> anyhow::Result<BootstrapResolution> {
    resolve_bootstrap_with(host, timeout, None, &system_lookup)
}

pub fn resolve_bootstrap_with(
    host: &str,
    timeout: f64,
    doh: Option<&dyn Fn(&str) -> Result<BTreeSet<String>, BootstrapDnsError>>,
    system: &dyn Fn(&str) -> Result<BTreeSet<String>, String>,
) -> anyhow::Result<BootstrapResolution> {
    let host = normalize_host(host);
    if host.is_empty() {
        return Ok(BootstrapResolution {
            addresses: BTreeSet::new(),
            source: "literal",
        });
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(BootstrapResolution {
            addresses: BTreeSet::from([ip.to_string()]),
            source: "literal",
        });
    }

    let default_doh = |name: &str| resolve_doh(name, timeout, &DOH_SERVERS);
    let lookup: &dyn Fn(&str) -> Result<BTreeSet<String>, BootstrapDnsError> =
        doh.unwrap_or(&default_doh);
    let mut trusted_error: Option<String> = None;
    let addresses: BTreeSet<String> = match lookup(&host) {
        Ok(set) => set.into_iter().filter(|a| usable_address(a)).collect(),
        // Trusted resolvers deny the name: neither ISP answers nor old cache are trustworthy.
        Err(BootstrapDnsError::NameNotFound(_)) => {
            return Err(BootstrapDnsError::NameNotFound(host).into());
        }
        Err(err) => {
            trusted_error = Some(err.to_string());
            BTreeSet::new()
        }
    };
    if !addresses.is_empty() {
        if let Some(cache) = current_cache() {
            cache.remember(&host, addresses.iter().cloned());
        }
        return Ok(BootstrapResolution {
            addresses,
            source: "doh",
        });
    }

    let mut system_error: Option<String> = None;
    let addresses: BTreeSet<String> = match system(&host) {
        Ok(set) => set.into_iter().filter(|a| usable_address(a)).collect(),
        Err(err) => {
            system_error = Some(err);
            BTreeSet::new()
        }
    };
    if !addresses.is_empty() {
        return Ok(BootstrapResolution {
            addresses,
            source: "system",
        });
    }

    let cached = current_cache().map(|cache| cache.lookup(&host)).unwrap_or_default();
    if !cached.is_empty() {
        return Ok(BootstrapResolution {
            addresses: cached.into_iter().collect(),
            source: "cache",
        });
    }

    match system_error {
        Some(err) => Err(anyhow::anyhow!(err)),
        None => Err(anyhow::anyhow!(BootstrapDnsError::Failed(format!(
            "server name not resolved: {}",
            trusted_error.as_deref().unwrap_or("None")
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_wire_format() {
        let query = build_query("Example.COM.", TYPE_A).unwrap();
        assert_eq!(&query[..12], &[0, 0, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&query[12..20], b"\x07example");
        assert_eq!(&query[query.len() - 4..], &[0, 1, 0, 1]);
        assert_eq!(query.len(), 12 + 1 + 7 + 1 + 3 + 1 + 4);
    }

    #[test]
    fn parse_response_roundtrip() {
        let mut payload = vec![0, 3, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        payload.extend_from_slice(b"\x01a\x00"); // question name "a"
        payload.extend_from_slice(&[0, 1, 0, 1]); // qtype A, class IN
        payload.extend_from_slice(&[0xC0, 0x0C]); // pointer to question name
        payload.extend_from_slice(&[0, 1, 0, 1]);
        payload.extend_from_slice(&[0, 0, 0, 60]); // ttl
        payload.extend_from_slice(&[0, 4, 1, 2, 3, 4]); // rdlength + A record
        let (rcode, addresses) = parse_response(&payload).unwrap();
        assert_eq!(rcode, 0);
        assert_eq!(addresses, vec!["1.2.3.4".to_string()]);

        assert!(parse_response(&[0, 0]).is_err());
    }

    #[test]
    fn usable_address_filters() {
        assert!(usable_address("1.2.3.4"));
        assert!(usable_address("2606:4700::1"));
        assert!(!usable_address("198.18.0.1"));
        assert!(!usable_address("198.19.255.255"));
        assert!(!usable_address("127.0.0.1"));
        assert!(!usable_address("0.0.0.0"));
        assert!(!usable_address("224.0.0.1"));
        assert!(!usable_address("example.com"));
    }

    #[test]
    fn normalize_host_strips() {
        assert_eq!(normalize_host(" [Example.COM.] "), "example.com");
        assert_eq!(normalize_host("Host.Zone."), "host.zone");
    }

    #[test]
    fn resolve_prefers_literal() {
        let resolution = resolve_bootstrap(" 1.2.3.4 ", 3.0).unwrap();
        assert_eq!(resolution.source, "literal");
        assert_eq!(resolution.addresses, BTreeSet::from(["1.2.3.4".to_string()]));
    }

    #[test]
    fn resolve_falls_back_to_system_then_cache() {
        configure_cache(None);
        let doh = |_host: &str| -> Result<BTreeSet<String>, BootstrapDnsError> {
            Err(BootstrapDnsError::Failed("down".into()))
        };
        let system = |_host: &str| -> Result<BTreeSet<String>, String> {
            Ok(BTreeSet::from(["5.6.7.8".to_string(), "198.18.0.9".to_string()]))
        };
        let resolution = resolve_bootstrap_with("blocked.example", 3.0, Some(&doh), &system).unwrap();
        assert_eq!(resolution.source, "system");
        // The fake-ip address is filtered out.
        assert_eq!(resolution.addresses, BTreeSet::from(["5.6.7.8".to_string()]));

        let fail = |_host: &str| -> Result<BTreeSet<String>, String> {
            Err("resolver down".to_string())
        };
        // Python: `raise system_error` when the system resolver produced an error.
        let err = resolve_bootstrap_with("blocked.example", 3.0, Some(&doh), &fail).unwrap_err();
        assert!(err.to_string().contains("resolver down"));
    }

    #[test]
    fn resolve_propagates_name_not_found() {
        configure_cache(None);
        let doh = |_host: &str| -> Result<BTreeSet<String>, BootstrapDnsError> {
            Err(BootstrapDnsError::NameNotFound("nx.example".into()))
        };
        let system = |_host: &str| -> Result<BTreeSet<String>, String> {
            Ok(BTreeSet::from(["5.6.7.8".to_string()]))
        };
        assert!(resolve_bootstrap_with("nx.example", 3.0, Some(&doh), &system).is_err());
    }

    #[test]
    fn cache_remember_and_lookup_roundtrip() {
        let dir = std::env::temp_dir().join(format!("lumen-bootstrap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bootstrap-lkg.json");
        configure_cache(Some(path.clone()));
        let cache = current_cache().unwrap();
        cache.remember("Server.Example.", vec!["9.9.9.9".into(), "198.18.5.5".into()]);
        let cached = cache.lookup("server.example");
        assert_eq!(cached, vec!["9.9.9.9".to_string()]);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"version\": 1"));
        configure_cache(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_doh_reports_all_missing() {
        configure_cache(None);
        let lookup = |_server: &str, _host: &str, _timeout: f64| -> Result<
            BTreeSet<String>,
            BootstrapDnsError,
        > {
            Err(BootstrapDnsError::Failed("offline".into()))
        };
        let err = resolve_doh_with_lookup("host.example", 0.5, &["1.1.1.1", "8.8.8.8"], lookup).unwrap_err();
        assert!(err.to_string().contains("no trusted resolver answered"));

        let missing = |_server: &str, _host: &str, _timeout: f64| -> Result<
            BTreeSet<String>,
            BootstrapDnsError,
        > {
            Err(BootstrapDnsError::NameNotFound("host.example".into()))
        };
        let err = resolve_doh_with_lookup("host.example", 0.5, &["1.1.1.1", "8.8.8.8"], missing).unwrap_err();
        assert!(matches!(err, BootstrapDnsError::NameNotFound(_)));
    }
}
