from __future__ import annotations

import sys
import threading
import time

from PyQt6.QtCore import QCoreApplication, QObject, QThread, pyqtSignal, pyqtSlot

from xray_fluent.qthread_utils import bind_thread_reference, stop_and_wait_for_thread


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


class _CompletingWorker(QThread):
    completed = pyqtSignal()

    def run(self) -> None:
        self.completed.emit()
        self.msleep(60)


class _Owner(QObject):
    def __init__(self) -> None:
        super().__init__()
        self.worker = None
        self.reference_seen_on_completed = None

    @pyqtSlot()
    def on_completed(self) -> None:
        self.reference_seen_on_completed = self.worker


def test_worker_reference_is_kept_until_qthread_finished() -> None:
    app = _app()
    owner = _Owner()
    worker = _CompletingWorker()
    owner.worker = worker
    bind_thread_reference(owner, "worker", worker)
    worker.completed.connect(owner.on_completed)

    worker.start()

    assert _spin_until(lambda: owner.reference_seen_on_completed is not None)
    assert owner.reference_seen_on_completed is worker
    assert owner.worker is worker
    assert worker.wait(1000)
    assert _spin_until(lambda: owner.worker is None)
    assert app is QCoreApplication.instance()


class _CancellableWorker(QThread):
    def __init__(self) -> None:
        super().__init__()
        self._stop = threading.Event()

    def cancel(self) -> None:
        self._stop.set()

    def run(self) -> None:
        while not self._stop.wait(0.01):
            pass


def test_cooperative_thread_shutdown_does_not_use_terminate() -> None:
    worker = _CancellableWorker()
    worker.start()

    stop_and_wait_for_thread(worker, stop=worker.cancel, label="test worker")

    assert not worker.isRunning()
