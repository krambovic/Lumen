//! Shared HTTP transports: proxied, direct (bypassing proxies and Windows
//! TUN routes), safe redirects, connectivity and warm latency probes.
//!
//! Port of `http_utils.py`, `direct_http.py`, `http_redirect_policy.py`,
//! `connectivity_test.py` and `http_latency.py`.
//!
//! TLS uses rustls with the native Windows trust store
//! (`rustls-tls-native-roots`), the equivalent of Python's `truststore`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lumen_platform::network_route_context::get_windows_default_route_context;
use lumen_platform::route_leases::{acquire_route, release_route};
use lumen_platform::run_text_owned;
use url::{Position, Url};

use crate::constants::APP_VERSION;

const FALLBACK_REDIRECTS: usize = 10;
const REDIRECT_ALLOWED_HEADERS: [&str; 3] = ["accept", "accept-encoding", "user-agent"];

/// Raised rather than silently leaking a direct-only request into TUN.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct DirectNetworkUnavailable(pub String);

fn is_ipv4(value: &str) -> bool {
    value.parse::<Ipv4Addr>().is_ok()
}

fn skip_dns_name(data: &[u8], mut offset: usize) -> Option<usize> {
    while offset < data.len() {
        let length = data[offset];
        if length == 0 {
            return Some(offset + 1);
        }
        if length & 0xC0 == 0xC0 {
            return Some(offset + 2);
        }
        offset += length as usize + 1;
    }
    None
}

/// Build an A-record query packet; `None` for invalid names.
fn build_dns_query(host: &str, transaction_id: u16) -> Option<Vec<u8>> {
    let ascii = idna::domain_to_ascii(host.trim_end_matches('.')).ok()?;
    let labels: Vec<&str> = ascii.split('.').filter(|p| !p.is_empty()).collect();
    if labels.is_empty() || labels.iter().any(|l| l.is_empty() || l.len() > 63) {
        return None;
    }
    let mut qname = Vec::new();
    for label in labels {
        qname.push(label.len() as u8);
        qname.extend_from_slice(label.as_bytes());
    }
    qname.push(0);
    if qname.len() > 255 {
        return None;
    }
    let mut packet = Vec::with_capacity(12 + qname.len() + 4);
    for v in [transaction_id, 0x0100, 1, 0, 0, 0] {
        packet.extend_from_slice(&v.to_be_bytes());
    }
    packet.extend_from_slice(&qname);
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    Some(packet)
}

/// Parse the first A answer of a response to `packet`.
fn parse_dns_answer(packet: &[u8], data: &[u8]) -> Option<String> {
    let be16 = |o: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(o)?, *data.get(o + 1)?])) };
    if data.len() < 12 || data[..2] != packet[..2] {
        return None;
    }
    let flags = be16(2)?;
    if flags & 0x8000 == 0 || flags & 0x020F != 0 {
        return None;
    }
    let question_count = be16(4)?;
    let answer_count = be16(6)?;
    let question = &packet[12..];
    if question_count != 1 || data.get(12..12 + question.len())? != question {
        return None;
    }
    let mut offset = 12;
    for _ in 0..question_count {
        offset = skip_dns_name(data, offset)? + 4;
    }
    for _ in 0..answer_count {
        offset = skip_dns_name(data, offset)?;
        let record_type = be16(offset)?;
        let record_class = be16(offset + 2)?;
        let data_length = be16(offset + 8)? as usize;
        offset += 10;
        if record_type == 1 && record_class == 1 && data_length == 4 {
            let b = data.get(offset..offset + 4)?;
            return Some(Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string());
        }
        offset += data_length;
    }
    None
}

