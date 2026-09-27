from __future__ import annotations

from dataclasses import replace

from .models import RoutingSettings
from .service_presets import SERVICE_PRESETS

ROUTING_PRESET_GLOBAL = "global"
ROUTING_PRESET_BLOCKED = "blocked"
ROUTING_PRESET_EXCEPT_RU = "except_ru"
ROUTING_PRESET_BLOCKED_CN = "blocked_cn"
ROUTING_PRESET_EXCEPT_CN = "except_cn"
ROUTING_PRESET_EXCEPT_IR = "except_ir"

BUILT_IN_ROUTING_PRESET_IDS = frozenset(
    {
        ROUTING_PRESET_GLOBAL,
        ROUTING_PRESET_BLOCKED,
        ROUTING_PRESET_EXCEPT_RU,
        ROUTING_PRESET_BLOCKED_CN,
        ROUTING_PRESET_EXCEPT_CN,
        ROUTING_PRESET_EXCEPT_IR,
    }
)

REGION_RUSSIA = "russia"
REGION_CHINA = "china"
REGION_IRAN = "iran"
REGIONAL_PRESETS = (REGION_RUSSIA, REGION_CHINA, REGION_IRAN)

RU_DIRECT_RULES = (
    "geosite:category-ru",
    "geoip:ru",
)

CN_DIRECT_RULES = ("geosite:cn", "geoip:cn")
IR_DIRECT_RULES = ("geosite:ir", "geoip:ir")

BLOCKED_PROXY_RULES = (
    "geosite:ru-blocked",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
)

BLOCKED_DIRECT_RULES = (
    "geosite:lumen-exclude",
    "geoip:lumen-exclude",
)

CN_BLOCKED_PROXY_RULES = (
    "geosite:gfw", "geosite:greatfire", "geosite:google",
    "geoip:facebook", "geoip:fastly", "geoip:google", "geoip:netflix",
    "geoip:telegram", "geoip:twitter",
)

# Rules managed by the built-in quick presets.  Keep this list exact: users may
# intentionally add their own geosite:/geoip: rules and those must survive a
# preset switch.  The two legacy entries remove data left by older Lumen builds.
INTERNAL_PRESET_DOMAIN_RULES = frozenset(
    {
        *RU_DIRECT_RULES,
        *CN_DIRECT_RULES,
        *IR_DIRECT_RULES,
        *BLOCKED_PROXY_RULES,
        *BLOCKED_DIRECT_RULES,
        *CN_BLOCKED_PROXY_RULES,
        "geosite:ru-available-only-inside",
        "geosite:category-media-ru-blocked",
    }
)


def normalize_regional_preset(value: str) -> str:
    normalized = str(value or "").strip().lower()
    return normalized if normalized in REGIONAL_PRESETS else REGION_RUSSIA


def regional_routing_preset_ids(region: str) -> tuple[str, ...]:
    region = normalize_regional_preset(region)
    if region == REGION_CHINA:
        return ROUTING_PRESET_GLOBAL, ROUTING_PRESET_BLOCKED_CN, ROUTING_PRESET_EXCEPT_CN
    if region == REGION_IRAN:
        return ROUTING_PRESET_GLOBAL, ROUTING_PRESET_EXCEPT_IR
    return ROUTING_PRESET_GLOBAL, ROUTING_PRESET_BLOCKED, ROUTING_PRESET_EXCEPT_RU


def equivalent_regional_preset(preset_id: str, region: str) -> str:
    choices = regional_routing_preset_ids(region)
    value = str(preset_id or "").strip().lower()
    if value == ROUTING_PRESET_GLOBAL:
        return ROUTING_PRESET_GLOBAL
    if value.startswith("except_"):
        return choices[-1]
    if value in {ROUTING_PRESET_BLOCKED, ROUTING_PRESET_BLOCKED_CN}:
        return choices[1] if len(choices) == 3 else ROUTING_PRESET_GLOBAL
    return value


def normalize_routing_preset_for_region(preset_id: str, region: str) -> str:
    """Map a built-in preset to its valid counterpart for *region*."""
    value = str(preset_id or "").strip().lower()
    if value not in BUILT_IN_ROUTING_PRESET_IDS:
        return value
    return equivalent_regional_preset(value, region)


def is_internal_preset_rule(value: str) -> bool:
    return str(value or "").strip().lower() in INTERNAL_PRESET_DOMAIN_RULES


def preset_requires_regional_geodata(preset_id: str) -> bool:
    value = str(preset_id or "").strip().lower()
    return value in BUILT_IN_ROUTING_PRESET_IDS and value != ROUTING_PRESET_GLOBAL


def custom_domain_rules(items: list[str]) -> list[str]:
    """Return only rules explicitly owned by the user."""
    return [item for item in items if not is_internal_preset_rule(item)]


def preset_domain_rules(preset_id: str) -> tuple[list[str], list[str]]:
    """Return internal direct/proxy rules for a built-in preset."""
    if preset_id == ROUTING_PRESET_BLOCKED:
        return list(BLOCKED_DIRECT_RULES), list(BLOCKED_PROXY_RULES)
    if preset_id == ROUTING_PRESET_EXCEPT_RU:
        return list(RU_DIRECT_RULES), []
    if preset_id == ROUTING_PRESET_BLOCKED_CN:
        return [], list(CN_BLOCKED_PROXY_RULES)
    if preset_id == ROUTING_PRESET_EXCEPT_CN:
        return list(CN_DIRECT_RULES), []
    if preset_id == ROUTING_PRESET_EXCEPT_IR:
        return list(IR_DIRECT_RULES), []
    return [], []


def proxy_default_services() -> dict[str, str]:
    return {
        preset.id: preset.default_action
        for preset in SERVICE_PRESETS
        if preset.default_action == "proxy"
    }


def repair_builtin_preset_service_routes(routing: RoutingSettings) -> RoutingSettings:
    """Migrate the blocked preset's unmatched-traffic fallback only.

    Built-in service actions are resolved from preset metadata at runtime;
    writing them into ``service_routes`` would turn inherited actions into fake
    per-service overrides and make the UI imply that every row was customized.
    """
    if str(routing.preset_id or "").strip().lower() not in {
        ROUTING_PRESET_BLOCKED,
        ROUTING_PRESET_BLOCKED_CN,
    }:
        return routing
    default_outbound = str(routing.tun_default_outbound or "").strip().lower()
    if not routing.tun_default_outbound_user_selected:
        default_outbound = "direct"
    if default_outbound == routing.tun_default_outbound:
        return routing
    return replace(routing, tun_default_outbound=default_outbound)


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
            service_routes={},
            tun_default_outbound="proxy",
            tun_default_outbound_user_selected=False,
        )

    if preset_id in {ROUTING_PRESET_BLOCKED, ROUTING_PRESET_BLOCKED_CN}:
        return replace(
            current,
            preset_id=preset_id,
            mode="rule",
            direct_domains=direct_domains,
            proxy_domains=proxy_domains,
            block_domains=block_domains,
            service_routes=proxy_default_services(),
            tun_default_outbound="direct",
            tun_default_outbound_user_selected=False,
        )

    if preset_id in {ROUTING_PRESET_EXCEPT_RU, ROUTING_PRESET_EXCEPT_CN, ROUTING_PRESET_EXCEPT_IR}:
        return replace(
            current,
            preset_id=preset_id,
            mode="global",
            direct_domains=direct_domains,
            proxy_domains=proxy_domains,
            block_domains=block_domains,
            service_routes={},
            tun_default_outbound="proxy",
            tun_default_outbound_user_selected=False,
        )

    return current
