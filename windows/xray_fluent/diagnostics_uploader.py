"""Bounded consent-generation-scoped transport; no I/O on the GUI thread.

Revocation invalidates queued/new work without joining network threads. Already
DISPATCHED requests cannot be unsent. The built-in HMAC is a protocol tag, NOT
client authentication: a secret distributed in every binary proves no identity.
"""
from __future__ import annotations
from collections import OrderedDict
import hashlib
import hmac as _hmac
import json
import logging
import os
import queue
import threading
import time
import urllib.parse
import urllib.request
import weakref
from pathlib import Path
from .constants import APP_VERSION as _APP_VERSION, DIAGNOSTICS_SECRET as _SECRET
from .secret_scrubber import prepare_record, scrub_text

_USER_AGENT = f"Lumen-Diagnostics/{_APP_VERSION}"
_EVENT_TIMEOUT, _BUNDLE_TIMEOUT, _BATCH_MAX = 10, 30, 50
_FLUSH_INTERVAL, _QUEUE_MAX = 30.0, 1000
_HEARTBEAT_INTERVAL = 15 * 60.0
_bundle_threads: set[threading.Thread] = set()
_bundle_threads_lock = threading.Lock()
_uploads_enabled = threading.Event()
_upload_epoch = 0
_state_lock = threading.RLock()
_clients = weakref.WeakSet()
_bundle_generations: OrderedDict[str, int] = OrderedDict()
_SEND_SLOTS = threading.BoundedSemaphore(2)

def get_upload_epoch() -> int:
    """Capture BEFORE scheduling a job; pass it to upload_bundle(epoch=...)."""
    with _state_lock:
        return _upload_epoch

def uploads_allowed(epoch: int) -> bool:
    with _state_lock:
        return _uploads_enabled.is_set() and epoch == _upload_epoch

def set_uploads_enabled(enabled: bool) -> int:
    """Invalidate old work even on a quick OFF -> ON, without joining workers."""
    global _upload_epoch
    with _state_lock:
        _upload_epoch += 1
        epoch = _upload_epoch
        if enabled:
            _uploads_enabled.set()
        else:
            _uploads_enabled.clear()
        clients = tuple(_clients)
    for client in clients:
        client._cancel_for_consent()
    return epoch

def _path_key(path: Path) -> str:
    return os.path.normcase(os.path.abspath(path))

def mark_bundle_generation(path: Path, epoch: int) -> None:
    """Bind recent exports to consent; delayed jobs must retain explicit epochs."""
    key = _path_key(path)
    with _state_lock:
        _bundle_generations[key] = epoch
        _bundle_generations.move_to_end(key)
        while len(_bundle_generations) > 256:
            _bundle_generations.popitem(last=False)

def _register(client) -> None:
    with _state_lock:
        _clients.add(client)

def _sanitize_event(value: object) -> dict:
    if not isinstance(value, dict):
        return {"msg": scrub_text(value)[:16 * 1024]}
    event = {}
    for key in ("ts", "level", "domain", "logger", "msg", "exc", "stack"):
        item = value.get(key)
        if isinstance(item, str):
            event[key] = scrub_text(item)[:16 * 1024]
    return event or {"msg": "[UNFORMATTABLE LOG EVENT]"}

def _safe_payload(payload: object) -> dict:
    from .diagnostics import _safe_version
    if not isinstance(payload, dict) or payload.get("kind") not in ("error-batch", "heartbeat"):
        raise ValueError("Unknown diagnostics payload")
    safe = {"kind": payload["kind"], "app_version": _safe_version(payload.get("app_version"))}
    if safe["kind"] == "heartbeat":
        ts = payload.get("ts")
        safe["ts"] = ts if type(ts) is int and ts >= 0 else 0
    else:
        events = payload.get("events")
        safe["events"] = [_sanitize_event(event) for event in events[:_BATCH_MAX]] if isinstance(events, list) else []
    return safe