/// Resolve an A record by asking `dns_server` directly over UDP/53.
pub fn resolve_a_direct(host: &str, dns_server: &str, timeout: f64) -> String {
    let transaction_id: u16 = rand::random();
    let Some(packet) = build_dns_query(host, transaction_id) else {
        return String::new();
    };
    let attempt = || -> std::io::Result<Vec<u8>> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_read_timeout(Some(Duration::from_secs_f64(timeout)))?;
        // Connected UDP rejects datagrams from a different source/port.
        sock.connect((dns_server, 53))?;
        sock.send(&packet)?;
        let mut buf = vec![0u8; 4096];
        let n = sock.recv(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    };
    match attempt() {
        Ok(data) => parse_dns_answer(&packet, &data).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

fn route_key(gateway: &str, ip: &str, index: u32) -> String {
    format!("{gateway}|{ip}|{index}")
}

#[derive(Default)]
struct RouteState {
    added: HashSet<String>,
    resolved: HashMap<String, String>,
}

/// Temporary physical-gateway /32 routes for one HTTP session
/// (`WindowsDirectRoute`). Routes are released on drop.
pub struct WindowsDirectRoute {
    bypass_required: bool,
    interface_index: u32,
    gateway: String,
    dns_servers: Vec<String>,
    state: Mutex<RouteState>,
}

impl WindowsDirectRoute {
    /// `__enter__`: discover the physical route; fail closed when TUN is
    /// active but no physical interface can be used.
    pub fn open() -> Result<Self, DirectNetworkUnavailable> {
        let mut route = Self {
            bypass_required: false,
            interface_index: 0,
            gateway: String::new(),
            dns_servers: Vec::new(),
            state: Mutex::new(RouteState::default()),
        };
        if !cfg!(windows) {
            return Ok(route);
        }
        let context = get_windows_default_route_context(false)
            .or_else(|| get_windows_default_route_context(true))
            .ok_or_else(|| {
                DirectNetworkUnavailable(
                    "Не удалось определить физический интернет-интерфейс для прямой загрузки".into(),
                )
            })?;
        route.bypass_required = context.tun_active;
        if !route.bypass_required {
            return Ok(route);
        }
        if !context.is_physical || context.interface_index == 0 || !is_ipv4(&context.next_hop) {
            return Err(DirectNetworkUnavailable(
                "Не удалось обойти TUN: физический интернет-интерфейс недоступен".into(),
            ));
        }
        route.interface_index = context.interface_index;
        route.gateway = context.next_hop;
        let mut dns = Vec::new();
        for item in context.dns_servers {
            if is_ipv4(&item) && !dns.contains(&item) {
                dns.push(item);
            }
        }
        route.dns_servers = dns;
        Ok(route)
    }

    pub fn bypass_required(&self) -> bool {
        self.bypass_required
    }

    fn has_exact_route(gateway: &str, index: u32, ip: &str) -> bool {
        let script = format!(
            "$route = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '{ip}/32' \
             -ErrorAction SilentlyContinue | Where-Object {{ \
             $_.InterfaceIndex -eq {index} -and $_.NextHop -eq '{gateway}' \
             }} | Select-Object -First 1; if ($route) {{ exit 0 }} else {{ exit 1 }}"
        );
        let command = vec![
            "powershell".to_string(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script,
        ];
        matches!(run_text_owned(&command, 3.0, false), Ok(r) if r.returncode == 0)
    }

    fn ensure_route(&self, ip: &str) -> bool {
        if !self.bypass_required {
            return true;
        }
        if self.state.lock().map(|s| s.added.contains(ip)).unwrap_or(false) {
            return true;
        }
        let gateway = self.gateway.clone();
        let index = self.interface_index;
        let ip_owned = ip.to_string();
        let create = {
            let (gateway, ip) = (gateway.clone(), ip_owned.clone());
            move || {
                let command: Vec<String> = [
                    "route", "add", &ip, "mask", "255.255.255.255", &gateway, "metric", "1", "if",
                    &index.to_string(),
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                matches!(run_text_owned(&command, 3.0, false), Ok(r) if r.returncode == 0)
            }
        };
        let exists = {
            let (gateway, ip) = (gateway.clone(), ip_owned.clone());
            move || Self::has_exact_route(&gateway, index, &ip)
        };
        let delete = {
            let (gateway, ip) = (gateway.clone(), ip_owned.clone());
            move || {
                let command: Vec<String> = [
                    "route", "delete", &ip, "mask", "255.255.255.255", &gateway, "if",
                    &index.to_string(),
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                let _ = run_text_owned(&command, 3.0, false);
            }
        };
        if !acquire_route(&route_key(&gateway, ip, index), create, exists, delete) {
            return false;
        }
        if let Ok(mut state) = self.state.lock() {
            state.added.insert(ip_owned);
        }
        true
    }

    /// Resolve `host` to an address that is reachable outside the TUN.
    pub fn resolve(&self, host: &str) -> Result<String, DirectNetworkUnavailable> {
        let host = host.trim().trim_matches(|c| c == '[' || c == ']').to_string();
        if host.is_empty() {
            return Err(DirectNetworkUnavailable("Пустой адрес сервера".into()));
        }
        if !self.bypass_required {
            return Ok(host);
        }
        if let Some(ip) = self.state.lock().ok().and_then(|s| s.resolved.get(&host).cloned()) {
            return Ok(ip);
        }
        if let Ok(addr) = host.parse::<Ipv4Addr>() {
            if !addr.is_loopback() && !self.ensure_route(&host) {
                return Err(DirectNetworkUnavailable(format!(
                    "Не удалось создать прямой маршрут к {host}"
                )));
            }
            if let Ok(mut s) = self.state.lock() {
                s.resolved.insert(host.clone(), host.clone());
            }
            return Ok(host);
        }
        for dns_server in &self.dns_servers {
            if !self.ensure_route(dns_server) {
                continue;
            }
            let ip = resolve_a_direct(&host, dns_server, 2.5);
            if ip.is_empty()
                || ["0.", "127.", "169.254.", "198.18.", "198.19."].iter().any(|p| ip.starts_with(p))
            {
                continue;
            }
            if !self.ensure_route(&ip) {
                continue;
            }
            if let Ok(mut s) = self.state.lock() {
                s.resolved.insert(host.clone(), ip.clone());
            }
            return Ok(ip);
        }
        Err(DirectNetworkUnavailable(format!(
            "Не удалось напрямую определить адрес сервера: {host}"
        )))
    }
}

impl Drop for WindowsDirectRoute {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            for ip in state.added.drain() {
                release_route(&route_key(&self.gateway, &ip, self.interface_index));
            }
            state.resolved.clear();
        }
    }
}

struct DirectResolver(Arc<WindowsDirectRoute>);

impl reqwest::dns::Resolve for DirectResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let route = self.0.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let bypass = route.bypass_required();
            let target = tokio::task::spawn_blocking(move || route.resolve(&host))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            let addrs: Vec<SocketAddr> = if bypass {
                vec![SocketAddr::new(target.parse().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?, 0)]
            } else {
                tokio::net::lookup_host((target.as_str(), 0)).await?.collect()
            };
            let iter: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(iter)
        })
    }
}

