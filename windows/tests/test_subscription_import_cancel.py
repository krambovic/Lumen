from __future__ import annotations

import sys
import time
from types import SimpleNamespace

from PyQt6.QtCore import QCoreApplication, QObject, QThread, pyqtSignal

from xray_fluent import subscription_worker as subscription_worker_module
from xray_fluent.qml_app.bridge.app_bridge import AppBridge
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
