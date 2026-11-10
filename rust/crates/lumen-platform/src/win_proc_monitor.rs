//! Per-process proxy usage monitor via Windows API
//! (port of `windows/xray_fluent/win_proc_monitor.py`).
//!
//! Uses GetExtendedTcpTable to find which processes have connections
//! to the SOCKS/HTTP ports, and GetPerTcpConnectionEStats for actual
//! per-connection byte counts (DataBytesIn/DataBytesOut).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[cfg(windows)]
use parking_lot::Mutex as ParkMutex;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetPerTcpConnectionEStats, SetPerTcpConnectionEStats,
    MIB_TCPROW_LH, MIB_TCPROW_LH_0, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
    MIB_TCP_STATE_ESTAB, TCP_ESTATS_DATA_RW_v0, TCP_ESTATS_DATA_ROD_v0,
    TcpConnectionEstatsData, TCP_TABLE_OWNER_PID_CONNECTIONS,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Cache PID -> (creation time, exe name); the creation time detects recycled
/// PIDs.
#[cfg(windows)]
struct ProcMonitorState {
    pid_cache: HashMap<u32, (u64, String)>,
    /// Connections with estats already enabled — avoid redundant Set calls.
    /// Key: (localAddr, localPort, remoteAddr, remotePort).
    estats_enabled: HashSet<[u32; 4]>,
    estats_retry_after: Option<Instant>,
}

#[cfg(windows)]
impl Default for ProcMonitorState {
    fn default() -> Self {
        Self {
            pid_cache: HashMap::new(),
            estats_enabled: HashSet::new(),
            estats_retry_after: None,
        }
    }
}

#[cfg(windows)]
fn monitor_state() -> &'static ParkMutex<ProcMonitorState> {
    use std::sync::OnceLock;
    static STATE: OnceLock<ParkMutex<ProcMonitorState>> = OnceLock::new();
    STATE.get_or_init(|| ParkMutex::new(ProcMonitorState::default()))
}

const PID_CACHE_MAX: usize = 512;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProxyProcessInfo {
    pub exe: String,
    pub connections: u64,
    pub pids: HashSet<u32>,
    /// actual TCP bytes received (download)
    pub bytes_in: u64,
    /// actual TCP bytes sent (upload)
    pub bytes_out: u64,
}

#[cfg(windows)]
fn process_creation_time(handle: HANDLE) -> u64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetProcessTimes;

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel_time = FILETIME::default();
    let mut user_time = FILETIME::default();
    unsafe {
        if GetProcessTimes(
            handle,
            &mut created,
            &mut exited,
            &mut kernel_time,
            &mut user_time,
        )
        .is_err()
        {
            return 0;
        }
    }
    ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64
}

/// Get exe name from PID. Cached per (pid, process creation time).
#[cfg(windows)]
fn pid_to_exe(pid: u32) -> String {
    use windows::core::PWSTR;

    let mut state = monitor_state().lock();
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).unwrap_or_default()
    };
    if handle.is_invalid() {
        // Access denied can also mean a different protected process reused
        // the PID. Do not return a former program without creation-time proof.
        state.pid_cache.remove(&pid);
        return String::new();
    }
    let result = (|| {
        let created = process_creation_time(handle);
        if created != 0 {
            if let Some((cached_time, cached_exe)) = state.pid_cache.get(&pid) {
                if *cached_time == created {
                    return Some(cached_exe.clone());
                }
            }
        }
        let mut buffer = [0u16; 260];
        let mut size = buffer.len() as u32;
        let name = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        if name.is_ok() {
            let full = String::from_utf16_lossy(&buffer[..size as usize]);
            let exe = full
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or_default()
                .to_string();
            if state.pid_cache.len() >= PID_CACHE_MAX {
                state.pid_cache.clear();
            }
            if created != 0 {
                state.pid_cache.insert(pid, (created, exe.clone()));
            }
            return Some(exe);
        }
        None
    })();
    unsafe {
        let _ = CloseHandle(handle);
    }
    result.unwrap_or_default()
}

#[cfg(not(windows))]
fn pid_to_exe(_pid: u32) -> String {
    String::new()
}

/// Best-effort PID to executable name lookup for traffic telemetry.
pub fn process_name_from_pid(pid: &str) -> String {
    // Python: int(pid or 0); anything unparsable or non-positive is empty.
    let Ok(value) = pid.trim().parse::<i64>() else {
        return String::new();
    };
    if value <= 0 {
        return String::new();
    }
    pid_to_exe(value as u32)
}

