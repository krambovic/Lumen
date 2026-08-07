from __future__ import annotations

from copy import deepcopy
from ipaddress import ip_address, ip_network
from typing import Any
from urllib.parse import urlsplit

from ...constants import (
    DEFAULT_DISCORD_SOCKS_PORT,
    DEFAULT_HTTP_PORT,
    DEFAULT_SOCKS_PORT,
    PROXY_HOST,
    DEFAULT_XRAY_STATS_API_PORT,
)
from ...models import AppSettings, Node, RoutingSettings
from ...multiplex import apply_xray_multiplex
from ...routing_presets import custom_domain_rules, preset_domain_rules
from ...routing_runtime import (
    build_xray_gui_routing_rules,
    collect_service_route_domains,
)
from ...xray_inbounds import build_xray_http_compat_inbound, build_xray_mixed_inbound
from ...xray_fragments import apply_xray_final_fragment, apply_xray_outbound_fragment


def _normalize_loglevel(value: str) -> str:
    normalized = value.lower().strip()
    if normalized == "warn":
        return "warning"
    if normalized in {"debug", "info", "warning", "error", "none"}:
        return normalized
    return "warning"


def _normalize_tls_certificate_pins(outbound: dict[str, Any]) -> None:
    stream = outbound.get("streamSettings")
    if not isinstance(stream, dict):
        return
    tls = stream.get("tlsSettings")
    if not isinstance(tls, dict):
        return
    pins = tls.get("pinnedPeerCertSha256")
    if isinstance(pins, list):
        pins = next((str(item).strip() for item in pins if str(item).strip()), "")
    if isinstance(pins, str):
        value = pins.strip().lower()
        if value:
            tls["pinnedPeerCertSha256"] = value
        else:
            tls.pop("pinnedPeerCertSha256", None)


def _xray_dns_server(address: str, transport: str) -> str:
    value = str(address or "").strip()
    kind = str(transport or "udp").strip().lower()
    if not value:
        return "8.8.8.8"
    if "://" in value:
        return value
    if kind == "https":
        return f"https://{value}/dns-query"
    if kind == "tls":
        return f"tls://{value}"
    if kind == "tcp":
        return f"tcp://{value}"
    return value


def _xray_dns_strategy(*values: str) -> str:
    normalized = {str(value or "").strip().lower() for value in values}
    if "ipv4_only" in normalized:
        return "UseIPv4"
    if "ipv6_only" in normalized:
        return "UseIPv6"
    return "UseIP"


def _xray_dns_entries(
    addresses: list[str],
    transport: str,
    *,
    domains: list[str] | None = None,
) -> list[str | dict[str, Any]]:
    entries: list[str | dict[str, Any]] = []
    domain_rules: list[str] = []
    for raw in domains or []:
        value = str(raw or "").strip()
        if not value or value.lower().startswith(("geoip:", "ip:")):
            continue
        if value.startswith(("domain:", "full:", "regexp:", "keyword:", "geosite:", "ext:")):
            domain_rules.append(value)
            continue
        try:
            ip_network(value, strict=False)
        except ValueError:
            domain_rules.append(f"domain:{value}")
    for address in addresses:
        value = _xray_dns_server(address, transport)
        if domain_rules:
            entries.append(
                {
                    "address": value,
                    "domains": list(domain_rules),
                    "skipFallback": True,
                }
            )
        else:
            entries.append(value)
    return entries


