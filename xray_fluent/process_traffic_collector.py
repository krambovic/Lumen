from __future__ import annotations

import os
import urllib.request
import json
from dataclasses import dataclass
from typing import Any

from .constants import SINGBOX_CLASH_API_PORT
from .win_proc_monitor import process_name_from_pid

# Processes to hide (internal, not user traffic)
_HIDDEN_PROCESSES = {"xray.exe", "sing-box.exe", "tun2socks.exe"}


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
_conn_bytes: dict[str, tuple[int, int]] = {}  # {conn_id: (upload, download)} — last seen per connection
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
        return f"system:{host}".lower(), f"Системный трафик ({host})"
    return "system:unknown", "Системный трафик"


def collect_process_stats(clash_api_port: int = SINGBOX_CLASH_API_PORT) -> list[ProcessTrafficSnapshot]:
    """Poll sing-box Clash API and aggregate traffic by process.

    Returns list of ProcessTrafficSnapshot sorted by total traffic (desc).
    Returns empty list on error.
    """
    try:
        url = f"http://127.0.0.1:{clash_api_port}/connections"
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req, timeout=2) as resp:
            data: dict[str, Any] = json.loads(resp.read())
    except Exception:
        return []

    connections = data.get("connections") or []

    # Track which connection IDs are still active
    active_conn_ids: set[str] = set()

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
        conn_up = conn.get("upload", 0)
        conn_down = conn.get("download", 0)
        conn_total = conn_up + conn_down

        # Track unique connection IDs and their bytes
        conn_id = conn.get("id", "")
        if conn_id:
            active_conn_ids.add(conn_id)
            if conn_id not in _conn_owner:
                _proc_total_connections[exe] = _proc_total_connections.get(exe, 0) + 1
            if exe not in _seen_connections:
                _seen_connections[exe] = set()
            _seen_connections[exe].add(conn_id)
            _conn_bytes[conn_id] = (conn_up, conn_down)
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

        if str(entry["display_exe"]).startswith("Системный трафик") and not display_exe.startswith("Системный трафик"):
            entry["display_exe"] = display_exe

    # Detect closed connections → accumulate their bytes into _proc_closed_bytes
    closed_ids = set(_conn_bytes.keys()) - active_conn_ids
    for cid in closed_ids:
        up, down = _conn_bytes.pop(cid)
        exe_key = _conn_owner.pop(cid, "")
        if exe_key:
            prev_closed = _proc_closed_bytes.get(exe_key, (0, 0))
            _proc_closed_bytes[exe_key] = (prev_closed[0] + up, prev_closed[1] + down)
            conn_set = _seen_connections.get(exe_key)
            if conn_set is not None:
                conn_set.discard(cid)
                if not conn_set:
                    _seen_connections.pop(exe_key, None)

    # Calculate per-process speed from delta
    import time as _time
    global _prev_time
    now = _time.monotonic()
    dt = max(0.5, now - _prev_time) if _prev_time > 0 else 2.0
    _prev_time = now

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
