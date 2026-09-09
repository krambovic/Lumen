from __future__ import annotations

import sys
import time
from types import SimpleNamespace

from PyQt6.QtCore import QCoreApplication, QObject, QThread, pyqtSignal

from xray_fluent import subscription_worker as subscription_worker_module
from xray_fluent.qml_app.bridge.app_bridge import (
    AppBridge,
    _subscription_snapshot_matches_local_nodes,
)
from xray_fluent.subscription_worker import SubscriptionFetchWorker


class _Signal:
    def __init__(self) -> None:
        self.count = 0
        self.args: list[tuple] = []

    def emit(self, *args) -> None:
        self.count += 1
        self.args.append(args)


class _Worker:
    def __init__(self) -> None:
        self.stopped = False

    def stop(self) -> None:
        self.stopped = True


class _Controller:
    def __init__(self) -> None:
        self.applied = []

    def apply_fetched_subscription(self, *args):
        self.applied.append(args)
        return 1, []


class _AutoUpdateController:
    profile_loaded = True

    def __init__(self) -> None:
        self.state = SimpleNamespace(
            subscriptions=[
                {
                    "id": "sub-1",
                    "name": "Work VPN",
                    "group": "Work VPN",
                    "url": "https://sub.example/list",
                }
            ],
            nodes=[
                SimpleNamespace(id="keep", subscription_id="sub-1", group="Work VPN"),
                SimpleNamespace(id="removed", subscription_id="sub-1", group="Work VPN"),
            ],
        )

    def apply_fetched_subscription(self, *args):
        self.state.nodes = [
            SimpleNamespace(id="keep", subscription_id="sub-1", group="Work VPN"),
            SimpleNamespace(id="new-1", subscription_id="sub-1", group="Work VPN"),
            SimpleNamespace(id="new-2", subscription_id="sub-1", group="Work VPN"),
        ]
        return 3, []


class _UnchangedUpdateController(_AutoUpdateController):
    def apply_fetched_subscription(self, *args):
        return len(self.state.nodes), []


class _FailedUpdateController(_AutoUpdateController):
    def apply_fetched_subscription(self, *args):
        return 0, ["Subscription server unavailable: HTTP 500"]


def _app() -> QCoreApplication:
    return QCoreApplication.instance() or QCoreApplication(sys.argv)


def _spin_until(predicate, timeout: float = 2.0) -> bool:
    app = _app()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        app.processEvents()
        if predicate():
            return True
        time.sleep(0.005)
    return bool(predicate())


class _CancelHarness(QObject):
    _sub_fetch_run = pyqtSignal(object, int)

    def __init__(self, thread: QThread, worker: SubscriptionFetchWorker) -> None:
        super().__init__()
        self._sub_importing = True
        self._sub_import_status = "Downloading..."
        self._sub_batches = {1: {"kind": "import", "added": 0, "errors": []}}
        self._sub_thread = thread
        self._sub_worker = worker
        self._retired_sub_threads = []
        self._retired_sub_workers = {}
        self.subscriptionImportingChanged = _Signal()
        self.subscriptionImportStatusChanged = _Signal()
        self.toast = _Signal()

    def _retire_sub_worker(self, thread: QThread, worker: SubscriptionFetchWorker | None) -> None:
        AppBridge._retire_sub_worker(self, thread, worker)


def test_cancel_subscription_import_stops_worker_and_clears_state() -> None:
    worker = _Worker()
    bridge = SimpleNamespace(
        _sub_importing=True,
        _sub_import_status="Downloading...",
        _sub_batches={1: {"kind": "import", "added": 0, "errors": []}},
        _sub_worker=worker,
        _sub_thread=None,
        subscriptionImportingChanged=_Signal(),
        subscriptionImportStatusChanged=_Signal(),
        toast=_Signal(),
    )

    AppBridge.cancelSubscriptionImport(bridge)

    assert bridge._sub_importing is False
    assert bridge._sub_import_status == ""
    assert bridge._sub_batches == {}
    assert worker.stopped is True
    assert bridge.subscriptionImportingChanged.count == 1
    assert bridge.subscriptionImportStatusChanged.count == 1
    assert bridge.toast.args


def test_cancelled_subscription_import_result_is_ignored() -> None:
    controller = _Controller()
    bridge = SimpleNamespace(
        controller=controller,
        _sub_batches={},
    )
    job = SimpleNamespace(url="https://sub.example", name="Sub", kind="import")

    AppBridge._on_sub_fetched(bridge, 7, job, "vless://...", {}, [])

    assert controller.applied == []


