from __future__ import annotations

import threading
import time

from PyQt6.QtCore import QCoreApplication, QTimer

from xray_fluent.app_controller import AppController


def test_transition_action_does_not_block_gui_event_loop() -> None:
    app = QCoreApplication.instance() or QCoreApplication([])
    controller = AppController()
    started = threading.Event()
    release = threading.Event()
    worker_thread_ids: list[int] = []

    def slow_transition(_action: str, _reason: str) -> bool:
        worker_thread_ids.append(threading.get_ident())
        started.set()
        return release.wait(2.0)

    controller._run_transition_action = slow_transition
    controller._transition_active = True
    gui_thread_id = threading.get_ident()

    try:
        before = time.monotonic()
        controller._execute_transition_action("connect", "test", 1)
        elapsed = time.monotonic() - before

        assert elapsed < 0.1
        assert started.wait(0.5)
        assert len(worker_thread_ids) == 1
        assert worker_thread_ids[0] != gui_thread_id

        gui_tick: list[bool] = []
        QTimer.singleShot(0, lambda: gui_tick.append(True))
        deadline = time.monotonic() + 0.5
        while not gui_tick and time.monotonic() < deadline:
            app.processEvents()
            time.sleep(0.005)
        assert gui_tick == [True]

        release.set()
        deadline = time.monotonic() + 1.0
        while controller._transition_active and time.monotonic() < deadline:
            app.processEvents()
            time.sleep(0.005)
        assert controller._transition_active is False
    finally:
        release.set()
        controller._join_transition_worker()
        controller._transition_timer.stop()
        controller._save_timer.stop()
        controller._startup_sync_timer.stop()
        controller._lock_timer.stop()
        controller._save_executor_shutdown = True
        controller._save_executor.shutdown(wait=True, cancel_futures=False)
