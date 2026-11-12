use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{ensure, Context as _, Result};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::TryRecvError;

use super::*;

fn fake_singbox() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("fake_singbox.rs");
    let exe = dir
        .path()
        .join(if cfg!(windows) { "sing-box.exe" } else { "sing-box" });
    fs::write(&source, include_str!("fake_singbox.rs"))?;
    let output = Command::new(option_env!("RUSTC").unwrap_or("rustc"))
        .args([source.as_os_str(), "-o".as_ref(), exe.as_os_str()])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((dir, exe))
}

/// Spawn the fixture in `run` mode with stdout/stderr discarded.
fn spawn_fixture_child(exe: &Path, config: &Value) -> Result<Child> {
    let config_dir = tempfile::tempdir()?;
    // Leak the config dir: the fixture reads it for its whole lifetime.
    let config_path = config_dir.keep().join("config.json");
    fs::write(&config_path, config_text(config, true))?;
    let mut command = hidden_command(exe);
    command
        .args(["run", "-c"])
        .arg(&config_path)
        .arg("-D")
        .arg(exe.parent().unwrap());
    command.current_dir(exe.parent().unwrap());
    command.stdout(Stdio::null()).stderr(Stdio::null());
    Ok(command.spawn()?)
}

fn spawn_parked_child(exe: &Path) -> Result<Process> {
    Ok(Arc::new(parking_lot::Mutex::new(spawn_fixture_child(
        exe,
        &json!({"park": true}),
    )?)))
}

fn spawn_exiting_child(exe: &Path) -> Result<Process> {
    Ok(Arc::new(parking_lot::Mutex::new(spawn_fixture_child(
        exe,
        &json!({"exit": 1}),
    )?)))
}

/// Kill children the test did not hand to `stop()`.
struct KillOnDrop(Process);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.lock().kill();
        let _ = self.0.lock().wait();
    }
}

