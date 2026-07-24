from __future__ import annotations

from typing import Any

from .constants import DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT, PROXY_HOST


def build_xray_sniffing(*, route_only: bool = False) -> dict[str, Any]:
    return {
        "enabled": True,
        "destOverride": ["http", "tls"],
        "routeOnly": bool(route_only),
    }


def build_xray_mixed_inbound(
    port: int = DEFAULT_SOCKS_PORT,
    *,
    tag: str = "socks-in",
    route_only: bool = False,
) -> dict[str, Any]:
    return {
        "tag": tag,
        "listen": PROXY_HOST,
        "port": int(port),
        "protocol": "mixed",
        "settings": {
            "auth": "noauth",
            "udp": True,
            "allowTransparent": False,
        },
        "sniffing": build_xray_sniffing(route_only=route_only),
    }


def build_xray_http_compat_inbound(
    port: int = DEFAULT_HTTP_PORT,
    *,
    tag: str = "http-in",
    route_only: bool = False,
) -> dict[str, Any]:
    return {
        "tag": tag,
        "listen": PROXY_HOST,
        "port": int(port),
        "protocol": "http",
        "settings": {},
        "sniffing": build_xray_sniffing(route_only=route_only),
    }


def normalize_xray_sniffing(payload: dict[str, Any], *, route_only: bool = False) -> int:
    changed = 0
    want_route_only = bool(route_only)
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        protocol = str(inbound.get("protocol") or "").strip().lower()
        if protocol not in {"mixed", "socks", "http", "tun"}:
            continue
        sniffing = inbound.get("sniffing")
        if not isinstance(sniffing, dict):
            inbound["sniffing"] = build_xray_sniffing(route_only=want_route_only)
            changed += 1
            continue
        if sniffing.get("enabled") is not True:
            sniffing["enabled"] = True
            changed += 1
        if sniffing.get("destOverride") != ["http", "tls"]:
            sniffing["destOverride"] = ["http", "tls"]
            changed += 1
        if sniffing.get("routeOnly") is not want_route_only:
            sniffing["routeOnly"] = want_route_only
            changed += 1
    return changed


def ensure_xray_mixed_proxy_inbound(
    payload: dict[str, Any],
    *,
    socks_port: int = DEFAULT_SOCKS_PORT,
    http_port: int = DEFAULT_HTTP_PORT,
    route_only: bool = False,
) -> int:
    inbounds = payload.setdefault("inbounds", [])
    if not isinstance(inbounds, list):
        inbounds = []
        payload["inbounds"] = inbounds

    changed = normalize_xray_sniffing(payload, route_only=route_only)
    main_index = -1
    http_index = -1

    for index, inbound in enumerate(inbounds):
        if not isinstance(inbound, dict):
            continue
        tag = str(inbound.get("tag") or "").strip()
        protocol = str(inbound.get("protocol") or "").strip().lower()
        try:
            port = int(inbound.get("port") or 0)
        except (TypeError, ValueError):
            port = 0
        if tag in {"socks-in", "mixed-in"} or (port == int(socks_port) and protocol in {"socks", "mixed"}):
            main_index = index
        elif tag == "http-in" or (port == int(http_port) and protocol == "http"):
            http_index = index

    main_inbound = build_xray_mixed_inbound(socks_port, route_only=route_only)
    if main_index >= 0 and isinstance(inbounds[main_index], dict):
        current = inbounds[main_index]
        for key, value in main_inbound.items():
            if current.get(key) != value:
                current[key] = value
                changed += 1
    else:
        inbounds.insert(0, main_inbound)
        changed += 1

    if int(http_port) != int(socks_port):
        http_inbound = build_xray_http_compat_inbound(http_port, route_only=route_only)
        if http_index >= 0 and isinstance(inbounds[http_index], dict):
            current = inbounds[http_index]
            for key, value in http_inbound.items():
                if current.get(key) != value:
                    current[key] = value
                    changed += 1
        else:
            insert_at = 1 if inbounds else 0
            inbounds.insert(insert_at, http_inbound)
            changed += 1

    return changed
