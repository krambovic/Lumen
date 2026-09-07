from __future__ import annotations

import json
import threading
from pathlib import Path

import pytest

from xray_fluent import traffic_history as history


@pytest.fixture
def store(monkeypatch, tmp_path):
    monkeypatch.setattr(history, "TRAFFIC_HISTORY_FILE", tmp_path / "history.json")
    storage = history.TrafficHistoryStorage()
    yield storage
    storage.close(3)


def read(store):
    return json.loads(store._path.read_text(encoding="utf-8"))


def test_atomic_temp_names_are_unique_and_flush_is_explicit(store, monkeypatch):
    replaced = []
    original = history.os.replace

    def record(src, dst):
        replaced.append((Path(src).name, Path(dst).name))
        return original(src, dst)

    monkeypatch.setattr(history.os, "replace", record)
    store.start_session("one", "tun")
    assert store.flush(3)
    store.update_session({"app.exe": (10, 20, "proxy")})
    assert store.flush(3)
    names = [name for name, _ in replaced]
    assert len(names) >= 2 and len(names) == len(set(names))
    assert all(name.startswith(".history.json.") and name.endswith(".tmp") for name in names)
    assert all(destination == "history.json" for _, destination in replaced)
    assert not list(store._path.parent.glob(".*.tmp"))
    assert read(store)["sessions"][0]["total_download"] == 20


def block_first_write(store, monkeypatch):
    entered, release = threading.Event(), threading.Event()
    snapshots, generations = [], []
    original = store._writer._write

    def blocked(generation, snapshot):
        snapshots.append(snapshot)
        generations.append(generation)
        if len(snapshots) == 1:
            entered.set()
            assert release.wait(5), "test did not release writer"
        return original(generation, snapshot)

    monkeypatch.setattr(store._writer, "_write", blocked)
    return entered, release, snapshots, generations


def test_snapshot_immutable_and_mutation_does_not_wait_for_disk(store, monkeypatch):
    entered, release, snapshots, _ = block_first_write(store, monkeypatch)
    store.start_session("original", "tun")
    assert entered.wait(3)
    completed = threading.Event()

    def mutate():
        store.update_session({"app.exe": (100, 200, "unknown")})
        completed.set()

    thread = threading.Thread(target=mutate)
    thread.start()
    try:
        assert completed.wait(2), "disk I/O holds the storage lock"
        assert snapshots[0]["sessions"][0]["total_upload"] == 0
        with pytest.raises(TypeError):
            snapshots[0]["sessions"][0]["node_name"] = "mutated"
    finally:
        release.set()
        thread.join(3)
    assert store.flush(3)
    assert read(store)["sessions"][0]["total_upload"] == 100


def test_clear_supersedes_inflight_and_coalesced_periodic_saves(store, monkeypatch):
    entered, release, snapshots, generations = block_first_write(store, monkeypatch)
    store.start_session("old", "proxy")
    assert entered.wait(3)
    try:
        for value in range(10):
            store.update_session({"app.exe": (value, value, "unknown")})
            store.save_periodic()
        store.clear()
        store.end_session()
        store.save_periodic()
        assert store.current_session is None
        assert store.flush(0) is False
    finally:
        release.set()
    assert store.flush(3)
    assert read(store) == {"sessions": [], "daily_totals": {}}
    assert generations == sorted(set(generations))
    assert len(snapshots) < 12  # pending generations are coalesced, not a backlog


def test_end_then_start_order_cannot_be_overwritten_by_stale_save(store, monkeypatch):
    entered, release, _, _ = block_first_write(store, monkeypatch)
    store.start_session("old", "tun")
    assert entered.wait(3)
    try:
        store.update_session({"app.exe": (10, 20, "direct")})
        store.save_periodic()
        store.end_session()
        store.start_session("new", "tun")
        store.update_session({"app.exe": (1, 2, "unknown")})
    finally:
        release.set()
    assert store.flush(3)
    sessions = read(store)["sessions"]
    assert [row["node_name"] for row in sessions] == ["old", "new"]
    assert sessions[0]["ended_at"] is not None and sessions[1]["ended_at"] is None
    daily = next(iter(read(store)["daily_totals"].values()))
    assert daily == {"upload": 11, "download": 22}


