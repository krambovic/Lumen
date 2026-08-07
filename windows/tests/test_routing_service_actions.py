from __future__ import annotations

from xray_fluent.models import RoutingSettings
from xray_fluent.routing_presets import build_routing_preset
from xray_fluent.routing_runtime import effective_service_action


def test_except_ru_services_display_inherited_proxy_route() -> None:
    routing = build_routing_preset(RoutingSettings(), "except_ru")

    # Steam's catalog recommendation is "direct", but except_ru has no
    # explicit service override and sends unmatched traffic through the proxy.
    assert routing.service_routes == {}
    assert effective_service_action(routing, "steam") == "proxy"


def test_explicit_service_route_overrides_preset_fallback() -> None:
    routing = build_routing_preset(RoutingSettings(), "except_ru")
    routing.service_routes = {"steam": "direct"}

    assert effective_service_action(routing, "steam") == "direct"


def test_blocked_preset_displays_its_direct_fallback() -> None:
    routing = build_routing_preset(RoutingSettings(), "blocked")

    assert effective_service_action(routing, "steam") == "direct"
    assert effective_service_action(routing, "youtube") == "proxy"