fn drain(events: &mut broadcast::Receiver<SingboxEvent>) -> Vec<SingboxEvent> {
    let mut seen = Vec::new();
    loop {
        match events.try_recv() {
            Ok(event) => seen.push(event),
            Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    seen
}

fn log_lines(events: &[SingboxEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            SingboxEvent::Log(line) => Some(line.as_str()),
            _ => None,
        })
        .collect()
}

// ── Runtime normalization (test_last_mile_*) ────────────────────────────────

#[test]
fn last_mile_runtime_normalization_matches_validation_and_startup() -> Result<()> {
    let original = json!({
        "outbounds": [
            {"type": "selector", "tag": "proxy", "outbounds": ["late-ss"]},
            {"type": "shadowsocks", "tag": "late-ss", "method": "chacha20-poly1305", "password": "secret"},
        ]
    });

    let normalized = normalized_runtime_config(&original)?;

    assert_eq!(normalized["outbounds"][1]["method"], "chacha20-ietf-poly1305");
    assert_eq!(original["outbounds"][1]["method"], "chacha20-poly1305");
    Ok(())
}

#[test]
fn last_mile_runtime_rejects_corrupted_shadowsocks_method() {
    let error = normalized_runtime_config(&json!({
        "outbounds": [{"type": "shadowsocks", "method": "зЋщчЭщн{о"}]
    }))
    .expect_err("corrupted method must be rejected");
    assert!(error.to_string().contains("Unsupported Shadowsocks method"));
}

#[test]
fn last_mile_shadowsocks_normalization_covers_plugin_uot_and_2022() -> Result<()> {
    use base64::Engine as _;
    let key32 = base64::engine::general_purpose::STANDARD.encode((0u8..32).collect::<Vec<u8>>());
    let normalized = normalized_runtime_config(&json!({
        "outbounds": [{
            "type": "shadowsocks",
            "method": "2022-blake3-aes-128-gcm",
            "password": key32,
            "plugin": "simple-obfs",
            "plugin_opts": "mode=http;host=cdn.example",
            "udp_over_tcp": {"enabled": true},
            "multiplex": {"enabled": true},
        }]
    }))?
    ["outbounds"][0]
        .clone();

    assert_eq!(normalized["method"], "2022-blake3-aes-256-gcm");
    assert_eq!(normalized["plugin"], "obfs-local");
    assert_eq!(normalized["plugin_opts"], "obfs=http;obfs-host=cdn.example");
    assert!(normalized.get("multiplex").is_none());
    Ok(())
}

#[test]
fn last_mile_normalizes_or_discards_invalid_v2ray_plugin_mux() -> Result<()> {
    let normalized = normalized_runtime_config(&json!({
        "outbounds": [
            {
                "type": "shadowsocks",
                "method": "aes-256-gcm",
                "password": "secret",
                "plugin": "v2ray-plugin",
                "plugin_opts": "mode=websocket;mux=true;host=cdn.example",
            },
            {
                "type": "shadowsocks",
                "method": "aes-256-gcm",
                "password": "secret",
                "plugin": "v2ray-plugin",
                "plugin_opts": "mode=websocket;mux=invalid;host=other.example",
            },
        ]
    }))?
    ["outbounds"]
        .clone();

    assert_eq!(normalized[0]["plugin_opts"], "mode=websocket;mux=1;host=cdn.example");
    assert_eq!(normalized[1]["plugin_opts"], "mode=websocket;host=other.example");
    Ok(())
}

// ── Noisy-line classification ───────────────────────────────────────────────

#[test]
fn routine_connection_logs_are_suppressed_when_not_normalized() {
    let lines = [
        "INFO inbound/tun[tun-in]: inbound connection from 172.18.0.1:12345",
        "INFO inbound/tun[tun-in]: inbound packet connection to 172.18.0.2:53",
        "INFO outbound/vless[proxy]: outbound connection to 203.0.113.1:443",
        "ERROR connection: connection upload closed: wsarecv: connection was closed",
    ];
    assert!(lines
        .iter()
        .all(|line| SingBoxManager::is_noisy_runtime_line(line)));
}

#[test]
fn actionable_runtime_errors_are_not_suppressed() {
    let lines = [
        "ERROR dns: exchange failed: context deadline exceeded",
        "FATAL start service: initialize rule-set failed",
        "ERROR connection: report handshake success: connection refused",
    ];
    assert!(!lines
        .iter()
        .any(|line| SingBoxManager::is_noisy_runtime_line(line)));
}

#[test]
fn routine_process_and_dns_info_noise_is_suppressed() {
    let lines = [
        "INFO [12345 0ms] router: found process path: C:\\Program Files\\Google\\Chrome\\chrome.exe",
        "INFO [12345 1ms] dns: exchanged chatgpt.com. IN A 104.18.0.1",
        "INFO [12345 2ms] dns: lookup succeeded for claude.ai",
        "INFO [12345 0ms] dns: cached gemini.google.com. IN A",
        "WARNING inbound/tun[tun-in]: open interface take too much time to finish!",
    ];
    assert!(lines
        .iter()
        .all(|line| SingBoxManager::is_noisy_runtime_line(line)));
}

#[test]
fn error_lines_win_over_info_noise_markers() {
    let repeated = "ERROR [1 5.0s] dns: lookup failed for api.openai.com: i/o timeout context deadline";
    assert!(!SingBoxManager::is_noisy_runtime_line(repeated));
    let plain = "ERROR router: found process failed unexpectedly";
    assert!(!SingBoxManager::is_noisy_runtime_line(plain));
}

#[test]
fn repeated_dns_runtime_errors_stay_visible() {
    let lines = [
        "ERROR [1 10.0s] dns: exchange failed for www.msftconnecttest.com. IN A: context deadline exceeded",
        "WARN dns: bad question size: 0",
    ];
    assert!(!lines
        .iter()
        .any(|line| SingBoxManager::is_noisy_runtime_line(line)));
}

#[test]
fn singbox_process_lookup_and_bad_question_noise_is_suppressed() {
    let lines = [
        "ERROR router: process DNS packet: unpack request: bad question name: dns: bad rdata",
        "INFO router: failed to search process: process not found for 172.18.0.1:63918",
        "INFO router: failed to search process: Access is denied.",
    ];
    assert!(lines
        .iter()
        .all(|line| SingBoxManager::is_noisy_runtime_line(line)));
}

#[test]
fn startup_chatter_filter_never_hides_errors() {
    assert!(SingBoxManager::is_startup_routine_line(
        "INFO outbound/masque[proxy]: outbound connection to 1.1.1.1:53"
    ));
    assert!(!SingBoxManager::is_startup_routine_line(
        "ERROR outbound/masque[proxy]: tunnel not initialized"
    ));
}

// ── Bounded runtime activity sampling ───────────────────────────────────────

#[test]
fn runtime_work_logs_are_sampled_without_emitting_process_paths() {
    let manager = SingBoxManager::with_config_path(PathBuf::from("unused.json"));
    let mut events = manager.subscribe();
    let now = 100.0f64;
    let first = "INFO outbound/vless[proxy]: outbound connection to example.org:443";
    let second = "INFO outbound/vless[proxy]: outbound connection to example.net:443";
    let inbound = "INFO inbound/mixed[socks-in]: inbound connection from 127.0.0.1:61234";
    let route = "INFO [123 2ms] router: match[4] domain_suffix=example.org => proxy";
    assert!(SingBoxManager::is_noisy_runtime_line(first));
    manager.emit_bounded_runtime_activity_at(first, now);
    manager.emit_bounded_runtime_activity_at(second, now);
    manager.emit_bounded_runtime_activity_at(inbound, now);
    manager.emit_bounded_runtime_activity_at(route, now);
    manager.emit_bounded_runtime_activity_at(
        "INFO router: found process path: C:\\private\\app.exe",
        now,
    );
    let seen = drain(&mut events);
    assert_eq!(
        log_lines(&seen),
        vec![first, second, inbound, route]
    );
    for _ in 0..17 {
        manager.emit_bounded_runtime_activity_at(second, now);
    }
    let seen = drain(&mut events);
    assert_eq!(log_lines(&seen).len(), 16, "{seen:?}");
    manager.emit_bounded_runtime_activity_at(second, now + 1.1);
    let seen = drain(&mut events);
    assert_eq!(
        log_lines(&seen),
        vec![
            "INFO [sing-box] 1 additional traffic events hidden in the previous second",
            second,
        ]
    );
}

// ── Reader pipeline ─────────────────────────────────────────────────────────

fn reader_events(manager: &SingBoxManager, process: &Process, payload: &[u8]) {
    let (reader, mut writer) = std::io::pipe().expect("pipe");
    let payload = payload.to_vec();
    let feed = thread::spawn(move || {
        let _ = writer.write_all(&payload);
        // Dropping the writer closes the pipe so the reader sees EOF.
    });
    read_output(reader, &manager.shared, process);
    feed.join().expect("feed thread");
    let _ = process.lock().wait();
}

#[test]
fn runtime_reader_shows_proxy_and_tun_work_without_process_path() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let process = spawn_exiting_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    // Python: manager._proc = proc
    manager.shared.state.lock().process = Some(process.clone());
    reader_events(
        &manager,
        &process,
        b"INFO inbound/mixed[socks-in]: inbound connection from 127.0.0.1:61234\n\
          INFO inbound/tun[tun-in]: inbound packet connection to 1.1.1.1:53\n\
          INFO [42 2ms] router: match[4] => proxy\n\
          INFO outbound/direct[direct]: outbound connection to 1.1.1.1:443\n\
          INFO router: found process path: C:\\private\\app.exe\n\
          ERROR router: found process path: C:\\private\\missing.exe\n",
    );
    // Drain what the reader emitted while it ran.
    let seen = drain(&mut events);
    let logs = log_lines(&seen);
    assert!(logs.len() >= 4, "{seen:?}");
    assert!(logs.iter().any(|line| line.contains("inbound/mixed")));
    assert!(logs.iter().any(|line| line.contains("inbound/tun")));
    assert!(logs.iter().any(|line| line.contains("router: match[")));
    assert!(logs.iter().any(|line| line.contains("outbound/direct")));
    assert!(!logs.iter().any(|line| line.contains("found process path")));
    Ok(())
}

