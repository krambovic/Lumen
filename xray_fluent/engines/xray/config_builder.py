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
        },
        {
            "type": "field",
            "inboundTag": ["discord-socks-in"],
            "outboundTag": "proxy",
        }
    ]

    routing_rules.extend(build_xray_gui_routing_rules(routing, settings))

    config: dict[str, Any] = {
        "log": {
            "loglevel": _normalize_loglevel(settings.log_level),
        },
        "inbounds": [
            {
                "tag": "socks-in",
                "listen": PROXY_HOST,
                "port": int(socks_port),
                "protocol": "socks",
                "settings": {
                    "auth": "noauth",
                    "udp": True,
                },
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls", "quic"],
                    "routeOnly": True,
                },
            },
            {
                "tag": "http-in",
                "listen": PROXY_HOST,
                "port": int(http_port),
                "protocol": "http",
                "settings": {},
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls"],
                    "routeOnly": True,
                },
            },
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
                    "destOverride": ["http", "tls", "quic"],
                    "routeOnly": True,
                },
            },
            {
                "tag": "api",
                "listen": PROXY_HOST,
                "port": api_port,
                "protocol": "dokodemo-door",
                "settings": {
                    "address": PROXY_HOST,
                },
            },
        ],
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

    return config
