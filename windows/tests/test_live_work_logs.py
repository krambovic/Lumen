"""Live log regression tests; no external core is launched or stopped."""
from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application.runtime_services import on_live_metrics
from xray_fluent.bounded_logs import LogRing
from xray_fluent.live_metrics_worker import LiveMetricsWorker
from xray_fluent.log_utils import parse_log_line
from xray_fluent.qml_app.bridge.app_bridge import AppBridge
from xray_fluent.qml_app.bridge.log_model import LogFilterModel, LogModel


def test_clear_keeps_model_live_and_discards_only_old_ui_lines() -> None:
    source = LogModel()
    filtered = LogFilterModel(source)
    pending = LogRing[str](maxlen=20, iterable=["[app] stale pending line"])
    source.append_line("[app] old line")
    emitted: list[str] = []
    def log_line(line: str) -> None:
        emitted.append(line)
        pending.append(line)
    bridge = SimpleNamespace(
        _pending_ui_logs=pending,
        _log_source_model=source,
        controller=SimpleNamespace(_log=log_line),
        _flush_ui_logs=lambda: source.append_lines(pending.drain()),
    )

    AppBridge.clearLogs(bridge)
    assert source.rowCount() == 1
    assert len(pending) == 0
    assert len(emitted) == 1
    source.append_line("[singbox] from 127.0.0.1:1234 accepted tcp:site.example:443 [proxy]")
    assert filtered.rowCount() == 2


def test_session_counters_update_ui_without_log_entries() -> None:
    entries: list[str] = []
    updates: list[dict] = []
    controller = SimpleNamespace(
        _shutting_down=False,
        _log=entries.append,
        live_metrics_updated=SimpleNamespace(emit=updates.append),
        _check_auto_switch=lambda *_args, **_kwargs: None,
    )
    sample = {"traffic_available": True, "upload_total": 1234, "download_total": 5678}

    on_live_metrics(controller, sample)

    assert updates == [sample]
    assert entries == []


def test_singbox_work_logs_use_existing_connection_snapshot_and_are_bounded() -> None:
    worker = LiveMetricsWorker("", 0, mode="singbox")
    worker._outbound_graph = {"proxies": {"proxy": {"type": "VLESS"}}}
    connections = [
        {
            "id": f"conn-{index}",
            "metadata": {
                "host": f"site-{index}.example",
                "destinationPort": 443,
                "sourceIP": "127.0.0.1",
                "sourcePort": 60000 + index,
                "network": "tcp",
                "processPath": "C:/private/process.exe",
            },
            "chains": ["proxy"],
        }
        for index in range(10)
    ]
    document = {"connections": connections}
    events = worker._new_connection_events(document)
    assert len(events) == 7  # six real connections + one bounded summary
    assert "from 127.0.0.1:60000 accepted tcp:site-0.example:443 [proxy]" in events[0]
    entry = parse_log_line(events[0])
    assert entry.source == "sing-box"
    assert entry.level == "info"
    assert "accepted tcp:site-0.example:443" in entry.message
    assert all("process.exe" not in event for event in events)
    assert "4 additional" in events[-1]
    assert worker._new_connection_events(document) == ()
    assert worker._new_connection_events({"connections": []}) == ()
    assert len(worker._new_connection_events(document)) == 7
