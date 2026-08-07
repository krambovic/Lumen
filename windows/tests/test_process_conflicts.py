from __future__ import annotations

import os

import xray_fluent.process_conflicts as process_conflicts
from xray_fluent.process_conflicts import (
    _local_proxy_ports,
    _running_processes_win32,
    find_conflicting_network_apps,
    scan_network_conflicts,
)


def test_known_clients_are_reported_by_product_name() -> None:
    processes = {
        10: "v2rayN.exe",
        11: "Happ.exe",
        12: "xray.exe",
    }
    assert find_conflicting_network_apps(processes) == ["Happ", "v2rayN"]


def test_local_ports_are_extracted_from_windows_proxy_value() -> None:
    value = "http=127.0.0.1:10809;https=localhost:10810;socks=10.0.0.1:10808"
    assert _local_proxy_ports(value) == {10809, 10810}


def test_scan_reports_foreign_cores_but_ignores_current_lumen_core(monkeypatch) -> None:
    monkeypatch.setattr(
        process_conflicts,
        "_running_processes",
        lambda: {101: "xray.exe", 202: "sing-box.exe", 303: "v2rayN.exe"},
    )
    monkeypatch.setattr(process_conflicts, "find_listening_port_conflicts", lambda *_args, **_kwargs: [])
    monkeypatch.setattr(process_conflicts, "has_foreign_system_proxy", lambda **_kwargs: False)

    snapshot = scan_network_conflicts({10808, 10809}, ignored_pids={101})

    assert snapshot["apps"] == ["sing-box", "v2rayN"]
    assert {item["pid"] for item in snapshot["processes"]} == {202, 303}


def test_native_windows_process_snapshot_contains_current_process() -> None:
    if os.name != "nt":
        return
    processes = _running_processes_win32()
    assert processes is not None
    assert os.getpid() in processes
