//! Port of `windows/xray_fluent/ping_worker.py`: TCP/UDP/ICMP/HTTP endpoint
//! pings, the TUN-bypassing host-route helper and the bounded ping worker.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, LazyLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use regex::Regex;

use crate::constants::ICMP_PING_TIMEOUT_MS;
use crate::http_utils::{resolve_a_direct, user_agent};
use crate::models::Node;
use crate::probe_capabilities::endpoint_method;
use lumen_platform::network_route_context::get_windows_default_route_context;
use lumen_platform::route_leases::{acquire_route, release_route};
use lumen_platform::subprocess::{decode_output, run_text_owned};

pub const MAX_PING_WORKERS: usize = 16;
pub const PING_METHODS: [&str; 4] = ["tcping", "icmp", "http", "real"];
const UDP_ENDPOINT_PROTOCOLS: [&str; 9] =
    ["hysteria", "hysteria2", "hy", "hy2", "tuic", "awg", "wireguard", "warp", "masque"];
/// Unused by the ping worker itself; kept for parity with the Python module.
pub const DIRECT_DNS_SERVERS: [&str; 3] = ["8.8.8.8", "1.1.1.1", "9.9.9.9"];

static ICMP_TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[=<]\s*(\d+(?:[.,]\d+)?)\s*ms").unwrap());

fn elapsed_ms(start: Instant) -> i64 {
    (start.elapsed().as_secs_f64() * 1000.0) as i64
}

fn duration(timeout: f64) -> Duration {
    Duration::from_secs_f64(if timeout.is_finite() && timeout > 0.001 { timeout } else { 0.001 })
}

fn strip_brackets(host: &str) -> &str {
    host.trim_matches(|c| c == '[' || c == ']')
}

/// `socket.create_connection((host, port), timeout)` round trip in ms.
pub fn tcp_ping(host: &str, port: i64, timeout: f64) -> Option<i64> {
    if host.is_empty() || port == 0 {
        return None;
    }
    let port = u16::try_from(port).ok()?;
    let start = Instant::now();
    let addrs = (host, port).to_socket_addrs().ok()?;
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, duration(timeout)).is_ok() {
            return Some(elapsed_ms(start));
        }
    }
    None
}

/// Measure a UDP endpoint without assuming a TCP listener.
///
/// Hysteria/Hysteria2 use QUIC over UDP. A tiny datagram makes the OS check
/// endpoint reachability; a reply yields the elapsed time. Servers that drop
/// unknown datagrams yield `None` so the caller can fall back to ICMP.
pub fn udp_ping(host: &str, port: i64, timeout: f64) -> Option<i64> {
    if host.is_empty() || port == 0 {
        return None;
    }
    let port = u16::try_from(port).ok()?;
    let start = Instant::now();
    let target = (host, port).to_socket_addrs().ok()?.find(SocketAddr::is_ipv4)?;
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(duration(timeout))).ok()?;
    sock.connect(target).ok()?;
    sock.send(&[0u8]).ok()?;
    let mut buf = [0u8; 1];
    sock.recv(&mut buf).ok()?;
    Some(elapsed_ms(start))
}

/// UDP probe followed by ICMP: Hysteria servers commonly ignore non-QUIC
/// datagrams, while ICMP still measures the endpoint without a proxy core.
pub fn hysteria_ping(host: &str, port: i64, timeout: f64) -> Option<i64> {
    udp_ping(host, port, timeout).or_else(|| icmp_ping(host, (timeout * 1000.0) as i64))
}

/// Direct HTTP(S) GET without a proxy core.
pub fn http_get_ping(host: &str, port: i64, timeout: f64) -> Option<i64> {
    if host.is_empty() || port == 0 {
        return None;
    }
    let authority =
        if host.contains(':') { String::from("[") + strip_brackets(host) + "]" } else { host.to_string() };
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(duration(timeout))
        .user_agent(user_agent())
        .build()
        .ok()?;
    for scheme in ["https", "http"] {
        let started = Instant::now();
        let url = format!("{scheme}://{authority}:{port}/");
        let Ok(response) = client.get(&url).send() else { continue };
        let Ok(mut response) = response.error_for_status() else { continue };
        let mut byte = [0u8; 1];
        if response.read(&mut byte).is_ok() {
            return Some(elapsed_ms(started));
        }
    }
    None
}

