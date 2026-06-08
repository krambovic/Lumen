"""Pure helpers for the QML Zapret tab"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from ...zapret_manager import PresetInfo, ZapretManager


def _fmt_date(iso: str) -> str:
    """Format an ISO timestamp as 'YYYY-MM-DD HH:MM' (same as the classic UI)."""
    if not iso:
        return ""
    try:
        return datetime.fromisoformat(iso).strftime("%Y-%m-%d %H:%M")
    except (ValueError, TypeError):
        return iso


def preset_to_map(info: PresetInfo) -> dict[str, Any]:
    """Convert a single PresetInfo into a QML-friendly map."""
    return {
        "name": info.name,
        "description": info.description or "",
        "argCount": int(info.arg_count),
        "created": _fmt_date(info.created),
        "modified": _fmt_date(info.modified),
        "createdRaw": info.created or "",
        "modifiedRaw": info.modified or "",
    }


def list_preset_maps() -> list[dict[str, Any]]:
    """Return the full preset list as a list of maps (empty list on failure)."""
    try:
        return [preset_to_map(p) for p in ZapretManager.list_preset_infos()]
    except Exception:
        return []
