from types import SimpleNamespace
import pytest
from xray_fluent import process_traffic_collector as collector
from xray_fluent import win_proc_monitor as native
from xray_fluent.live_metrics_worker import _metrics_idle_delay

def test_pid_resolution_runs_once_per_snapshot_not_per_connection(monkeypatch):
    calls = []
    monkeypatch.setattr(collector, "process_name_from_pid", lambda pid: calls.append(pid) or "browser.exe")
    data = {"connections": [
        {"id": str(i), "metadata": {"processID": 123, "host": "example.test"}, "upload": 1, "download": 2}
        for i in range(5000)
    ]}
    collector.reset_connection_tracking()
    first = collector.collect_process_stats(connections_document=data)
    assert len(calls) == 1
    assert first[0].connections == 5000
    collector.collect_process_stats(connections_document=data)
    assert len(calls) == 2  # never cache a PID across independent snapshots
    collector.reset_connection_tracking()

def test_estats_access_denied_does_not_retry_for_thousands_of_sockets(monkeypatch):
    calls = []
    monkeypatch.setattr(native, "_iphlpapi", SimpleNamespace(
        SetPerTcpConnectionEStats=lambda *a: calls.append(1) or 5,
    ))
    monkeypatch.setattr(native, "_estats_retry_after", 0.0)
    monkeypatch.setattr(native, "_estats_enabled", set())
    for i in range(5000):
        row = native._MIB_TCPROW_OWNER_PID(dwLocalPort=i)
        assert not native._enable_estats(row)
    assert len(calls) == 1

def test_pid_access_failure_cannot_return_a_previous_process_name(monkeypatch):
    monkeypatch.setattr(native, "_pid_cache", {123: (1, "old.exe")})
    monkeypatch.setattr(native, "_kernel32", SimpleNamespace(OpenProcess=lambda *a: None))
    assert native._pid_to_exe(123) == ""
    assert 123 not in native._pid_cache

@pytest.mark.parametrize("cpu,wall,expected", [(0.01,0.02,0.98), (0.8,1.2,3.2), (2.0,3.0,8.0)])
def test_busy_telemetry_gets_a_real_cpu_budget(cpu, wall, expected):
    assert _metrics_idle_delay(1.0, wall, cpu) == pytest.approx(expected)

def test_network_wait_is_not_mistaken_for_cpu_load():
    assert _metrics_idle_delay(1.0, 5.0, 0.001) == pytest.approx(0.05)
