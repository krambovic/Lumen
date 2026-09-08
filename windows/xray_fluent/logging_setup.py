"""Domain-separated bounded asynchronous logging; only the worker touches files."""
from __future__ import annotations
from collections import deque
import json
import logging
import re
import threading
import time
from pathlib import Path
from .log_utils import is_routine_core_log
from .secret_scrubber import ScrubbingFilter, prepare_record, scrub_text

ROOT_LOGGER_NAME = "xray_fluent"
CORE_LOGGER_NAME = "xray_fluent.core"
TRAFFIC_LOGGER_NAME = "xray_fluent.traffic"
APP_LOGGER_NAME = "xray_fluent.app"
_CORE_CHILDREN = frozenset({"xray_fluent.zapret_manager"})
_TRAFFIC_CHILDREN = frozenset()
_MAX_BYTES = 2 * 1024 * 1024
_BACKUP_COUNT = 3
_FILE_QUEUE_MAX = 1024
_HUMAN_FMT = "%(asctime)s | %(levelname)-7s | %(xdomain)-7s | %(name)s | %(message)s"
_DATEFMT = "%Y-%m-%d %H:%M:%S"
_configured = False
_config_lock = threading.RLock()
_file_handler: _AsyncFileHandler | None = None
_upload_handler = None
_heartbeat_sender = None

def _domain_for(name: str) -> str:
    if name == CORE_LOGGER_NAME or name.startswith(CORE_LOGGER_NAME + ".") or name in _CORE_CHILDREN:
        return "core"
    if name == TRAFFIC_LOGGER_NAME or name.startswith(TRAFFIC_LOGGER_NAME + ".") or name in _TRAFFIC_CHILDREN:
        return "traffic"
    return "app"

class _DomainFilter(ScrubbingFilter):
    def __init__(self, only: str | None = None) -> None:
        super().__init__()
        self._only = only
    def filter(self, record: logging.LogRecord) -> bool:
        super().filter(record)
        domain = getattr(record, "xdomain", "") or _domain_for(record.name)
        record.xdomain = domain if domain in {"app", "core", "traffic"} else _domain_for(record.name)
        return self._only is None or record.xdomain == self._only

class _DiagnosticFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        msg = prepare_record(record).msg.lower()
        return not (any(token in msg for token in (
            "connection:", "handshake", "dial tcp", "unexpected http response status", "unexpected response status",
        )) or is_routine_core_log(msg))

class _EngineNoiseFilter(logging.Filter):
    _ENGINE_LINE_RE = re.compile(r"\[(info|warning|error|debug)\]", re.IGNORECASE)
    _ENGINE_TOKENS = ("common/errors", "infra/conf", "deprecated", "migrate to")
    _APP_TOKENS = (
        "права администратора", "прав администратора", "повышенными правами",
        "windivert могут работать нестабильно", "select a server first", "сначала выберите сервер",
    )
    def filter(self, record: logging.LogRecord) -> bool:
        msg = prepare_record(record).msg
        low = msg.lower()
        if any(token in low for token in self._APP_TOKENS):
            return False
        if (getattr(record, "xdomain", "") or _domain_for(record.name)) != "core":
            return True
        return not (self._ENGINE_LINE_RE.search(msg) or any(token in low for token in self._ENGINE_TOKENS) or is_routine_core_log(msg))

class _HumanFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        safe = prepare_record(record)
        safe.xdomain = getattr(safe, "xdomain", "") or _domain_for(safe.name)
        try:
            return scrub_text(super().format(safe))
        except Exception:
            return "[LOG FORMATTING FAILED] " + safe.msg

class _JsonLinesFormatter(logging.Formatter):
    def __init__(self, *, utc: bool = False) -> None:
        super().__init__()
        self._utc = utc
    def format(self, record: logging.LogRecord) -> str:
        safe = prepare_record(record)
        try:
            tm = time.gmtime(safe.created) if self._utc else time.localtime(safe.created)
            ts = time.strftime("%Y-%m-%dT%H:%M:%S", tm) + f".{int(safe.msecs):03d}"
            if self._utc:
                ts += "Z"
        except (ValueError, OverflowError, OSError):
            ts = ""
        payload = {"ts": ts, "level": safe.levelname,
                   "domain": getattr(safe, "xdomain", "") or _domain_for(safe.name),
                   "logger": safe.name, "msg": safe.msg}
        if safe.exc_text:
            payload["exc"] = safe.exc_text
        if safe.stack_info:
            payload["stack"] = safe.stack_info
        return json.dumps(payload, ensure_ascii=False)