/// Network byte order to host byte order for port.
#[cfg(windows)]
fn ntohs(port: u32) -> u16 {
    (((port & 0xFF) << 8) | ((port >> 8) & 0xFF)) as u16
}

#[cfg(windows)]
fn make_tcprow(row: &MIB_TCPROW_OWNER_PID) -> MIB_TCPROW_LH {
    MIB_TCPROW_LH {
        Anonymous: MIB_TCPROW_LH_0 {
            dwState: row.dwState,
        },
        dwLocalAddr: row.dwLocalAddr,
        dwLocalPort: row.dwLocalPort,
        dwRemoteAddr: row.dwRemoteAddr,
        dwRemotePort: row.dwRemotePort,
    }
}

#[cfg(windows)]
fn conn_key(row: &MIB_TCPROW_OWNER_PID) -> [u32; 4] {
    [
        row.dwLocalAddr,
        row.dwLocalPort,
        row.dwRemoteAddr,
        row.dwRemotePort,
    ]
}

/// Enable per-connection byte tracking. Requires admin. Idempotent.
#[cfg(windows)]
fn enable_estats(row: &MIB_TCPROW_OWNER_PID) -> bool {
    let mut state = monitor_state().lock();
    if let Some(retry_after) = state.estats_retry_after {
        if Instant::now() < retry_after {
            return false;
        }
    }
    let key = conn_key(row);
    if state.estats_enabled.contains(&key) {
        return true;
    }
    let tcp_row = make_tcprow(row);
    let rw = TCP_ESTATS_DATA_RW_v0 {
        EnableCollection: true,
    };
    // The RW struct is a single BOOLEAN byte, exactly like the ctypes c_ubyte.
    let rw_bytes = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(rw) as *const u8,
            std::mem::size_of::<TCP_ESTATS_DATA_RW_v0>(),
        )
    };
    let ret = unsafe {
        SetPerTcpConnectionEStats(
            &tcp_row,
            TcpConnectionEstatsData,
            rw_bytes,
            0,
            0,
        )
    };
    if ret == 0 {
        state.estats_enabled.insert(key);
        return true;
    }
    if ret == 5 || ret == 50 {
        // access denied / unsupported: retry once, not for every socket
        state.estats_retry_after = Some(Instant::now() + std::time::Duration::from_secs(60));
    }
    false
}

/// Get (DataBytesIn, DataBytesOut) for a TCP connection.
///
/// Returns None if the API call fails (connection closed, access denied, etc.).
#[cfg(windows)]
fn get_estats_bytes(row: &MIB_TCPROW_OWNER_PID) -> Option<(u64, u64)> {
    let tcp_row = make_tcprow(row);
    let mut rod = TCP_ESTATS_DATA_ROD_v0::default();
    let rod_bytes = unsafe {
        std::slice::from_raw_parts_mut(
            std::ptr::addr_of_mut!(rod) as *mut u8,
            std::mem::size_of::<TCP_ESTATS_DATA_ROD_v0>(),
        )
    };
    let ret = unsafe {
        GetPerTcpConnectionEStats(
            &tcp_row,
            TcpConnectionEstatsData,
            None,
            0,
            None,
            0,
            Some(rod_bytes),
            0,
        )
    };
    if ret == 0 {
        Some((rod.DataBytesIn, rod.DataBytesOut))
    } else {
        None
    }
}

