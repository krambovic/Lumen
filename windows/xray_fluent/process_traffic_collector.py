from __future__ import annotations

import ntpath
import threading
import time
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from .constants import SINGBOX_CLASH_API_PORT
from .i18n import tr
from .metrics_api import ClashApiClient, MetricsApiError
from .traffic_route_classifier import RouteClassifier
from .win_proc_monitor import process_name_from_pid

_HIDDEN_PROCESSES = {"xray.exe", "sing-box.exe"}
_MAX_REASONABLE_BYTES_PER_SEC = 256 * 1024 ** 2


@dataclass(frozen=True, slots=True)
class ProcessTrafficSnapshot:
    exe: str
    upload: int
    download: int
    connections: int
    total_connections: int = 0
    route: str = "unknown"  # proxy | direct | mixed | unknown
    proxy_bytes: int = 0
    direct_bytes: int = 0
    top_host: str = ""
    down_speed: float = 0.0
    up_speed: float = 0.0
    unknown_bytes: int = 0  # append to preserve existing positional construction


_lock = threading.Lock()
_generation = 0
_seen_connections: dict[str, set[str]] = {}
_conn_owner: dict[str, str] = {}
_conn_bytes: dict[str, tuple[int, int]] = {}
_conn_raw_bytes: dict[str, tuple[int, int]] = {}
_conn_route_bytes: dict[str, dict[str, int]] = {}
_proc_total_connections: dict[str, int] = {}
_proc_closed_bytes: dict[str, tuple[int, int]] = {}
_proc_closed_route_bytes: dict[str, dict[str, int]] = {}
_proc_display_names: dict[str, str] = {}
_prev_proc_total: dict[str, tuple[int, int]] = {}
_prev_time = 0.0


def connection_tracking_epoch() -> int:
    with _lock:
        return _generation


def reset_connection_tracking() -> None:
    """Invalidate even a fetch already in flight in a retiring worker."""
    global _prev_time, _generation
    with _lock:
        for cache in (_seen_connections, _conn_owner, _conn_bytes, _conn_raw_bytes,
                      _conn_route_bytes, _proc_total_connections, _proc_closed_bytes,
                      _proc_closed_route_bytes, _proc_display_names, _prev_proc_total):
            cache.clear()
        _prev_time = 0.0
        _generation += 1


def _metadata_value(meta: Mapping[str, Any], *keys: str) -> Any:
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


def _process_name_from_metadata(meta: Mapping[str, Any], pid_names: dict[str, str] | None = None) -> tuple[str, str]:
    process_path = str(_metadata_value(meta, "processPath", "process_path", "process") or "").strip()
    explicit_name = str(_metadata_value(meta, "processName", "process_name", "program", "exe") or "").strip()
    if process_path or explicit_name:
        name = ntpath.basename(process_path or explicit_name).strip() or process_path or explicit_name
        return name.lower(), name
    pid = _metadata_value(meta, "processID", "processId", "pid", "uid")
    pid_key = str(pid or "").strip()
    if pid_key:
        # win_proc_monitor owns the PID cache and keys it by process creation
        # time. A second PID-only cache here would return the previous program
        # after Windows recycles the numeric PID.
        if pid_names is None:
            name = process_name_from_pid(pid_key)
        else:
            if pid_key not in pid_names:
                pid_names[pid_key] = process_name_from_pid(pid_key)
            name = pid_names[pid_key]
        if name:
            return name.lower(), name
    host = str(_metadata_value(meta, "host", "destinationIP", "destination_ip", "dstIP", "dst_ip") or "").strip()
    if host:
        return f"system:{host}".lower(), tr("Системный трафик ({addr})", addr=host)
    return "system:unknown", tr("Системный трафик")


