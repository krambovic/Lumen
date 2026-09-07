from __future__ import annotations

import json
import os
import tempfile
import threading
import time
import uuid
from collections.abc import Mapping
from dataclasses import dataclass, field
from datetime import datetime, timezone, timedelta
from pathlib import Path
from types import MappingProxyType
from typing import Any

from .constants import DATA_DIR

TRAFFIC_HISTORY_FILE = DATA_DIR / "traffic_history.json"
_MAX_REASONABLE_BYTES_PER_SECOND = 2 * 1024 ** 3
_MIN_REASONABLE_SESSION_BYTES = 2 * 1024 ** 4
_HARD_REASONABLE_SESSION_BYTES = 1024 ** 5
_ROUTES = frozenset({"proxy", "direct", "mixed", "unknown"})


def _counter(value: Any) -> int:
    try:
        return max(0, int(value))
    except (TypeError, ValueError, OverflowError):
        return 0


def _freeze(value: Any) -> Any:
    if isinstance(value, Mapping):
        return MappingProxyType({k: _freeze(v) for k, v in value.items()})
    if isinstance(value, (list, tuple)):
        return tuple(_freeze(v) for v in value)
    return value


def _json_ready(value: Any) -> Any:
    if isinstance(value, Mapping):
        return {k: _json_ready(v) for k, v in value.items()}
    if isinstance(value, tuple):
        return [_json_ready(v) for v in value]
    return value


@dataclass
class ProcessTrafficEntry:
    upload: int = 0
    download: int = 0
    route: str = "unknown"
    proxy_bytes: int = 0
    direct_bytes: int = 0
    unknown_bytes: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {"upload": self.upload, "download": self.download, "route": self.route,
                "proxy_bytes": self.proxy_bytes, "direct_bytes": self.direct_bytes,
                "unknown_bytes": self.unknown_bytes}

    @staticmethod
    def from_dict(data: dict[str, Any]) -> ProcessTrafficEntry:
        entry = ProcessTrafficEntry(
            upload=_counter(data.get("upload", 0)), download=_counter(data.get("download", 0)),
            route=str(data.get("route", "unknown")),
            proxy_bytes=_counter(data.get("proxy_bytes", 0)),
            direct_bytes=_counter(data.get("direct_bytes", 0)),
            unknown_bytes=_counter(data.get("unknown_bytes", 0)),
        )
        if not any(key in data for key in ("proxy_bytes", "direct_bytes", "unknown_bytes")):
            # Legacy mixed totals cannot be split honestly; retain them as unknown.
            kind = entry.route if entry.route in {"proxy", "direct"} else "unknown"
            setattr(entry, kind + "_bytes", entry.upload + entry.download)
        return entry


@dataclass
class TrafficSession:
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    started_at: str = ""
    ended_at: str | None = None
    node_name: str = ""
    mode: str = ""
    total_upload: int = 0
    total_download: int = 0
    processes: dict[str, ProcessTrafficEntry] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {"id": self.id, "started_at": self.started_at, "ended_at": self.ended_at,
                "node_name": self.node_name, "mode": self.mode,
                "total_upload": self.total_upload, "total_download": self.total_download,
                "processes": {key: value.to_dict() for key, value in self.processes.items()}}

    @staticmethod
    def from_dict(data: dict[str, Any]) -> TrafficSession:
        processes = data.get("processes") or {}
        if not isinstance(processes, dict):
            raise ValueError("invalid process history")
        return TrafficSession(
            id=str(data.get("id") or uuid.uuid4()), started_at=str(data.get("started_at", "")),
            ended_at=data.get("ended_at"), node_name=str(data.get("node_name", "")),
            mode=str(data.get("mode", "")), total_upload=_counter(data.get("total_upload", 0)),
            total_download=_counter(data.get("total_download", 0)),
            processes={str(key): ProcessTrafficEntry.from_dict(value)
                       for key, value in processes.items() if isinstance(value, dict)},
        )


