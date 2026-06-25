"""Centralized, domain-separated logging for Lumen KVN.

Splits logs into per-domain rotating files inside data/logs/:
  - core.log     - VPN engines (xray / sing-box / TUN / zapret)
  - app.log      - application, UI and Qt runtime
  - traffic.log  - traffic / speed / ping / auto-switch events
  - errors.log   - aggregated WARNING+ from every domain, JSON Lines
"""
from __future__ import annotations

import json
import logging
import logging.handlers
import time
from pathlib import Path

ROOT_LOGGER_NAME = "xray_fluent"
CORE_LOGGER_NAME = "xray_fluent.core"
TRAFFIC_LOGGER_NAME = "xray_fluent.traffic"
APP_LOGGER_NAME = "xray_fluent.app"

_CORE_CHILDREN = frozenset({"xray_fluent.zapret_manager"})
_TRAFFIC_CHILDREN = frozenset()

_MAX_BYTES = 2 * 1024 * 1024
_BACKUP_COUNT = 3
_HUMAN_FMT = "%(asctime)s | %(levelname)-7s | %(xdomain)-7s | %(name)s | %(message)s"
_DATEFMT = "%Y-%m-%d %H:%M:%S"

_configured = False


def _domain_for(name: str) -> str:
    if (
        name == CORE_LOGGER_NAME
        or name.startswith(CORE_LOGGER_NAME + ".")
        or name in _CORE_CHILDREN
    ):
        return "core"
    if (
        name == TRAFFIC_LOGGER_NAME
        or name.startswith(TRAFFIC_LOGGER_NAME + ".")
        or name in _TRAFFIC_CHILDREN
    ):
        return "traffic"
    return "app"


class _DomainFilter(logging.Filter):
    """Tag each record with its domain and optionally keep one domain only."""

    def __init__(self, only: str | None = None) -> None:
        super().__init__()
        self._only = only

    def filter(self, record: logging.LogRecord) -> bool:
        domain = getattr(record, "xdomain", "") or _domain_for(record.name)
        record.xdomain = domain
        return self._only is None or domain == self._only


class _DiagnosticFilter(logging.Filter):
    """Filter out noisy connection/handshake/timeout logs from errors.log and diagnostics upload."""

    def filter(self, record: logging.LogRecord) -> bool:
        msg = str(record.msg or "").lower()
        if any(token in msg for token in ("connection:", "handshake", "dial tcp", "unexpected http response status", "unexpected response status")):
            return False
        return True


class _JsonLinesFormatter(logging.Formatter):
    """One compact JSON object per line"""

    def format(self, record: logging.LogRecord) -> str:
        domain = getattr(record, "xdomain", "") or _domain_for(record.name)
        payload = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(record.created))
            + f".{int(record.msecs):03d}",
            "level": record.levelname,
            "domain": domain,
            "logger": record.name,
            "msg": record.getMessage(),
        }
        if record.exc_info:
            payload["exc"] = self.formatException(record.exc_info)
        return json.dumps(payload, ensure_ascii=False)


def configure_logging(log_dir: Path, *, upload_url: str = "", app_version: str = "") -> None:
    """Idempotently install the domain-separated file handlers."""
    global _configured
    root = logging.getLogger(ROOT_LOGGER_NAME)
    root.setLevel(logging.DEBUG)
    if _configured:
        return
    log_dir.mkdir(parents=True, exist_ok=True)
    human = logging.Formatter(_HUMAN_FMT, datefmt=_DATEFMT)

    def _file(name: str, only, level: int, fmt: logging.Formatter) -> None:
        handler = logging.handlers.RotatingFileHandler(
            log_dir / name,
            maxBytes=_MAX_BYTES,
            backupCount=_BACKUP_COUNT,
            encoding="utf-8",
            delay=True,
        )
        handler.setLevel(level)
        handler.addFilter(_DomainFilter(only))
        handler.setFormatter(fmt)
        root.addHandler(handler)

    _file("core.log", "core", logging.DEBUG, human)
    _file("app.log", "app", logging.DEBUG, human)
    _file("traffic.log", "traffic", logging.DEBUG, human)
    errors_handler = logging.handlers.RotatingFileHandler(
        log_dir / "errors.log",
        maxBytes=_MAX_BYTES,
        backupCount=_BACKUP_COUNT,
        encoding="utf-8",
        delay=True,
    )
    errors_handler.setLevel(logging.WARNING)
    errors_handler.addFilter(_DomainFilter(None))
    errors_handler.addFilter(_DiagnosticFilter())
    errors_handler.setFormatter(_JsonLinesFormatter())
    root.addHandler(errors_handler)

    if upload_url:
        try:
            from .diagnostics_uploader import HttpDiagnosticsHandler

            uploader = HttpDiagnosticsHandler(upload_url, app_version=app_version)
            uploader.setLevel(logging.WARNING)
            uploader.addFilter(_DomainFilter(None))
            uploader.addFilter(_DiagnosticFilter())
            uploader.setFormatter(_JsonLinesFormatter())
            root.addHandler(uploader)
        except Exception:
            root.warning(
                "[app] diagnostics auto-upload init failed",
                exc_info=True,
            )

    _configured = True


def get_logger(domain: str) -> logging.Logger:
    return logging.getLogger(
        {
            "core": CORE_LOGGER_NAME,
            "traffic": TRAFFIC_LOGGER_NAME,
            "app": APP_LOGGER_NAME,
        }.get(domain, APP_LOGGER_NAME)
    )