/// Default `User-Agent` used by every Lumen HTTP client.
pub fn user_agent() -> String {
    format!("Lumen/{APP_VERSION}")
}

/// Client that ignores system/environment proxies and resolves every host
/// through the physical gateway when TUN is up. Redirects are not followed
/// automatically; use [`get_with_safe_redirects`]. Keep the returned route
/// alive for the lifetime of the client (it owns the temporary routes).
pub fn direct_client(timeout: f64) -> Result<(reqwest::Client, Arc<WindowsDirectRoute>), DirectNetworkUnavailable> {
    let route = Arc::new(WindowsDirectRoute::open()?);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs_f64(timeout.max(0.001)))
        .dns_resolver(Arc::new(DirectResolver(route.clone())))
        .user_agent(user_agent())
        .build()
        .map_err(|e| DirectNetworkUnavailable(e.to_string()))?;
    Ok((client, route))
}

/// Client through `proxy_url` (`build_proxy_opener`); without a proxy the
/// client still disables environment proxy detection.
pub fn proxy_client(proxy_url: Option<&str>, timeout: f64) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs_f64(timeout.max(0.001)))
        .user_agent(user_agent());
    match proxy_url.map(str::trim).filter(|p| !p.is_empty()) {
        Some(proxy) => builder = builder.proxy(reqwest::Proxy::all(proxy)?),
        None => builder = builder.no_proxy(),
    }
    Ok(builder.build()?)
}

