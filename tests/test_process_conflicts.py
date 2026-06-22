from __future__ import annotations

import os

from xray_fluent.process_conflicts import (
    _local_proxy_ports,
    _running_processes_win32,
    find_conflicting_network_apps,
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


def test_native_windows_process_snapshot_contains_current_process() -> None:
    if os.name != "nt":
        return
    processes = _running_processes_win32()
    assert processes is not None
    assert os.getpid() in processes