#[test]
fn runtime_reader_hides_process_path_but_keeps_actionable_errors() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let process = spawn_exiting_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    // Python: manager._proc = proc
    manager.shared.state.lock().process = Some(process.clone());
    reader_events(
        &manager,
        &process,
        b"INFO router: found process path: C:\\Program Files\\app.exe\n\
          ERROR dns: exchange failed: context deadline exceeded\n",
    );
    let seen = drain(&mut events);
    let logs = log_lines(&seen);
    assert!(!logs.iter().any(|line| line.contains("found process path")));
    assert!(logs
        .iter()
        .any(|line| line.contains("context deadline exceeded")));
    Ok(())
}

#[test]
fn reader_does_not_report_transient_startup_exit_before_retry() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let process = spawn_exiting_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(wait_for_process(&process, Duration::from_secs(5)));
    {
        let mut state = manager.shared.state.lock();
        state.process = Some(process.clone());
        state.starting = true;
    }
    handle_process_exit(&manager.shared, &process, 1, false);
    let seen = drain(&mut events);
    assert!(!seen.iter().any(|event| matches!(event, SingboxEvent::Error(_))));
    assert!(!seen.iter().any(|event| matches!(event, SingboxEvent::Stopped(_))));
    assert_eq!(manager.last_exit_code(), Some(1));
    Ok(())
}

