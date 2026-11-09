//! Native adapter enumeration and physical default-route selection.
//!
//! Port of `windows/xray_fluent/windows_network.py` (GetAdaptersAddresses).

use std::net::Ipv4Addr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;

const IF_OPER_STATUS_UP: i32 = 1;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_TUNNEL: u32 = 131;

static TUN_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)lumen|xftun|singbox|wintun|(?:^|[^a-z])tun(?:[^a-z]|$)").unwrap()
});

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsAdapterSnapshot {
    pub alias: String,
    pub interface_index: u32,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub metric: u32,
    pub is_up: bool,
    pub if_type: u32,
    pub description: String,
}

impl WindowsAdapterSnapshot {
    pub fn looks_like_tun(&self) -> bool {
        TUN_NAME_RE.is_match(&format!("{} {}", self.alias, self.description))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsNativeRouteContext {
    pub interface_alias: String,
    pub interface_index: u32,
    pub next_hop: String,
    pub dns_servers: Vec<String>,
    pub tun_active: bool,
}

#[cfg(windows)]
unsafe fn ipv4_from_socket_address(
    value: &windows::Win32::Networking::WinSock::SOCKET_ADDRESS,
) -> Option<String> {
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    if value.lpSockaddr.is_null()
        || (value.iSockaddrLength as usize) < std::mem::size_of::<SOCKADDR_IN>()
    {
        return None;
    }
    unsafe {
        let sockaddr = &*(value.lpSockaddr as *const SOCKADDR_IN);
        if sockaddr.sin_family != AF_INET {
            return None;
        }
        let raw = sockaddr.sin_addr.S_un.S_addr;
        Some(Ipv4Addr::from(u32::from_be(raw)).to_string())
    }
}

#[cfg(windows)]
unsafe fn pwstr_to_string(value: windows::core::PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    unsafe { value.to_string().unwrap_or_default().trim().to_string() }
}

/// Snapshot all IPv4 adapters with gateways and DNS servers.
#[cfg(windows)]
pub fn query_adapter_snapshots() -> Vec<WindowsAdapterSnapshot> {
    use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_GATEWAYS, GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    let mut size: u32 = 16 * 1024;
    let mut buffer: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
    let mut result = 0u32;
    for _ in 0..3 {
        result = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                GAA_FLAG_INCLUDE_GATEWAYS,
                None,
                Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if result != ERROR_BUFFER_OVERFLOW.0 {
            break;
        }
        buffer = vec![0; (size as usize).div_ceil(8)];
    }
    if result != 0 {
        return Vec::new();
    }

    let mut snapshots = Vec::new();
    let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    for _ in 0..256 {
        if current.is_null() {
            break;
        }
        unsafe {
            let item = &*current;
            let mut gateways = Vec::new();
            let mut gw = item.FirstGatewayAddress;
            for _ in 0..32 {
                if gw.is_null() {
                    break;
                }
                if let Some(ip) = ipv4_from_socket_address(&(*gw).Address) {
                    if !gateways.contains(&ip) {
                        gateways.push(ip);
                    }
                }
                gw = (*gw).Next;
            }
            let mut dns_servers = Vec::new();
            let mut dns = item.FirstDnsServerAddress;
            for _ in 0..32 {
                if dns.is_null() {
                    break;
                }
                if let Some(ip) = ipv4_from_socket_address(&(*dns).Address) {
                    if !dns_servers.contains(&ip) {
                        dns_servers.push(ip);
                    }
                }
                dns = (*dns).Next;
            }
            snapshots.push(WindowsAdapterSnapshot {
                alias: pwstr_to_string(item.FriendlyName),
                description: pwstr_to_string(item.Description),
                interface_index: item.Anonymous1.Anonymous.IfIndex,
                gateway: gateways.first().cloned().unwrap_or_default(),
                dns_servers,
                metric: item.Ipv4Metric,
                is_up: item.OperStatus.0 == IF_OPER_STATUS_UP,
                if_type: item.IfType,
            });
            current = item.Next;
        }
    }
    snapshots
}

#[cfg(not(windows))]
pub fn query_adapter_snapshots() -> Vec<WindowsAdapterSnapshot> {
    Vec::new()
}

/// Pick the lowest-metric physical adapter with a gateway; report whether any
/// TUN-looking adapter is up.
pub fn select_windows_route_context(
    adapters: &[WindowsAdapterSnapshot],
) -> Option<WindowsNativeRouteContext> {
    let active: Vec<&WindowsAdapterSnapshot> =
        adapters.iter().filter(|a| a.is_up && a.interface_index > 0).collect();
    let tun_active = active.iter().any(|a| a.looks_like_tun());
    let physical = active
        .iter()
        .filter(|a| {
            !a.gateway.is_empty()
                && !a.looks_like_tun()
                && a.if_type != IF_TYPE_SOFTWARE_LOOPBACK
                && a.if_type != IF_TYPE_TUNNEL
        })
        .min_by_key(|a| (if a.metric > 0 { a.metric as u64 } else { 1u64 << 31 }, a.interface_index))?;
    Some(WindowsNativeRouteContext {
        interface_alias: if physical.alias.is_empty() {
            physical.description.clone()
        } else {
            physical.alias.clone()
        },
        interface_index: physical.interface_index,
        next_hop: physical.gateway.clone(),
        dns_servers: physical.dns_servers.clone(),
        tun_active,
    })
}

/// Wait until no adapter named in `names` is up (a core's TUN teardown).
pub fn wait_for_adapters_gone(names: &[&str], timeout: f64) -> bool {
    let wanted: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
    loop {
        let present = query_adapter_snapshots()
            .iter()
            .any(|a| a.is_up && wanted.contains(&a.alias.to_lowercase()));
        if !present {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn query_windows_route_context() -> Option<WindowsNativeRouteContext> {
    select_windows_route_context(&query_adapter_snapshots())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(alias: &str, index: u32, gateway: &str, metric: u32) -> WindowsAdapterSnapshot {
        WindowsAdapterSnapshot {
            alias: alias.into(),
            interface_index: index,
            gateway: gateway.into(),
            dns_servers: vec!["192.168.1.1".into()],
            metric,
            is_up: true,
            if_type: 6,
            description: String::new(),
        }
    }

    #[test]
    fn selects_lowest_metric_physical_and_detects_tun() {
        let adapters = vec![
            adapter("Wi-Fi", 12, "192.168.1.1", 35),
            adapter("Ethernet", 7, "10.0.0.1", 25),
            adapter("LumenTun", 40, "172.19.0.1", 1),
        ];
        let ctx = select_windows_route_context(&adapters).unwrap();
        assert_eq!(ctx.interface_alias, "Ethernet");
        assert_eq!(ctx.next_hop, "10.0.0.1");
        assert!(ctx.tun_active);
    }

    #[test]
    fn tun_regex_matches_like_python() {
        assert!(adapter("xftun", 1, "", 0).looks_like_tun());
        assert!(adapter("my tun 2", 1, "", 0).looks_like_tun());
        assert!(!adapter("Ethernet tunnelbroker", 1, "", 0).looks_like_tun());
        assert!(select_windows_route_context(&[adapter("x", 1, "", 1)]).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn native_query_does_not_crash() {
        let _ = query_adapter_snapshots();
    }
}