def test_auto_update_toast_names_subscriptions_with_real_add_remove_counts() -> None:
    controller = _AutoUpdateController()
    toast = _Signal()
    bridge = SimpleNamespace(
        controller=controller,
        _quitting=False,
        _sub_batches={
            4: {"kind": "auto", "added": 0, "errors": [], "changes": []}
        },
        toast=toast,
    )
    job = SimpleNamespace(
        url="https://sub.example/list",
        name="",
        kind="update",
    )

    AppBridge._on_sub_fetched(bridge, 4, job, "payload", {}, [])
    AppBridge._on_sub_batch_completed(bridge, 4, 1)

    assert toast.count == 1
    message = toast.args[0][1]
    assert "Work VPN" in message
    assert "+2" in message
    assert "-1" in message


def test_manual_update_does_not_report_the_unchanged_server_total() -> None:
    controller = _UnchangedUpdateController()
    toast = _Signal()
    bridge = SimpleNamespace(
        controller=controller,
        _quitting=False,
        _sub_batches={
            5: {"kind": "update_all", "added": 0, "errors": [], "changes": []}
        },
        toast=toast,
    )
    job = SimpleNamespace(
        url="https://sub.example/list",
        name="",
        kind="update",
    )

    AppBridge._on_sub_fetched(bridge, 5, job, "payload", {}, [])
    AppBridge._on_sub_batch_completed(bridge, 5, 1)

    assert toast.count == 1
    assert "44" not in toast.args[0][1]
    assert "измен" in toast.args[0][1].lower() or "change" in toast.args[0][1].lower()


def test_failed_update_reports_only_the_named_subscription_error() -> None:
    controller = _FailedUpdateController()
    toast = _Signal()
    bridge = SimpleNamespace(
        controller=controller,
        _quitting=False,
        _sub_batches={
            6: {"kind": "update", "added": 0, "errors": [], "changes": []}
        },
        toast=toast,
    )
    job = SimpleNamespace(
        url="https://sub.example/list",
        name="",
        kind="update",
    )

    AppBridge._on_sub_fetched(bridge, 6, job, "", {}, [])
    AppBridge._on_sub_batch_completed(bridge, 6, 1)

    assert toast.count == 1
    level, message = toast.args[0]
    assert level == "warning"
    assert "Work VPN" in message
    assert "500" in message


def test_update_all_lists_changed_subscription_names_separated_by_commas() -> None:
    toast = _Signal()
    bridge = SimpleNamespace(
        _sub_batches={
            7: {
                "kind": "update_all",
                "added": 99,
                "errors": [],
                "changes": [
                    {"name": "Work VPN", "added": 2, "removed": 1},
                    {"name": "Home VPN", "added": 1, "removed": 0},
                ],
            }
        },
        toast=toast,
    )

    AppBridge._on_sub_batch_completed(bridge, 7, 2)

    assert toast.count == 1
    message = toast.args[0][1]
    assert "Work VPN (+2 / -1), Home VPN (+1)" in message
    assert "99" not in message


def test_local_subscription_deletion_invalidates_cached_snapshot() -> None:
    subscription = {"id": "sub-1", "node_count": 2}
    nodes = [SimpleNamespace(subscription_id="sub-1")]

    assert not _subscription_snapshot_matches_local_nodes(subscription, nodes)
    assert _subscription_snapshot_matches_local_nodes(
        subscription,
        [SimpleNamespace(subscription_id="sub-1"), SimpleNamespace(subscription_id="sub-1")],
    )


def test_worker_stop_aborts_active_http_response(monkeypatch) -> None:
    worker = SubscriptionFetchWorker()
    response = object()
    aborted = []
    monkeypatch.setattr(subscription_worker_module, "abort_http_response", aborted.append)
    worker._register_response(response)

    worker.stop()

    assert aborted == [response]


def test_cancel_live_subscription_thread_retires_it_until_finished(monkeypatch) -> None:
    _app()
    thread = QThread()
    worker = SubscriptionFetchWorker()
    worker.moveToThread(thread)
    bridge = _CancelHarness(thread, worker)
    bridge._sub_fetch_run.connect(worker.run_batch)
    thread.finished.connect(worker.deleteLater)
    thread.start()
    assert thread.isRunning()

    def fail_if_waited(*args, **kwargs):
        raise AssertionError("cancellation must not block the GUI thread")

    monkeypatch.setattr(
        "xray_fluent.qml_app.bridge.app_bridge.stop_and_wait_for_thread",
        fail_if_waited,
    )

    AppBridge.cancelSubscriptionImport(bridge)

    assert bridge._sub_thread is None
    assert bridge._sub_worker is None
    assert bridge._retired_sub_workers.get(thread) is worker
    assert thread in bridge._retired_sub_threads
    assert thread.wait(1000)
    assert _spin_until(lambda: thread not in bridge._retired_sub_threads)
    assert thread not in bridge._retired_sub_workers