/// Best-effort transport ping for protocols without an Xray test adapter.
pub fn endpoint_ping(host: &str, port: i64, protocol: &str, method: &str, timeout: f64) -> Option<i64> {
    match method {
        // A real probe requires an authenticated proxy core.
        "real" => None,
        "icmp" => icmp_ping(host, (timeout * 1000.0) as i64),
        "http" => http_get_ping(host, port, timeout),
        _ if UDP_ENDPOINT_PROTOCOLS.contains(&protocol) => hysteria_ping(host, port, timeout),
        _ => tcp_ping(host, port, timeout),
    }
}

/// IPv4 first, then IPv6 (the same preference as TCPing).
pub fn icmp_addresses(host: &str) -> Vec<SocketAddr> {
    let Ok(addrs) = (strip_brackets(host), 0u16).to_socket_addrs() else { return Vec::new() };
    let addrs: Vec<SocketAddr> = addrs.collect();
    let mut result = Vec::new();
    if let Some(v4) = addrs.iter().find(|a| a.is_ipv4()) {
        result.push(*v4);
    }
    if let Some(v6) = addrs.iter().find(|a| a.is_ipv6()) {
        result.push(*v6);
    }
    result
}

/// One ICMP echo; round trip in ms or `None`. Windows uses the native
/// IcmpSendEcho API because ping.exe output is localized.
pub fn icmp_ping(host: &str, timeout_ms: i64) -> Option<i64> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let timeout_ms = if timeout_ms == 0 { ICMP_PING_TIMEOUT_MS as i64 } else { timeout_ms }.max(200);
    if cfg!(windows) {
        for addr in icmp_addresses(host) {
            let measured = match addr {
                SocketAddr::V4(v4) => lumen_platform::icmp::echo_v4(*v4.ip(), timeout_ms as u32),
                SocketAddr::V6(v6) => lumen_platform::icmp::echo_v6(*v6.ip(), v6.scope_id(), timeout_ms as u32),
            };
            if measured.is_some() {
                return measured;
            }
        }
        return None;
    }
    let timeout_sec = ((timeout_ms as f64 / 1000.0).round() as i64).max(1);
    let command: Vec<String> =
        vec!["ping".into(), "-c".into(), "1".into(), "-W".into(), timeout_sec.to_string(), host.to_string()];
    let result = run_text_owned(&command, (timeout_ms as f64 / 1000.0 + 1.0).max(2.0), false).ok()?;
    if result.returncode != 0 {
        return None;
    }
    let output = decode_output(&result.stdout) + &decode_output(&result.stderr);
    let captured = ICMP_TIME_RE.captures(&output)?;
    let value: f64 = captured[1].replace(',', ".").parse().ok()?;
    Some((value.round() as i64).max(1))
}

pub fn is_ipv4_address(value: &str) -> bool {
    value.matches('.').count() == 3 && value.parse::<Ipv4Addr>().is_ok()
}

pub fn resolve_ipv4(host: &str) -> String {
    if is_ipv4_address(host) {
        return host.to_string();
    }
    let Ok(addrs) = (host, 0u16).to_socket_addrs() else { return String::new() };
    for addr in addrs {
        if let IpAddr::V4(ip) = addr.ip() {
            return ip.to_string();
        }
    }
    String::new()
}

const DETECT_GATEWAY_SCRIPT: &str = concat!(
    "$routes = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue ",
    "| Where-Object { $_.NextHop -and $_.NextHop -ne '0.0.0.0' } ",
    "| Sort-Object RouteMetric, InterfaceMetric; ",
    "$route = $routes | Where-Object { ",
    "$alias = [string]$_.InterfaceAlias; ",
    "$alias -notmatch '(?i)lumen|xftun|wintun|tun' ",
    "} | Select-Object -First 1; ",
    "if (-not $route) { $route = $routes | Select-Object -First 1 }; ",
    "if (-not $route) { exit 1 }; ",
    "$route.NextHop",
);

