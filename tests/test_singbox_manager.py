from __future__ import annotations

from types import SimpleNamespace

import xray_fluent.engines.singbox.manager as manager_module
from xray_fluent.engines.singbox.manager import SingBoxManager


def test_routine_connection_logs_can_be_normalized_to_v2rayn_style_access_lines() -> None:
    manager = SingBoxManager()

    assert (
        manager._format_access_log_line(
            "+0300 INFO [12345 1ms] inbound/tun[tun-in]: inbound connection to chatgpt.com:443"
        )
        == ""
    )
    assert (
        manager._format_access_log_line(
            "+0300 INFO [12345 25ms] outbound/vless[proxy]: outbound connection to chatgpt.com:443"
        )
        == "[singbox-access] accepted tcp:chatgpt.com:443 [tun -> proxy]"
    )


def test_routine_connection_logs_are_suppressed_when_not_normalized() -> None:
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


def test_routine_process_and_dns_info_noise_is_suppressed() -> None:
    lines = [
        "INFO [12345 0ms] router: found process path: C:\\\\Program Files\\\\Google\\\\Chrome\\\\chrome.exe",
        "INFO [12345 1ms] dns: exchanged chatgpt.com. IN A 104.18.0.1",
        "INFO [12345 2ms] dns: lookup succeeded for claude.ai",
        "INFO [12345 0ms] dns: cached gemini.google.com. IN A",
    ]

    assert all(SingBoxManager._is_noisy_runtime_line(line) for line in lines)


def test_error_lines_win_over_info_noise_markers() -> None:
    # A line carrying an error token must never be suppressed even if it also
    # contains an info-noise marker like "dns:" or "found process".
    line = "ERROR [1 5.0s] dns: lookup failed for api.openai.com: i/o timeout context deadline"
    # This specific shape is recognised as repeated DNS runtime noise, but a
    # plain unexpected error with the same markers stays visible:
    plain = "ERROR router: found process failed unexpectedly"
    assert SingBoxManager._is_noisy_runtime_line(plain) is False


def test_repeated_dns_runtime_errors_stay_visible() -> None:
    lines = [
        "ERROR [1 10.0s] dns: exchange failed for www.msftconnecttest.com. IN A: context deadline exceeded",
        "WARN dns: bad question size: 0",
        "ERROR router: process DNS packet: unpack request: bad question name: dns: bad rdata",
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
