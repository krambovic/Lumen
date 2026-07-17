from __future__ import annotations

import os
import threading
import time
from dataclasses import dataclass

from .windows_network import query_windows_route_context


@dataclass(slots=True)
class WindowsDefaultRouteContext:
    interface_alias: str
    dns_servers: tuple[str, ...] = ()
    interface_index: int = 0
    next_hop: str = ""
    is_physical: bool = False
    tun_active: bool = False


_ROUTE_CONTEXT_TTL_SECONDS = 30.0
_ROUTE_CONTEXT_WAIT_TIMEOUT_SECONDS = 3.0
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
        refresh.wait(_ROUTE_CONTEXT_WAIT_TIMEOUT_SECONDS)
        with _route_context_lock:
            return _route_context_value

    try:
        value = _query_windows_default_route_context()
    except Exception:
        value = None
    finally:
        with _route_context_lock:
            _route_context_value = value
            # A transient Windows network-table failure must not block every
            # subscription in an "update all" batch for the full cache TTL.
            _route_context_cached_at = time.monotonic() if value is not None else 0.0
            active_refresh = _route_context_refresh
            _route_context_refresh = None
            if active_refresh is not None:
                active_refresh.set()
    return value


def _query_windows_default_route_context() -> WindowsDefaultRouteContext | None:
    native = query_windows_route_context()
    if native is None:
        return None
    return WindowsDefaultRouteContext(
        interface_alias=native.interface_alias,
        dns_servers=native.dns_servers,
        interface_index=native.interface_index,
        next_hop=native.next_hop,
        is_physical=True,
        tun_active=native.tun_active,
    )