fn powershell(script: &str, timeout: f64) -> Option<lumen_platform::subprocess::CompletedProcess> {
    let command: Vec<String> =
        vec!["powershell".into(), "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script.into()];
    run_text_owned(&command, timeout, false).ok()
}

/// Physical IPv4 default gateway, preferring non-TUN adapters.
pub fn detect_direct_gateway() -> String {
    if !cfg!(windows) {
        return String::new();
    }
    let Some(result) = powershell(DETECT_GATEWAY_SCRIPT, 6.0) else { return String::new() };
    if result.returncode != 0 {
        return String::new();
    }
    let output = decode_output(&result.stdout);
    output.trim().lines().next().map(|line| line.trim().to_string()).unwrap_or_default()
}

pub fn looks_like_tun_gateway(gateway: &str) -> bool {
    ["172.18.", "172.19.", "198.18.", "198.19."].iter().any(|prefix| gateway.starts_with(prefix))
}

/// sing-box fake-ip pool (198.18.0.0/15) and the TUN subnet must never be
/// used as a real ping target or host route destination.
pub fn looks_like_fake_ip(ip: &str) -> bool {
    ["198.18.", "198.19.", "172.18.", "172.19."].iter().any(|prefix| ip.starts_with(prefix))
}

pub fn has_direct_host_route(ip: &str, gateway: &str, interface_index: u32) -> bool {
    if !cfg!(windows) || ip.is_empty() || gateway.is_empty() || !is_ipv4_address(ip) || !is_ipv4_address(gateway) {
        return false;
    }
    let index = interface_index.to_string();
    let script = String::from("$routes = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '")
        + ip
        + "/32' -ErrorAction SilentlyContinue; $route = $routes | Where-Object { $_.NextHop -eq '"
        + gateway
        + "' -and ("
        + &index
        + " -eq 0 -or $_.InterfaceIndex -eq "
        + &index
        + ") } | Select-Object -First 1; if ($route) { exit 0 } else { exit 1 }";
    powershell(&script, 3.0).is_some_and(|result| result.returncode == 0)
}

/// Offset after a (possibly compressed) DNS name, `None` when truncated.
pub fn skip_dns_name(data: &[u8], mut idx: usize) -> Option<usize> {
    loop {
        let length = *data.get(idx)?;
        if length == 0 {
            return Some(idx + 1);
        }
        if length & 0xC0 == 0xC0 {
            return Some(idx + 2);
        }
        idx += length as usize + 1;
    }
}

#[derive(Default)]
struct DnsCache {
    entries: HashMap<String, (Instant, String)>,
    order: VecDeque<String>,
}

static DNS_CACHE: LazyLock<Mutex<DnsCache>> = LazyLock::new(|| Mutex::new(DnsCache::default()));
const DNS_CACHE_MAX: usize = 1024;

#[derive(Default)]
struct PrepareEvent {
    done: Mutex<bool>,
    ready: Condvar,
}

impl PrepareEvent {
    fn wait(&self, timeout: Duration) -> bool {
        let mut done = self.done.lock();
        if !*done {
            self.ready.wait_for(&mut done, timeout);
        }
        *done
    }

    fn set(&self) {
        *self.done.lock() = true;
        self.ready.notify_all();
    }
}

struct SetOnDrop(Arc<PrepareEvent>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.set();
    }
}

#[derive(Default)]
struct BypassState {
    preparing: HashMap<String, Arc<PrepareEvent>>,
    added_ips: HashSet<String>,
    dns_routes: HashSet<String>,
    covered_ips: HashSet<String>,
    host_ips: HashMap<String, String>,
}

pub type CancelCheck = Arc<dyn Fn() -> bool + Send + Sync>;

/// Temporary physical-gateway /32 routes so that pings measure the real
/// endpoint while TUN captures the default route (`_WindowsPingBypass`).
/// Fails closed: hosts that cannot be routed directly are not measured.
pub struct WindowsPingBypass {
    enabled: bool,
    gateway: String,
    interface_index: u32,
    cancelled: CancelCheck,
    requested_dns: Option<Vec<String>>,
    dns_servers: Vec<String>,
    state: Mutex<BypassState>,
}

fn route_key(gateway: &str, ip: &str, interface_index: u32) -> String {
    format!("{gateway}|{ip}|{interface_index}")
}

fn route_command(action: &str, ip: &str, gateway: &str, interface_index: u32, metric: bool) -> Vec<String> {
    let mut command: Vec<String> =
        vec!["route".into(), action.into(), ip.into(), "mask".into(), "255.255.255.255".into(), gateway.into()];
    if metric {
        command.extend(["metric".to_string(), "1".to_string()]);
    }
    if interface_index != 0 {
        command.extend(["if".to_string(), interface_index.to_string()]);
    }
    command
}