#[test]
fn singbox_exit_during_windows_shutdown_is_not_reported_as_runtime_error() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let process = spawn_exiting_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    {
        let mut state = manager.shared.state.lock();
        state.process = Some(process.clone());
        state.running = true;
    }
    handle_process_exit(&manager.shared, &process, 1, true);
    let seen = drain(&mut events);
    assert!(!seen.iter().any(|event| matches!(event, SingboxEvent::Error(_))));
    Ok(())
}

// ── Readiness probes ────────────────────────────────────────────────────────

#[test]
fn proxy_ready_marker_avoids_bare_tcp_probe_errors() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    manager.observe_startup_line("NOTICE sing-box started (0.4s)");
    let process = spawn_parked_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    // The marker returns before any TCP probe could fire against these
    // (deliberately unbound) ports.
    assert!(manager.wait_until_proxy_ready(&process, &[59991, 59992, 59993], 10.0));
    Ok(())
}

#[test]
fn native_core_ready_marker_skips_slow_windows_probe() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    manager.observe_startup_line("NOTICE sing-box started (0.42s)");
    let process = spawn_parked_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(manager.wait_until_tun_ready(&process, "tun0", 1.0));
    Ok(())
}

#[test]
fn direct_masque_confirmation_uses_native_handshake_marker() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    manager.observe_startup_line(
        "NOTICE outbound/masque[proxy]: Connected to MASQUE server 162.159.198.2:443",
    );
    let process = spawn_parked_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(manager.wait_for_profile_confirmation(&process, 0.1));
    Ok(())
}

#[test]
fn transient_masque_error_does_not_override_later_handshake() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    manager.observe_startup_line("ERROR outbound/masque[proxy]: tunnel not initialized");
    manager.observe_startup_line(
        "NOTICE outbound/masque[proxy]: Connected to MASQUE server 162.159.198.2:443",
    );
    let process = spawn_parked_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(manager.wait_for_profile_confirmation(&process, 0.1));
    Ok(())
}

