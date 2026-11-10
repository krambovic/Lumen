//! Foreign VPN/proxy client detection and conflict resolution
//! (port of `windows/xray_fluent/process_conflicts.py`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};

use crate::subprocess::{decode_output, hidden_command, run_merged, run_text_owned};

const KNOWN_CONFLICTS: [(&str, &str); 20] = [
    ("amneziawg.exe", "AmneziaWG"),
    ("amneziavpn.exe", "AmneziaVPN"),
    ("clash-verge.exe", "Clash Verge"),
    ("clash-verge-rev.exe", "Clash Verge Rev"),
    ("clash.exe", "Clash"),
    ("flclash.exe", "FlClash"),
    ("happ-desktop.exe", "Happ"),
    ("happ.exe", "Happ"),
    ("hiddify.exe", "Hiddify"),
    ("karing.exe", "Karing"),
    ("mihomo.exe", "Mihomo"),
    ("nekobox.exe", "NekoBox"),
    ("nekoray.exe", "NekoRay"),
    ("nordvpn.exe", "NordVPN"),
    ("openvpn.exe", "OpenVPN"),
    ("outline-client.exe", "Outline"),
    ("protonvpn.exe", "Proton VPN"),
    ("psiphon3.exe", "Psiphon"),
    ("throne.exe", "Throne"),
    ("v2rayn.exe", "v2rayN"),
];
const CORE_CONFLICTS: [(&str, &str); 2] =
    [("xray.exe", "Xray"), ("sing-box.exe", "sing-box")];

/// Python `os.getpid()`.
fn own_pid() -> u32 {
    std::process::id()
}

/// One listening port held by a foreign process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConflict {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
}

/// Python `str(value or "")` for the mixed JSON payloads of the scan.
fn json_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(windows)]
fn running_processes() -> HashMap<u32, String> {
    if let Some(native) = running_processes_win32(None) {
        return native;
    }
    running_processes_tasklist()
}

#[cfg(not(windows))]
fn running_processes() -> HashMap<u32, String> {
    HashMap::new()
}

/// `tasklist /FO CSV /NH` fallback with the same PID cleanup as Python.
#[cfg(windows)]
fn running_processes_tasklist() -> HashMap<u32, String> {
    let result = match run_text_owned(
        &[
            "tasklist".to_string(),
            "/FO".to_string(),
            "CSV".to_string(),
            "/NH".to_string(),
        ],
        4.0,
        false,
    ) {
        Ok(result) => result,
        Err(_) => return HashMap::new(),
    };
    let text = decode_output(&result.stdout);
    let mut processes: HashMap<u32, String> = HashMap::new();
    for row in parse_csv_rows(&text) {
        if row.len() < 2 {
            continue;
        }
        let pid_text = row[1].replace('\u{00a0}', "").replace(' ', "");
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };
        processes.insert(pid, row[0].trim().to_string());
    }
    processes
}

/// Minimal CSV row splitter (quotes, `""` escapes), matching the fields
/// `csv.reader` yields for `tasklist` output.
fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_quotes {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        field.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                } else {
                    field.push(c);
                }
            } else if c == '"' {
                in_quotes = true;
            } else if c == ',' {
                fields.push(std::mem::take(&mut field));
            } else {
                field.push(c);
            }
        }
        fields.push(field);
        rows.push(fields);
    }
    rows
}

/// Enumerate process names through Toolhelp without spawning tasklist.
/// `parent_ids` collects pid -> parent pid when requested.
#[cfg(windows)]
pub fn running_processes_win32(
    mut parent_ids: Option<&mut HashMap<u32, u32>>,
) -> Option<HashMap<u32, String>> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    // SAFETY: the snapshot handle is owned by this function only.
    let owned = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_err() {
        return None;
    }
    let mut processes: HashMap<u32, String> = HashMap::new();
    loop {
        processes.insert(
            entry.th32ProcessID,
            String::from_utf16_lossy(&entry.szExeFile)
                .trim_end_matches('\0')
                .to_string(),
        );
        if let Some(parent_ids) = parent_ids.as_deref_mut() {
            parent_ids.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        }
        if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
            // ERROR_NO_MORE_FILES is the normal end of the snapshot; any other
            // failure still leaves the collected rows, like the Python loop.
            break;
        }
    }
    drop(owned);
    Some(processes)
}

