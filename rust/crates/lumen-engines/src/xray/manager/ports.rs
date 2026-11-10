use std::net::{Ipv4Addr, TcpStream};
use std::path::Path;
use std::time::Duration;

use lumen_core::models::str_field;
use serde_json::Value;

pub(super) type PortRoles = Vec<(u16, String)>;

pub(super) fn required_ports(config: &Value) -> PortRoles {
    let mut ports = PortRoles::new();
    let Some(inbounds) = config.get("inbounds").and_then(Value::as_array) else {
        return ports;
    };
    for inbound in inbounds {
        let Some(port) = inbound["port"]
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
        else {
            continue;
        };
        if port == 0 {
            continue;
        }
        let protocol = str_field(inbound, "protocol").trim().to_lowercase();
        let tag = str_field(inbound, "tag").trim().to_lowercase();
        let role = match (protocol.as_str(), tag.as_str()) {
            ("http", _) => "HTTP".into(),
            ("socks", _) => "SOCKS".into(),
            (_, "api") => "API".into(),
            (_, "") if protocol.is_empty() => "local".into(),
            (_, "") => protocol,
            _ => tag,
        };
        if let Some((_, old_role)) = ports.iter_mut().find(|(existing, _)| *existing == port) {
            *old_role = role;
        } else {
            ports.push((port, role));
        }
    }
    ports
}

pub(super) fn port_ready(port: u16, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), timeout).is_ok()
}

pub(super) fn missing_ports(ports: &PortRoles) -> String {
    let missing: Vec<_> = ports
        .iter()
        .filter(|(port, _)| !port_ready(*port, Duration::from_millis(500)))
        .map(|(port, role)| format!("{role} {port}"))
        .collect();
    if missing.is_empty() {
        "нужные порты".into()
    } else {
        missing.join(", ")
    }
}

#[cfg(windows)]
pub(super) fn ensure_available(
    ports: &PortRoles,
    exe: &Path,
    log: impl Fn(String),
) -> Result<(), String> {
    use lumen_platform::subprocess::run_text;

    for (port, role) in ports {
        if !port_ready(*port, Duration::from_millis(500)) {
            continue;
        }
        let Some(pid) = listening_pid(*port) else {
            continue;
        };
        let pid_filter = format!("PID eq {pid}");
        let name = run_text(
            &["tasklist", "/FI", &pid_filter, "/FO", "CSV", "/NH"],
            5.0,
            false,
        )
        .ok()
        .map(|result| {
            result
                .output_text()
                .trim_start()
                .strip_prefix('"')
                .and_then(|row| row.split_once('"'))
                .map(|(name, _)| name.trim().to_owned())
                .unwrap_or_default()
        })
        .unwrap_or_default();
        if name.eq_ignore_ascii_case("xray.exe") && kill_stale_listener(*port, pid, exe) {
            std::thread::sleep(Duration::from_millis(500));
            if !port_ready(*port, Duration::from_millis(500)) {
                log(format!(
                    "[xray] terminated stale xray.exe PID {pid} on port {port}"
                ));
                continue;
            }
        }
        return Err(conflict_message(*port, role, pid, &name));
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn ensure_available(_: &PortRoles, _: &Path, _: impl Fn(String)) -> Result<(), String> {
    // The Python implementation has no port-owner query outside Windows.
    Ok(())
}

#[cfg(windows)]
fn listening_pid(port: u16) -> Option<u32> {
    let result =
        lumen_platform::subprocess::run_text(&["netstat", "-ano", "-p", "tcp"], 5.0, false).ok()?;
    parse_listening_pid(&result.output_text(), port)
}

#[cfg(any(windows, test))]
fn parse_listening_pid(text: &str, port: u16) -> Option<u32> {
    text.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        if !columns.next()?.eq_ignore_ascii_case("TCP") {
            return None;
        }
        let (_, local_port) = columns.next()?.rsplit_once(':')?;
        if local_port.parse::<u16>().ok()? != port {
            return None;
        }
        columns.next()?;
        if !columns.next()?.eq_ignore_ascii_case("LISTENING") {
            return None;
        }
        Some(columns.next()?.parse().unwrap_or(0))
    })
}

#[cfg(windows)]
fn kill_stale_listener(port: u16, pid: u32, exe: &Path) -> bool {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use lumen_platform::subprocess::{is_same_path, run_text};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    if pid == 0 || pid == std::process::id() {
        return false;
    }
    // Keep the process object alive while checking the path and killing by PID.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };
    let _owned_handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .is_err()
    {
        return false;
    }
    let owner = std::ffi::OsString::from_wide(&buffer[..length as usize]);
    if !is_same_path(Some(Path::new(&owner)), Some(exe)) || listening_pid(port) != Some(pid) {
        return false;
    }
    run_text(
        &["taskkill", "/PID", &pid.to_string(), "/T", "/F"],
        5.0,
        false,
    )
    .is_ok_and(|result| result.returncode == 0)
}

#[cfg(any(windows, test))]
fn conflict_message(port: u16, role: &str, pid: u32, name: &str) -> String {
    let owner = if pid == 0 {
        "другим процессом".into()
    } else if name.eq_ignore_ascii_case("xray.exe") {
        format!("другим VPN/прокси-клиентом (PID {pid})")
    } else if name.is_empty() {
        format!("PID {pid}")
    } else {
        format!("процессом {name} (PID {pid})")
    };
    let hint = match role {
        "HTTP" => " Измените HTTP порт в настройках или закройте конфликтующее приложение.",
        "SOCKS" => " Измените SOCKS порт в настройках или закройте конфликтующее приложение.",
        "API" => " Перезапустите приложение или завершите зависший Xray, который держит API порт.",
        _ => "",
    };
    format!("{role} порт {port} уже занят {owner}.{hint}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_port_order_with_last_role_and_only_accepts_integer_ports() {
        let config = json!({"inbounds": [null, {"port": "1080"}, {"port": 1.5}, {"port": 0},
            {"port": 1080, "protocol": "socks"}, {"port": 18085, "tag": " API "},
            {"port": 1080, "protocol": " http "}, {"port": 10101, "tag": "mixed"}]});
        assert_eq!(
            required_ports(&config),
            vec![
                (1080, "HTTP".into()),
                (18085, "API".into()),
                (10101, "mixed".into())
            ]
        );
    }

    #[test]
    fn only_reports_listeners_on_the_requested_local_port() {
        let output =
            "TCP 127.0.0.1:8000 127.0.0.1:1080 ESTABLISHED 5\nTCP [::1]:1080 [::]:0 LISTENING 4242";
        assert_eq!(parse_listening_pid(output, 1080), Some(4242));
        assert_eq!(parse_listening_pid(output, 8000), None);
        assert!(
            conflict_message(1080, "SOCKS", 4242, "XRAY.EXE")
                .contains("другим VPN/прокси-клиентом (PID 4242)")
        );
    }
}