class HttpDiagnosticsHandler(logging.Handler):
    def __init__(self, url: str, *, app_version: str = "", flush_interval: float = _FLUSH_INTERVAL) -> None:
        super().__init__()
        self._url, self._app_version = url, app_version
        self._epoch = get_upload_epoch()
        self._flush_interval = max(0.01, flush_interval)
        self._queue = queue.Queue(maxsize=_QUEUE_MAX)
        self._stop = threading.Event()
        self._lifecycle_lock = threading.Lock()
        self._accepted = self._dropped = 0
        self._thread = threading.Thread(target=self._run, name="diag-uploader", daemon=True)
        _register(self)
        self._thread.start()
    def _active(self) -> bool:
        return not self._stop.is_set() and uploads_allowed(self._epoch)
    @property
    def stats(self) -> dict[str, int]:
        with self._lifecycle_lock:
            return {"accepted": self._accepted, "dropped": self._dropped, "queued": self._queue.qsize()}
    def emit(self, record: logging.LogRecord) -> None:
        if not self._active():
            return
        try:
            formatted = scrub_text(self.format(prepare_record(record)))
            try:
                event = _sanitize_event(json.loads(formatted))
            except (ValueError, RecursionError):
                event = {"msg": formatted[:16 * 1024]}
            with self._lifecycle_lock:
                if not self._active():
                    return
                try:
                    self._queue.put_nowait(event)
                    self._accepted += 1
                except queue.Full:
                    self._dropped += 1
        except Exception:
            # handleError can expose the raw record on stderr; never call it.
            with self._lifecycle_lock:
                self._dropped += 1
    def _run(self) -> None:
        batch = []
        last = time.monotonic()
        try:
            while self._active():
                remaining = max(0.01, self._flush_interval - (time.monotonic() - last))
                try:
                    item = self._queue.get(timeout=min(0.2, remaining))
                    self._queue.task_done()
                    if item is None:
                        break
                    batch.append(item)
                except queue.Empty:
                    pass
                due = time.monotonic() - last >= self._flush_interval
                if batch and (len(batch) >= _BATCH_MAX or due):
                    if not self._post_events(batch):
                        with self._lifecycle_lock:
                            self._dropped += len(batch)
                    batch = []
                    last = time.monotonic()
        finally:
            with self._lifecycle_lock:
                self._dropped += len(batch)
                self._discard_pending()
    def _post_events(self, batch) -> bool:
        if not self._active():
            return False
        body = json.dumps(_safe_payload({"kind": "error-batch", "app_version": self._app_version,
                                        "events": batch}), ensure_ascii=False).encode("utf-8")
        return _send(self._url, body, "application/json", _EVENT_TIMEOUT,
                     epoch=self._epoch, cancel_event=self._stop)
    def _discard_pending(self) -> None:
        while True:
            try:
                item = self._queue.get_nowait()
            except queue.Empty:
                break
            self._queue.task_done()
            if item is not None:
                self._dropped += 1
    def _cancel_for_consent(self) -> None:
        with self._lifecycle_lock:
            self._stop.set()
            self._discard_pending()
            self._queue.put_nowait(None)
    def close(self, *, wait: bool = True, timeout: float = 1.0) -> None:
        self._cancel_for_consent()
        if wait and self._thread is not threading.current_thread():
            self._thread.join(max(0.0, timeout))
        super().close()

def _sign_headers(body: bytes) -> dict:
    """Legacy protocol tag only, not authentication or authorization."""
    if not _SECRET:
        return {}
    ts = str(int(time.time()))
    digest = hashlib.sha256(body).hexdigest()
    signature = _hmac.new(_SECRET.encode("utf-8"), f"{ts}.{digest}".encode("utf-8"), hashlib.sha256).hexdigest()
    return {"X-Diag-Timestamp": ts, "X-Diag-Signature": signature}

class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

def _send(url: str, body: bytes, content_type: str, timeout: int, headers: dict | None = None, *,
          epoch: int | None = None, cancel_event: threading.Event | None = None) -> bool:
    epoch = get_upload_epoch() if epoch is None else epoch
    def cancelled() -> bool:
        return not uploads_allowed(epoch) or (cancel_event is not None and cancel_event.is_set())
    if cancelled() or not _SEND_SLOTS.acquire(blocking=False):
        return False
    try:
        parsed = urllib.parse.urlsplit(url)
        loopback = parsed.hostname in {"localhost", "127.0.0.1", "::1"}
        if (not parsed.hostname or parsed.username is not None or parsed.password is not None
                or parsed.fragment or not (parsed.scheme == "https" or parsed.scheme == "http" and loopback)):
            return False
        if content_type == "application/json":
            body = json.dumps(_safe_payload(json.loads(body)), ensure_ascii=False).encode("utf-8")
        elif content_type == "application/zip":
            from .diagnostics import sanitize_diagnostic_zip
            body = sanitize_diagnostic_zip(body, cancelled=cancelled)
        else:
            return False
        all_headers = {"Content-Type": content_type, "User-Agent": _USER_AGENT}
        if headers:
            from .diagnostics import _safe_version
            all_headers["X-App-Version"] = _safe_version(headers.get("X-App-Version"))
            all_headers["X-Filename"] = "diagnostics.zip"
        all_headers.update(_sign_headers(body))
        request = urllib.request.Request(url, data=body, method="POST", headers=all_headers)
        # Explicit proxy policy: no implicit registry discovery or redirects.
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())
        if cancelled():
            return False
        # Dispatch boundary. Never hold the consent lock across network I/O.
        with opener.open(request, timeout=timeout):
            pass
        return True
    except Exception:
        return False
    finally:
        _SEND_SLOTS.release()

