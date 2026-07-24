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
        self._last_fingerprint = self._fingerprint()

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
            try:
                local_ip = socket.gethostbyname(socket.gethostname())
            except OSError:
                local_ip = "0.0.0.0"

        return local_ip
