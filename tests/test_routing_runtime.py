from __future__ import annotations

from xray_fluent.models import RoutingSettings
from xray_fluent.routing_runtime import (
    build_singbox_gui_dns_rules,
    build_singbox_gui_route_rules,
    build_xray_gui_routing_rules,
)


class _Settings:
    tun_mode = True


def _contains_youtube_direct_singbox(rule: dict) -> bool:
    return (
        rule.get("outbound") == "direct"
        and "youtube.com" in [str(item) for item in rule.get("domain_suffix") or []]
    )


def _contains_youtube_direct_xray(rule: dict) -> bool:
    return (
        rule.get("outboundTag") == "direct"
        and "domain:youtube.com" in [str(item) for item in rule.get("domain") or []]
    )


def test_service_direct_overrides_global_proxy_for_singbox() -> None:
    routing = RoutingSettings(
        mode="global",
        service_routes={"youtube": "direct"},
        tun_default_outbound="proxy",
    )

    rules, _ = build_singbox_gui_route_rules(routing)
    dns_rules, _ = build_singbox_gui_dns_rules(routing)

    assert any(_contains_youtube_direct_singbox(rule) for rule in rules)
    assert any(
        rule.get("server") == "bootstrap-dns"
        and "youtube.com" in [str(item) for item in rule.get("domain_suffix") or []]
        for rule in dns_rules
    )


def test_service_direct_overrides_global_proxy_for_xray() -> None:
    routing = RoutingSettings(
        mode="global",
        service_routes={"youtube": "direct"},
        tun_default_outbound="proxy",
    )

    rules = build_xray_gui_routing_rules(routing, _Settings())

    assert any(_contains_youtube_direct_xray(rule) for rule in rules)
    assert rules[-1].get("outboundTag") == "proxy"


def test_service_off_does_not_generate_proxy_override() -> None:
    routing = RoutingSettings(
        mode="global",
        service_routes={"youtube": "off", "discord": "", "telegram": "block"},
        tun_default_outbound="proxy",
    )

    rules, _ = build_singbox_gui_route_rules(routing)

    assert not any(
        "youtube.com" in [str(item) for item in rule.get("domain_suffix") or []]
        for rule in rules
    )
    assert not any(
        "telegram.org" in [str(item) for item in rule.get("domain_suffix") or []]
        for rule in rules
    )