/// Find processes connected to the proxy ports with per-connection byte
/// counts.
///
/// Uses GetExtendedTcpTable for connection/PID discovery and
/// GetPerTcpConnectionEStats for actual network bytes per connection.
/// Requires Administrator for byte counts; falls back to 0 without admin.
#[cfg(windows)]
pub fn get_proxy_connections(socks_port: u16, http_port: u16) -> Vec<ProxyProcessInfo> {
    use windows::Win32::Networking::WinSock::AF_INET;

    let target_ports: HashSet<u16> = HashSet::from([socks_port, http_port]);
    let localhost: u32 = 0x0100007F; // 127.0.0.1 in network byte order

    let mut size: u32 = 0;
    unsafe {
        GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
            0,
        )
    };

    let mut buffer = vec![0u8; size.max(1) as usize];
    let ret = unsafe {
        GetExtendedTcpTable(
            Some(buffer.as_mut_ptr() as *mut core::ffi::c_void),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
            0,
        )
    };
    if ret != 0 {
        return Vec::new();
    }

    let header_size = std::mem::size_of::<MIB_TCPTABLE_OWNER_PID>();
    if (buffer.len() as usize) < header_size {
        return Vec::new();
    }
    let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let entries = table.dwNumEntries as usize;
    // The windows crate declares table as a single-element array; the buffer
    // holds dwNumEntries rows (checked against the reported size).
    if buffer.len() < header_size + (entries.max(1) - 1) * std::mem::size_of::<MIB_TCPROW_OWNER_PID>()
    {
        return Vec::new();
    }
    let rows = unsafe {
        std::slice::from_raw_parts(
            (table as *const MIB_TCPTABLE_OWNER_PID)
                .byte_add(std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table))
                .cast::<MIB_TCPROW_OWNER_PID>(),
            entries,
        )
    };

    let mut by_exe: HashMap<String, ProxyProcessInfo> = HashMap::new();
    let mut pid_names: HashMap<u32, String> = HashMap::new();
    let mut live_keys: HashSet<[u32; 4]> = HashSet::new();
    for row in rows {
        if row.dwState != MIB_TCP_STATE_ESTAB.0 as u32 {
            continue;
        }
        let remote_port = ntohs(row.dwRemotePort);
        if !target_ports.contains(&remote_port) {
            continue;
        }
        if row.dwRemoteAddr != localhost {
            continue;
        }

        let pid = row.dwOwningPid;
        if !pid_names.contains_key(&pid) {
            pid_names.insert(pid, pid_to_exe(pid));
        }
        let exe = pid_names[&pid].clone();
        let lowered = exe.to_lowercase();
        if exe.is_empty() || lowered == "xray.exe" || lowered == "sing-box.exe" {
            continue;
        }

        let key = conn_key(row);
        live_keys.insert(key);
        let estats = if enable_estats(row) {
            get_estats_bytes(row)
        } else {
            None
        };

        let entry = by_exe
            .entry(exe.clone())
            .or_insert_with(|| ProxyProcessInfo {
                exe: exe.clone(),
                ..Default::default()
            });
        entry.connections += 1;
        entry.pids.insert(pid);
        if let Some((bytes_in, bytes_out)) = estats {
            entry.bytes_in += bytes_in;
            entry.bytes_out += bytes_out;
        }
    }

    // Drop closed connections so a recycled 4-tuple gets eStats enabled again.
    {
        let mut state = monitor_state().lock();
        state.estats_enabled.retain(|key| live_keys.contains(key));
    }

    let mut result: Vec<ProxyProcessInfo> = by_exe.into_values().collect();
    result.sort_by(|left, right| right.connections.cmp(&left.connections));
    result
}

#[cfg(not(windows))]
pub fn get_proxy_connections(_socks_port: u16, _http_port: u16) -> Vec<ProxyProcessInfo> {
    Vec::new()
}

/// Clear PID->exe cache and estats tracking. Call on disconnect.
#[cfg(windows)]
pub fn clear_pid_cache() {
    let mut state = monitor_state().lock();
    state.pid_cache.clear();
    state.estats_enabled.clear();
    state.estats_retry_after = None;
}

#[cfg(not(windows))]
pub fn clear_pid_cache() {}

/// Monotonic seconds matching `time.monotonic()` usage in the Python module.
pub fn monotonic_seconds() -> f64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn process_name_lookup_resolves_the_current_process() {
        let exe = std::env::current_exe().unwrap();
        let expected = exe.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(
            process_name_from_pid(&std::process::id().to_string()),
            expected
        );
        assert_eq!(process_name_from_pid("0"), "");
        assert_eq!(process_name_from_pid("-3"), "");
        assert_eq!(process_name_from_pid("not-a-pid"), "");
        assert_eq!(process_name_from_pid(""), "");
    }

    #[test]
    fn proxy_connections_see_a_loopback_client_of_the_test_process() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (_accepted, _) = listener.accept().unwrap();

        let infos = get_proxy_connections(port, port);
        let exe = std::env::current_exe().unwrap();
        let expected = exe.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            infos
                .iter()
                .any(|info| info.exe == expected && info.connections >= 1),
            "expected the test process among {infos:?}"
        );

        clear_pid_cache();
    }
}