def collect_process_stats(
    clash_api_port: int = SINGBOX_CLASH_API_PORT, *, clash_api_secret: str = "",
    connections_document: Mapping[str, Any] | None = None,
    outbound_graph: Mapping[str, Any] | RouteClassifier | None = None,
    tracking_epoch: int | None = None,
) -> list[ProcessTrafficSnapshot]:
    """Aggregate one immutable /connections snapshot; no second fetch per tick.

    The standalone polling API is retained. Workers should supply the document
    they already fetched and their creation-time tracking_epoch. No metadata or
    missing chain ever implies direct traffic. Errors do not close connections.
    """
    epoch = connection_tracking_epoch() if tracking_epoch is None else tracking_epoch
    data = connections_document
    if data is None:
        if not clash_api_secret:
            return []
        client = ClashApiClient(clash_api_port, clash_api_secret)
        try:
            data = client.get("/connections")
            if outbound_graph is None:
                try:
                    outbound_graph = client.get("/proxies")
                except MetricsApiError:
                    pass
        except MetricsApiError:
            return []
        finally:
            client.close()
    if not isinstance(data, Mapping):
        return []
    connections = data.get("connections")
    if not isinstance(connections, (list, tuple)):
        return []
    classifier = outbound_graph if isinstance(outbound_graph, RouteClassifier) else RouteClassifier(outbound_graph)
    global _prev_time
    with _lock:
        if epoch != _generation:
            return []
        now = time.monotonic()
        dt = max(0.5, now - _prev_time) if _prev_time > 0 else 2.0
        _prev_time = now
        max_delta = int(_MAX_REASONABLE_BYTES_PER_SEC * dt)
        active_ids: set[str] = set()
        by_proc: dict[str, dict[str, Any]] = {}
        pid_names: dict[str, str] = {}  # snapshot-local, never stale across PID generations
        for conn in connections:
            if not isinstance(conn, Mapping):
                continue
            meta = conn.get("metadata")
            if not isinstance(meta, Mapping):
                meta = {}
            exe, display_exe = _process_name_from_metadata(meta, pid_names)
            if exe in _HIDDEN_PROCESSES:
                continue
            _proc_display_names[exe] = display_exe
            entry = by_proc.setdefault(exe, _empty_entry())
            cid = str(conn.get("id") or "")
            if cid and cid in active_ids:
                continue  # malformed duplicate must not inflate active totals
            if cid and cid in _conn_owner and _conn_owner[cid] != exe:
                _close_connection(cid)
            previous = _conn_bytes.get(cid, (0, 0))
            up, down = _validated_connection_bytes(
                cid, _safe_int(conn.get("upload")), _safe_int(conn.get("download")), max_delta,
            )
            route = classifier.classify(conn.get("chains"))
            if cid:
                active_ids.add(cid)
                if cid not in _conn_owner:
                    _proc_total_connections[exe] = _proc_total_connections.get(exe, 0) + 1
                _seen_connections.setdefault(exe, set()).add(cid)
                _conn_owner[cid] = exe
                route_totals = _conn_route_bytes.setdefault(cid, _empty_routes())
                route_totals[route] += max(0, up - previous[0]) + max(0, down - previous[1])
                for kind, value in route_totals.items():
                    entry[kind + "_bytes"] += value
            entry["upload"] += up
            entry["download"] += down
            entry["conns"] += 1
            entry["routes"].add(route)
            host = str(_metadata_value(meta, "host", "destinationIP") or "")
            if host:
                entry["hosts"][host] = entry["hosts"].get(host, 0) + up + down

        for cid in set(_conn_bytes) - active_ids:
            _close_connection(cid)
        # Closed-only processes stay visible with zero speed, preserving both
        # overall and route-specific session totals after the last socket ends.
        for exe in _proc_closed_bytes:
            by_proc.setdefault(exe, _empty_entry())
        result: list[ProcessTrafficSnapshot] = []
        for exe, entry in by_proc.items():
            closed_up, closed_down = _proc_closed_bytes.get(exe, (0, 0))
            up, down = entry["upload"] + closed_up, entry["download"] + closed_down
            for kind, value in _proc_closed_route_bytes.get(exe, {}).items():
                entry[kind + "_bytes"] += value
            routes = entry["routes"] | {kind for kind in ("proxy", "direct", "unknown") if entry[kind + "_bytes"]}
            route = "unknown" if not routes or "unknown" in routes else ("mixed" if len(routes) > 1 else next(iter(routes)))
            prev_up, prev_down = _prev_proc_total.get(exe, (up, down))
            _prev_proc_total[exe] = (up, down)
            result.append(ProcessTrafficSnapshot(
                exe=_proc_display_names.get(exe, exe), upload=up, download=down,
                connections=entry["conns"], total_connections=_proc_total_connections.get(exe, 0),
                route=route, proxy_bytes=entry["proxy_bytes"], direct_bytes=entry["direct_bytes"],
                unknown_bytes=entry["unknown_bytes"],
                top_host=max(entry["hosts"], key=entry["hosts"].get) if entry["hosts"] else "",
                up_speed=max(0.0, (up - prev_up) / dt), down_speed=max(0.0, (down - prev_down) / dt),
            ))
        result.sort(key=lambda item: item.upload + item.download, reverse=True)
        return result


def _empty_routes() -> dict[str, int]:
    return {"proxy": 0, "direct": 0, "unknown": 0}


def _empty_entry() -> dict[str, Any]:
    return {"upload": 0, "download": 0, "conns": 0, "routes": set(),
            "proxy_bytes": 0, "direct_bytes": 0, "unknown_bytes": 0, "hosts": {}}


def _close_connection(cid: str) -> None:
    up, down = _conn_bytes.pop(cid, (0, 0))
    _conn_raw_bytes.pop(cid, None)
    route_totals = _conn_route_bytes.pop(cid, _empty_routes())
    exe = _conn_owner.pop(cid, "")
    if not exe:
        return
    closed = _proc_closed_bytes.get(exe, (0, 0))
    _proc_closed_bytes[exe] = (closed[0] + up, closed[1] + down)
    closed_routes = _proc_closed_route_bytes.setdefault(exe, _empty_routes())
    for kind, value in route_totals.items():
        closed_routes[kind] += value
    seen = _seen_connections.get(exe)
    if seen is not None:
        seen.discard(cid)
        if not seen:
            _seen_connections.pop(exe, None)


def _safe_int(value: Any) -> int:
    try:
        return max(0, int(value or 0))
    except (TypeError, ValueError, OverflowError):
        return 0


def _validated_connection_bytes(cid: str, raw_up: int, raw_down: int, max_delta: int) -> tuple[int, int]:
    if not cid:
        return 0, 0  # no stable identity -> cannot invent cumulative deltas
    previous = _conn_bytes.get(cid, (0, 0))
    raw_previous = _conn_raw_bytes.get(cid)
    if raw_previous is None:
        up, down = 0, 0  # first sample establishes a baseline, not session traffic
    else:
        delta_up, delta_down = max(0, raw_up - raw_previous[0]), max(0, raw_down - raw_previous[1])
        up = previous[0] + (delta_up if delta_up <= max_delta else 0)
        down = previous[1] + (delta_down if delta_down <= max_delta else 0)
    _conn_raw_bytes[cid] = (raw_up, raw_down)
    _conn_bytes[cid] = (up, down)
    return up, down
