from __future__ import annotations

import zipfile

import pytest

from xray_fluent.core_resource_updater import _install_singbox_rule_sets
from xray_fluent.geodata_resources import SINGBOX_BINARY_RULE_SETS
from xray_fluent.models import RoutingSettings
from xray_fluent.routing_presets import (
    build_routing_preset,
    equivalent_regional_preset,
    preset_domain_rules,
    regional_routing_preset_ids,
)
from xray_fluent.routing_runtime import (
    _ensure_singbox_rule_sets,
    apply_singbox_gui_routing,
    build_singbox_gui_route_rules,
    build_xray_gui_routing_rules,
    split_xray_domain_ip,
)


class _TunSettings:
    tun_mode = True


def test_except_ru_keeps_custom_rules_out_of_system_storage() -> None:
    current = RoutingSettings(
        preset_id="blocked",
        direct_domains=["geosite:lumen-exclude", "2ip.ru"],
        proxy_domains=["geoip:ru-blocked", "example.com"],
    )

    routing = build_routing_preset(current, "except_ru")

    assert routing.preset_id == "except_ru"
    assert routing.direct_domains == ["2ip.ru"]
    assert routing.proxy_domains == ["example.com"]


@pytest.mark.parametrize("preset_id", ["global", "blocked", "except_ru"])
def test_quick_preset_keeps_application_routing(preset_id: str) -> None:
    process_rules = [
        {"process": "browser.exe", "action": "proxy"},
        {"process": "game.exe", "action": "direct"},
    ]
    process_preset_routes = {
        "telegram": "proxy",
        "torrents": "direct",
    }
    current = RoutingSettings(
        process_rules=process_rules,
        process_preset_routes=process_preset_routes,
    )

    routing = build_routing_preset(current, preset_id)

    assert routing.process_rules == process_rules
    assert routing.process_preset_routes == process_preset_routes


def test_except_ru_uses_full_xray_geodata_and_custom_rule_wins() -> None:
    routing = RoutingSettings(
        mode="global",
        preset_id="except_ru",
        proxy_domains=["vk.com"],
        tun_default_outbound="proxy",
    )

    rules = build_xray_gui_routing_rules(routing, _TunSettings())

    proxy_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("outboundTag") == "proxy" and "domain:vk.com" in rule.get("domain", [])
    )
    geosite_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("outboundTag") == "direct"
        and "geosite:ru-available-only-inside" in rule.get("domain", [])
    )
    assert proxy_index < geosite_index
    assert any(
        rule.get("outboundTag") == "direct" and "geoip:ru" in rule.get("ip", [])
        for rule in rules
    )
    assert split_xray_domain_ip(["geosite:category-ru"])[0] == ["geosite:category-ru"]


def test_blocked_preset_uses_full_rules_in_both_runtimes() -> None:
    routing = build_routing_preset(RoutingSettings(), "blocked")

    xray_rules = build_xray_gui_routing_rules(routing, _TunSettings())
    singbox_rules, singbox_sets = build_singbox_gui_route_rules(routing)

    assert any(
        "geosite:ru-blocked" in rule.get("domain", [])
        for rule in xray_rules
    )
    assert {
        "geosite-ru-blocked",
        "geoip-ru-blocked",
        "geoip-ru-blocked-community",
    }.issubset(singbox_sets)
    assert any(rule.get("outbound") == "proxy" for rule in singbox_rules)


def test_singbox_prefers_packaged_binary_rule_set(monkeypatch, tmp_path) -> None:
    import xray_fluent.geodata_resources as resources

    monkeypatch.setattr(resources, "SINGBOX_RULE_SET_DIR", tmp_path)
    path = tmp_path / "geosite-category-ru.srs"
    path.write_bytes(b"SRS" * 32)
    route: dict = {}

    _ensure_singbox_rule_sets(route, {"geosite-category-ru"})

    assert route["rule_set"] == [
        {
            "type": "local",
            "tag": "geosite-category-ru",
            "format": "binary",
            "path": str(path),
        }
    ]