#[cfg(not(windows))]
pub fn running_processes_win32(
    _parent_ids: Option<&mut HashMap<u32, u32>>,
) -> Option<HashMap<u32, String>> {
    None
}

/// Cover the gap between CreateProcess and manager/worker registration.
#[cfg(windows)]
fn own_descendant_pids() -> HashSet<u32> {
    let mut parents: HashMap<u32, u32> = HashMap::new();
    let _ = running_processes_win32(Some(&mut parents));
    let mut owned: HashSet<u32> = HashSet::from([own_pid()]);
    loop {
        let children: HashSet<u32> = parents
            .iter()
            .filter(|(pid, parent)| owned.contains(parent) && !owned.contains(*pid))
            .map(|(pid, _)| *pid)
            .collect();
        if children.is_empty() {
            return owned;
        }
        owned.extend(children);
    }
}

#[cfg(not(windows))]
fn own_descendant_pids() -> HashSet<u32> {
    HashSet::from([own_pid()])
}

/// Return a process image path when Windows allows a limited query.
#[cfg(windows)]
fn process_executable_path(pid: u32) -> Option<PathBuf> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    if pid == 0 {
        return None;
    }
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.unwrap_or_default();
    if handle.is_invalid() {
        return None;
    }
    let result = (|| {
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        }
        .ok()?;
        let len = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(size as usize);
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
    })();
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

#[cfg(not(windows))]
fn process_executable_path(_pid: u32) -> Option<PathBuf> {
    None
}

/// `os.path.normcase` comparison of two resolved paths.
#[cfg(windows)]
fn same_executable_path(left: Option<&Path>, right: Option<&Path>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    let normcase = |path: &Path| path.to_string_lossy().to_lowercase();
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => normcase(&a) == normcase(&b),
        _ => normcase(left) == normcase(right),
    }
}

#[cfg(not(windows))]
fn same_executable_path(_left: Option<&Path>, _right: Option<&Path>) -> bool {
    false
}

/// `_KNOWN_CONFLICTS` lookup only (the Startup-page app list).
fn known_conflict_label(process_name: &str) -> Option<&'static str> {
    let lookup = process_name.trim().to_lowercase();
    KNOWN_CONFLICTS
        .iter()
        .find(|(name, _)| *name == lookup)
        .map(|(_, label)| *label)
}

/// `_KNOWN_CONFLICTS` then `_CORE_CONFLICTS` (the conflict scan).
fn conflict_label(process_name: &str) -> Option<&'static str> {
    let lookup = process_name.trim().to_lowercase();
    KNOWN_CONFLICTS
        .iter()
        .chain(CORE_CONFLICTS.iter())
        .find(|(name, _)| *name == lookup)
        .map(|(_, label)| *label)
}

/// Product labels of known foreign VPN/proxy clients among running processes.
pub fn find_conflicting_network_apps(processes: Option<&HashMap<u32, String>>) -> Vec<String> {
    let default_map;
    let processes = match processes {
        Some(processes) => processes,
        None => {
            default_map = running_processes();
            &default_map
        }
    };
    let mut found: Vec<String> = processes
        .values()
        .filter_map(|process_name| known_conflict_label(process_name))
        .map(str::to_string)
        .collect();
    // Python: sorted(set(...), key=str.casefold).
    found.sort_by_key(|label| label.to_lowercase());
    found.dedup();
    found
}

pub fn is_process_name_running(process_name: &str) -> bool {
    let target = process_name.trim().to_lowercase();
    if target.is_empty() {
        return false;
    }
    running_processes()
        .into_values()
        .any(|name| name.trim().to_lowercase() == target)
}

