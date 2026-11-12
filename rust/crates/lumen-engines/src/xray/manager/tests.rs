use std::fs;
use std::net::TcpListener;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde_json::json;

use super::*;

fn fake_xray() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("fake_xray.rs");
    let exe = dir
        .path()
        .join(if cfg!(windows) { "xray.exe" } else { "xray" });
    fs::write(&source, include_str!("fake_xray.rs"))?;
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

fn local_config() -> Result<Value> {
    let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    Ok(json!({"inbounds": [{"port": port, "protocol": "http"}], "label": "Пример"}))
}

#[test]
fn starts_a_validated_core_and_keeps_it_running_when_a_new_config_is_invalid() -> Result<()> {
    let (_fixture, exe) = fake_xray()?;
    let exe = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("xray_config.json");
    let mut manager = XrayManager::with_config_path(config_path.clone());
    let mut events = manager.subscribe();
    let mut config = local_config()?;
    assert_eq!(get_xray_version(exe).as_deref(), Some("Xray fixture 1.0"));
    assert_eq!(
        manager.validate_config(exe, &config),
        (true, "Configuration OK".into())
    );

    assert!(manager.start(exe, &config, false));
    assert!(manager.is_running());
    let running_pid = manager.process_id().expect("active Xray process");
    let saved = fs::read_to_string(&config_path)?;
    assert!(saved.contains(r"\u041f\u0440\u0438\u043c\u0435\u0440"));
    config["reject"] = json!(true);
    assert!(!manager.start(exe, &config, false));
    assert!(manager.is_running());
    assert_eq!(manager.process_id(), Some(running_pid));
    assert_eq!(fs::read_to_string(&config_path)?, saved);
    assert!(manager.stop(true, false));
    assert!(!manager.is_running());
    assert_eq!(manager.process_id(), None);
    assert!(manager.last_exit_expected());
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);

    let seen: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(seen.contains(&XrayEvent::Started));
    assert!(seen.contains(&XrayEvent::StateChanged(true)));
    assert!(
        seen.iter()
            .any(|event| matches!(event, XrayEvent::Stopped(_)))
    );
    assert!(seen.contains(&XrayEvent::StateChanged(false)));
    assert!(seen.contains(&XrayEvent::Error(
        "Xray не принимает новый конфиг: validation rejected".into()
    )));
    assert!(seen.contains(&XrayEvent::Log("[Warning] fixture stderr".into())));
    Ok(())
}

#[test]
fn reports_fakedns_startup_crash_once() -> Result<()> {
    let (_fixture, exe) = fake_xray()?;
    let exe = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let mut manager = XrayManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let mut config = local_config()?;
    config["crash"] = json!(true);

    assert!(!manager.start(exe, &config, true));
    assert!(!manager.last_exit_expected());
    assert_eq!(manager.last_exit_code(), Some(23));
    let seen: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let errors: Vec<_> = seen
        .iter()
        .filter_map(|event| match event {
            XrayEvent::Error(message) => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "{seen:?}");
    assert!(errors[0].contains("во время запуска: текущий Xray core упал на секции FakeDNS"));
    assert!(!seen.contains(&XrayEvent::Started));
    Ok(())
}

#[test]
fn restarts_once_when_the_first_process_does_not_open_its_port() -> Result<()> {
    let (_fixture, exe) = fake_xray()?;
    let exe = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let mut manager = XrayManager::with_config_path(dir.path().join("runtime.json"));
    let mut events = manager.subscribe();
    let mut config = local_config()?;
    config["delay_first"] = json!(true);
    assert!(manager.start_with_timeout(exe, &config, true, Duration::from_secs(1)));
    assert!(manager.is_running());
    assert!(manager.stop(true, true));

    let seen: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    let retries = seen
        .iter()
        .filter(
            |event| matches!(event, XrayEvent::Log(text) if text.contains("restarting core once")),
        )
        .count();
    assert_eq!(retries, 1, "{seen:?}");
    assert_eq!(
        seen.iter()
            .filter(|event| **event == XrayEvent::Started)
            .count(),
        1
    );
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, XrayEvent::Error(_)))
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn only_reclaims_a_listening_xray_from_the_same_executable_path() -> Result<()> {
    let (_fixture, exe) = fake_xray()?;
    let exe = exe.to_str().context("fixture path")?;
    let dir = tempfile::tempdir()?;
    let foreign_exe = dir.path().join("xray.exe");
    fs::copy(exe, &foreign_exe)?;
    let mut old = XrayManager::with_config_path(dir.path().join("old.json"));
    let mut next = XrayManager::with_config_path(dir.path().join("next.json"));
    let mut events = next.subscribe();
    let config = local_config()?;

    assert!(old.start(exe, &config, true));
    assert!(!next.start(
        foreign_exe.to_str().context("foreign fixture path")?,
        &config,
        true
    ));
    assert!(old.is_running());
    assert!(
        matches!(events.try_recv()?, XrayEvent::Error(message) if message.contains("другим VPN/прокси-клиентом"))
    );

    assert!(next.start(exe, &config, true));
    assert!(next.is_running());
    assert!(old.stop(true, true));
    assert!(!old.is_running());
    assert!(next.stop(true, true));
    let seen: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(seen.iter().any(
        |event| matches!(event, XrayEvent::Log(line) if line.contains("terminated stale xray.exe"))
    ));
    Ok(())
}
