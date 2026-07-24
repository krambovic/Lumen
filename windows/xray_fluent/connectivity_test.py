from __future__ import annotations

import time
import threading
from urllib.request import ProxyHandler, Request

from .http_utils import build_opener
from .constants import APP_VERSION

from PyQt6.QtCore import QThread, pyqtSignal


class ConnectivityTestWorker(QThread):
    result = pyqtSignal(bool, str, object)

    def __init__(self, http_port: int, url: str, timeout: float = 8.0, tun_mode: bool = False):
        super().__init__()
        self._http_port = http_port
        self._url = url
        self._timeout = timeout
        self._tun_mode = tun_mode
        self._cancelled = threading.Event()
        self._response = None
        self._response_lock = threading.Lock()
        self.setObjectName("lumen-connectivity-test")

    def cancel(self) -> None:
        self._cancelled.set()
        with self._response_lock:
            response = self._response
        if response is not None:
            try:
                response.close()
            except Exception:
                pass

    def run(self) -> None:
        if self._tun_mode:
            opener = build_opener()
        else:
            proxy_url = f"http://127.0.0.1:{self._http_port}"
            opener = build_opener(
                ProxyHandler(
                    {
                        "http": proxy_url,
                        "https": proxy_url,
                    }
                )
            )

        request = Request(self._url, headers={"User-Agent": f"Lumen/{APP_VERSION}"})
        started = time.perf_counter()
        try:
            if self._cancelled.is_set():
                return
            with opener.open(request, timeout=self._timeout) as response:
                with self._response_lock:
                    self._response = response
                try:
                    response.read(16)
                    status_code = getattr(response, "status", 200)
                finally:
                    with self._response_lock:
                        self._response = None
            if self._cancelled.is_set():
                return
            elapsed_ms = int((time.perf_counter() - started) * 1000)
            self.result.emit(True, f"{self._url} -> HTTP {status_code}", elapsed_ms)
        except Exception as exc:
            if not self._cancelled.is_set():
                self.result.emit(False, f"{self._url} -> {exc}", None)