/// `SafeRedirectHandler` decision: true when the redirect must be blocked.
pub fn redirect_blocked(source: &Url, dest: &Url, raw_location: &str) -> bool {
    if !matches!(dest.scheme(), "http" | "https") {
        return true;
    }
    if source.scheme() == "https" && dest.scheme() != "https" {
        return true;
    }
    if dest[Position::AfterScheme..Position::BeforePath].contains('@') {
        return true;
    }
    // The url crate drops an empty `@` userinfo that urlsplit keeps.
    if let Some(index) = raw_location.find("://") {
        let rest = &raw_location[index + 3..];
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        if rest[..end].contains('@') {
            return true;
        }
    }
    false
}

fn origin(url: &Url) -> (String, String, Option<u16>) {
    (url.scheme().into(), url.host_str().unwrap_or("").to_lowercase(), url.port_or_known_default())
}

/// GET following redirects with the SafeRedirectHandler policy: unsafe
/// targets are rejected and identifying headers never cross origins.
pub async fn get_with_safe_redirects(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> anyhow::Result<reqwest::Response> {
    let mut current = Url::parse(url.trim())?;
    let mut current_headers: BTreeMap<String, String> =
        headers.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect();
    for _ in 0..=FALLBACK_REDIRECTS {
        let mut request = client.get(current.clone());
        for (name, value) in &current_headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        if !matches!(status, 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .trim()
            .to_string();
        if location.is_empty() {
            return Ok(response);
        }
        let dest = current.join(&location)?;
        if redirect_blocked(&current, &dest, &location) {
            anyhow::bail!("HTTP Error {status}: Unsafe subscription redirect blocked");
        }
        if origin(&current) != origin(&dest) {
            current_headers.retain(|name, _| REDIRECT_ALLOWED_HEADERS.contains(&name.as_str()));
        }
        current = dest;
    }
    anyhow::bail!("HTTP Error 310: redirect limit reached")
}

/// `urlopen_direct`: GET without proxies and outside an active TUN.
pub async fn get_direct(url: &str, headers: &BTreeMap<String, String>, timeout: f64) -> anyhow::Result<(reqwest::Response, Arc<WindowsDirectRoute>)> {
    let (client, route) = direct_client(timeout)?;
    let response = get_with_safe_redirects(&client, url, headers).await?;
    Ok((response, route))
}

/// `urlopen_proxy_first`: through the app proxy first, then direct.
pub async fn get_proxy_first(
    url: &str,
    headers: &BTreeMap<String, String>,
    timeout: f64,
    proxy_url: Option<&str>,
) -> anyhow::Result<(reqwest::Response, Option<Arc<WindowsDirectRoute>>)> {
    if let Some(proxy) = proxy_url.filter(|p| !p.trim().is_empty()) {
        if let Ok(client) = proxy_client(Some(proxy), timeout) {
            if let Ok(response) = get_with_safe_redirects(&client, url, headers).await {
                return Ok((response, None));
            }
        }
    }
    let (response, route) = get_direct(url, headers, timeout).await?;
    Ok((response, Some(route)))
}

/// Read the full body with a hard size cap.
pub async fn read_body_limited(mut response: reqwest::Response, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > max_bytes {
            anyhow::bail!("ответ превышает допустимый размер {max_bytes} байт");
        }
    }
    Ok(body)
}

/// `ConnectivityTestWorker.run`: `(ok, message, elapsed_ms)`.
pub async fn connectivity_test(
    http_port: u16,
    url: &str,
    timeout: f64,
    tun_mode: bool,
    proxy_url: &str,
) -> (bool, String, Option<u64>) {
    let proxy = if tun_mode {
        None
    } else if proxy_url.trim().is_empty() {
        Some(format!("http://127.0.0.1:{http_port}"))
    } else {
        Some(proxy_url.trim().to_string())
    };
    let client = match proxy_client(proxy.as_deref(), timeout) {
        Ok(c) => c,
        Err(exc) => return (false, format!("{url} -> {exc}"), None),
    };
    let started = Instant::now();
    let client = if tun_mode {
        // build_opener() keeps urllib's redirect following.
        match reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(timeout))
            .user_agent(user_agent())
            .build()
        {
            Ok(c) => c,
            Err(_) => client,
        }
    } else {
        client
    };
    match client.get(url).send().await {
        Ok(mut response) => {
            let status = response.status().as_u16();
            if status >= 400 {
                return (false, format!("{url} -> HTTP Error {status}: {}", response.status().canonical_reason().unwrap_or("")), None);
            }
            let _ = response.chunk().await;
            let elapsed = started.elapsed().as_millis() as u64;
            (true, format!("{url} -> HTTP {status}"), Some(elapsed))
        }
        Err(exc) => (false, format!("{url} -> {exc}"), None),
    }
}

