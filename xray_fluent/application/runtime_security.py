from __future__ import annotations

import secrets
import string
from typing import Any

from ..constants import PROXY_HOST


_PROXY_INBOUND_TYPES = {"socks", "http", "mixed"}
_LOCAL_LISTEN_HOSTS = {PROXY_HOST, "localhost", "::1"}
_XRAY_LOCAL_INBOUND_TAGS = {"api", "__app_metrics_api_in", "discord-socks-in"}
_SINGBOX_LOCAL_INBOUND_TAGS = {
    "__app_hybrid_protect_in",
    "__app_hybrid_relay_in",
    "discord-socks-in",
}


def generate_local_proxy_credentials(*, prefix: str = "local", password_length: int = 24) -> tuple[str, str]:
    suffix = secrets.token_hex(3)
    username = f"{prefix}-{suffix}"
    alphabet = string.ascii_letters + string.digits
    password = "".join(secrets.choice(alphabet) for _ in range(password_length))
    return username, password


def strip_xray_proxy_inbounds(payload: dict[str, Any], *, keep_tags: set[str] | None = None) -> int:
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return 0

    keep = {str(tag).strip() for tag in (keep_tags or set()) if str(tag).strip()}
    filtered: list[Any] = []
    removed = 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            filtered.append(inbound)
            continue
        tag = str(inbound.get("tag") or "").strip()
        protocol = str(inbound.get("protocol") or "").strip().lower()
        if tag in keep or protocol not in _PROXY_INBOUND_TYPES:
            filtered.append(inbound)
            continue
        removed += 1

    if removed:
        payload["inbounds"] = filtered
    return removed


def strip_singbox_proxy_inbounds(payload: dict[str, Any], *, keep_tags: set[str] | None = None) -> int:
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return 0

    keep = {str(tag).strip() for tag in (keep_tags or set()) if str(tag).strip()}
    filtered: list[Any] = []
    removed = 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            filtered.append(inbound)
            continue
        tag = str(inbound.get("tag") or "").strip()
        inbound_type = str(inbound.get("type") or inbound.get("protocol") or "").strip().lower()
        if tag in keep or inbound_type not in _PROXY_INBOUND_TYPES:
            filtered.append(inbound)
            continue
        removed += 1

    if removed:
        payload["inbounds"] = filtered
    return removed


def _is_loopback_listen(value: Any) -> bool:
    return str(value or "").strip().lower() in _LOCAL_LISTEN_HOSTS


def clamp_xray_local_inbounds(payload: dict[str, Any]) -> int:
    """Keep local proxy/control inbounds on loopback only.

    The public system proxy must remain no-auth for WinINET/Necko compatibility,
    so the meaningful safety boundary is never exposing those inbounds to LAN.
    """
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return 0

    changed = 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        tag = str(inbound.get("tag") or "").strip()
        protocol = str(inbound.get("protocol") or "").strip().lower()
        should_clamp = protocol in _PROXY_INBOUND_TYPES or tag in _XRAY_LOCAL_INBOUND_TAGS
        if not should_clamp:
            continue
        if not _is_loopback_listen(inbound.get("listen")):
            inbound["listen"] = PROXY_HOST
            changed += 1
        if tag in {"api", "__app_metrics_api_in"} or protocol == "dokodemo-door":
            settings = inbound.get("settings")
            if isinstance(settings, dict) and not _is_loopback_listen(settings.get("address")):
                settings["address"] = PROXY_HOST
                changed += 1
    return changed


def clamp_singbox_local_inbounds(payload: dict[str, Any]) -> int:
    """Keep sing-box local proxy/control inbounds on loopback only."""
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return 0

    changed = 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        tag = str(inbound.get("tag") or "").strip()
        inbound_type = str(inbound.get("type") or inbound.get("protocol") or "").strip().lower()
        should_clamp = inbound_type in _PROXY_INBOUND_TYPES or tag in _SINGBOX_LOCAL_INBOUND_TAGS
        if not should_clamp:
            continue
        if not _is_loopback_listen(inbound.get("listen")):
            inbound["listen"] = PROXY_HOST
            changed += 1
    return changed


def set_xray_socks_inbound_auth(
    payload: dict[str, Any],
    *,
    username: str,
    password: str,
    tag: str | None = None,
) -> bool:
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return False

    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        if str(inbound.get("protocol") or "").strip().lower() not in {"socks", "mixed"}:
            continue
        inbound_tag = str(inbound.get("tag") or "").strip()
        if tag is not None and inbound_tag != tag:
            continue
        settings = inbound.get("settings")
        if not isinstance(settings, dict):
            settings = {}
            inbound["settings"] = settings
        settings["auth"] = "password"
        settings["accounts"] = [{"user": username, "pass": password}]
        return True
    return False