def test_close_has_bounded_wait_and_finishes_its_final_generation(store, monkeypatch):
    entered, release, _, _ = block_first_write(store, monkeypatch)
    store.start_session("node", "tun")
    assert entered.wait(3)
    try:
        store.update_session({"app.exe": (4, 5, "unknown")})
        assert store.close(0) is False
        with pytest.raises(RuntimeError):
            store.start_session("resurrect", "tun")
    finally:
        release.set()
    assert store.close(3) is True
    assert not store._writer._thread.is_alive()
    assert read(store)["sessions"][0]["ended_at"] is not None
    assert read(store)["sessions"][0]["total_upload"] == 4


def test_getters_never_expose_mutable_internal_sessions_or_daily_rows(store):
    store.start_session("node", "tun")
    store.update_session({"app.exe": (10, 20, "proxy")})
    current = store.current_session
    current.node_name = "changed"
    current.processes["app.exe"].upload = 900
    store.get_sessions()[0].processes.clear()
    daily = store.get_daily_totals()
    next(iter(daily.values()))["upload"] = 1234
    assert store.current_session.node_name == "node"
    assert store.current_session.processes["app.exe"].upload == 10
    assert next(iter(store.get_daily_totals().values()))["upload"] == 10


def test_unknown_route_bytes_survive_roundtrip_and_aggregation(store):
    store.start_session("node", "tun")
    store.update_session({"app.exe": (100, 200, "unknown", 80, 70, 150)})
    store.end_session()
    assert store.flush(3)
    row = read(store)["sessions"][0]["processes"]["app.exe"]
    assert (row["proxy_bytes"], row["direct_bytes"], row["unknown_bytes"]) == (80, 70, 150)
    restored = history.TrafficHistoryStorage()
    try:
        assert restored.get_process_totals()["app.exe"]["unknown_bytes"] == 150
        assert restored.get_process_totals()["app.exe"]["route"] == "unknown"
    finally:
        restored.close(3)


def test_legacy_mixed_rows_do_not_invent_direct_proxy_split(store):
    store.start_session("old", "tun")
    store.update_session({"app.exe": (10, 20, "mixed")})
    row = store.current_session.processes["app.exe"]
    assert row.unknown_bytes == 30 and row.proxy_bytes == row.direct_bytes == 0
    assert history.ProcessTrafficEntry.from_dict({"upload": 1, "download": 2, "route": "mixed"}).unknown_bytes == 3


def test_write_failure_is_reported_and_a_later_flush_can_recover(store, monkeypatch):
    original = store._writer._write

    def fail(*args):
        raise OSError("injected, no user data")

    monkeypatch.setattr(store._writer, "_write", fail)
    store.start_session("node", "proxy")
    assert store.flush(3) is False
    assert "OSError" in store.last_save_error
    monkeypatch.setattr(store._writer, "_write", original)
    assert store.flush(3) is True
    assert store.last_save_error == ""


def test_corrupt_history_is_quarantined_and_new_history_flushed(monkeypatch, tmp_path):
    target = tmp_path / "corrupt.json"
    target.write_text('{"sessions": [{', encoding="utf-8")
    monkeypatch.setattr(history, "TRAFFIC_HISTORY_FILE", target)
    storage = history.TrafficHistoryStorage()
    try:
        storage.start_session("new", "proxy")
        assert storage.flush(3)
        backups = list(tmp_path.glob("corrupt.json.corrupt-*"))
        assert len(backups) == 1 and backups[0].read_text() == '{"sessions": [{'
        assert read(storage)["sessions"][0]["node_name"] == "new"
    finally:
        storage.close(3)
