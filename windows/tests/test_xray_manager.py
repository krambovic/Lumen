from __future__ import annotations

from pathlib import Path

from xray_fluent.engines.xray.manager import XrayManager


def test_foreign_xray_is_not_killed_when_it_owns_proxy_port(monkeypatch) -> None:
    manager = XrayManager()
    manager._exe_path = Path("C:/Program Files/Lumen/core/xray.exe")
    killed: list[int] = []
    monkeypatch.setattr(manager, "_is_port_ready", lambda _port: True)
    monkeypatch.setattr(manager, "_find_listening_port_owner", lambda _port: (4242, "xray.exe"))
    monkeypatch.setattr(manager, "_lookup_process_path", lambda _pid: Path("C:/v2rayN/bin/xray.exe"))
    monkeypatch.setattr(manager, "_kill_pid", lambda pid: killed.append(pid) or True)

    message = manager._ensure_ports_available({10808: "SOCKS"})

    assert killed == []
    assert "10808" in str(message)
    assert "другим VPN/прокси-клиентом" in str(message)


def test_only_lumen_xray_can_be_cleaned_up_as_stale(monkeypatch) -> None:
    manager = XrayManager()
    manager._exe_path = Path("C:/Program Files/Lumen/core/xray.exe")
    readiness = iter((True, False))
    killed: list[int] = []
    monkeypatch.setattr(manager, "_is_port_ready", lambda _port: next(readiness))
    monkeypatch.setattr(manager, "_find_listening_port_owner", lambda _port: (4343, "xray.exe"))
    monkeypatch.setattr(manager, "_lookup_process_path", lambda _pid: manager._exe_path)
    monkeypatch.setattr(manager, "_kill_pid", lambda pid: killed.append(pid) or True)
    monkeypatch.setattr("xray_fluent.engines.xray.manager.sleep_with_events", lambda _seconds: None)

    message = manager._ensure_ports_available({10808: "SOCKS"})

    assert message is None
    assert killed == [4343]
