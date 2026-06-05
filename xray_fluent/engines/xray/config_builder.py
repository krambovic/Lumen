from __future__ import annotations

from copy import deepcopy
from typing import Any

from ...constants import (
    DEFAULT_DISCORD_SOCKS_PORT,
    DEFAULT_HTTP_PORT,
    DEFAULT_SOCKS_PORT,
    PROXY_HOST,
    DEFAULT_XRAY_STATS_API_PORT,
)
from ...models import AppSettings, Node, RoutingSettings
from ...routing_runtime import build_xray_gui_routing_rules
from ...xray_inbounds import build_xray_http_compat_inbound, build_xray_mixed_inbound
from ...xray_fragments import apply_xray_final_fragment, apply_xray_outbound_fragment


def _normalize_loglevel(value: str) -> str:
    normalized = value.lower().strip()
    if normalized == "warn":
        return "warning"
    if normalized in {"debug", "info", "warning", "error", "none"}:
        return normalized
    return "warning"


def build_xray_config(
    node: Node,
    routing: RoutingSettings,
    settings: AppSettings,
    api_port: int = 0,
    *,
    socks_port: int = DEFAULT_SOCKS_PORT,
    http_port: int = DEFAULT_HTTP_PORT,
) -> dict[str, Any]:
    if not api_port:
        api_port = DEFAULT_XRAY_STATS_API_PORT
    proxy_outbound = deepcopy(node.outbound)
    proxy_outbound["tag"] = "proxy"

    routing_rules: list[dict[str, Any]] = [
        {
            "type": "field",
            "inboundTag": ["api"],
            "outboundTag": "api",
        }
    ]
    if settings.discord_proxy_enabled:
        routing_rules.append(
            {
                "type": "field",
                "inboundTag": ["discord-socks-in"],
                "outboundTag": "proxy",
            }
        )

    routing_rules.extend(build_xray_gui_routing_rules(routing, settings))

    inbounds: list[dict[str, Any]] = [
        build_xray_mixed_inbound(socks_port),
    ]
    if int(http_port) != int(socks_port):
        inbounds.append(build_xray_http_compat_inbound(http_port))
    if settings.discord_proxy_enabled:
        inbounds.append(
            {
                "tag": "discord-socks-in",
                "listen": PROXY_HOST,
                "port": DEFAULT_DISCORD_SOCKS_PORT,
                "protocol": "socks",
                "settings": {
                    "auth": "noauth",
                    "udp": True,
                },
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls"],
                    "routeOnly": False,
                },
            }
        )
    inbounds.append(
        {
            "tag": "api",
            "listen": PROXY_HOST,
            "port": api_port,
            "protocol": "dokodemo-door",
            "settings": {
                "address": PROXY_HOST,
            },
        }
    )

    config: dict[str, Any] = {
        "log": {
            "loglevel": _normalize_loglevel(settings.log_level),
        },
        "inbounds": inbounds,
        "outbounds": [
            proxy_outbound,
            {
                "tag": "direct",
                "protocol": "freedom",
                "settings": {},
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": {},
            },
            {
                "tag": "api",
                "protocol": "freedom",
                "settings": {},
            },
        ],
        "policy": {
            "system": {
                "statsInboundUplink": True,
                "statsInboundDownlink": True,
                "statsOutboundUplink": True,
                "statsOutboundDownlink": True,
            }
        },
        "stats": {},
        "api": {
            "tag": "api",
            "services": ["StatsService"],
        },
        "routing": {
            "domainStrategy": "AsIs",
            "rules": routing_rules,
        },
    }

    if routing.dns_mode == "builtin":
        config["dns"] = {
            "servers": [
                "1.1.1.1",
                "8.8.8.8",
                "localhost",
            ],
            "queryStrategy": "UseIP",
        }

    if settings.enable_xray_fragment:
        apply_xray_outbound_fragment(config)
    if settings.enable_final_fragment:
        apply_xray_final_fragment(config)

    return config
