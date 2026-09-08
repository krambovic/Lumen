from __future__ import annotations

import logging
import time
from collections.abc import Callable
from typing import Any

from PyQt6.QtCore import QObject, QThread, QTimer, Qt, pyqtSlot


class _ThreadCleanup(QObject):
    def __init__(
        self,
        owner: QObject,
        worker: QThread,
        callback: Callable[[], Any],
        *,
        delete_worker: bool,
    ) -> None:
        super().__init__(owner)
        self._owner = owner
        self._worker = worker
        self._callback = callback
        self._delete_worker = delete_worker
        self._done = False

    @pyqtSlot()
    def run(self) -> None:
        if self._done:
            return
        # finished can precede thread-local teardown; never destroy too early.
        try:
            pending = self._worker.isRunning() or not self._worker.wait(0)
        except RuntimeError:
            pending = False
            self._delete_worker = False
        if pending:
            QTimer.singleShot(10, self.run)
            return
        self._done = True
        try:
            self._callback()
        finally:
            if self._delete_worker:
                self._worker.deleteLater()
            guards = getattr(self._owner, "_qthread_cleanup_guards", None)
            if isinstance(guards, list):
                try:
                    guards.remove(self)
                except ValueError:
                    pass
            self.deleteLater()


def _connect_cleanup(
    owner: QObject,
    worker: QThread,
    callback: Callable[[], Any],
    *,
    delete_worker: bool,
) -> None:
    guards = getattr(owner, "_qthread_cleanup_guards", None)
    if not isinstance(guards, list):
        guards = []
        setattr(owner, "_qthread_cleanup_guards", guards)
    guard = _ThreadCleanup(owner, worker, callback, delete_worker=delete_worker)
    guards.append(guard)
    worker.finished.connect(guard.run, Qt.ConnectionType.QueuedConnection)
    if worker.isFinished():
        QTimer.singleShot(0, guard.run)


def bind_thread_reference(owner: QObject, attribute: str, worker: QThread) -> None:
    """Keep a QThread referenced until Qt reports that it has fully stopped."""

    def _release() -> None:
        if getattr(owner, attribute, None) is worker:
            setattr(owner, attribute, None)

    _connect_cleanup(owner, worker, _release, delete_worker=True)


def retain_thread_until_finished(
    owner: QObject,
    workers: list[QThread],
    worker: QThread,
    *,
    delete_worker: bool = True,
    on_finished: Callable[[], Any] | None = None,
) -> None:
    """Retain a superseded worker without keeping it after completion."""
    if worker in workers:
        return
    workers.append(worker)

    def _release() -> None:
        try:
            workers.remove(worker)
        except ValueError:
            pass
        if on_finished is not None:
            on_finished()

    _connect_cleanup(owner, worker, _release, delete_worker=delete_worker)


_late_shutdown_threads: set = set()


def is_thread_pending(worker) -> bool:
    try:
        return worker.isRunning() or not worker.wait(0)
    except RuntimeError:
        return False


def has_pending_shutdown_threads() -> bool:
    for worker in list(_late_shutdown_threads):
        try:
            finished = not worker.isRunning() and worker.wait(0)
        except RuntimeError:
            finished = True
        if finished:
            _late_shutdown_threads.discard(worker)
    return bool(_late_shutdown_threads)


def stop_and_wait_for_thread(
    worker: QThread | None,
    *,
    stop: Callable[[], Any] | None = None,
    label: str = "worker",
    logger: logging.Logger | None = None,
    timeout: float = 5.0,
    deadline: float | None = None,
) -> bool:
    # A false result transfers lifetime responsibility, never a deletion permit.
    if worker is None:
        return True
    end = deadline if deadline is not None else time.monotonic() + max(0.0, timeout)
    if stop is not None:
        try:
            stop()
        except Exception:
            if logger is not None:
                logger.warning("[app] Failed to cancel %s", label, exc_info=True)
    if isinstance(worker, QThread) and worker == QThread.currentThread():
        _late_shutdown_threads.add(worker)
        return False
    while True:
        try:
            finished = worker.wait(0)
        except RuntimeError:
            finished = True
        if finished:
            _late_shutdown_threads.discard(worker)
            return True
        remaining = end - time.monotonic()
        if remaining <= 0:
            _late_shutdown_threads.add(worker)
            # Parent destruction must not destroy a still-running QThread.
            if isinstance(worker, QThread) and worker.thread() == QThread.currentThread():
                worker.setParent(None)
            if logger is not None:
                logger.warning("[app] %s still finishing; retained for asynchronous shutdown", label)
            return False
        try:
            finished = worker.wait(max(1, min(250, int(remaining * 1000))))
        except RuntimeError:
            finished = True
        if finished:
            _late_shutdown_threads.discard(worker)
            return True
