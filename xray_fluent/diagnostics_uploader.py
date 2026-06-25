"""Fail-safe background HTTP uploader for diagnostic data.

Auto-sends WARNING+ log records to a remote endpoint as JSON batches, and
can upload full diagnostic zip bundles. All network I/O runs on daemon
threads and never raises into the app: if the endpoint is unset or
unreachable, the application is completely unaffected.
"""
from __future__ import annotations

import hashlib
import hmac as _hmac
import json
import logging
import queue
import threading
import time
import urllib.request
from pathlib import Path

from .constants import (
    APP_VERSION as _APP_VERSION,
    DIAGNOSTICS_SECRET as _SECRET,
)
from .data_paths import get_install_id

_USER_AGENT = f"LumenKVN-Diagnostics/{_APP_VERSION}"

_EVENT_TIMEOUT = 10
_BUNDLE_TIMEOUT = 30
_BATCH_MAX = 50
_FLUSH_INTERVAL = 30.0
_QUEUE_MAX = 1000


class HttpDiagnosticsHandler(logging.Handler):
    """Logging handler that POSTs formatted records to an HTTP endpoint."""

    def __init__(self, url: str, *, app_version: str = "",
                 flush_interval: float = _FLUSH_INTERVAL) -> None:
        super().__init__()
        self._url = url
        self._app_version = app_version
        self._flush_interval = flush_interval
        self._queue = queue.Queue(maxsize=_QUEUE_MAX)
        self._stop = threading.Event()
        self._thread = threading.Thread(
            target=self._run, name="diag-uploader", daemon=True
        )
        self._thread.start()

    def emit(self, record: logging.LogRecord) -> None:
        try:
            self._queue.put_nowait(self.format(record))
        except queue.Full:
            pass
        except Exception:
            pass


    def _run(self) -> None:
        batch = []
        last = time.monotonic()
        while not self._stop.is_set():
            timeout = max(0.5, self._flush_interval - (time.monotonic() - last))
            try:
                batch.append(self._queue.get(timeout=timeout))
            except queue.Empty:
                pass
            due = (time.monotonic() - last) >= self._flush_interval
            if batch and (len(batch) >= _BATCH_MAX or due):
                self._post_events(batch)
                batch = []
                last = time.monotonic()

    def _post_events(self, batch) -> None:
        try:
            events = [json.loads(item) for item in batch]
        except Exception:
            events = [{"msg": item} for item in batch]
        body = json.dumps(
            {"kind": "error-batch", "app_version": self._app_version,
             "install_id": get_install_id(), "events": events},
            ensure_ascii=False,
        ).encode("utf-8")
        _send(self._url, body, "application/json", _EVENT_TIMEOUT)

    def close(self) -> None:
        self._stop.set()
        super().close()


def _sign_headers(body: bytes) -> dict:
    """HMAC-SHA256 signature headers for the request body.

    sig = hex(HMAC(secret, timestamp + "." + hex(sha256(body)))).
    Returns an empty dict when no shared secret is configured.
    """
    if not _SECRET:
        return {}
    ts = str(int(time.time()))
    digest = hashlib.sha256(body).hexdigest()
    sig = _hmac.new(
        _SECRET.encode("utf-8"),
        (ts + "." + digest).encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    return {"X-Diag-Timestamp": ts, "X-Diag-Signature": sig}


def _send(url: str, body: bytes, content_type: str, timeout: int,
          headers: dict | None = None) -> None:
    try:
        all_headers = {"Content-Type": content_type, "User-Agent": _USER_AGENT}
        all_headers.update(_sign_headers(body))
        if headers:
            all_headers.update(headers)
        req = urllib.request.Request(
            url, data=body, method="POST", headers=all_headers
        )
        with urllib.request.urlopen(req, timeout=timeout):
            pass
    except Exception:
        pass


def upload_bundle(url: str, zip_path: Path, *, app_version: str = "") -> None:
    """Fire-and-forget upload of a full diagnostic zip on a daemon thread."""
    if not url:
        return

    def _worker() -> None:
        try:
            data = Path(zip_path).read_bytes()
        except Exception:
            return
        _send(
            url, data, "application/zip", _BUNDLE_TIMEOUT,
            {"X-App-Version": app_version, "X-Filename": Path(zip_path).name},
        )

    threading.Thread(target=_worker, name="diag-bundle-upload", daemon=True).start()
