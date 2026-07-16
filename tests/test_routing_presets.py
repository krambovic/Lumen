from __future__ import annotations

import zipfile

import pytest

from xray_fluent.core_resource_updater import _install_singbox_rule_sets
from xray_fluent.geodata_resources import SINGBOX_BINARY_RULE_SETS
from xray_fluent.models import RoutingSettings
from xray_fluent.routing_presets import build_routing_preset
from xray_fluent.routing_runtime import (
    _ensure_singbox_rule_sets,
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
        and "geosite:category-ru" in rule.get("domain", [])
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