class _RotatingFileSink:
    """Not a Handler: logging.shutdown must not close worker streams elsewhere."""
    def __init__(self, path: Path, domain: str | None, level: int, formatter: logging.Formatter) -> None:
        self.path, self.domain, self.level, self.formatter = path, domain, level, formatter
        self._stream = None
        self._size = 0
    def _open(self) -> None:
        if self._stream is None:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            self._stream = self.path.open("ab")
            self._size = self._stream.tell()
    def handle(self, record: logging.LogRecord) -> None:
        if self.domain is not None and record.xdomain != self.domain:
            return
        if self.domain is None and not _DiagnosticFilter().filter(record):
            return
        data = (self.formatter.format(record) + "\n").encode("utf-8", errors="replace")
        try:
            self._open()
            if self._size and self._size + len(data) > _MAX_BYTES:
                self.close()
                for index in range(_BACKUP_COUNT, 1, -1):
                    previous = self.path.with_name(f"{self.path.name}.{index - 1}")
                    if previous.exists():
                        previous.replace(self.path.with_name(f"{self.path.name}.{index}"))
                self.path.replace(self.path.with_name(f"{self.path.name}.1"))
                self._open()
            self._stream.write(data)
            self._stream.flush()
            self._size += len(data)
        except Exception:
            self.close()
            raise
    def close(self) -> None:
        stream, self._stream = self._stream, None
        if stream is not None:
            stream.close()

class _AsyncFileHandler(logging.Handler):
    """Drop newest on overflow; FIFO, snapshot flush barriers, bounded joins."""
    def __init__(self, sinks, *, capacity: int = _FILE_QUEUE_MAX) -> None:
        if type(capacity) is not int or capacity < 1:
            raise ValueError("capacity must be a positive integer")
        super().__init__(logging.DEBUG)
        self._sinks = tuple(sinks)
        self._capacity = capacity
        self._pending = deque()
        self._condition = threading.Condition()
        self._stopping = False
        self._accepted = self._completed = self._dropped = self._failures = 0
        self._thread = threading.Thread(target=self._run, name="lumen-file-logs", daemon=True)
        self._thread.start()
    @property
    def stats(self) -> dict[str, int]:
        with self._condition:
            return {"accepted": self._accepted, "completed": self._completed,
                    "queued": len(self._pending), "dropped": self._dropped, "sink_failures": self._failures}
    @property
    def dropped(self) -> int:
        return self.stats["dropped"]
    def emit(self, record: logging.LogRecord) -> None:
        try:
            safe = prepare_record(record)
            safe.xdomain = getattr(safe, "xdomain", "") or _domain_for(safe.name)
        except Exception:
            with self._condition:
                self._dropped += 1
            return
        with self._condition:
            if self._stopping or len(self._pending) >= self._capacity:
                self._dropped += 1
                return
            self._accepted += 1
            self._pending.append((self._accepted, safe))
            self._condition.notify()
    def _run(self) -> None:
        try:
            while True:
                with self._condition:
                    self._condition.wait_for(lambda: self._pending or self._stopping)
                    if not self._pending:
                        break
                    sequence, record = self._pending.popleft()
                for sink in self._sinks:
                    if record.levelno < sink.level:
                        continue
                    try:
                        sink.handle(record)
                    except Exception:
                        with self._condition:
                            self._failures += 1
                with self._condition:
                    self._completed = sequence
                    self._condition.notify_all()
        finally:
            for sink in self._sinks:
                try:
                    sink.close()
                except Exception:
                    with self._condition:
                        self._failures += 1
            with self._condition:
                self._condition.notify_all()
    def flush(self, timeout: float = 1.0) -> bool:
        if threading.current_thread() is self._thread:
            return False
        with self._condition:
            target = self._accepted
            return self._condition.wait_for(lambda: self._completed >= target, timeout=max(0.0, timeout))
    def close(self, *, wait: bool = True, timeout: float = 1.0) -> None:
        with self._condition:
            self._stopping = True
            self._condition.notify_all()
        if wait and threading.current_thread() is not self._thread:
            self._thread.join(max(0.0, timeout))
        super().close()

