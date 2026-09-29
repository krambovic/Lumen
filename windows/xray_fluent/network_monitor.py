from __future__ import annotations

import socket

from PyQt6.QtCore import QObject, QTimer, pyqtSignal, pyqtSlot


class NetworkMonitor(QObject):
    network_changed = pyqtSignal(str, str)

    def __init__(self, interval_ms: int = 5000, parent: QObject | None = None):
        super().__init__(parent)
        self._timer = QTimer(self)
        self._timer.setInterval(interval_ms)
        self._timer.timeout.connect(self._check)
        # The monitor is constructed before the first window frame. Network
        # probing there can stall startup on a busy or offline Windows boot.
        self._last_fingerprint = ""

    def start(self) -> None:
        self._last_fingerprint = self._fingerprint()
        self._timer.start()

    @pyqtSlot()
    def stop(self) -> None:
        self._timer.stop()

    def _check(self) -> None:
        current = self._fingerprint()
        if current != self._last_fingerprint:
            previous = self._last_fingerprint
            self._last_fingerprint = current
            self.network_changed.emit(previous, current)

    @staticmethod
    def _fingerprint() -> str:
        local_ip = "0.0.0.0"
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
                sock.connect(("8.8.8.8", 80))
                local_ip = sock.getsockname()[0]
        except OSError:
            # Hostname resolution can block for a long time while DNS services
            # are still starting. A later timer tick detects the real address.
            pass

        return local_ip
