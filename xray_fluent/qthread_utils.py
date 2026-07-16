from __future__ import annotations

import logging
import time
from collections.abc import Callable
from typing import Any

from PyQt6.QtCore import QObject, QThread, Qt, pyqtSlot


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

    @pyqtSlot()
    def run(self) -> None:
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


def stop_and_wait_for_thread(
    worker: QThread | None,
    *,
    stop: Callable[[], Any] | None = None,
    label: str = "worker",
    logger: logging.Logger | None = None,
    timeout: float = 5.0,
) -> bool:
    """Cooperatively stop and join a QThread without unsafe termination."""
    if worker is None:
        return True
    if stop is not None:
        try:
            stop()
        except Exception:
            if logger is not None:
                logger.warning("[app] Failed to request %s shutdown", label, exc_info=True)
    if not worker.isRunning():
        return True

    started = time.monotonic()
    deadline = started + max(0.1, timeout)
    while not worker.wait(250):
        if time.monotonic() >= deadline:
            if logger is not None:
                logger.warning(
                    "[app] Timed out waiting for %s to stop after %.1fs; shutdown continues",
                    label,
                    time.monotonic() - started,
                )
            return False
    return True