def configure_logging(log_dir: Path, *, upload_url: str = "", app_version: str = "") -> None:
    """Install once; all file creation and rotation is deferred to the worker."""
    global _configured, _file_handler
    with _config_lock:
        if _configured:
            return
        if _file_handler is not None and _file_handler._thread.is_alive():
            raise RuntimeError("Previous file logger is still draining")
        root = logging.getLogger(ROOT_LOGGER_NAME)
        root.setLevel(logging.DEBUG)
        human = _HumanFormatter(_HUMAN_FMT, datefmt=_DATEFMT)
        sinks = [_RotatingFileSink(Path(log_dir) / f"{domain}.log", domain, logging.DEBUG, human)
                 for domain in ("core", "app", "traffic")]
        sinks.append(_RotatingFileSink(Path(log_dir) / "errors.log", None, logging.WARNING, _JsonLinesFormatter()))
        _file_handler = _AsyncFileHandler(sinks)
        _file_handler.addFilter(_DomainFilter())
        root.addHandler(_file_handler)
        _configured = True
        configure_diagnostics_upload(upload_url=upload_url, app_version=app_version)

def flush_logging(timeout: float = 1.0) -> bool:
    handler = _file_handler
    return handler.flush(timeout) if handler is not None else True

def logging_stats() -> dict[str, int]:
    handler = _file_handler
    return handler.stats if handler is not None else {key: 0 for key in ("accepted", "completed", "queued", "dropped", "sink_failures")}

def shutdown_logging(*, wait: bool = True, timeout: float = 1.0) -> bool:
    """Detach immediately; False means a worker still owns pending file I/O."""
    global _configured
    with _config_lock:
        configure_diagnostics_upload(upload_url="")
        handler = _file_handler
        if handler is not None:
            logging.getLogger(ROOT_LOGGER_NAME).removeHandler(handler)
        _configured = False
    if handler is None:
        return True
    handler.close(wait=wait, timeout=timeout)
    return not handler._thread.is_alive()

def configure_diagnostics_upload(*, upload_url: str = "", app_version: str = "") -> None:
    global _upload_handler, _heartbeat_sender
    from .diagnostics_uploader import HeartbeatSender, HttpDiagnosticsHandler, set_uploads_enabled
    with _config_lock:
        set_uploads_enabled(bool(upload_url))
        root = logging.getLogger(ROOT_LOGGER_NAME)
        if _upload_handler is not None:
            root.removeHandler(_upload_handler)
            _upload_handler.close(wait=False)
            _upload_handler = None
        if _heartbeat_sender is not None:
            _heartbeat_sender.stop(wait=False)
            _heartbeat_sender = None
        if not upload_url:
            return
        try:
            uploader = HttpDiagnosticsHandler(upload_url, app_version=app_version)
            uploader.setLevel(logging.WARNING)
            uploader.addFilter(_DomainFilter())
            uploader.addFilter(_DiagnosticFilter())
            uploader.addFilter(_EngineNoiseFilter())
            uploader.setFormatter(_JsonLinesFormatter(utc=True))
            root.addHandler(uploader)
            _upload_handler = uploader
            _heartbeat_sender = HeartbeatSender(upload_url, app_version=app_version)
        except Exception:
            set_uploads_enabled(False)
            if _upload_handler is not None:
                root.removeHandler(_upload_handler)
                _upload_handler.close(wait=False)
                _upload_handler = None
            root.warning("[app] diagnostics auto-upload init failed")

def get_logger(domain: str) -> logging.Logger:
    return logging.getLogger({"core": CORE_LOGGER_NAME, "traffic": TRAFFIC_LOGGER_NAME,
                              "app": APP_LOGGER_NAME}.get(domain, APP_LOGGER_NAME))
