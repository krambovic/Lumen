from __future__ import annotations

import urllib.request

import xray_fluent.direct_http as direct_http


class _FakeRoute:
    def __init__(self) -> None:
        self.entered = False
        self.exited = False

    def __enter__(self):
        self.entered = True
        return self

    def __exit__(self, *_args):
        self.exited = True

    def resolve(self, host: str) -> str:
        return host


def test_direct_opener_disables_all_urllib_proxies(monkeypatch) -> None:
    route = _FakeRoute()
    monkeypatch.setattr(direct_http, "WindowsDirectRoute", lambda: route)

    with direct_http.DirectUrlOpener() as opener:
        proxy_handlers = [item for item in opener.handlers if isinstance(item, urllib.request.ProxyHandler)]
        # Passing ProxyHandler({}) suppresses urllib's implicit environment /
        # Windows proxy handler; urllib omits the empty handler itself.
        assert proxy_handlers == []
        assert any(isinstance(item, direct_http._DirectHTTPHandler) for item in opener.handlers)
        assert any(isinstance(item, direct_http._DirectHTTPSHandler) for item in opener.handlers)

    assert route.entered is True
    assert route.exited is True


def test_direct_route_fails_closed_without_physical_interface(monkeypatch) -> None:
    monkeypatch.setattr(direct_http.os, "name", "nt")
    calls: list[bool] = []

    def missing_context(*, force_refresh: bool = False):
        calls.append(force_refresh)
        return None

    monkeypatch.setattr(direct_http, "get_windows_default_route_context", missing_context)

    route = direct_http.WindowsDirectRoute()
    try:
        route.__enter__()
    except direct_http.DirectNetworkUnavailable:
        pass
    else:
        raise AssertionError("direct-only request must not fall back to a TUN route")
    assert calls == [False, True]


def test_direct_route_retries_transient_context_failure(monkeypatch) -> None:
    from xray_fluent.network_route_context import WindowsDefaultRouteContext

    calls: list[bool] = []

    def transient_context(*, force_refresh: bool = False):
        calls.append(force_refresh)
        if not force_refresh:
            return None
        return WindowsDefaultRouteContext(
            "Ethernet",
            ("1.1.1.1",),
            interface_index=13,
            next_hop="192.168.1.1",
            is_physical=True,
            tun_active=False,
        )

    monkeypatch.setattr(direct_http.os, "name", "nt")
    monkeypatch.setattr(direct_http, "get_windows_default_route_context", transient_context)

    route = direct_http.WindowsDirectRoute()
    route.__enter__()
    assert calls == [False, True]
    route.__exit__(None, None, None)


def test_direct_route_needs_no_host_route_when_tun_is_inactive(monkeypatch) -> None:
    from xray_fluent.network_route_context import WindowsDefaultRouteContext

    monkeypatch.setattr(direct_http.os, "name", "nt")
    monkeypatch.setattr(
        direct_http,
        "get_windows_default_route_context",
        lambda: WindowsDefaultRouteContext(
            "Ethernet",
            ("1.1.1.1",),
            interface_index=13,
            next_hop="192.168.1.1",
            is_physical=True,
            tun_active=False,
        ),
    )
    route = direct_http.WindowsDirectRoute()
    route.__enter__()
    assert route.resolve("subscription.example") == "subscription.example"
    route.__exit__(None, None, None)
