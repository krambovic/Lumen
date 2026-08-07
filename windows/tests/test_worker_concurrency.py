from __future__ import annotations

import threading
import time

from xray_fluent import ping_worker, process_traffic_collector
from xray_fluent.models import Node
from xray_fluent.ping_worker import _WindowsPingBypass
from xray_fluent.speed_test_worker import SpeedTestWorker


def _bypass(monkeypatch, added: list[str], deleted: list[str]) -> _WindowsPingBypass:
    bypass = _WindowsPingBypass([], True)
    bypass._gateway = "192.168.0.1"
    monkeypatch.setattr(bypass, "_route_add", lambda ip: added.append(ip) or True)
    monkeypatch.setattr(bypass, "_route_delete", lambda ip: deleted.append(ip))
    return bypass


def test_concurrent_bypasses_do_not_delete_each_others_routes(monkeypatch) -> None:
    ping_worker._ROUTE_REFS.clear()
    added: list[str] = []
    deleted: list[str] = []
    first = _bypass(monkeypatch, added, deleted)
    second = _bypass(monkeypatch, added, deleted)

    assert first._acquire_route("8.8.8.8") is True
    assert second._acquire_route("8.8.8.8") is True
    assert added == ["8.8.8.8"]

    first._release_route("8.8.8.8")
    assert deleted == []

    second._release_route("8.8.8.8")
    assert deleted == ["8.8.8.8"]
    assert ping_worker._ROUTE_REFS == {}


def test_failed_route_add_is_not_refcounted(monkeypatch) -> None:
    ping_worker._ROUTE_REFS.clear()
    deleted: list[str] = []
    bypass = _WindowsPingBypass([], True)
    bypass._gateway = "192.168.0.1"
    monkeypatch.setattr(bypass, "_route_add", lambda _ip: False)
    monkeypatch.setattr(bypass, "_route_delete", lambda ip: deleted.append(ip))

    assert bypass._acquire_route("1.1.1.1") is False
    assert ping_worker._ROUTE_REFS == {}


def _patch_clash_connections(monkeypatch, payload: bytes) -> None:
    class _Response:
        def __enter__(self):
            return self

        def __exit__(self, *_exc):
            return False

        @staticmethod
        def read() -> bytes:
            return payload

    monkeypatch.setattr(
        process_traffic_collector.urllib.request,
        "urlopen",
        lambda *_args, **_kwargs: _Response(),
    )


def test_collector_holds_the_lock_while_mutating_shared_state(monkeypatch) -> None:
    process_traffic_collector.reset_connection_tracking()
    _patch_clash_connections(
        monkeypatch,
        b'{"connections": [{"id": "c1", "upload": 1, "download": 2, "metadata": {"processName": "app.exe"}}]}',
    )
    original = process_traffic_collector._process_name_from_metadata
    locked: list[bool] = []

    def _probe(meta):
        free = process_traffic_collector._lock.acquire(blocking=False)
        if free:
            process_traffic_collector._lock.release()
        locked.append(not free)
        return original(meta)

    monkeypatch.setattr(process_traffic_collector, "_process_name_from_metadata", _probe)

    process_traffic_collector.collect_process_stats(clash_api_secret="secret")

    assert locked == [True]


def test_reset_cannot_clear_the_maps_while_the_collector_owns_them() -> None:
    process_traffic_collector.reset_connection_tracking()
    process_traffic_collector._conn_bytes["c1"] = (1, 1)
    done = threading.Event()

    def _reset() -> None:
        process_traffic_collector.reset_connection_tracking()
        done.set()

    resetter = threading.Thread(target=_reset, name="reset")
    with process_traffic_collector._lock:
        resetter.start()
        assert done.wait(0.2) is False
        assert process_traffic_collector._conn_bytes == {"c1": (1, 1)}
    resetter.join(5.0)

    assert done.is_set()
    assert process_traffic_collector._conn_bytes == {}


def _worker() -> SpeedTestWorker:
    return SpeedTestWorker([], "xray.exe")


class _StuckProcess:
    def __init__(self) -> None:
        self.terminated = False
        self.killed = False

    def poll(self):
        return None

    def terminate(self) -> None:
        self.terminated = True

    def kill(self) -> None:
        self.killed = True


def test_cancel_does_not_wait_for_the_temporary_cores() -> None:
    worker = _worker()
    proc = _StuckProcess()
    worker._register_process(proc)

    started = time.monotonic()
    worker.cancel()
    elapsed = time.monotonic() - started

    assert worker.was_cancelled is True
    assert proc.terminated is True
    assert proc.killed is False
    assert elapsed < 0.2


def test_run_finally_still_escalates_to_kill() -> None:
    worker = _worker()
    proc = _StuckProcess()
    worker._register_process(proc)

    worker._terminate_all_processes()

    assert proc.killed is True


def test_port_reservation_failure_still_yields_a_node_result(monkeypatch) -> None:
    worker = _worker()
    node = Node(name="n", scheme="vless", server="one.example", port=443)

    def _fail() -> tuple[int, object]:
        raise OSError("no ephemeral ports")

    monkeypatch.setattr(worker, "_reserve_port", _fail)

    assert worker._test_node(node) == (node, None, False)