/// Listening sockets on `ports` held by other processes (`netstat -ano -p tcp`).
#[cfg(windows)]
pub fn find_listening_port_conflicts(
    ports: &HashSet<u16>,
    ignored_pids: Option<&HashSet<u32>>,
    ignored_executable_paths: Option<&HashSet<PathBuf>>,
    processes: Option<&HashMap<u32, String>>,
) -> Vec<PortConflict> {
    if ports.is_empty() {
        return Vec::new();
    }
    let ignored: HashSet<u32> = ignored_pids.cloned().unwrap_or_default();
    let ignored_paths: Vec<PathBuf> = ignored_executable_paths
        .map(|paths| {
            paths
                .iter()
                .filter(|path| !path.as_os_str().to_string_lossy().trim().is_empty())
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let default_map;
    let processes = match processes {
        Some(processes) => processes,
        None => {
            default_map = running_processes();
            &default_map
        }
    };

    let mut command = hidden_command("netstat");
    command.args(["-ano", "-p", "tcp"]);
    let text = match run_merged(command, 4.0) {
        Ok(result) => decode_output(&result.stdout),
        Err(_) => return Vec::new(),
    };

    let port_re = Regex::new(r":(\d+)$").expect("static regex");
    let mut found: HashMap<u16, PortConflict> = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts[parts.len() - 2].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some(captures) = port_re.captures(parts[1]) else {
            continue;
        };
        let Ok(port) = captures[1].parse::<u16>() else {
            continue;
        };
        if !ports.contains(&port) {
            continue;
        }
        let Ok(pid) = parts[parts.len() - 1].parse::<u32>() else {
            continue;
        };
        if ignored.contains(&pid) {
            continue;
        }
        if ignored_paths
            .iter()
            .any(|path| same_executable_path(process_executable_path(pid).as_deref(), Some(path)))
        {
            continue;
        }
        found.insert(
            port,
            PortConflict {
                port,
                pid,
                process_name: processes
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| "неизвестный процесс".to_string()),
            },
        );
    }
    let mut ports: Vec<u16> = found.keys().copied().collect();
    ports.sort_unstable();
    ports
        .into_iter()
        .map(|port| found[&port].clone())
        .collect()
}

#[cfg(not(windows))]
pub fn find_listening_port_conflicts(
    _ports: &HashSet<u16>,
    _ignored_pids: Option<&HashSet<u32>>,
    _ignored_executable_paths: Option<&HashSet<PathBuf>>,
    _processes: Option<&HashMap<u32, String>>,
) -> Vec<PortConflict> {
    Vec::new()
}

#[cfg(windows)]
fn system_proxy_server() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    let path = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let key = match winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_READ)
    {
        Ok(key) => key,
        Err(_) => return String::new(),
    };
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key
        .get_value::<String, _>("ProxyServer")
        .unwrap_or_default()
        .trim()
        .to_string();
    if enabled != 0 && !server.is_empty() {
        server
    } else {
        String::new()
    }
}

#[cfg(not(windows))]
fn system_proxy_server() -> String {
    String::new()
}

/// Local loopback ports referenced by a Windows ProxyServer value.
fn local_proxy_ports(proxy_server: &str) -> HashSet<u16> {
    let mut ports: HashSet<u16> = HashSet::new();
    for chunk in proxy_server.split([';', ' ', '\t', '\n', '\r']) {
        let value = chunk
            .split_once('=')
            .map(|(_, tail)| tail)
            .unwrap_or(chunk)
            .trim();
        if value.is_empty() {
            continue;
        }
        // Python urlsplit accepts a netloc-only reference; the url crate needs
        // a scheme, so borrow one without changing the authority.
        let parsed = if value.contains("://") {
            url::Url::parse(value)
        } else {
            url::Url::parse(&format!("http://{value}"))
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let host = match parsed.host() {
            Some(url::Host::Domain(domain)) => domain.to_lowercase(),
            Some(url::Host::Ipv4(ip)) => ip.to_string(),
            Some(url::Host::Ipv6(ip)) => ip.to_string(),
            None => String::new(),
        };
        if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
            continue;
        }
        if let Some(port) = parsed.port() {
            ports.insert(port);
        }
    }
    ports
}

/// A foreign client already holds the Windows system proxy.
pub fn has_foreign_system_proxy(
    ignored_pids: Option<&HashSet<u32>>,
    ignored_executable_paths: Option<&HashSet<PathBuf>>,
    processes: Option<&HashMap<u32, String>>,
) -> bool {
    let server = system_proxy_server();
    if server.is_empty() {
        return false;
    }
    let ports = local_proxy_ports(&server);
    if ports.is_empty() {
        return true;
    }
    !find_listening_port_conflicts(&ports, ignored_pids, ignored_executable_paths, processes)
        .is_empty()
}

