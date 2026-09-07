from __future__ import annotations

import logging
import time

from PyQt6.QtCore import QEvent, QObject, QTimer


class ShutdownCoordinator(QObject):
    # Swallow an ordinary Quit event while cancelled work finishes. The GUI
    # event loop remains alive, including queued QThread cleanup callbacks.
    def __init__(self, bridge, app):
        super().__init__(app)
        self.bridge = bridge
        self.app = app
        self._ready = False
        self._timer = QTimer(self)
        self._timer.setInterval(100)
        self._timer.timeout.connect(self._poll)

    def eventFilter(self, obj, event):
        if obj is not self.app or event.type() != QEvent.Type.Quit or self._ready:
            return False
        self.bridge.shutdown(deadline=time.monotonic() + 5.0)
        if getattr(self.bridge.controller, "_system_shutdown", False):
            return False  # Windows controls the final logoff deadline.
        if not self.bridge.controller.has_pending_shutdown_work():
            self._finish()
            return False
        self._timer.start()
        return True

    def _finish(self):
        try:
            self.bridge.controller.finalize_shutdown()
        except Exception:
            logging.getLogger("xray_fluent.app").exception("Final owned-runtime cleanup failed")
        self._ready = True
        self._timer.stop()

    def _poll(self):
        if self.bridge.controller.has_pending_shutdown_work():
            return
        self._finish()
        self.app.quit()

    def close(self):
        self._timer.stop()