def _xray_proxy_dns_route_rule(addresses: list[str], transport: str) -> dict[str, Any] | None:
    """Pin remote DNS transports to the proxy even in direct-fallback presets."""
    domains: list[str] = []
    ips: list[str] = []
    for address in addresses:
        value = _xray_dns_server(address, transport)
        parsed = urlsplit(value if "://" in value else f"//{value}")
        host = str(parsed.hostname or "").strip().strip("[]")
        if not host:
            continue
        try:
            ip_address(host)
        except ValueError:
            domain = f"full:{host}"
            if domain not in domains:
                domains.append(domain)
        else:
            if host not in ips:
                ips.append(host)
    if not domains and not ips:
        return None
    rule: dict[str, Any] = {
        "type": "field",
        "network": "tcp,udp",
        "outboundTag": "proxy",
    }
    if domains:
        rule["domain"] = domains
    if ips:
        rule["ip"] = ips
    return rule


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
    _normalize_tls_certificate_pins(proxy_outbound)
    apply_xray_multiplex(
        proxy_outbound,
        enabled=settings.multiplex_enabled,
        concurrency=settings.multiplex_concurrency,
    )

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

    route_only = bool(getattr(settings, "sniff_route_only", False))
    auth_enabled = bool(
        getattr(settings, "proxy_auth_enabled", False)
        and str(getattr(settings, "proxy_auth_username", "") or "").strip()
        and str(getattr(settings, "proxy_auth_password", "") or "")
    )
    auth_username = str(getattr(settings, "proxy_auth_username", "") or "").strip() if auth_enabled else ""
    auth_password = str(getattr(settings, "proxy_auth_password", "") or "") if auth_enabled else ""
    inbounds: list[dict[str, Any]] = [
        build_xray_mixed_inbound(
            socks_port,
            route_only=route_only,
            username=auth_username,
            password=auth_password,
        ),
    ]
    if routing.dns_mode == "builtin":
        proxy_dns_servers = routing.dns_proxy_servers or [routing.dns_proxy_server]
        proxy_dns_rule = _xray_proxy_dns_route_rule(proxy_dns_servers, routing.dns_proxy_type)
        if proxy_dns_rule is not None:
            routing_rules.append(proxy_dns_rule)
    if int(http_port) != int(socks_port):
        inbounds.append(
            build_xray_http_compat_inbound(
                http_port,
                route_only=route_only,
                username=auth_username,
                password=auth_password,
            )
        )
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

    if getattr(settings, "proxy_allow_lan", False):
        for inbound in inbounds:
            if str(inbound.get("tag") or "") != "api":
                inbound["listen"] = "0.0.0.0"  # v2rayN-style Allow LAN for local proxy ports

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
        preset_direct, preset_proxy = preset_domain_rules(routing.preset_id)
        service_direct, service_proxy, _service_block = collect_service_route_domains(routing)
        direct_domains = [*service_direct, *custom_domain_rules(routing.direct_domains)]
        proxy_domains = [*service_proxy, *custom_domain_rules(routing.proxy_domains)]
        if routing.dns_geo_check:
            direct_domains.extend(preset_direct)
            proxy_domains.extend(preset_proxy)
        bootstrap_servers = [
            str(item).strip()
            for item in routing.dns_bootstrap_servers
            if str(item).strip()
        ]
        proxy_servers = routing.dns_proxy_servers or [routing.dns_proxy_server]
        dns_config: dict[str, Any] = {
            "servers": [
                *(
                    _xray_dns_entries(
                        bootstrap_servers,
                        routing.dns_bootstrap_type,
                        domains=direct_domains,
                    )
                    if direct_domains
                    else []
                ),
                *(
                    _xray_dns_entries(
                        proxy_servers,
                        routing.dns_proxy_type,
                        domains=proxy_domains,
                    )
                    if proxy_domains
                    else []
                ),
                # The final resolver must also be proxied.  `localhost` here
                # handed every unmatched query to the Windows/ISP resolver and
                # was the source of the leak visible on browser DNS tests.
                *_xray_dns_entries(proxy_servers, routing.dns_proxy_type),
            ],
            "queryStrategy": _xray_dns_strategy(
                routing.dns_proxy_strategy,
                routing.dns_bootstrap_strategy,
            ),
        }
        if routing.dns_parallel_query:
            dns_config["enableParallelQuery"] = True
        if routing.dns_optimistic_cache:
            dns_config["serveStale"] = True
            dns_config["serveExpiredTTL"] = 86400
        if routing.dns_hosts:
            dns_config["hosts"] = {
                domain: addresses[0] if len(addresses) == 1 else list(addresses)
                for domain, addresses in routing.dns_hosts.items()
            }
        config["dns"] = dns_config

    if settings.enable_xray_fragment:
        apply_xray_outbound_fragment(
            config,
            packets=settings.fragment_packets,
            length=settings.fragment_length,
            delay=settings.fragment_delay,
            tail_fragment=settings.tail_fragment_enabled,
        )
    if settings.enable_final_fragment:
        apply_xray_final_fragment(
            config,
            packets=settings.fragment_packets,
            length=settings.fragment_length,
            delay=settings.fragment_delay,
            tail_fragment=settings.tail_fragment_enabled,
        )

    return config
