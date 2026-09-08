from __future__ import annotations
from copy import deepcopy
from types import SimpleNamespace as NS
from urllib.parse import urlsplit
import threading
import pytest
from PyQt6.QtCore import QObject, QThread
from xray_fluent.app_controller import AppController
from xray_fluent.application import runtime_services as runtime
from xray_fluent.application.health_listener import add_xray_health_listener
from xray_fluent.bounded_logs import LogRing
from xray_fluent.live_metrics_worker import LiveMetricsWorker
from xray_fluent.qml_app.bridge.app_bridge import AppBridge
from xray_fluent.qml_app.bridge.process_model import ProcessModel
from xray_fluent.secret_scrubber import StreamingSecretScrubber
from xray_fluent.models import AppSettings, AppState, Node
from xray_fluent import qthread_utils


def config(protocol="vless"):
    return {"outbounds": [{"tag": "proxy", "protocol": protocol}], "inbounds": [],
            "routing": {"rules": [{"type": "field", "domain": ["example.invalid"], "outboundTag": "direct"}]}}


def test_health_listener_is_authenticated_loopback_and_forces_selected_outbound():
    payload = config()
    original = deepcopy(payload["routing"]["rules"])
    url = urlsplit(add_xray_health_listener(payload, port=19086))
    assert url.scheme == "http" and url.hostname == "127.0.0.1" and url.port == 19086
    assert url.username == "lumen-health" and len(url.password) >= 40
    inbound = payload["inbounds"][-1]
    assert inbound["settings"]["accounts"] == [{"user": url.username, "pass": url.password}]
    assert inbound["listen"] == "127.0.0.1"
    assert payload["routing"]["rules"][0] == {"type": "field", "inboundTag": [inbound["tag"]], "outboundTag": "proxy"}
    assert payload["routing"]["rules"][1:] == original


@pytest.mark.parametrize("protocol", ["freedom", "blackhole", "future", "loopback"])
def test_health_listener_does_not_trust_tag_name(protocol):
    payload = config(protocol)
    before = deepcopy(payload)
    assert add_xray_health_listener(payload, port=19086) == ""
    assert payload == before


@pytest.mark.parametrize("port", [0, -1, 65536, True])
def test_invalid_health_port_does_not_mutate_config(port):
    payload = config()
    before = deepcopy(payload)
    assert add_xray_health_listener(payload, port=port) == ""
    assert payload == before


def test_duplicate_health_target_is_unknown():
    payload = config()
    payload["outbounds"].append({"tag": "proxy", "protocol": "freedom"})
    assert add_xray_health_listener(payload, port=19086) == ""


def test_runtime_wires_active_identity_not_ui_selection(monkeypatch):
    captured = {}
    class Worker:
        def __init__(self, *args, **kwargs):
            captured.update(kwargs)
            self.metrics = NS(connect=lambda callback: None)
        def start(self):
            captured["started"] = True
    monkeypatch.setattr(runtime, "LiveMetricsWorker", Worker)
    session = NS(node_id="active", ping_host="endpoint", ping_port=443, xray_inbound_tags=(),
                 clash_api_secret="test-secret", clash_api_selector="chosen-selector", health_proxy_url="")
    c = NS(_active_session=session, selected_node=NS(id="merely-highlighted"), _active_core="singbox",
           _metrics_worker=None, _log=lambda msg: None, _xray_api_port=0,
           state=NS(settings=NS(xray_path="unused")), get_effective_proxy_ports=lambda: (10808, 10809),
           _on_live_metrics=lambda payload: None)
    runtime.start_metrics_worker(c)
    assert captured["active_profile_id"] == "active"
    assert captured["active_outbound_tag"] == "chosen-selector"
    assert captured["health_proxy_url"] == "" and captured["started"]


def test_runtime_forwards_fresh_health_and_full_route_counters():
    checks, updates, saves = [], [], []
    c = NS(live_metrics_updated=NS(emit=lambda data: None),
           _check_auto_switch=lambda *args, **kwargs: checks.append((args, kwargs)),
           _traffic_history=NS(update_session=updates.append, save_periodic=lambda: saves.append(True)),
           _traffic_save_counter=14,
           _save_executor=NS(submit=lambda *_: pytest.fail("history must use its own writer")))
    payload = {"down_bps": 0, "up_bps": 20, "health_status": "HEALTHY", "health_checked_at": 10.0,
               "health_profile_id": "active", "process_stats": [NS(exe="app", upload=30, download=70,
                route="mixed", proxy_bytes=20, direct_bytes=50, unknown_bytes=30)]}
    runtime.on_live_metrics(c, payload)
    assert checks[0][1] == {"up_bps": 20.0, "health_status": "HEALTHY", "health_checked_at": 10.0, "health_profile_id": "active"}
    assert updates == [{"app": (30, 70, "mixed", 20, 50, 30)}] and saves == [True]


def test_process_model_keeps_unclassified_bytes_unknown():
    model = ProcessModel()
    model.set_stats([{"exe": "app", "upload": 100, "download": 200}])
    assert model.data(model.index(0, 0), model.RouteRole) == "unknown"
    assert model.data(model.index(0, 0), model.UnknownBytesRole) == 300
    assert model.data(model.index(0, 0), model.DirectBytesRole) == 0