class _HistoryWriter:
    """One lazy writer per storage; only the newest pending generation is kept.

    No disk operation holds the storage/GUI lock (nor the queue condition).
    Durable generations increase monotonically even across clear/end/coalescing.
    """

    def __init__(self, path: Path) -> None:
        self.path = path
        self._condition = threading.Condition()
        self._pending: tuple[int, Mapping[str, Any]] | None = None
        self._thread: threading.Thread | None = None
        self._latest = self._durable = self._failed = 0
        self._closing = False
        self.last_error = ""

    def submit(self, generation: int, snapshot: Mapping[str, Any]) -> None:
        with self._condition:
            if self._closing:
                raise RuntimeError("history writer is closed")
            if generation <= self._latest:
                return
            self._latest = generation
            self._pending = generation, snapshot
            if self._thread is None:
                self._thread = threading.Thread(target=self._run, name="lumen-history-writer", daemon=True)
                self._thread.start()
            self._condition.notify_all()

    def _run(self) -> None:
        while True:
            with self._condition:
                self._condition.wait_for(lambda: self._pending is not None or self._closing)
                if self._pending is None:
                    return
                generation, snapshot = self._pending
                self._pending = None
            try:
                committed = self._write(generation, snapshot)
            except Exception as exc:
                with self._condition:
                    self._failed = max(self._failed, generation)
                    self.last_error = f"traffic history write failed ({type(exc).__name__})"
                    self._condition.notify_all()
            else:
                with self._condition:
                    if committed:
                        self._durable = max(self._durable, generation)
                        self.last_error = ""
                    self._condition.notify_all()

    def _write(self, generation: int, snapshot: Mapping[str, Any]) -> bool:
        # Conversion and JSON serialization, directory creation, flush/fsync and
        # replace all run off-thread, after the immutable snapshot was captured.
        payload = json.dumps(_json_ready(snapshot), ensure_ascii=False)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        staged: Path | None = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w", encoding="utf-8", newline="\n", dir=self.path.parent,
                prefix=f".{self.path.name}.", suffix=".tmp", delete=False,
            ) as handle:
                staged = Path(handle.name)
                handle.write(payload)
                handle.flush()
                os.fsync(handle.fileno())
            with self._condition:
                superseded = generation < self._latest
            if superseded:
                return False
            # There is only one committer: even if submit races this replace, a
            # newer generation can only be written AFTER this one, never before.
            os.replace(staged, self.path)
            return True
        finally:
            if staged is not None:
                try:
                    staged.unlink(missing_ok=True)
                except OSError:
                    pass

    def flush(self, timeout: float = 2.0) -> bool:
        deadline = time.monotonic() + max(0.0, timeout)
        with self._condition:
            target = self._latest
            while self._durable < target:
                if self._failed >= target and self._pending is None:
                    return False
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return False
                self._condition.wait(remaining)
            return True

    def close(self, timeout: float = 2.0) -> bool:
        deadline = time.monotonic() + max(0.0, timeout)
        with self._condition:
            self._closing = True
            self._condition.notify_all()
            thread = self._thread
        if thread is not None and thread is not threading.current_thread():
            thread.join(max(0.0, deadline - time.monotonic()))
        with self._condition:
            return (thread is None or not thread.is_alive()) and self._durable >= self._latest