/// Full conflict snapshot: `{"apps", "ports", "processes", "unknown_client"}`.
#[cfg(windows)]
pub fn scan_network_conflicts(
    ports: &HashSet<u16>,
    ignored_pids: Option<&HashSet<u32>>,
    ignored_executable_paths: Option<&HashSet<PathBuf>>,
) -> Value {
    let processes = running_processes();
    let mut ignored: HashSet<u32> = ignored_pids.cloned().unwrap_or_default();
    ignored.extend(own_descendant_pids());
    let ignored_paths: HashSet<PathBuf> = ignored_executable_paths
        .map(|paths| {
            paths
                .iter()
                .filter(|path| !path.as_os_str().to_string_lossy().trim().is_empty())
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let mut conflicting_processes: Vec<Value> = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();
    for (pid, process_name) in &processes {
        if ignored.contains(pid) || *pid <= 4 || *pid == own_pid() {
            continue;
        }
        let Some(label) = conflict_label(process_name) else {
            continue;
        };
        // The process manager can briefly hold an old Popen object while
        // its replacement has already inherited the same Lumen core path.
        // Match by canonical executable path as well as PID so our own
        // Xray/sing-box is never presented as a foreign VPN client.
        let process_path = process_executable_path(*pid);
        if ignored_paths
            .iter()
            .any(|path| same_executable_path(process_path.as_deref(), Some(path)))
        {
            continue;
        }
        seen_pids.insert(*pid);
        conflicting_processes.push(json!({
            "pid": pid,
            "name": process_name,
            "label": label,
        }));
    }

    let port_conflicts = find_listening_port_conflicts(
        ports,
        Some(&ignored),
        Some(&ignored_paths),
        Some(&processes),
    );
    for conflict in &port_conflicts {
        if conflict.pid <= 4 || conflict.pid == own_pid() {
            continue;
        }
        if ignored_paths
            .iter()
            .any(|path| {
                same_executable_path(process_executable_path(conflict.pid).as_deref(), Some(path))
            })
        {
            continue;
        }
        if seen_pids.insert(conflict.pid) {
            conflicting_processes.push(json!({
                "pid": conflict.pid,
                "name": conflict.process_name,
                "label": conflict.process_name,
            }));
        }
    }

    let mut apps: Vec<String> = conflicting_processes
        .iter()
        .filter_map(|item| item["label"].as_str().map(str::to_string))
        .collect();
    apps.sort_by_key(|label| label.to_lowercase());
    apps.dedup();
    json!({
        "apps": apps,
        "ports": port_conflicts.iter().map(|conflict| json!({
            "port": conflict.port,
            "pid": conflict.pid,
            "process_name": conflict.process_name,
        })).collect::<Vec<Value>>(),
        "processes": conflicting_processes,
        "unknown_client": apps.is_empty() && (!port_conflicts.is_empty() || has_foreign_system_proxy(
            Some(&ignored),
            Some(&ignored_paths),
            Some(&processes),
        )),
    })
}

#[cfg(not(windows))]
pub fn scan_network_conflicts(
    _ports: &HashSet<u16>,
    _ignored_pids: Option<&HashSet<u32>>,
    _ignored_executable_paths: Option<&HashSet<PathBuf>>,
) -> Value {
    json!({"apps": [], "ports": [], "processes": [], "unknown_client": false})
}

/// Terminate only processes that still match a freshly scanned PID/name.
/// Returns `(closed, failed)` product labels.
#[cfg(windows)]
pub fn terminate_scanned_conflicts(processes: &[Value]) -> (Vec<String>, Vec<String>) {
    let running = running_processes();
    let owned = own_descendant_pids();
    let mut closed: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for item in processes {
        // Python: int(item.get("pid") or 0); unparsable rows are skipped.
        let pid: i64 = match item.get("pid") {
            Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0) as i64,
            Some(Value::Bool(flag)) => *flag as i64,
            Some(Value::String(text)) => text.trim().parse::<i64>().unwrap_or(0),
            _ => 0,
        };
        if pid <= 0 {
            continue;
        }
        let pid = pid as u32;
        let expected_name = json_text(item.get("name")).trim().to_string();
        let label = {
            let scanned = json_text(item.get("label"));
            let expected = if expected_name.is_empty() {
                None
            } else {
                Some(expected_name.clone())
            };
            if !scanned.is_empty() {
                scanned
            } else {
                expected.unwrap_or_else(|| pid.to_string())
            }
        };
        let current_name = running
            .get(&pid)
            .map(|name| name.trim().to_string())
            .unwrap_or_default();
        if pid <= 4 || owned.contains(&pid) || current_name.is_empty() {
            continue;
        }
        if expected_name.is_empty()
            || !current_name.to_lowercase().eq(&expected_name.to_lowercase())
        {
            failed.push(label);
            continue;
        }
        let result = run_text_owned(
            &[
                "taskkill".to_string(),
                "/PID".to_string(),
                pid.to_string(),
                "/T".to_string(),
                "/F".to_string(),
            ],
            6.0,
            false,
        );
        match result {
            Ok(result) if result.returncode == 0 => closed.push(label),
            _ => failed.push(label),
        }
    }
    (closed, failed)
}

#[cfg(not(windows))]
pub fn terminate_scanned_conflicts(_processes: &[Value]) -> (Vec<String>, Vec<String>) {
    (Vec::new(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(u32, &str)]) -> HashMap<u32, String> {
        entries
            .iter()
            .map(|(pid, name)| (*pid, name.to_string()))
            .collect()
    }

    #[test]
    fn known_clients_are_reported_by_product_name() {
        let processes = map(&[(10, "v2rayN.exe"), (11, "Happ.exe"), (12, "xray.exe")]);
        assert_eq!(
            find_conflicting_network_apps(Some(&processes)),
            vec!["Happ".to_string(), "v2rayN".to_string()]
        );
    }

    #[test]
    fn local_ports_are_extracted_from_windows_proxy_value() {
        let value = "http=127.0.0.1:10809;https=localhost:10810;socks=10.0.0.1:10808";
        let ports = local_proxy_ports(value);
        assert_eq!(ports, HashSet::from([10809, 10810]));
    }

    #[test]
    fn local_ports_accept_schemes_and_ipv6() {
        let ports = local_proxy_ports("socks5://127.0.0.1:1080 [::1]:10808");
        assert_eq!(ports, HashSet::from([1080, 10808]));
        // Non-loopback hosts and portless chunks contribute nothing.
        assert!(local_proxy_ports("example.com:1080").is_empty());
        assert!(local_proxy_ports("127.0.0.1").is_empty());
    }

    #[test]
    fn csv_rows_survive_quoted_image_names() {
        let rows = parse_csv_rows("\"a,b.exe\",\"123\",\"Console\",\"1\"");
        assert_eq!(rows[0][0], "a,b.exe");
        assert_eq!(rows[0][1], "123");
        assert_eq!(rows[0].len(), 4);
    }

    #[test]
    fn json_text_stringifies_payload_fields() {
        assert_eq!(json_text(Some(&json!("name"))), "name");
        assert_eq!(json_text(Some(&json!(123))), "123");
        assert_eq!(json_text(Some(&Value::Null)), "");
        assert_eq!(json_text(None), "");
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_process_snapshot_contains_current_process() {
        let processes = running_processes_win32(None).expect("toolhelp snapshot");
        assert!(processes.contains_key(&own_pid()));
    }

    #[cfg(windows)]
    #[test]
    fn path_identity_ignores_case_and_resolution() {
        let own_path = PathBuf::from(r"C:\Lumen\core\xray.exe");
        assert!(same_executable_path(Some(&own_path), Some(&own_path)));
        assert!(!same_executable_path(
            Some(&own_path),
            Some(Path::new(r"C:\Other\sing-box.exe"))
        ));
        assert!(!same_executable_path(Some(&own_path), None));
    }

    #[cfg(windows)]
    #[test]
    fn descendant_pids_cover_the_process_tree() {
        let mut parents: HashMap<u32, u32> = HashMap::new();
        let _ = running_processes_win32(Some(&mut parents));
        // The shell that spawned this test may or may not be visible, but the
        // scan must always include our own PID.
        assert!(own_descendant_pids().contains(&own_pid()));
    }

    #[cfg(windows)]
    #[test]
    fn terminate_skips_pidless_and_mismatched_rows() {
        // A pid with no running match is skipped entirely (no closed/failed).
        let (closed, failed) = terminate_scanned_conflicts(&[json!({"pid": 4, "name": "system"})]);
        assert!(closed.is_empty() && failed.is_empty());

        // Non-parsable payloads are skipped like the Python int() guard.
        let (closed, failed) = terminate_scanned_conflicts(&[json!({"pid": "abc"})]);
        assert!(closed.is_empty() && failed.is_empty());
    }
}