impl WindowsPingBypass {
    pub fn new(enabled: bool, cancelled: Option<CancelCheck>, dns_servers: Option<Vec<String>>) -> Self {
        Self {
            enabled: enabled && cfg!(windows),
            gateway: String::new(),
            interface_index: 0,
            cancelled: cancelled.unwrap_or_else(|| Arc::new(|| false)),
            requested_dns: dns_servers,
            dns_servers: Vec::new(),
            state: Mutex::new(BypassState::default()),
        }
    }

    fn is_cancelled(&self) -> bool {
        (self.cancelled)()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    /// `__enter__`: capture the physical default route and its adapter DNS.
    pub fn enter(&mut self) {
        if !self.enabled || self.is_cancelled() {
            return;
        }
        let Some(context) = get_windows_default_route_context(false) else { return };
        if !context.is_physical || self.is_cancelled() {
            return;
        }
        self.gateway = context.next_hop.clone();
        self.interface_index = context.interface_index;
        if !is_ipv4_address(&self.gateway) || looks_like_tun_gateway(&self.gateway) {
            self.gateway.clear();
            return;
        }
        // Bootstrap uses only the physical adapter DNS (or explicit config),
        // never hard-coded public fallback resolvers or TUN/system DNS.
        let candidates = self.requested_dns.clone().unwrap_or(context.dns_servers);
        let mut servers: Vec<String> = Vec::new();
        for ip in candidates {
            if is_ipv4_address(&ip) && !servers.contains(&ip) {
                servers.push(ip);
            }
        }
        servers.truncate(3);
        self.dns_servers = servers;
    }

    fn route_add(&self, ip: &str) -> bool {
        if self.is_cancelled() {
            return false;
        }
        run_text_owned(&route_command("add", ip, &self.gateway, self.interface_index, true), 4.0, false)
            .is_ok_and(|result| result.returncode == 0)
    }

    fn acquire_route(&self, ip: &str) -> bool {
        let (gateway, index, owned_ip) = (self.gateway.clone(), self.interface_index, ip.to_string());
        acquire_route(
            &route_key(&self.gateway, ip, self.interface_index),
            || self.route_add(ip),
            || has_direct_host_route(ip, &self.gateway, self.interface_index),
            move || {
                let _ = run_text_owned(&route_command("delete", &owned_ip, &gateway, index, false), 4.0, false);
            },
        )
    }

    fn resolve_real_ip(&self, host: &str) -> String {
        if is_ipv4_address(host) {
            return host.to_string();
        }
        let key = format!("{}|{}|{}|{}", self.gateway, self.interface_index, host.to_lowercase(), self.dns_servers.join(","));
        {
            let cache = DNS_CACHE.lock();
            if let Some((expires, ip)) = cache.entries.get(&key) {
                if *expires > Instant::now() {
                    return ip.clone();
                }
            }
        }
        let mut ip = String::new();
        for dns_server in &self.dns_servers {
            if self.is_cancelled() {
                break;
            }
            {
                let mut state = self.state.lock();
                if !state.added_ips.contains(dns_server) {
                    if !self.acquire_route(dns_server) {
                        continue;
                    }
                    state.added_ips.insert(dns_server.clone());
                    state.dns_routes.insert(dns_server.clone());
                }
            }
            ip = resolve_a_direct(host, dns_server, 1.0);
            if !ip.is_empty() && !looks_like_fake_ip(&ip) {
                break;
            }
            ip.clear();
        }
        let mut cache = DNS_CACHE.lock();
        if cache.entries.len() >= DNS_CACHE_MAX {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
        if !cache.entries.contains_key(&key) {
            cache.order.push_back(key.clone());
        }
        let ttl = Duration::from_secs(if ip.is_empty() { 5 } else { 60 });
        cache.entries.insert(key, (Instant::now() + ttl, ip.clone()));
        ip
    }

    /// Ensure a direct route to `host`; `false` means "do not measure".
    pub fn prepare_host(&self, host: &str) -> bool {
        if !self.enabled {
            return !self.is_cancelled();
        }
        let host = strip_brackets(host.trim()).to_string();
        if host.is_empty() || self.gateway.is_empty() || self.is_cancelled() {
            return false;
        }
        // The IPv4 bypass must fail closed for IPv6 instead of silently using TUN.
        if host.contains(':') {
            return false;
        }
        let (event, owner) = {
            let mut state = self.state.lock();
            match state.preparing.get(&host) {
                Some(event) => (event.clone(), false),
                None => {
                    let event = Arc::new(PrepareEvent::default());
                    state.preparing.insert(host.clone(), event.clone());
                    (event, true)
                }
            }
        };
        if !owner {
            while !event.wait(Duration::from_millis(100)) {
                if self.is_cancelled() {
                    return false;
                }
            }
            return self.can_ping_direct(&host);
        }
        let _set = SetOnDrop(event);
        let ip = self.resolve_real_ip(&host);
        if ip.is_empty() || self.is_cancelled() || ["127.", "0.", "169.254."].iter().any(|p| ip.starts_with(p)) {
            return false;
        }
        let mut state = self.state.lock();
        if !state.added_ips.contains(&ip) && !self.acquire_route(&ip) {
            return false;
        }
        state.added_ips.insert(ip.clone());
        state.covered_ips.insert(ip.clone());
        state.host_ips.insert(host, ip);
        true
    }

    pub fn direct_ip(&self, host: &str) -> String {
        let host = host.trim();
        if !self.enabled {
            return host.to_string();
        }
        self.state.lock().host_ips.get(host).cloned().unwrap_or_default()
    }

    pub fn can_ping_direct(&self, host: &str) -> bool {
        if !self.enabled {
            return true;
        }
        let state = self.state.lock();
        state.host_ips.get(host.trim()).is_some_and(|ip| !ip.is_empty() && state.covered_ips.contains(ip))
    }

    /// `__exit__`: release every leased route. Idempotent.
    pub fn exit(&self) {
        let mut state = self.state.lock();
        for ip in state.added_ips.drain() {
            release_route(&route_key(&self.gateway, &ip, self.interface_index));
        }
        state.dns_routes.clear();
        state.covered_ips.clear();
        state.host_ips.clear();
    }
}

impl Drop for WindowsPingBypass {
    fn drop(&mut self) {
        self.exit();
    }
}

/// Signals of the Qt `PingWorker` (`result`, `progress`, `completed`).
#[derive(Debug, Clone, PartialEq)]
pub enum PingEvent {
    Result { node_id: String, ms: Option<i64> },
    Progress { current: usize, total: usize },
    Completed,
}

pub struct PingWorker {
    nodes: Vec<Node>,
    timeout: f64,
    bypass_tun: bool,
    method: String,
    cancelled: Arc<AtomicBool>,
}

impl PingWorker {
    pub fn new(nodes: Vec<Node>, timeout: f64, bypass_tun: bool, method: &str) -> Self {
        let method = if PING_METHODS.contains(&method) { method } else { "tcping" };
        Self { nodes, timeout, bypass_tun, method: method.to_string(), cancelled: Arc::new(AtomicBool::new(false)) }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn measure(&self, node: &Node, bypass: &WindowsPingBypass) -> Option<i64> {
        if self.is_cancelled() {
            return None;
        }
        if !bypass.prepare_host(&node.server) {
            return None;
        }
        let direct = bypass.direct_ip(&node.server);
        let target = if direct.is_empty() { node.server.clone() } else { direct };
        if self.method == "http" || self.method == "real" {
            // These methods require the speed test worker and a full core profile.
            return None;
        }
        if endpoint_method(node, &self.method) == "icmp" {
            let mut measured = icmp_ping(&target, (self.timeout * 1000.0) as i64);
            if measured.is_none() && self.method == "icmp" && endpoint_method(node, "tcping") == "tcping" {
                // Like Android's InetAddress.isReachable: a host that drops
                // ICMP is still reachable when it completes a TCP handshake.
                measured = tcp_ping(&target, node.port, self.timeout);
            }
            return measured;
        }
        tcp_ping(&target, node.port, self.timeout)
    }

    /// Blocking run; events are delivered in completion order. No measurement
    /// outlives its direct host route.
    pub fn run(&self, emit: &mut dyn FnMut(PingEvent)) {
        let total = self.nodes.len();
        if total == 0 {
            emit(PingEvent::Completed);
            return;
        }
        let max_workers = MAX_PING_WORKERS.min(total);
        let flag = self.cancelled.clone();
        let mut bypass = WindowsPingBypass::new(self.bypass_tun, Some(Arc::new(move || flag.load(Ordering::SeqCst))), None);
        bypass.enter();
        let next = AtomicUsize::new(0);
        let (sender, receiver) = mpsc::channel::<(String, Option<i64>)>();
        std::thread::scope(|scope| {
            for index in 0..max_workers {
                let sender = sender.clone();
                let (bypass, next) = (&bypass, &next);
                let spawned = std::thread::Builder::new().name(format!("ping_{index}")).spawn_scoped(scope, move || {
                    loop {
                        if self.is_cancelled() {
                            break;
                        }
                        let Some(node) = self.nodes.get(next.fetch_add(1, Ordering::SeqCst)) else { break };
                        let ms = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.measure(node, bypass)))
                            .unwrap_or(None);
                        if sender.send((node.id.clone(), ms)).is_err() {
                            break;
                        }
                    }
                });
                if spawned.is_err() && index == 0 {
                    self.cancel();
                }
            }
            drop(sender);
            let mut completed = 0;
            while !self.is_cancelled() {
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok((node_id, ms)) => {
                        completed += 1;
                        emit(PingEvent::Result { node_id, ms });
                        emit(PingEvent::Progress { current: completed, total });
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        bypass.exit();
        emit(PingEvent::Completed);
    }

    /// Run on a background thread (`QThread.start`).
    pub fn start(self) -> PingWorkerHandle {
        let cancelled = self.cancelled.clone();
        let running = Arc::new(AtomicBool::new(true));
        let (sender, events) = mpsc::channel();
        let still_running = running.clone();
        let thread = std::thread::Builder::new()
            .name("lumen-ping-worker".into())
            .spawn(move || {
                self.run(&mut |event| {
                    let _ = sender.send(event);
                });
                still_running.store(false, Ordering::SeqCst);
            })
            .ok();
        if thread.is_none() {
            running.store(false, Ordering::SeqCst);
        }
        PingWorkerHandle { cancelled, running, events, thread }
    }
}

pub struct PingWorkerHandle {
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    pub events: Receiver<PingEvent>,
    thread: Option<JoinHandle<()>>,
}

impl PingWorkerHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn helpers() {
        assert!(is_ipv4_address("10.0.0.1"));
        assert!(!is_ipv4_address("10.0.1"));
        assert!(!is_ipv4_address("example.com"));
        assert!(looks_like_fake_ip("198.18.0.5") && !looks_like_fake_ip("8.8.8.8"));
        assert!(looks_like_tun_gateway("172.19.0.1"));
        assert_eq!(skip_dns_name(&[3, b'w', b'w', b'w', 0, 9], 0), Some(5));
        assert_eq!(skip_dns_name(&[0xC0, 0x0C], 0), Some(2));
        assert_eq!(skip_dns_name(&[5, b'a'], 0), None);
        assert_eq!(resolve_ipv4("127.0.0.1"), "127.0.0.1");
        assert_eq!(tcp_ping("", 80, 1.0), None);
        assert_eq!(endpoint_ping("127.0.0.1", 1, "vless", "real", 1.0), None);
        assert_eq!(PingWorker::new(Vec::new(), 1.0, false, "bogus").method(), "tcping");
    }

    #[test]
    fn worker_pings_local_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port() as i64;
        let mut nodes = Vec::new();
        for _ in 0..3 {
            let mut node = Node::new();
            node.scheme = "vless".into();
            node.server = "127.0.0.1".into();
            node.port = port;
            nodes.push(node);
        }
        let ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let mut events = Vec::new();
        PingWorker::new(nodes, 1.0, false, "tcping").run(&mut |event| events.push(event));
        assert_eq!(events.last(), Some(&PingEvent::Completed));
        let results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                PingEvent::Result { node_id, ms } => Some((node_id.clone(), *ms)),
                _ => None,
            })
            .collect();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|(id, ms)| ids.contains(id) && ms.is_some()));
        assert!(events.contains(&PingEvent::Progress { current: 3, total: 3 }));
        let mut handle = PingWorker::new(Vec::new(), 1.0, false, "icmp").start();
        handle.join();
        assert_eq!(handle.events.try_iter().collect::<Vec<_>>(), vec![PingEvent::Completed]);
    }
}