#[test]
fn profile_initialization_error_fails_fast() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("runtime.json"));
    manager.observe_startup_line("ERROR outbound/masque[proxy]: tunnel not initialized");
    let process = spawn_parked_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(!manager.wait_for_profile_startup_settle(&process, 1.0));
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_tun_ready_script_matches_python_template() {
    let script = windows_tun_ready_script(1234, "tun0", 8000);
    assert!(script.starts_with("$deadline = [DateTime]::UtcNow.AddMilliseconds(8000); "));
    assert!(script.contains("Get-Process -Id 1234"));
    assert!(script.contains("Get-NetIPAddress -InterfaceAlias 'tun0'"));
    assert!(script.contains("Get-NetRoute -InterfaceAlias 'tun0'"));
    assert!(script.contains("@('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1')"));
    assert!(script.ends_with("Start-Sleep -Milliseconds 500 }; exit 1"));
}

#[cfg(windows)]
#[test]
fn windows_tun_ready_probe_fails_when_the_core_is_gone() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let process = spawn_exiting_child(&exe)?;
    let _guard = KillOnDrop(process.clone());
    assert!(wait_for_process(&process, Duration::from_secs(5)));
    assert!(!SingBoxManager::wait_for_windows_tun_ready(&process, "tun0", 1.0));
    Ok(())
}

// ── Config shape helpers ────────────────────────────────────────────────────

#[test]
fn proxy_runtime_ports_are_detected_without_tun() {
    let config = json!({
        "inbounds": [
            {"type": "mixed", "listen": "127.0.0.1", "listen_port": 10808},
            {"type": "http", "listen": "127.0.0.1", "listen_port": 10809},
        ]
    });
    assert_eq!(SingBoxManager::extract_tun_interface_name(&config), "");
    assert_eq!(SingBoxManager::extract_local_proxy_ports(&config), vec![10808, 10809]);
}

#[test]
fn empty_tun_interface_uses_lumen_default_alias() {
    assert_eq!(
        SingBoxManager::extract_tun_interface_name(&json!({"inbounds": [{"type": "tun"}]})),
        "singbox_tun"
    );
}

#[test]
fn custom_warp_and_masque_require_runtime_readiness() {
    assert!(SingBoxManager::requires_profile_outbound_readiness(
        &json!({"endpoints": [{"type": "warp", "tag": "proxy"}]})
    ));
    assert!(SingBoxManager::requires_profile_outbound_readiness(
        &json!({"outbounds": [{"type": "masque", "tag": "proxy"}]})
    ));
    assert!(!SingBoxManager::requires_profile_outbound_readiness(&json!({
        "outbounds": [{
            "type": "masque",
            "tag": "proxy",
            "server": "162.159.198.2",
            "private_key": "private",
            "public_key": "public",
            "address": ["172.16.0.2/32"],
        }]
    })));
    assert!(!SingBoxManager::requires_profile_outbound_readiness(
        &json!({"endpoints": [{"type": "wireguard", "tag": "proxy", "amnezia": {"jc": 4}}]})
    ));
    assert!(!SingBoxManager::requires_profile_outbound_readiness(
        &json!({"endpoints": [{"type": "wireguard", "tag": "proxy"}]})
    ));
}

#[test]
fn tun_interface_name_is_restricted_before_powershell_embedding() {
    // Quotes, separators and newlines must never be accepted as an adapter
    // alias, even when they came from an imported raw sing-box profile.
    let names = managed_tun_names_ps("bad'; Get-Process");
    assert!(!names.contains("bad'"));
    assert!(managed_tun_names_ps("good_name").contains("good_name"));
}

// ── Direct-MASQUE compatibility ─────────────────────────────────────────────

#[test]
fn direct_masque_requires_lumen_compatible_core() {
    let detail = direct_masque_version_error("1.13.14-extended-2.5.1");
    assert!(detail.contains("Lumen-compatible"), "{detail}");
    assert!(detail.contains("tunnel uninitialized"), "{detail}");
    let unknown = direct_masque_version_error("");
    assert!(unknown.contains("installed: unknown"), "{unknown}");
}