def upload_bundle(url: str, zip_path: Path, *, app_version: str = "", epoch: int | None = None) -> None:
    """Scrub old archives in a bounded worker; send only in the captured epoch.

    Retain an epoch from BEFORE scheduling/generation for delayed UI jobs.
    Recent export_diagnostics results are also automatically epoch-bound.
    """
    if not url:
        return
    with _state_lock:
        recorded_epoch = _bundle_generations.get(_path_key(zip_path))
        epoch = recorded_epoch if epoch is None and recorded_epoch is not None else (
            _upload_epoch if epoch is None else epoch)
    if not uploads_allowed(epoch) or recorded_epoch is not None and recorded_epoch != epoch:
        return
    def worker() -> None:
        try:
            from .diagnostics import _MAX_BUNDLE_BYTES, sanitize_diagnostic_zip
            if not uploads_allowed(epoch):
                return
            with Path(zip_path).open("rb") as stream:
                data = stream.read(_MAX_BUNDLE_BYTES + 1)
            safe = sanitize_diagnostic_zip(data, cancelled=lambda: not uploads_allowed(epoch))
            if uploads_allowed(epoch):
                _send(url, safe, "application/zip", _BUNDLE_TIMEOUT,
                      {"X-App-Version": app_version, "X-Filename": "diagnostics.zip"}, epoch=epoch)
        except Exception:
            pass
        finally:
            with _bundle_threads_lock:
                _bundle_threads.discard(threading.current_thread())
    thread = threading.Thread(target=worker, name="diag-bundle-upload", daemon=True)
    with _bundle_threads_lock:
        if len(_bundle_threads) >= 2:
            return
        _bundle_threads.add(thread)
    try:
        thread.start()
    except Exception:
        with _bundle_threads_lock:
            _bundle_threads.discard(thread)

def wait_for_bundle_uploads(timeout: float = _BUNDLE_TIMEOUT + 1) -> None:
    """Explicit worker/test drain ONLY; never used by GUI consent-off."""
    deadline = time.monotonic() + max(0.0, timeout)
    with _bundle_threads_lock:
        threads = list(_bundle_threads)
    for thread in threads:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        if thread is not threading.current_thread():
            thread.join(remaining)

class HeartbeatSender:
    """Coarse consent-scoped activity ping, without install ID or fingerprint."""
    def __init__(self, url: str, *, app_version: str = "", interval: float = _HEARTBEAT_INTERVAL) -> None:
        self._url, self._app_version = url, app_version
        self._epoch = get_upload_epoch()
        self._interval = max(0.01, interval)
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="diag-heartbeat", daemon=True)
        _register(self)
        self._thread.start()
    def _run(self) -> None:
        while not self._stop.is_set() and uploads_allowed(self._epoch):
            self._ping()
            self._stop.wait(self._interval)
    def _ping(self) -> None:
        if self._stop.is_set() or not uploads_allowed(self._epoch):
            return
        try:
            body = json.dumps(_safe_payload({"kind": "heartbeat", "app_version": self._app_version,
                                            "ts": int(time.time())}), ensure_ascii=False).encode("utf-8")
            _send(self._url, body, "application/json", _EVENT_TIMEOUT,
                  epoch=self._epoch, cancel_event=self._stop)
        except Exception:
            pass
    def _cancel_for_consent(self) -> None:
        self._stop.set()
    def stop(self, *, wait: bool = True, timeout: float = 1.0) -> None:
        self._stop.set()
        if wait and self._thread is not threading.current_thread():
            self._thread.join(max(0.0, timeout))
