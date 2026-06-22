from __future__ import annotations

from types import SimpleNamespace

import xray_fluent.engines.singbox.manager as manager_module
from xray_fluent.engines.singbox.manager import SingBoxManager


def test_routine_connection_logs_are_suppressed() -> None:
    lines = [
        "INFO inbound/tun[tun-in]: inbound connection from 172.18.0.1:12345",
        "INFO inbound/tun[tun-in]: inbound packet connection to 172.18.0.2:53",
        "INFO outbound/vless[proxy]: outbound connection to 203.0.113.1:443",
        "ERROR connection: connection upload closed: wsarecv: connection was closed",
    ]

    assert all(SingBoxManager._is_noisy_runtime_line(line) for line in lines)


def test_actionable_runtime_errors_are_not_suppressed() -> None:
    lines = [
        "ERROR dns: exchange failed: context deadline exceeded",
        "FATAL start service: initialize rule-set failed",
        "ERROR connection: report handshake success: connection refused",
    ]

    assert not any(SingBoxManager._is_noisy_runtime_line(line) for line in lines)


def test_windows_tun_readiness_uses_single_persistent_probe(monkeypatch) -> None:
    calls = []

    def fake_run(command, **kwargs):
        calls.append((command, kwargs))
        return SimpleNamespace(returncode=0)

    proc = SimpleNamespace(pid=1234, poll=lambda: None)
    monkeypatch.setattr(manager_module, "run_text_pumped", fake_run)

    assert SingBoxManager._wait_for_windows_tun_ready(proc, "singbox_tun", 8.0)
    assert len(calls) == 1
    assert "Get-NetIPAddress" in calls[0][0][-1]
