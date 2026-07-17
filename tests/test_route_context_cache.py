from __future__ import annotations

import xray_fluent.network_route_context as route_manager


def test_default_route_context_is_cached(monkeypatch) -> None:
    calls = 0
    expected = route_manager.WindowsDefaultRouteContext("Ethernet", ("1.1.1.1",))

    def fake_query():
        nonlocal calls
        calls += 1
        return expected

    monkeypatch.setattr(route_manager.os, "name", "nt")
    monkeypatch.setattr(route_manager, "_query_windows_default_route_context", fake_query)
    route_manager.invalidate_windows_default_route_context()

    assert route_manager.get_windows_default_route_context() == expected
    assert route_manager.get_windows_default_route_context() == expected
    assert calls == 1
    route_manager.invalidate_windows_default_route_context()


def test_failed_default_route_context_is_not_cached(monkeypatch) -> None:
    calls = 0
    expected = route_manager.WindowsDefaultRouteContext("Ethernet", ("1.1.1.1",))

    def fake_query():
        nonlocal calls
        calls += 1
        return None if calls == 1 else expected

    monkeypatch.setattr(route_manager.os, "name", "nt")
    monkeypatch.setattr(route_manager, "_query_windows_default_route_context", fake_query)
    route_manager.invalidate_windows_default_route_context()

    assert route_manager.get_windows_default_route_context(force_refresh=True) is None
    assert route_manager.get_windows_default_route_context() == expected
    assert calls == 2
    route_manager.invalidate_windows_default_route_context()
