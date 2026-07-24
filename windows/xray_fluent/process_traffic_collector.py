from __future__ import annotations

import os
import urllib.request
import json
from dataclasses import dataclass
from typing import Any

from .constants import SINGBOX_CLASH_API_PORT
from .win_proc_monitor import process_name_from_pid
from .i18n import tr

# Processes to hide (internal, not user traffic)
_HIDDEN_PROCESSES = {"xray.exe", "sing-box.exe"}
_MAX_REASONABLE_BYTES_PER_SEC = 256 * 1024 ** 2
_MAX_FIRST_SAMPLE_BYTES = 512 * 1024 ** 2


@dataclass(slots=True)
class ProcessTrafficSnapshot:
    exe: str            # "chrome.exe"
    upload: int         # bytes total (cumulative)
    download: int       # bytes total (cumulative)
    connections: int    # active connection count
    total_connections: int = 0  # all-time unique connections
    route: str = "direct"      # "proxy" | "direct" | "mixed"
    proxy_bytes: int = 0   # bytes through proxy
    direct_bytes: int = 0  # bytes through direct
    top_host: str = ""     # most traffic host/domain
    down_speed: float = 0.0  # bytes/sec download
    up_speed: float = 0.0    # bytes/sec upload


# Session-scoped state
_seen_connections: dict[str, set[str]] = {}
_conn_owner: dict[str, str] = {}
_conn_bytes: dict[str, tuple[int, int]] = {}
_conn_raw_bytes: dict[str, tuple[int, int]] = {}
_proc_total_connections: dict[str, int] = {}
_proc_closed_bytes: dict[str, tuple[int, int]] = {}  # {exe: (closed_up, closed_down)} — bytes from closed connections
_prev_proc_total: dict[str, tuple[int, int]] = {}  # {exe: (total_up, total_down)} — for speed calc
_prev_time: float = 0.0
_metadata_process_cache: dict[str, str] = {}


def reset_connection_tracking() -> None:
    """Call on disconnect to reset session counters."""
    _seen_connections.clear()
    _conn_owner.clear()
    _conn_bytes.clear()
    _conn_raw_bytes.clear()
    _proc_total_connections.clear()
    _proc_closed_bytes.clear()
    _prev_proc_total.clear()
    _metadata_process_cache.clear()
    global _prev_time
    _prev_time = 0.0


def _metadata_value(meta: dict[str, Any], *keys: str) -> Any:
    for key in keys:
        value = meta.get(key)
        if value not in (None, ""):
            return value
    lowered = {str(k).lower(): v for k, v in meta.items()}
    for key in keys:
        value = lowered.get(key.lower())
        if value not in (None, ""):
            return value
    return ""


def _process_name_from_metadata(meta: dict[str, Any]) -> tuple[str, str]:
    process_path = str(_metadata_value(meta, "processPath", "process_path", "process") or "").strip()
    if process_path:
        name = os.path.basename(process_path).strip() or process_path
        return name.lower(), name

    explicit_name = str(_metadata_value(meta, "processName", "process_name", "program", "exe") or "").strip()
    if explicit_name:
        name = os.path.basename(explicit_name).strip() or explicit_name
        return name.lower(), name

    pid = _metadata_value(meta, "processID", "processId", "pid", "uid")
    pid_key = str(pid or "").strip()
    if pid_key:
        cached = _metadata_process_cache.get(pid_key)
        if cached:
            return cached.lower(), cached
        name = process_name_from_pid(pid_key)
        if name:
            _metadata_process_cache[pid_key] = name
            return name.lower(), name

    host = str(_metadata_value(meta, "host", "destinationIP", "destination_ip", "dstIP", "dst_ip") or "").strip()
    if host:
        return f"system:{host}".lower(), tr("Системный трафик ({addr})", addr=host)
    return "system:unknown", tr("Системный трафик")