class TrafficHistoryStorage:
    """Thread-safe history with asynchronous atomic persistence.

    start_session/end_session/clear/save_periodic enqueue ordered snapshots.
    Use flush(timeout) when a caller needs durability and close(timeout) at final
    shutdown. Both return False on timeout/write failure; last_save_error gives
    an honest diagnostic. Getters return detached copies, never mutable internals.
    """

    def __init__(self) -> None:
        self._lock = threading.RLock()
        self._sessions: list[TrafficSession] = []
        self._daily_totals: dict[str, dict[str, int]] = {}
        self._current_session: TrafficSession | None = None
        self._path = Path(TRAFFIC_HISTORY_FILE)  # stable target, including test isolation
        self._generation = 0
        self._closed = False
        self._writer = _HistoryWriter(self._path)
        self._load()

    def _load(self) -> None:
        if not self._path.exists():
            return
        try:
            data = json.loads(self._path.read_text(encoding="utf-8"))
            if not isinstance(data, dict):
                raise ValueError("invalid history root")
        except Exception:
            self._quarantine_corrupt_file()
            return
        changed = False
        raw_sessions = data.get("sessions", [])
        if not isinstance(raw_sessions, list):
            raw_sessions, changed = [], True
        for item in raw_sessions:
            if not isinstance(item, dict):
                changed = True
                continue
            try:
                session = TrafficSession.from_dict(item)
            except (TypeError, ValueError, OverflowError):
                changed = True
                continue
            changed = self._sanitize_session(session) or changed
            self._sessions.append(session)
        self._daily_totals = self._build_daily_totals_from_sessions(self._sessions)
        if self._daily_totals != data.get("daily_totals"):
            changed = True
        if changed:
            self._save()

    def _quarantine_corrupt_file(self) -> None:
        stamp = datetime.now(timezone.utc).strftime("%Y%m%d%H%M%S")
        backup = self._path.with_name(f"{self._path.name}.corrupt-{stamp}-{uuid.uuid4().hex[:8]}")
        try:
            os.replace(self._path, backup)
        except OSError:
            pass

    def _save_locked(self) -> None:
        self._daily_totals = self._build_daily_totals_from_sessions(self._sessions)
        snapshot = _freeze({"sessions": [session.to_dict() for session in self._sessions],
                            "daily_totals": self._daily_totals})
        self._generation += 1
        self._writer.submit(self._generation, snapshot)

    def _save(self) -> None:
        with self._lock:
            if not self._closed:
                self._save_locked()

    def start_session(self, node_name: str, mode: str) -> str:
        with self._lock:
            if self._closed:
                raise RuntimeError("traffic history is closed")
            self._end_session_locked(save=False)
            session = TrafficSession(started_at=datetime.now(timezone.utc).isoformat(), node_name=node_name, mode=mode)
            self._current_session = session
            self._sessions.append(session)
            self._cleanup_old_sessions()
            self._save_locked()
            return session.id

    def update_session(self, process_stats: dict[str, tuple]) -> None:
        """Accept {exe: (up, down, route)} or (up, down, route, proxy, direct, unknown).

        Extended route counters are cumulative *bytes*, not fractions. The legacy
        three-tuple remains supported; a legacy mixed total is wholly unknown.
        """
        with self._lock:
            if not self._closed:
                self._update_session_locked(process_stats)

    def _update_session_locked(self, process_stats: dict[str, tuple]) -> None:
        session = self._current_session
        if session is None:
            return
        for exe, values in process_stats.items():
            if len(values) not in {3, 6}:
                raise ValueError("process traffic requires three or six fields")
            up, down, route = values[:3]
            route = str(route) if route in _ROUTES else "unknown"
            entry = session.processes.setdefault(exe, ProcessTrafficEntry(route=route))
            entry.upload, entry.download = _counter(up), _counter(down)
            if entry.route != route:
                entry.route = "unknown" if "unknown" in {entry.route, route} else "mixed"
            if len(values) == 6:
                entry.proxy_bytes, entry.direct_bytes, entry.unknown_bytes = map(_counter, values[3:])
            else:
                entry.proxy_bytes = entry.direct_bytes = entry.unknown_bytes = 0
                kind = route if route in {"proxy", "direct"} else "unknown"
                setattr(entry, kind + "_bytes", entry.upload + entry.download)
        self._sanitize_session(session)

    @staticmethod
    def _parse_iso_datetime(value: str | None) -> datetime | None:
        if not value:
            return None
        try:
            parsed = datetime.fromisoformat(value)
            return parsed.replace(tzinfo=timezone.utc) if parsed.tzinfo is None else parsed.astimezone(timezone.utc)
        except (TypeError, ValueError, OverflowError):
            return None

    _sanitize_counter = staticmethod(_counter)

    def _session_duration_seconds(self, started_at: str, ended_at: str | None) -> int:
        started = self._parse_iso_datetime(started_at)
        ended = self._parse_iso_datetime(ended_at) or datetime.now(timezone.utc)
        return max(0, int((ended - started).total_seconds())) if started is not None else 0

    def _session_limit_bytes(self, started_at: str, ended_at: str | None) -> int:
        seconds = self._session_duration_seconds(started_at, ended_at)
        return max(_MIN_REASONABLE_SESSION_BYTES,
                   min(_HARD_REASONABLE_SESSION_BYTES, seconds * _MAX_REASONABLE_BYTES_PER_SECOND))

    def _sanitize_session(self, session: TrafficSession) -> bool:
        before = session.to_dict()
        limit = self._session_limit_bytes(session.started_at, session.ended_at)
        for entry in session.processes.values():
            up, down = _counter(entry.upload), _counter(entry.download)
            entry.upload, entry.download = (up if up <= limit else 0), (down if down <= limit else 0)
            if entry.route not in _ROUTES:
                entry.route = "unknown"
            entry.proxy_bytes, entry.direct_bytes, entry.unknown_bytes = (
                _counter(entry.proxy_bytes), _counter(entry.direct_bytes), _counter(entry.unknown_bytes))
            total = entry.upload + entry.download
            routed = entry.proxy_bytes + entry.direct_bytes + entry.unknown_bytes
            if routed > total:
                entry.proxy_bytes = entry.direct_bytes = 0
                entry.unknown_bytes = total
                entry.route = "unknown"
            elif routed < total:
                entry.unknown_bytes += total - routed
            if entry.unknown_bytes:
                entry.route = "unknown"
        if session.processes:
            up = sum(entry.upload for entry in session.processes.values())
            down = sum(entry.download for entry in session.processes.values())
        else:
            up, down = _counter(session.total_upload), _counter(session.total_download)
        session.total_upload, session.total_download = (up if up <= limit else 0), (down if down <= limit else 0)
        return before != session.to_dict()

    def _session_day_key(self, started_at: str) -> str:
        parsed = self._parse_iso_datetime(started_at)
        if parsed is not None:
            return parsed.strftime("%Y-%m-%d")
        return started_at[:10] if len(started_at) >= 10 and started_at[4:5] == "-" and started_at[7:8] == "-" else ""

    def _build_daily_totals_from_sessions(self, sessions: list[TrafficSession]) -> dict[str, dict[str, int]]:
        totals: dict[str, dict[str, int]] = {}
        for session in sessions:
            key = self._session_day_key(session.started_at)
            if key:
                daily = totals.setdefault(key, {"upload": 0, "download": 0})
                daily["upload"] += session.total_upload
                daily["download"] += session.total_download
        return totals

    def end_session(self) -> None:
        with self._lock:
            if not self._closed:
                self._end_session_locked()

    def _end_session_locked(self, *, save: bool = True) -> None:
        if self._current_session is not None:
            self._current_session.ended_at = datetime.now(timezone.utc).isoformat()
            self._current_session = None
            if save:
                self._save_locked()

    def save_periodic(self) -> None:
        with self._lock:
            if self._current_session is not None and not self._closed:
                self._save_locked()

    def clear(self) -> None:
        with self._lock:
            if self._closed:
                raise RuntimeError("traffic history is closed")
            self._sessions.clear()
            self._daily_totals.clear()
            self._current_session = None
            self._save_locked()

    def flush(self, timeout: float = 2.0) -> bool:
        with self._lock:
            if not self._closed:
                self._save_locked()
        return self._writer.flush(timeout)

    def close(self, timeout: float = 2.0) -> bool:
        with self._lock:
            if not self._closed:
                self._end_session_locked(save=False)
                self._save_locked()
                self._closed = True
        return self._writer.close(timeout)

    @property
    def last_save_error(self) -> str:
        return self._writer.last_error

    def get_sessions(self, days: int = 30) -> list[TrafficSession]:
        cutoff = (datetime.now(timezone.utc) - timedelta(days=days)).isoformat()
        with self._lock:
            return [TrafficSession.from_dict(session.to_dict()) for session in self._sessions if session.started_at >= cutoff]

    def get_daily_totals(self, days: int = 30) -> dict[str, dict[str, int]]:
        cutoff = (datetime.now(timezone.utc) - timedelta(days=days)).strftime("%Y-%m-%d")
        with self._lock:
            daily = self._build_daily_totals_from_sessions(self._sessions)
            return {key: dict(value) for key, value in daily.items() if key >= cutoff}

    def get_process_totals(self, days: int = 30) -> dict[str, dict[str, int | str]]:
        totals: dict[str, dict[str, Any]] = {}
        for session in self.get_sessions(days):
            for exe, entry in session.processes.items():
                result = totals.setdefault(exe, {"upload": 0, "download": 0, "route": entry.route,
                                                "proxy_bytes": 0, "direct_bytes": 0, "unknown_bytes": 0})
                for key in ("upload", "download", "proxy_bytes", "direct_bytes", "unknown_bytes"):
                    result[key] += getattr(entry, key)
                if entry.route != result["route"]:
                    result["route"] = "unknown" if "unknown" in {entry.route, result["route"]} else "mixed"
        return totals

    @property
    def current_session(self) -> TrafficSession | None:
        with self._lock:
            return TrafficSession.from_dict(self._current_session.to_dict()) if self._current_session is not None else None

    def _cleanup_old_sessions(self) -> None:
        cutoff = (datetime.now(timezone.utc) - timedelta(days=365)).isoformat()
        self._sessions = [session for session in self._sessions if session.started_at >= cutoff]
        self._daily_totals = self._build_daily_totals_from_sessions(self._sessions)
