from __future__ import annotations

import json
import os
import threading
import time
from dataclasses import dataclass

from .subprocess_utils import CREATE_NO_WINDOW, result_output_text, run_text_pumped


@dataclass(slots=True)
class WindowsDefaultRouteContext:
    interface_alias: str
    dns_servers: tuple[str, ...] = ()
    interface_index: int = 0
    next_hop: str = ""
    is_physical: bool = False
    tun_active: bool = False


_ROUTE_CONTEXT_TTL_SECONDS = 30.0
_route_context_lock = threading.Lock()
_route_context_value: WindowsDefaultRouteContext | None = None
_route_context_cached_at = 0.0
_route_context_refresh: threading.Event | None = None


def invalidate_windows_default_route_context() -> None:
    global _route_context_cached_at
    with _route_context_lock:
        _route_context_cached_at = 0.0


def get_windows_default_route_context(
    *, force_refresh: bool = False
) -> WindowsDefaultRouteContext | None:
    if os.name != "nt":
        return None
    global _route_context_value, _route_context_cached_at, _route_context_refresh
    now = time.monotonic()
    with _route_context_lock:
        if not force_refresh and now - _route_context_cached_at < _ROUTE_CONTEXT_TTL_SECONDS:
            return _route_context_value
        refresh = _route_context_refresh
        owns_refresh = refresh is None
        if owns_refresh:
            refresh = threading.Event()
            _route_context_refresh = refresh
    if not owns_refresh:
        refresh.wait(6.5)
        with _route_context_lock:
            return _route_context_value

    try:
        value = _query_windows_default_route_context()
    except Exception:
        value = None
    finally:
        with _route_context_lock:
            _route_context_value = value
            _route_context_cached_at = time.monotonic()
            active_refresh = _route_context_refresh
            _route_context_refresh = None
            if active_refresh is not None:
                active_refresh.set()
    return value


def _query_windows_default_route_context() -> WindowsDefaultRouteContext | None:
    script = (
        "$routes = @(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue "
        "| Where-Object { $_.NextHop -and $_.NextHop -ne '0.0.0.0' } "
        "| Sort-Object RouteMetric, InterfaceMetric); "
        "$physical = $routes | Where-Object { "
        "$alias = [string]$_.InterfaceAlias; $alias -notmatch '(?i)lumen|xftun|singbox|wintun|tun' "
        "} | Select-Object -First 1; "
        "$route = $physical; "
        "if (-not $route) { $route = $routes | Select-Object -First 1 }; "
        "if (-not $route) { exit 1 }; "
        "$tunRoute = Get-NetRoute -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { "
        "$alias = [string]$_.InterfaceAlias; "
        "$alias -match '(?i)lumen|xftun|singbox|wintun|tun' -and "
        "$_.DestinationPrefix -in @('0.0.0.0/0', '0.0.0.0/1', '128.0.0.0/1') "
        "} | Select-Object -First 1; "
        "$dns = @(Get-DnsClientServerAddress -InterfaceIndex $route.InterfaceIndex -AddressFamily IPv4 "
        "-ErrorAction SilentlyContinue | ForEach-Object { $_.ServerAddresses } "
        "| Where-Object { $_ -match '^\\d{1,3}(\\.\\d{1,3}){3}$' }); "
        "@{ interface_alias = $route.InterfaceAlias; dns_servers = $dns; "
        "interface_index = [int]$route.InterfaceIndex; next_hop = [string]$route.NextHop; "
        "is_physical = [bool]$physical; tun_active = [bool]$tunRoute } | ConvertTo-Json -Compress"
    )
    try:
        result = run_text_pumped(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            timeout=6,
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        return None
    if result.returncode != 0:
        return None
    try:
        payload = json.loads(result_output_text(result) or "{}")
    except json.JSONDecodeError:
        return None
    interface_alias = str(payload.get("interface_alias") or "").strip()
    if not interface_alias:
        return None
    dns_raw = payload.get("dns_servers") or []
    if isinstance(dns_raw, str):
        dns_raw = [dns_raw]
    dns_servers = tuple(str(item).strip() for item in dns_raw if str(item).strip())
    return WindowsDefaultRouteContext(
        interface_alias=interface_alias,
        dns_servers=dns_servers,
        interface_index=int(payload.get("interface_index") or 0),
        next_hop=str(payload.get("next_hop") or "").strip(),
        is_physical=bool(payload.get("is_physical", False)),
        tun_active=bool(payload.get("tun_active", False)),
    )