def test_geodata_archive_installs_only_required_singbox_sets(tmp_path) -> None:
    archive = tmp_path / "sing-box.zip"
    with zipfile.ZipFile(archive, "w") as zip_file:
        for member in SINGBOX_BINARY_RULE_SETS.values():
            zip_file.writestr(member, (member.encode("utf-8") + b"-") * 8)

    target = tmp_path / "rules"
    _install_singbox_rule_sets(archive, target)

    assert {path.name for path in target.iterdir()} == {
        member.rsplit("/", 1)[-1] for member in SINGBOX_BINARY_RULE_SETS.values()
    }


def test_regional_quick_presets_match_v2rayn_profiles() -> None:
    assert regional_routing_preset_ids("russia") == ("global", "blocked", "except_ru")
    assert regional_routing_preset_ids("china") == ("global", "blocked_cn", "except_cn")
    assert regional_routing_preset_ids("iran") == ("global", "except_ir")
    assert equivalent_regional_preset("except_ru", "china") == "except_cn"
    assert equivalent_regional_preset("blocked", "iran") == "global"


def test_china_and_iran_presets_use_regional_geodata() -> None:
    china_direct, china_proxy = preset_domain_rules("blocked_cn")
    assert china_direct == []
    assert {"geosite:gfw", "geosite:greatfire", "geoip:telegram"}.issubset(china_proxy)
    assert preset_domain_rules("except_cn") == (["geosite:cn", "geoip:cn"], [])
    assert preset_domain_rules("except_ir") == (["geosite:ir", "geoip:ir"], [])


@pytest.mark.parametrize(
    ("preset_id", "expected_outbound", "expected_final", "expected_rules"),
    [
        (
            "blocked_cn",
            "proxy",
            "direct",
            {
                "geosite:gfw",
                "geosite:greatfire",
                "geosite:google",
                "geoip:facebook",
                "geoip:fastly",
                "geoip:google",
                "geoip:netflix",
                "geoip:telegram",
                "geoip:twitter",
            },
        ),
        ("except_cn", "direct", "proxy", {"geosite:cn", "geoip:cn"}),
        ("except_ir", "direct", "proxy", {"geosite:ir", "geoip:ir"}),
    ],
)
def test_regional_presets_render_the_intended_routes_for_both_cores(
    monkeypatch,
    preset_id: str,
    expected_outbound: str,
    expected_final: str,
    expected_rules: set[str],
) -> None:
    routing = build_routing_preset(RoutingSettings(), preset_id)

    xray_rules = build_xray_gui_routing_rules(routing, _TunSettings())
    xray_rendered = {
        *(
            value
            for rule in xray_rules
            if rule.get("outboundTag") == expected_outbound
            for value in rule.get("domain", [])
        ),
        *(
            value
            for rule in xray_rules
            if rule.get("outboundTag") == expected_outbound
            for value in rule.get("ip", [])
        ),
    }
    assert expected_rules.issubset(xray_rendered)
    assert xray_rules[-1] == {
        "type": "field",
        "network": "tcp,udp",
        "outboundTag": expected_final,
    }

    # Rule-set installation is tested separately. Here the generated routing
    # contract is verified without depending on machine-local downloaded data.
    monkeypatch.setattr(
        "xray_fluent.routing_runtime._ensure_singbox_rule_sets",
        lambda _route, _tags: None,
    )
    payload = {"route": {"rules": []}}
    apply_singbox_gui_routing(payload, routing)
    expected_tags = {value.replace(":", "-") for value in expected_rules}
    matching_tags = {
        tag
        for rule in payload["route"]["rules"]
        if rule.get("outbound") == expected_outbound
        for tag in rule.get("rule_set", [])
    }
    assert expected_tags.issubset(matching_tags)
    assert payload["route"]["final"] == expected_final