def test_local_proxy_socket_does_not_prove_vpn_route(monkeypatch):
    import xray_fluent.live_metrics_worker as live
    process = NS(exe="app", bytes_in=100, bytes_out=200, connections=1)
    monkeypatch.setattr(live, "get_proxy_connections", lambda *args: [process])
    worker = LiveMetricsWorker("", 0)
    previous, totals = {}, {}
    try:
        worker._collect_proxy_process_stats(previous, totals)
        process.bytes_in += 50
        process.bytes_out += 30
        row = worker._collect_proxy_process_stats(previous, totals)[0]
        assert row.route == "unknown" and row.unknown_bytes == 80
        assert row.proxy_bytes == row.direct_bytes == 0
    finally:
        worker.stop()


def test_core_secret_markers_are_processed_before_queue_overflow():
    c = NS(_core_log_streams={"xray": StreamingSecretScrubber()}, _pending_core_logs=LogRing(maxlen=2))
    for line in ["-----BEGIN PRIVATE KEY-----", "private-body-one", "private-body-two", "private-body-three"]:
        AppController._queue_core_log(c, "xray", line)
    assert len(c._pending_core_logs) == 2
    assert all("private-body" not in line for _, line in c._pending_core_logs)


def test_ui_log_producer_does_not_call_qt_or_localize():
    c = NS(_pending_ui_logs=LogRing(maxlen=4),
           _localized_log_line=lambda line: pytest.fail("GUI work from producer"))
    thread = threading.Thread(target=lambda: [AppBridge._queue_ui_log(c, str(i)) for i in range(100)])
    thread.start()
    thread.join(2)
    assert not thread.is_alive() and list(c._pending_ui_logs) == ["96", "97", "98", "99"]
    assert c._pending_ui_logs.dropped == 96


def test_zero_shutdown_budget_retains_instead_of_waiting():
    waits = []
    worker = NS(wait=lambda ms: waits.append(ms) or False)
    class HashableWorker:
        def wait(self, ms):
            return worker.wait(ms)
    thread = HashableWorker()
    try:
        assert qthread_utils.stop_and_wait_for_thread(thread, timeout=0) is False
        assert waits == [0]
        assert thread in qthread_utils._late_shutdown_threads
    finally:
        qthread_utils._late_shutdown_threads.discard(thread)


def test_cleanup_callback_is_idempotent():
    owner, worker, calls = QObject(), QThread(), []
    cleanup = qthread_utils._ThreadCleanup(owner, worker, lambda: calls.append(True), delete_worker=False)
    cleanup.run()
    cleanup.run()
    assert calls == [True]


def test_history_writer_keeps_normal_quit_alive():
    c = NS(_save_futures_lock=threading.Lock(), _save_futures=set(),
           _background_threads_lock=threading.Lock(), _background_threads=set(),
           _transition_worker_thread=None, _traffic_history=NS(writer_running=True))
    assert AppController.has_pending_shutdown_work(c)
    c._traffic_history.writer_running = False
    assert not AppController.has_pending_shutdown_work(c)


def test_stale_startup_disable_cannot_overwrite_new_user_choice():
    events = []
    c = NS(_startup_settings_generation=2, _shutting_down=False, profile_loaded=True,
           state=NS(settings=NS(launch_on_startup=True)), settings_changed=NS(emit=events.append),
           schedule_save=lambda: events.append("saved"))
    AppController._accept_external_startup_disable(c, 1)
    assert c.state.settings.launch_on_startup and events == []
    AppController._accept_external_startup_disable(c, 2)
    assert not c.state.settings.launch_on_startup and events[-1] == "saved"


def test_updating_startup_preferences_invalidates_reconciliation(monkeypatch):
    import xray_fluent.app_controller as module
    monkeypatch.setattr(module, "set_startup_enabled", lambda *args: None)
    monkeypatch.setattr(module, "build_startup_command", lambda **kwargs: "test-command")
    original = AppSettings(launch_on_startup=False)
    changed = deepcopy(original)
    changed.launch_on_startup = True
    c = NS(profile_loaded=True, state=NS(settings=original), _startup_settings_generation=3,
           settings_changed=NS(emit=lambda value: None), schedule_save=lambda: None,
           connected=False, _desired_connected=False, _queue_windows_preferences=lambda **kwargs: None)
    AppController.update_settings(c, changed)
    assert c._startup_settings_generation == 4


def test_diagnostic_export_is_queued_and_uses_owned_snapshot(monkeypatch, tmp_path):
    import xray_fluent.diagnostics_uploader as uploader
    monkeypatch.setattr(uploader, "get_upload_epoch", lambda: 7)
    jobs, built, signals = [], [], []
    controller = NS(state=AppState(nodes=[Node(name="before")]), recent_logs=LogRing(iterable=["before-log"]),
                    _start_background_task=lambda target, name: jobs.append(target),
                    build_diagnostics=lambda **kwargs: built.append(kwargs) or tmp_path / "diagnostics.zip")
    bridge = NS(_diagnostics_exporting=False, _quitting=False, controller=controller,
                _diagnostics_finished=NS(emit=lambda *args: signals.append(args)))
    AppBridge.exportDiagnostics(bridge)
    assert not built and len(jobs) == 1
    controller.state.nodes[0].name = "after"
    controller.recent_logs.append("after-log")
    jobs[0]()
    assert built[0]["state"].nodes[0].name == "before"
    assert built[0]["logs"] == ("before-log",)
    assert built[0]["epoch"] == 7 and built[0]["upload"] is False
    assert signals[0][1] == ""
