from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.constants import SUBSCRIPTION_FETCHER_EXE_NAME
from xray_fluent.models import RoutingSettings
from xray_fluent.routing_presets import repair_builtin_preset_service_routes
from xray_fluent.routing_runtime import (
    apply_singbox_gui_routing,
    build_singbox_gui_dns_rules,
    build_singbox_gui_route_rules,
    build_xray_gui_routing_rules,
    effective_service_action,
    routing_with_ip_preference,
    service_route_selection,
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


def test_explicit_private_ip_route_wins_over_lan_bypass_in_both_cores() -> None:
    routing = RoutingSettings(
        mode="global",
        preset_id="global",
        proxy_domains=["10.0.0.1/32"],
        bypass_lan=True,
    )

    xray_rules = build_xray_gui_routing_rules(routing, _Settings())
    xray_proxy = next(
        index for index, rule in enumerate(xray_rules)
        if rule.get("outboundTag") == "proxy" and "10.0.0.1/32" in rule.get("ip", [])
    )
    xray_private = next(
        index for index, rule in enumerate(xray_rules)
        if rule.get("outboundTag") == "direct" and "geoip:private" in rule.get("ip", [])
    )

    singbox_rules, _ = build_singbox_gui_route_rules(routing)
    singbox_proxy = next(
        index for index, rule in enumerate(singbox_rules)
        if rule.get("outbound") == "proxy" and "10.0.0.1/32" in rule.get("ip_cidr", [])
    )
    singbox_private = next(
        index for index, rule in enumerate(singbox_rules)
        if rule.get("outbound") == "direct" and rule.get("ip_is_private") is True
    )

    assert xray_proxy < xray_private
    assert singbox_proxy < singbox_private


def test_apply_singbox_gui_routing_preserves_imported_route_and_dns_rules() -> None:
    imported_route = {
        "domain_suffix": ["user.example"],
        "action": "route",
        "outbound": "direct",
    }
    imported_dns = {
        "domain_suffix": ["user.example"],
        "action": "route",
        "server": "bootstrap-dns",
    }
    payload = {
        "route": {"rules": [imported_route.copy()]},
        "dns": {
            "servers": [
                {"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"},
                {"tag": "proxy-dns", "type": "https", "server": "dns.google"},
            ],
            "rules": [imported_dns.copy()],
        },
    }
    routing = RoutingSettings(mode="global", preset_id="global")

    apply_singbox_gui_routing(payload, routing)
    apply_singbox_gui_routing(payload, routing)

    assert payload["route"]["rules"].count(imported_route) == 1
    assert payload["dns"]["rules"].count(imported_dns) == 1


def test_apply_singbox_gui_routing_replaces_previous_generated_domain_rules() -> None:
    payload = {
        "route": {"rules": [], "final": "proxy"},
        "dns": {
            "servers": [
                {"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"},
                {"tag": "proxy-dns", "type": "https", "server": "dns.google"},
            ],
            "rules": [],
        },
    }

    apply_singbox_gui_routing(payload, RoutingSettings(proxy_domains=["old.example"]))
    apply_singbox_gui_routing(payload, RoutingSettings(proxy_domains=["new.example"]))

    route_text = str(payload["route"]["rules"])
    dns_text = str(payload["dns"]["rules"])
    assert "old.example" not in route_text
    assert "old.example" not in dns_text
    assert "new.example" in route_text
    assert "new.example" in dns_text


def test_custom_rule_routing_honors_direct_tun_default_setting() -> None:
    payload = {"route": {"rules": [], "final": "direct"}}
    routing = RoutingSettings(
        mode="rule",
        preset_id="custom-user-preset",
        tun_default_outbound="direct",
    )

    apply_singbox_gui_routing(payload, routing)

    assert payload["route"]["final"] == "direct"


def test_custom_rule_routing_honors_proxy_tun_default_setting() -> None:
    payload = {"route": {"rules": [], "final": "direct"}}

    apply_singbox_gui_routing(
        payload,
        RoutingSettings(
            mode="rule",
            preset_id="custom-user-preset",
            tun_default_outbound="proxy",
        ),
    )

    assert payload["route"]["final"] == "proxy"


def test_routing_settings_restore_tun_default_and_migrate_legacy_blocked_state() -> None:
    restored = RoutingSettings.from_dict(
        {
            "mode": "rule",
            "preset_id": "custom-user-preset",
            "tun_default_outbound": "direct",
            "tun_default_outbound_explicit": True,
        }
    )
    legacy_blocked = RoutingSettings.from_dict({"mode": "rule", "preset_id": "blocked"})
    legacy_custom = RoutingSettings.from_dict(
        {"mode": "rule", "preset_id": "custom-user-preset", "tun_default_outbound": "direct"}
    )
    legacy_blocked_with_stale_proxy = RoutingSettings.from_dict(
        {"mode": "rule", "preset_id": "blocked", "tun_default_outbound": "proxy"}
    )

    assert restored.tun_default_outbound == "direct"
    assert restored.to_dict()["tun_default_outbound"] == "direct"
    assert restored.to_dict()["tun_default_outbound_explicit"] is True
    assert legacy_blocked.tun_default_outbound == "direct"
    assert legacy_custom.tun_default_outbound == "proxy"
    assert legacy_blocked_with_stale_proxy.tun_default_outbound == "direct"


def test_blocked_preset_migrates_old_serialized_proxy_fallback_unless_user_overrode_it() -> None:
    old_default = RoutingSettings.from_dict(
        {
            "mode": "rule",
            "preset_id": "blocked",
            "tun_default_outbound": "proxy",
            # 1.9.16 wrote this as true on every save, including untouched defaults.
            "tun_default_outbound_explicit": True,
        }
    )
    explicit_override = RoutingSettings.from_dict(
        {
            "mode": "rule",
            "preset_id": "blocked",
            "tun_default_outbound": "proxy",
            "tun_default_outbound_user_selected_v2": True,
        }
    )

    assert old_default.tun_default_outbound == "direct"
    assert old_default.tun_default_outbound_user_selected is False
    assert explicit_override.tun_default_outbound == "proxy"
    assert explicit_override.to_dict()["tun_default_outbound_user_selected_v2"] is True


def test_default_blocked_tun_routes_unmatched_traffic_direct() -> None:
    routing = repair_builtin_preset_service_routes(RoutingSettings())
    payload = {"route": {"rules": [], "final": "proxy"}}

    apply_singbox_gui_routing(payload, routing)

    assert routing.preset_id == "blocked"
    assert routing.tun_default_outbound == "direct"
    assert payload["route"]["final"] == "direct"


def test_unmatched_tun_fallback_does_not_turn_inherited_services_into_overrides() -> None:
    routing = RoutingSettings(
        preset_id="custom",
        service_routes={"youtube": "proxy"},
        tun_default_outbound="direct",
    )

    assert service_route_selection(routing, "youtube") == "proxy"
    assert service_route_selection(routing, "spotify") == "default"
    assert service_route_selection(
        RoutingSettings(
            preset_id="custom",
            service_routes={"youtube": "proxy"},
            tun_default_outbound="proxy",
        ),
        "spotify",
    ) == "default"
    assert effective_service_action(routing, "youtube") == "proxy"
    assert effective_service_action(routing, "spotify") == "direct"
    assert effective_service_action(
        RoutingSettings(preset_id="custom", tun_default_outbound="proxy"),
        "spotify",
    ) == "proxy"

    direct_rules, _ = build_singbox_gui_route_rules(routing)
    proxy_fallback = RoutingSettings(
        preset_id="custom",
        service_routes={"youtube": "proxy"},
        tun_default_outbound="proxy",
    )
    proxy_rules, _ = build_singbox_gui_route_rules(proxy_fallback)
    youtube_rule = lambda rules: next(
        rule
        for rule in rules
        if "youtube.com" in [str(item) for item in rule.get("domain_suffix") or []]
    )
    assert youtube_rule(direct_rules)["outbound"] == "proxy"
    assert youtube_rule(proxy_rules)["outbound"] == "proxy"


def test_service_list_shows_effective_routes_without_creating_overrides() -> None:
    from xray_fluent.qml_app.bridge.app_bridge import AppBridge

    routing = RoutingSettings(preset_id="custom", service_routes={"youtube": "proxy"}, tun_default_outbound="direct")
    bridge = SimpleNamespace(controller=SimpleNamespace(state=SimpleNamespace(routing=routing)))
    rows = AppBridge.serviceList.fget(bridge)
    by_id = {row["id"]: row for row in rows}

    assert by_id["youtube"]["action"] == "proxy"
    assert by_id["discord"]["action"] == "direct"
    assert all(row["action"] in {"direct", "proxy"} for row in rows)
    assert routing.service_routes == {"youtube": "proxy"}


def test_changing_tun_fallback_changes_only_route_final_not_matching_rules() -> None:
    routes_by_fallback = []
    finals = []
    for fallback in ("direct", "proxy"):
        routing = RoutingSettings(
            preset_id="blocked",
            service_routes={},
            tun_default_outbound=fallback,
            tun_default_outbound_user_selected=True,
        )
        payload = {"route": {"rules": []}}
        apply_singbox_gui_routing(payload, routing)
        routes_by_fallback.append(payload["route"]["rules"])
        finals.append(payload["route"]["final"])

    assert finals == ["direct", "proxy"]
    assert routes_by_fallback[0] == routes_by_fallback[1]


def test_blocked_preset_inherited_service_rules_survive_removed_overrides() -> None:
    routing = RoutingSettings(
        preset_id="blocked",
        service_routes={},
        tun_default_outbound="direct",
    )

    rules, _ = build_singbox_gui_route_rules(routing)

    assert any(
        rule.get("outbound") == "proxy"
        and "youtube.com" in [str(item) for item in rule.get("domain_suffix") or []]
        for rule in rules
    )


def test_subscription_fetcher_is_always_routed_direct_before_user_process_rules() -> None:
    payload = {"route": {"rules": [], "final": "proxy"}}
    routing = RoutingSettings(
        process_rules=[
            {"process": SUBSCRIPTION_FETCHER_EXE_NAME, "action": "proxy"},
        ],
    )

    apply_singbox_gui_routing(payload, routing)
    apply_singbox_gui_routing(payload, routing)

    rules = payload["route"]["rules"]
    helper_rules = [
        rule
        for rule in rules
        if SUBSCRIPTION_FETCHER_EXE_NAME in rule.get("process_name", [])
    ]
    assert helper_rules[0]["outbound"] == "direct"
    assert helper_rules[1]["outbound"] == "proxy"
    assert len([rule for rule in helper_rules if rule.get("outbound") == "direct"]) == 1


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
