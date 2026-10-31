from __future__ import annotations

from xray_fluent.service_presets import SERVICE_PRESETS


def test_max_ru_is_not_a_routing_service() -> None:
    assert all(preset.id != "maxru" for preset in SERVICE_PRESETS)


def test_epic_and_anticheat_domains_go_direct_together() -> None:
    owners = {
        domain: preset
        for preset in SERVICE_PRESETS
        for domain in preset.domains
        if domain in {"epicgames.com", "epicgames.dev", "easyanticheat.net", "battleye.com"}
    }
    assert {preset.id for preset in owners.values()} == {"epic_fortnite"}
    assert owners["epicgames.com"].default_action == "direct"
