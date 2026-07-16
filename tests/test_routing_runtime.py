from __future__ import annotations

from xray_fluent.models import RoutingSettings
from xray_fluent.routing_runtime import (
    apply_singbox_gui_routing,
    build_singbox_gui_dns_rules,
    build_singbox_gui_route_rules,
    build_xray_gui_routing_rules,
    routing_with_ip_preference,
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


def test_fake_dns_ranges_do_not_fall_through_to_lan_bypass() -> None:
    routing = RoutingSettings(
        mode="global",
        dns_fake_enabled=True,
        bypass_lan=True,
        tun_default_outbound="proxy",
    )

    rules, _ = build_singbox_gui_route_rules(routing)

    fake_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("ip_cidr") == ["198.18.0.0/15", "fc00::/18"]
    )
    private_index = next(index for index, rule in enumerate(rules) if rule.get("ip_is_private") is True)
    assert fake_index < private_index
    assert rules[fake_index]["outbound"] == "proxy"


def test_domain_rules_stay_ahead_of_fake_dns_fallback() -> None:
    routing = RoutingSettings(
        mode="rule",
        dns_fake_enabled=True,
        proxy_domains=["chatgpt.com"],
        tun_default_outbound="direct",
    )

    rules, _ = build_singbox_gui_route_rules(routing)

    domain_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("outbound") == "proxy" and "chatgpt.com" in rule.get("domain_suffix", [])
    )
    fake_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("ip_cidr") == ["198.18.0.0/15", "fc00::/18"]
    )
    assert domain_index < fake_index
    assert rules[fake_index]["outbound"] == "direct"


def test_custom_rule_routing_falls_back_to_proxy_without_tun_default_setting() -> None:
    payload = {"route": {"rules": [], "final": "direct"}}
    routing = RoutingSettings(
        mode="rule",
        preset_id="custom-user-preset",
        # A legacy saved value must no longer control the hidden fallback.
        tun_default_outbound="direct",
    )

    apply_singbox_gui_routing(payload, routing)

    assert payload["route"]["final"] == "proxy"


def test_apply_singbox_gui_routing_keeps_browser_doh_reject_before_fake_dns() -> None:
    payload = {
        "route": {"rules": [], "final": "proxy"},
        "dns": {
            "servers": [
                {"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"},
                {"tag": "proxy-dns", "type": "https", "server": "dns.google"},
                {"tag": "fake-dns", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
            ],
            "rules": [],
        },
    }

    apply_singbox_gui_routing(payload, RoutingSettings(mode="global", tun_default_outbound="proxy"))

    dns_rules = payload["dns"]["rules"]
    https_index = next(index for index, rule in enumerate(dns_rules) if rule.get("query_type") == ["HTTPS", "SVCB"])
    doh_index = next(
        index
        for index, rule in enumerate(dns_rules)
        if rule.get("action") == "reject"
        and "dns.google" in [str(item) for item in rule.get("domain_suffix") or []]
    )
    fake_index = next(index for index, rule in enumerate(dns_rules) if rule.get("server") == "fake-dns")

    assert https_index < doh_index < fake_index


def test_runtime_ip_preference_does_not_force_strict_strategies() -> None:
    routing = RoutingSettings(
        dns_bootstrap_strategy="ipv4_only",
        dns_proxy_strategy="prefer_ipv4",
    )

    effective = routing_with_ip_preference(routing, prefer_ipv6=True)

    assert effective is not routing
    assert routing.dns_proxy_strategy == "prefer_ipv4"
    assert effective.dns_bootstrap_strategy == "ipv4_only"
    assert effective.dns_proxy_strategy == "prefer_ipv6"


def test_dns_geo_check_controls_only_preset_dns_rules() -> None:
    disabled = RoutingSettings(
        preset_id="blocked",
        dns_geo_check=False,
        proxy_domains=["example.com"],
    )
    enabled = RoutingSettings(
        preset_id="blocked",
        dns_geo_check=True,
        proxy_domains=["example.com"],
    )

    disabled_rules, disabled_sets = build_singbox_gui_dns_rules(disabled)
    enabled_rules, enabled_sets = build_singbox_gui_dns_rules(enabled)

    assert any("example.com" in rule.get("domain_suffix", []) for rule in disabled_rules)
    assert not disabled_sets
    assert "geosite-ru-blocked" in enabled_sets
    assert len(enabled_rules) > len(disabled_rules)