#[test]
fn direct_masque_compatibility_accepts_lumen_core() {
    assert_eq!(direct_masque_version_error("1.13.14-extended-2.5.1-lumen.1"), "");
    assert_eq!(direct_masque_version_error("1.13.14-lumen.1"), "");
}

#[test]
fn direct_masque_check_skips_plain_configs() {
    assert_eq!(direct_masque_version_error_for_config(&json!({"outbounds": []})), "");
}

fn direct_masque_version_error_for_config(config: &Value) -> String {
    if !SingBoxManager::requires_lumen_direct_masque(config) {
        return String::new();
    }
    direct_masque_version_error("1.13.14-extended-2.5.1")
}

// ── Startup classifiers ─────────────────────────────────────────────────────

#[test]
fn element_not_found_is_a_retryable_wintun_startup_race() {
    let manager = SingBoxManager::with_config_path(PathBuf::from("unused.json"));
    manager
        .shared
        .state
        .lock()
        .last_output_lines
        .push_back(
            "FATAL start inbound/tun: configure tun interface: set ipv6 address: Element not found."
                .into(),
        );
    assert!(manager.startup_error_is_retryable());
    assert!(manager.startup_error_is_stale_adapter());
}

#[test]
fn ipv6_auto_disable_strips_the_ipv6_prefix_the_planner_emits() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("singbox.json");
    fs::write(
        &config_path,
        config_text(
            &json!({
                "inbounds": [{
                    "type": "tun",
                    "tag": "tun-in",
                    "address": ["172.18.0.1/30", "fdfe:dcba:9876::1/126"],
                }]
            }),
            true,
        ),
    )?;
    let manager = SingBoxManager::with_config_path(config_path.clone());
    let mut events = manager.subscribe();

    manager.disable_ipv6_in_singbox_config();

    let payload: Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
    assert_eq!(payload["inbounds"][0]["address"], json!(["172.18.0.1/30"]));
    let seen = drain(&mut events);
    assert!(!seen.iter().any(|event| matches!(
        event,
        SingboxEvent::Log(line) if line.contains("Failed to auto-disable")
    )));
    Ok(())
}

#[test]
fn startup_classifiers_survive_concurrent_reader_output() {
    let manager = SingBoxManager::with_config_path(PathBuf::from("unused.json"));
    {
        let mut state = manager.shared.state.lock();
        for index in 0..100 {
            state.last_output_lines.push_back(format!("INFO line {index}"));
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let shared = manager.shared.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let mut state = shared.state.lock();
                ring_push(&mut state.last_output_lines, "INFO more output".into());
            }
        })
    };
    for _ in 0..200 {
        assert!(!manager.startup_error_is_retryable());
        assert!(!manager.startup_error_is_stale_adapter());
        assert!(!manager.startup_error_is_ipv6_disabled());
    }
    stop.store(true, Ordering::SeqCst);
    writer.join().unwrap();
}

// ── Lifecycle against the real fixture ──────────────────────────────────────

#[test]
fn validates_configs_and_queries_the_version_from_the_fixture() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let exe_str = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let manager = SingBoxManager::with_config_path(dir.path().join("singbox_config.json"));

    assert_eq!(
        get_singbox_version(exe_str).as_deref(),
        Some("1.13.14-extended-2.5.1")
    );
    let config = json!({"inbounds": [{"type": "mixed", "listen_port": 10808}]});
    assert_eq!(
        manager.validate_config(exe_str, &config),
        (true, "Configuration OK".into())
    );
    let (valid, output) = manager.validate_config(exe_str, &json!({"reject": true}));
    assert!(!valid);
    assert!(output.contains("rejected"), "{output}");

    // The Lumen marker flips the version line the fixture reports.
    fs::write(exe.parent().unwrap().join("lumen_marker"), b"")?;
    assert_eq!(
        get_singbox_version(exe_str).as_deref(),
        Some("1.13.14-extended-2.5.1-lumen.1")
    );
    Ok(())
}