def collect_process_stats(
    clash_api_port: int = SINGBOX_CLASH_API_PORT,
    *,
    clash_api_secret: str = "",
) -> list[ProcessTrafficSnapshot]:
    """Poll sing-box Clash API and aggregate traffic by process.

    Returns list of ProcessTrafficSnapshot sorted by total traffic (desc).
    Returns empty list on error.
    """
    if not clash_api_secret:
        return []
    try:
        url = f"http://127.0.0.1:{clash_api_port}/connections"
        req = urllib.request.Request(
            url,
            headers={"Authorization": f"Bearer {clash_api_secret}"},
            method="GET",
        )
        with urllib.request.urlopen(req, timeout=2) as resp:
            data: dict[str, Any] = json.loads(resp.read())
    except Exception:
        return []

    connections = data.get("connections") or []

    # Track which connection IDs are still active
    active_conn_ids: set[str] = set()

    import time as _time
    global _prev_time
    now = _time.monotonic()
    dt = max(0.5, now - _prev_time) if _prev_time > 0 else 2.0
    _prev_time = now
    max_delta = int(_MAX_REASONABLE_BYTES_PER_SEC * dt)

    # Aggregate by process exe name
    by_proc: dict[str, dict[str, Any]] = {}
    for conn in connections:
        meta = conn.get("metadata") or {}
        exe, display_exe = _process_name_from_metadata(meta)

        if exe in _HIDDEN_PROCESSES:
            continue

        if exe not in by_proc:
            by_proc[exe] = {
                "upload": 0, "download": 0, "conns": 0, "routes": set(),
                "proxy_bytes": 0, "direct_bytes": 0, "hosts": {},
                "display_exe": display_exe,
            }

        entry = by_proc[exe]
        # Track unique connection IDs and their bytes
        conn_id = str(conn.get("id", "") or "")
        raw_up = _safe_int(conn.get("upload", 0))
        raw_down = _safe_int(conn.get("download", 0))
        conn_up, conn_down = _validated_connection_bytes(conn_id, raw_up, raw_down, max_delta)
        conn_total = conn_up + conn_down
        if conn_id:
            active_conn_ids.add(conn_id)
            if conn_id not in _conn_owner:
                _proc_total_connections[exe] = _proc_total_connections.get(exe, 0) + 1
            if exe not in _seen_connections:
                _seen_connections[exe] = set()
            _seen_connections[exe].add(conn_id)
            _conn_owner[conn_id] = exe
        entry["upload"] += conn_up
        entry["download"] += conn_down
        entry["conns"] += 1

        # Route + per-route bytes
        chains = conn.get("chains") or []
        is_proxy = False
        if chains:
            chain = chains[0].lower()
            if "proxy" in chain:
                entry["routes"].add("proxy")
                entry["proxy_bytes"] += conn_total
                is_proxy = True
            else:
                entry["routes"].add("direct")
                entry["direct_bytes"] += conn_total

        # Track hosts (domain or IP)
        host = meta.get("host") or meta.get("destinationIP") or ""
        if host:
            entry["hosts"][host] = entry["hosts"].get(host, 0) + conn_total

        if str(entry["display_exe"]).startswith(("Системный трафик", "System traffic")) and not display_exe.startswith(("Системный трафик", "System traffic")):
            entry["display_exe"] = display_exe

    # Detect closed connections → accumulate their bytes into _proc_closed_bytes
    closed_ids = set(_conn_bytes.keys()) - active_conn_ids
    for cid in closed_ids:
        up, down = _conn_bytes.pop(cid)
        _conn_raw_bytes.pop(cid, None)
        exe_key = _conn_owner.pop(cid, "")
        if exe_key:
            prev_closed = _proc_closed_bytes.get(exe_key, (0, 0))
            _proc_closed_bytes[exe_key] = (prev_closed[0] + up, prev_closed[1] + down)
            conn_set = _seen_connections.get(exe_key)
            if conn_set is not None:
                conn_set.discard(cid)
                if not conn_set:
                    _seen_connections.pop(exe_key, None)

    # Build snapshots
    result: list[ProcessTrafficSnapshot] = []
    for exe, stats in by_proc.items():
        routes = stats["routes"]
        if len(routes) > 1:
            route = "mixed"
        elif routes:
            route = next(iter(routes))
        else:
            route = "direct"

        top_host = ""
        if stats["hosts"]:
            top_host = max(stats["hosts"], key=stats["hosts"].get)

        total_conns = max(stats["conns"], _proc_total_connections.get(exe, 0))

        # Total bytes = active connections + closed connections
        closed_up, closed_down = _proc_closed_bytes.get(exe, (0, 0))
        total_up = stats["upload"] + closed_up
        total_down = stats["download"] + closed_down

        # Speed from monotonic total delta
        prev_up, prev_down = _prev_proc_total.get(exe, (0, 0))
        up_speed = max(0.0, (total_up - prev_up) / dt)
        down_speed = max(0.0, (total_down - prev_down) / dt)
        _prev_proc_total[exe] = (total_up, total_down)

        result.append(ProcessTrafficSnapshot(
            exe=stats["display_exe"],
            upload=total_up,
            download=total_down,
            connections=stats["conns"],
            total_connections=total_conns,
            route=route,
            proxy_bytes=stats["proxy_bytes"],
            direct_bytes=stats["direct_bytes"],
            top_host=top_host,
            down_speed=down_speed,
            up_speed=up_speed,
        ))

    # Sort by total traffic descending
    result.sort(key=lambda s: s.upload + s.download, reverse=True)
    return result


def _safe_int(value: Any) -> int:
    try:
        return max(0, int(value or 0))
    except Exception:
        return 0


def _validated_connection_bytes(conn_id: str, raw_up: int, raw_down: int, max_delta: int) -> tuple[int, int]:
    if not conn_id:
        return min(raw_up, _MAX_FIRST_SAMPLE_BYTES), min(raw_down, _MAX_FIRST_SAMPLE_BYTES)

    prev_up, prev_down = _conn_bytes.get(conn_id, (0, 0))
    prev_raw_up, prev_raw_down = _conn_raw_bytes.get(conn_id, (0, 0))

    if conn_id not in _conn_raw_bytes:
        up = 0
        down = 0
    else:
        raw_delta_up = max(0, raw_up - prev_raw_up)
        raw_delta_down = max(0, raw_down - prev_raw_down)
        up = prev_up + (raw_delta_up if raw_delta_up <= max_delta else 0)
        down = prev_down + (raw_delta_down if raw_delta_down <= max_delta else 0)

    _conn_raw_bytes[conn_id] = (raw_up, raw_down)
    _conn_bytes[conn_id] = (up, down)
    return up, down
