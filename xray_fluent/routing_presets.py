from __future__ import annotations

from dataclasses import replace

from .models import RoutingSettings
from .service_presets import SERVICE_PRESETS

ROUTING_PRESET_GLOBAL = "global"
ROUTING_PRESET_BLOCKED = "blocked"
ROUTING_PRESET_EXCEPT_RU = "except_ru"

RU_DIRECT_RULES = (
    "geosite:category-ru",
    "geoip:ru",
)

BLOCKED_PROXY_RULES = (
    "geosite:category-media-ru-blocked",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
)

BLOCKED_DIRECT_RULES = (
    "geosite:lumen-exclude",
    "geoip:lumen-exclude",
)


def proxy_default_services() -> dict[str, str]:
    return {
        preset.id: preset.default_action
        for preset in SERVICE_PRESETS
        if preset.default_action == "proxy"
    }


def build_routing_preset(current: RoutingSettings, preset_id: str) -> RoutingSettings:
    if preset_id == ROUTING_PRESET_GLOBAL:
        return replace(
            current,
            mode="global",
            direct_domains=[],
            proxy_domains=[],
            block_domains=[],
            process_rules=[],
            process_preset_routes={},
            service_routes={},
            tun_default_outbound="proxy",
        )

    if preset_id == ROUTING_PRESET_BLOCKED:
        return replace(
            current,
            mode="rule",
            direct_domains=list(BLOCKED_DIRECT_RULES),
            proxy_domains=list(BLOCKED_PROXY_RULES),
            block_domains=[],
            process_rules=[],
            process_preset_routes={},
            service_routes=proxy_default_services(),
            tun_default_outbound="direct",
        )

    if preset_id == ROUTING_PRESET_EXCEPT_RU:
        return replace(
            current,
            mode="global",
            direct_domains=list(RU_DIRECT_RULES),
            proxy_domains=[],
            block_domains=[],
            process_rules=[],
            process_preset_routes={},
            service_routes={},
            tun_default_outbound="proxy",
        )

    return current