/// Verified HTTP GET latency over a kept-alive local proxy tunnel
/// (`WarmHTTPProbe`). The first request warms the tunnel and is discarded;
/// the result is the best of two follow-up samples.
#[derive(Default)]
pub struct WarmHttpProbe {
    client: Option<(String, String, reqwest::Client)>,
}

impl WarmHttpProbe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&mut self) {
        self.client = None;
    }

    pub async fn measure(&mut self, proxy_url: &str, url: &str, timeout: f64) -> anyhow::Result<u64> {
        let proxy = Url::parse(proxy_url)?;
        let target = Url::parse(url)?;
        if proxy.scheme() != "http" || !matches!(proxy.host_str(), Some("127.0.0.1") | Some("[::1]") | Some("::1")) {
            anyhow::bail!("a guaranteed local HTTP proxy is required");
        }
        if target.scheme() != "https" || target.host_str().unwrap_or("").is_empty() {
            anyhow::bail!("an HTTPS health target is required");
        }
        let fresh = !matches!(&self.client, Some((p, u, _)) if p == proxy_url && u == url);
        if fresh {
            self.close();
            let client = reqwest::Client::builder()
                .proxy(reqwest::Proxy::all(proxy_url)?)
                .pool_max_idle_per_host(1)
                .pool_idle_timeout(Duration::from_secs(90))
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs_f64(timeout))
                .build()?;
            self.client = Some((proxy_url.to_string(), url.to_string(), client));
        }
        let client = self.client.as_ref().map(|(_, _, c)| c.clone()).unwrap();
        let result: anyhow::Result<u64> = async {
            if fresh {
                Self::request(&client, url).await?;
            }
            let mut best = f64::MAX;
            for _ in 0..2 {
                let started = Instant::now();
                Self::request(&client, url).await?;
                best = best.min(started.elapsed().as_secs_f64() * 1000.0);
            }
            Ok((best.round() as u64).max(1))
        }
        .await;
        if result.is_err() {
            self.close();
        }
        result
    }

    async fn request(client: &reqwest::Client, url: &str) -> anyhow::Result<()> {
        let response = client
            .get(url)
            .header("Connection", "keep-alive")
            .header("Cache-Control", "no-cache")
            .send()
            .await?;
        let status = response.status().as_u16();
        let closing = response
            .headers()
            .get("connection")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("close"));
        let _ = read_body_limited(response, 64 * 1024).await;
        if !(200..300).contains(&status) || closing {
            anyhow::bail!("health target did not preserve the verified HTTP tunnel");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_packet_roundtrip() {
        let packet = build_dns_query("example.com", 0x1234).unwrap();
        assert_eq!(&packet[..2], &[0x12, 0x34]);
        let mut response = packet.clone();
        response[2] = 0x81;
        response[3] = 0x80;
        response[7] = 1; // ancount
        response.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184, 216, 34]);
        assert_eq!(parse_dns_answer(&packet, &response).as_deref(), Some("93.184.216.34"));
        let mut wrong_id = response.clone();
        wrong_id[0] = 0;
        assert!(parse_dns_answer(&packet, &wrong_id).is_none());
        assert!(build_dns_query("", 1).is_none());
    }

    #[test]
    fn safe_redirect_rules() {
        let src = Url::parse("https://a.example/x").unwrap();
        assert!(redirect_blocked(&src, &Url::parse("http://a.example/y").unwrap(), "http://a.example/y"));
        assert!(redirect_blocked(&src, &Url::parse("https://a.example/y").unwrap(), "https://@a.example/y"));
        assert!(!redirect_blocked(&src, &Url::parse("https://b.example/y").unwrap(), "/y"));
    }
}
