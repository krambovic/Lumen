from __future__ import annotations

from xray_fluent.service_presets import SERVICE_PRESETS


def test_max_ru_is_not_a_routing_service() -> None:
    assert all(preset.id != "maxru" for preset in SERVICE_PRESETS)
