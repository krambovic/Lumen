from __future__ import annotations

from typing import Any


_SUPPORTED_PROXY_TYPES = {"vless", "vmess", "trojan", "shadowsocks"}


def normalize_mux_concurrency(value: Any) -> int:
    try:
        concurrency = int(value)
    except Exception:
        concurrency = 8
    return max(1, min(32, concurrency))


def apply_xray_multiplex(outbound: dict[str, Any], *, enabled: bool, concurrency: int = 8) -> bool:
    protocol = str(outbound.get("protocol") or "").strip().lower()
    if protocol not in _SUPPORTED_PROXY_TYPES or _uses_xtls_vision(outbound):
        return False

    if not enabled:
        outbound["mux"] = {"enabled": False, "concurrency": -1}
        return False

    outbound["mux"] = {
        "enabled": True,
        "concurrency": normalize_mux_concurrency(concurrency),
    }
    return True


def apply_singbox_multiplex(outbound: dict[str, Any], *, enabled: bool, concurrency: int = 8) -> bool:
    outbound_type = str(outbound.get("type") or "").strip().lower()
    if outbound_type not in _SUPPORTED_PROXY_TYPES or _uses_singbox_xtls_vision(outbound):
        outbound.pop("multiplex", None)
        return False

    if not enabled:
        outbound.pop("multiplex", None)
        return False

    streams = normalize_mux_concurrency(concurrency)
    outbound["multiplex"] = {
        "enabled": True,
        "protocol": "smux",
        "max_connections": 4,
        "min_streams": 4,
        "max_streams": streams,
    }
    return True


def _uses_xtls_vision(outbound: dict[str, Any]) -> bool:
    settings = outbound.get("settings")
    if not isinstance(settings, dict):
        return False
    vnext = settings.get("vnext")
    if not isinstance(vnext, list) or not vnext:
        return False
    users = vnext[0].get("users") if isinstance(vnext[0], dict) else None
    if not isinstance(users, list) or not users:
        return False
    user = users[0] if isinstance(users[0], dict) else {}
    return str(user.get("flow") or "").strip().lower() == "xtls-rprx-vision"


def _uses_singbox_xtls_vision(outbound: dict[str, Any]) -> bool:
    return str(outbound.get("flow") or "").strip().lower() == "xtls-rprx-vision"
