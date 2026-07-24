"""GUI-free helpers for the QML History tab.

Faithful port of the formatting logic in ui/history_page.py so the QML page can
render identical figures.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from ...i18n import tr


def fmt_bytes(b: int) -> str:
    value = float(max(0, int(b)))
    units = ["B", "KB", "MB", "GB", "TB", "PB", "EB"]
    unit_idx = 0
    while value >= 1024.0 and unit_idx < len(units) - 1:
        value /= 1024.0
        unit_idx += 1
    if unit_idx == 0:
        return f"{int(value)} {units[unit_idx]}"
    if unit_idx <= 2:
        return f"{value:.1f} {units[unit_idx]}"
    return f"{value:.2f} {units[unit_idx]}"


def fmt_datetime(iso: str) -> str:
    if not iso:
        return ""
    try:
        dt = datetime.fromisoformat(iso).astimezone()
        return dt.strftime("%d.%m.%Y %H:%M")
    except Exception:
        return iso[:16]


def fmt_duration(start: str, end: str | None) -> str:
    if not start:
        return ""
    try:
        s = datetime.fromisoformat(start)
        e = datetime.fromisoformat(end) if end else datetime.now(timezone.utc)
        delta = e - s
        total_sec = int(delta.total_seconds())
        if total_sec < 0:
            return ""
        hours, rem = divmod(total_sec, 3600)
        minutes, secs = divmod(rem, 60)
        if hours > 0:
            return tr("{hours}ч {minutes}м", hours=hours, minutes=minutes)
        elif minutes > 0:
            return tr("{minutes}м {secs}с", minutes=minutes, secs=secs)
        else:
            return tr("{secs}с", secs=secs)
    except Exception:
        return ""


def _duration_seconds(start: str, end: str | None) -> int:
    """Raw session length in seconds, for sorting (0 when unknown)."""
    if not start:
        return 0
    try:
        s = datetime.fromisoformat(start)
        e = datetime.fromisoformat(end) if end else datetime.now(timezone.utc)
        total_sec = int((e - s).total_seconds())
        return total_sec if total_sec > 0 else 0
    except Exception:
        return 0


_MODE_LABELS = {
    "xray": "Прокси",
    "singbox": "TUN (sing-box)",
}

_ROUTE_LABELS = {
    "proxy": "VPN",
    "direct": "Прямой",
    "mixed": "Смешанный",
}


def _mode_label(mode: str) -> str:
    return _MODE_LABELS.get(mode, mode)


def _route_label(route: str) -> str:
    return _ROUTE_LABELS.get(route, route)


def _is_tun(mode: str) -> bool:
    return "tun" in (mode or "").lower() or mode == "singbox"


def build_history_payload(storage: Any, days: int = 30) -> dict[str, Any]:
    """Return a QML-friendly dict mirroring ui/history_page._refresh()."""
    if storage is None:
        return {
            "summaryDown": "0 B",
            "summaryUp": "0 B",
            "summarySessions": 0,
            "sessions": [],
            "daily": [],
            "procs": [],
        }

    sessions = list(storage.get_sessions(days))
    sessions.sort(key=lambda s: s.started_at, reverse=True)

    total_up = sum(s.total_upload for s in sessions)
    total_down = sum(s.total_download for s in sessions)

    session_rows: list[dict[str, Any]] = []
    for s in sessions:
        proc_keys = sorted(s.processes.keys())
        procs = ", ".join(proc_keys[:5])
        if len(proc_keys) > 5:
            procs += f" (+{len(proc_keys) - 5})"
        session_rows.append({
            "date": fmt_datetime(s.started_at),
            "node": s.node_name,
            "mode": s.mode or "",
            "isTun": _is_tun(s.mode),
            "duration": fmt_duration(s.started_at, s.ended_at),
            "down": fmt_bytes(s.total_download),
            "hasDown": s.total_download > 0,
            "up": fmt_bytes(s.total_upload),
            "procs": procs,
            # Raw values for client-side column sorting.
            "ts": s.started_at or "",
            "durSec": _duration_seconds(s.started_at, s.ended_at),
            "downB": int(s.total_download),
            "upB": int(s.total_upload),
        })

    daily = storage.get_daily_totals(days)
    daily_rows: list[dict[str, Any]] = []
    for date_key, totals in sorted(daily.items(), key=lambda kv: kv[0], reverse=True):
        down = int(totals.get("download", 0))
        up_b = int(totals.get("upload", 0))
        daily_rows.append({
            "day": date_key,
            "down": fmt_bytes(down),
            "hasDown": down > 0,
            "up": fmt_bytes(up_b),
            # Raw values for client-side column sorting.
            "downB": down,
            "upB": up_b,
        })

    proc_totals = storage.get_process_totals(days)
    proc_rows: list[dict[str, Any]] = []
    for exe, stats in sorted(proc_totals.items(), key=lambda kv: kv[1]["download"], reverse=True):
        down = int(stats["download"])
        up_b = int(stats["upload"])
        proc_rows.append({
            "exe": exe,
            "down": fmt_bytes(down),
            "hasDown": down > 0,
            "up": fmt_bytes(up_b),
            "route": str(stats.get("route", "")),
            # Raw values for client-side column sorting.
            "downB": down,
            "upB": up_b,
        })

    return {
        "summaryDown": fmt_bytes(total_down),
        "summaryUp": fmt_bytes(total_up),
        "summarySessions": len(sessions),
        "sessions": session_rows,
        "daily": daily_rows,
        "procs": proc_rows,
    }
