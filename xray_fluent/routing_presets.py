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
    "geosite:ru-blocked",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
)

BLOCKED_DIRECT_RULES = (
    "geosite:lumen-exclude",
    "geoip:lumen-exclude",
)

def custom_domain_rules(items: list[str]) -> list[str]:
    """Return only rules explicitly owned by the user."""
    return [
        item
        for item in items
        if not str(item).strip().lower().startswith(("geosite:", "geoip:"))
    ]


def preset_domain_rules(preset_id: str) -> tuple[list[str], list[str]]:
    """Return internal direct/proxy rules for a built-in preset."""
    if preset_id == ROUTING_PRESET_BLOCKED:
        return list(BLOCKED_DIRECT_RULES), list(BLOCKED_PROXY_RULES)
    if preset_id == ROUTING_PRESET_EXCEPT_RU:
        return list(RU_DIRECT_RULES), []
    return [], []


def proxy_default_services() -> dict[str, str]:
    return {
        preset.id: preset.default_action
        for preset in SERVICE_PRESETS
        if preset.default_action == "proxy"
    }


def build_routing_preset(current: RoutingSettings, preset_id: str) -> RoutingSettings:
    direct_domains = custom_domain_rules(current.direct_domains)
    proxy_domains = custom_domain_rules(current.proxy_domains)
    block_domains = custom_domain_rules(current.block_domains)
    if preset_id == ROUTING_PRESET_GLOBAL:
        return replace(
            current,
            preset_id=preset_id,
            mode="global",
            direct_domains=direct_domains,
            proxy_domains=proxy_domains,
            block_domains=block_domains,
            process_rules=[],
            process_preset_routes={},
            service_routes={},
            tun_default_outbound="proxy",
        )

    if preset_id == ROUTING_PRESET_BLOCKED:
        return replace(
            current,
            preset_id=preset_id,
            mode="rule",
            direct_domains=direct_domains,
            proxy_domains=proxy_domains,
            block_domains=block_domains,
            process_rules=[],
            process_preset_routes={},
            service_routes=proxy_default_services(),
            tun_default_outbound="direct",
        )

    if preset_id == ROUTING_PRESET_EXCEPT_RU:
        return replace(
            current,
            preset_id=preset_id,
            mode="global",
            direct_domains=direct_domains,
            proxy_domains=proxy_domains,
            block_domains=block_domains,
            process_rules=[],
            process_preset_routes={},
            service_routes={},
            tun_default_outbound="proxy",
        )

    return current