#[test]
fn starts_and_stops_a_local_proxy_core() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let exe_str = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port();
    let config = json!({"inbounds": [{"type": "mixed", "listen": "127.0.0.1", "listen_port": port}]});
    let config_path = dir.path().join("singbox_config.json");
    let mut manager = SingBoxManager::with_config_path(config_path.clone());
    let mut events = manager.subscribe();

    assert!(manager.start(exe_str, &config, false));
    assert!(manager.is_running());
    assert!(manager.process_id().is_some());
    let saved = fs::read_to_string(&config_path)?;
    assert!(saved.contains(r#""listen_port""#));

    assert!(manager.stop(true, true));
    assert!(!manager.is_running());
    assert_eq!(manager.process_id(), None);

    let seen = drain(&mut events);
    assert!(seen.iter().any(|event| matches!(event, SingboxEvent::Started)));
    assert!(seen
        .iter()
        .any(|event| matches!(event, SingboxEvent::StateChanged(true))));
    assert!(seen
        .iter()
        .any(|event| matches!(event, SingboxEvent::Stopped(_))));
    assert!(seen
        .iter()
        .any(|event| matches!(event, SingboxEvent::StateChanged(false))));
    let logs = log_lines(&seen);
    assert!(logs
        .iter()
        .any(|line| line.contains("startup attempt 1/1, proxy ports=(")
            && line.ends_with(",)")),
        "{logs:?}");
    assert!(logs
        .iter()
        .any(|line| line.contains("local proxy inbounds ready in") && line.ends_with(" ms")));
    assert!(logs
        .iter()
        .any(|line| line.contains("runtime ready in") && line.contains("proxy ports=(")));
    Ok(())
}

#[test]
fn fast_stop_does_not_block_on_adapter_release_probe() -> Result<()> {
    let (_fixture, exe) = fake_singbox()?;
    let _dir = tempfile::tempdir()?;
    let mut manager = SingBoxManager::with_config_path(PathBuf::from("unused.json"));
    let process = spawn_parked_child(&exe)?;
    {
        let mut state = manager.shared.state.lock();
        state.process = Some(process.clone());
        state.tun_mode = true;
    }
    assert!(manager.stop(true, true));
    // `stop` leaves ownership bookkeeping to the reader, exactly like Python.
    drop(process);
    Ok(())
}

#[test]
fn legacy_tun_aliases_never_authorize_adapter_deletion() {
    // No PowerShell probe may run from the cleanup hooks (they are no-ops).
    SingBoxManager::cleanup_orphaned_tun_adapters(5.0, "Lumen-TUN 1");
    let manager = SingBoxManager::with_config_path(PathBuf::from("unused.json"));
    manager.purge_stale_wintun_devices(20.0, "Lumen-TUN 1");
    SingBoxManager::wait_tun_released(0.0, "Lumen-TUN 1");
}

#[test]
fn rotate_tun_interface_name_rewrites_the_failed_inbound() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("singbox_config.json");
    fs::write(
        &config_path,
        config_text(
            &json!({"inbounds": [
                {"type": "mixed", "listen_port": 10808},
                {"type": "tun", "tag": "tun-in", "interface_name": "singbox_tun"},
            ]}),
            true,
        ),
    )?;
    let manager = SingBoxManager::with_config_path(config_path.clone());
    let mut events = manager.subscribe();
    let fresh = manager.rotate_tun_interface_name("singbox_tun")?;
    assert!(fresh.starts_with("singbox_tun-"));
    let payload: Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
    assert_eq!(payload["inbounds"][1]["interface_name"], fresh.as_str());
    assert_eq!(payload["inbounds"][0].get("interface_name"), None);
    let seen = drain(&mut events);
    assert!(seen.iter().any(|event| matches!(
        event,
        SingboxEvent::Log(line) if line.contains("adapter name collision")
    )));

    let missing = manager.rotate_tun_interface_name("never-used");
    let error = missing.expect_err("unknown previous name must fail");
    assert!(error.to_string().contains("Cannot find the failed TUN inbound"));
    Ok(())
}
