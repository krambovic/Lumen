from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import hashlib
from ipaddress import ip_address, ip_network
import json
import secrets
import socket
from pathlib import Path
from typing import Any

from ...application.runtime_security import generate_local_proxy_credentials, strip_singbox_proxy_inbounds
from ...constants import (
    DEFAULT_DISCORD_SOCKS_PORT,
    PROXY_HOST,
    SINGBOX_CLASH_API_PORT,
    SINGBOX_XRAY_RELAY_PORT,
    SS_PROTECT_PORT_END,
    SS_PROTECT_PORT_START,
)
from ...models import Node, RoutingSettings
from ...multiplex import apply_xray_multiplex
from ...routing_runtime import apply_singbox_gui_routing
from ...xray_fragments import apply_xray_final_fragment
from .config_builder import build_singbox_outbound


_SS_PROTECT_METHOD = "chacha20-ietf-poly1305"
_APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG = "__app_hybrid_protect_in"
_APP_XRAY_SIDECAR_RELAY_INBOUND_TAG = "__app_hybrid_relay_in"
_APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG = "__app_hybrid_protect_out"
_APP_DISCORD_PROXY_INBOUND_TAG = "discord-socks-in"


@dataclass(slots=True)
class SingboxDocumentState:
    source_path: Path
    text: str
    text_hash: str
    has_proxy_outbound: bool
    file_mtime_ns: int = 0
    file_size: int = 0


@dataclass(slots=True)
class ParsedSingboxDocument:
    source_path: Path
    text: str
    text_hash: str
    payload: dict[str, Any]
    has_proxy_outbound: bool


@dataclass(slots=True)
class SingboxXraySidecarPlan:
    relay_port: int
    relay_username: str
    relay_password: str
    protect_port: int
    protect_password: str
    config: dict[str, Any]


@dataclass(slots=True)
class SingboxRuntimePlan:
    outcome: str  # native_singbox | hybrid_xray_sidecar
    source_path: Path
    text_hash: str
    singbox_config: dict[str, Any]
    has_proxy_outbound: bool
    used_selected_node: bool
    xray_sidecar: SingboxXraySidecarPlan | None

    @property
    def is_hybrid(self) -> bool:
        return self.outcome == "hybrid_xray_sidecar"


def inspect_singbox_document_text(source_path: Path, text: str) -> SingboxDocumentState:
    text_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    has_proxy_outbound = False
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        payload = None
    if isinstance(payload, dict):
        has_proxy_outbound = _config_has_proxy_outbound(payload)
    return SingboxDocumentState(
        source_path=source_path,
        text=text,
        text_hash=text_hash,
        has_proxy_outbound=has_proxy_outbound,
    )


def parse_singbox_document(source_path: Path, text: str) -> ParsedSingboxDocument:
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{source_path.name}: {_format_json_error_message(text, exc)}") from exc
    if not isinstance(payload, dict):
        raise ValueError("Корень sing-box config должен быть JSON-объектом.")
    text_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    has_proxy_outbound = _config_has_proxy_outbound(payload)
    return ParsedSingboxDocument(
        source_path=source_path,
        text=text,
        text_hash=text_hash,
        payload=payload,
        has_proxy_outbound=has_proxy_outbound,
    )


def classify_node_for_singbox(node: Node | None) -> str:
    if node is None:
        return "native_singbox"
    if _node_should_use_xray_sidecar(node):
        return "hybrid_xray_sidecar"
    try:
        build_singbox_outbound(node, tag="proxy")
    except ValueError:
        return "hybrid_xray_sidecar"
    return "native_singbox"


def plan_singbox_runtime(
    document: ParsedSingboxDocument,
    node: Node | None,
    *,
    routing: RoutingSettings | None = None,
    enable_final_fragment: bool = True,
    fragment_packets: str = "tlshello",
    fragment_length: str = "50-100",
    fragment_delay: str = "10-20",
    tail_fragment_enabled: bool = False,
    multiplex_enabled: bool = False,
    multiplex_concurrency: int = 8,
    discord_proxy_enabled: bool = False,
    preferred_relay_port: int = 0,
    preferred_protect_port: int = 0,
    preferred_protect_password: str = "",
) -> SingboxRuntimePlan:
    runtime_config = deepcopy(document.payload)
    strip_singbox_proxy_inbounds(runtime_config)
    _ensure_singbox_metrics_contract(runtime_config)
    _ensure_singbox_tun_runtime_contract(
        runtime_config,
        routing=routing,
        enable_final_fragment=enable_final_fragment,
    )

    outbounds = runtime_config.get("outbounds")
    proxy_index = _find_proxy_outbound_index(outbounds)
    if proxy_index is None:
        if routing is not None:
            apply_singbox_gui_routing(runtime_config, routing)
        _ensure_singbox_discord_proxy_contract(runtime_config, enabled=discord_proxy_enabled)
        _validate_runtime_dns_contract(runtime_config)
        return SingboxRuntimePlan(
            outcome="native_singbox",
            source_path=document.source_path,
            text_hash=document.text_hash,
            singbox_config=runtime_config,
            has_proxy_outbound=False,
            used_selected_node=False,
            xray_sidecar=None,
        )

    if node is None:
        raise ValueError("В конфиге есть outbound tag `proxy`. Выберите сервер для запуска sing-box.")

    force_sidecar = _node_should_use_xray_sidecar(node)
    try:
        native_proxy = None if force_sidecar else build_singbox_outbound(
            node,
            tag="proxy",
            multiplex_enabled=multiplex_enabled,
            multiplex_concurrency=multiplex_concurrency,
        )
    except ValueError:
        native_proxy = None

    if native_proxy is None:
        plan = _plan_hybrid_runtime(
            document,
            runtime_config=runtime_config,
            proxy_index=proxy_index,
            node=node,
            enable_final_fragment=enable_final_fragment,
            fragment_packets=fragment_packets,
            fragment_length=fragment_length,
            fragment_delay=fragment_delay,
            tail_fragment_enabled=tail_fragment_enabled,
            multiplex_enabled=multiplex_enabled,
            multiplex_concurrency=multiplex_concurrency,
            preferred_relay_port=preferred_relay_port,
            preferred_protect_port=preferred_protect_port,
            preferred_protect_password=preferred_protect_password,
        )
        if routing is not None:
            apply_singbox_gui_routing(plan.singbox_config, routing)
        _ensure_singbox_discord_proxy_contract(plan.singbox_config, enabled=discord_proxy_enabled)
        _validate_runtime_dns_contract(plan.singbox_config)
        return plan

    assert isinstance(outbounds, list)
    native_is_endpoint = _is_singbox_endpoint(native_proxy)
    if native_is_endpoint:
        outbounds.pop(proxy_index)
        _replace_or_append_tagged(_ensure_list(runtime_config, "endpoints"), "proxy", native_proxy)
    else:
        outbounds[proxy_index] = native_proxy
    if routing is not None:
        apply_singbox_gui_routing(runtime_config, routing)
    if native_is_endpoint:
        _ensure_endpoint_server_bootstrap_contract(runtime_config, native_proxy)
    else:
        _ensure_proxy_server_bootstrap_contract(runtime_config, native_proxy, node.server)
    _ensure_singbox_discord_proxy_contract(runtime_config, enabled=discord_proxy_enabled)
    _validate_runtime_dns_contract(runtime_config)
    return SingboxRuntimePlan(
        outcome="native_singbox",
        source_path=document.source_path,
        text_hash=document.text_hash,
        singbox_config=runtime_config,
        has_proxy_outbound=True,
        used_selected_node=True,
        xray_sidecar=None,
    )


def _plan_hybrid_runtime(
    document: ParsedSingboxDocument,
    *,
    runtime_config: dict[str, Any],
    proxy_index: int,
    node: Node,
    enable_final_fragment: bool,
    fragment_packets: str,
    fragment_length: str,
    fragment_delay: str,
    tail_fragment_enabled: bool,
    multiplex_enabled: bool,
    multiplex_concurrency: int,
    preferred_relay_port: int,
    preferred_protect_port: int,
    preferred_protect_password: str,
) -> SingboxRuntimePlan:
    relay_port = preferred_relay_port if preferred_relay_port > 0 else _find_free_port(preferred=SINGBOX_XRAY_RELAY_PORT)
    excluded_ports = {relay_port}
    protect_port = preferred_protect_port if preferred_protect_port > 0 else _find_free_port(
        preferred=SS_PROTECT_PORT_START,
        port_range=range(SS_PROTECT_PORT_START, SS_PROTECT_PORT_END),
        excluded=excluded_ports,
    )
    protect_password = preferred_protect_password or _generate_ss_password()
    relay_username, relay_password = generate_local_proxy_credentials(prefix="sidecar")

    outbounds = runtime_config.setdefault("outbounds", [])
    assert isinstance(outbounds, list)
    outbounds[proxy_index] = {
        "type": "socks",
        "tag": "proxy",
        "server": PROXY_HOST,
        "server_port": relay_port,
        "username": relay_username,
        "password": relay_password,
        # Keep the relay on loopback so sing-box does not bind it to the
        # physical adapter via auto-detect rules.
        "inet4_bind_address": PROXY_HOST,
    }

    _replace_or_append_tagged(
        _ensure_list(runtime_config, "inbounds"),
        _APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG,
        {
            "type": "shadowsocks",
            "tag": _APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG,
            "listen": PROXY_HOST,
            "listen_port": protect_port,
            "method": _SS_PROTECT_METHOD,
            "password": protect_password,
        },
    )
    _ensure_hybrid_protect_route(runtime_config)
    _remove_singbox_tls_fragment_rule(runtime_config)
    _validate_runtime_dns_contract(runtime_config)

    sidecar = SingboxXraySidecarPlan(
        relay_port=relay_port,
        relay_username=relay_username,
        relay_password=relay_password,
        protect_port=protect_port,
        protect_password=protect_password,
        config=_build_xray_sidecar_config(
            node,
            relay_port=relay_port,
            relay_username=relay_username,
            relay_password=relay_password,
            protect_port=protect_port,
            protect_password=protect_password,
            enable_final_fragment=enable_final_fragment,
            fragment_packets=fragment_packets,
            fragment_length=fragment_length,
            fragment_delay=fragment_delay,
            tail_fragment_enabled=tail_fragment_enabled,
            multiplex_enabled=multiplex_enabled,
            multiplex_concurrency=multiplex_concurrency,
        ),
    )
    return SingboxRuntimePlan(
        outcome="hybrid_xray_sidecar",
        source_path=document.source_path,
        text_hash=document.text_hash,
        singbox_config=runtime_config,
        has_proxy_outbound=True,
        used_selected_node=True,
        xray_sidecar=sidecar,
    )


def _node_should_use_xray_sidecar(node: Node | None) -> bool:
    """Return True only for transports that sing-box cannot run natively."""
    outbound = node.outbound if node is not None else None
    if not isinstance(outbound, dict):
        return False

    protocol = str(outbound.get("protocol") or node.scheme or "").strip().lower()
    if protocol != "vless":
        return False

    stream_settings = outbound.get("streamSettings")
    if isinstance(stream_settings, dict):
        network = str(stream_settings.get("network") or "").strip().lower()
        if network == "raw" or "finalmask" in stream_settings:
            return True
    return False


def _build_xray_sidecar_config(
    node: Node,
    *,
    relay_port: int,
    relay_username: str,
    relay_password: str,
    protect_port: int,
    protect_password: str,
    enable_final_fragment: bool = True,
    fragment_packets: str = "tlshello",
    fragment_length: str = "50-100",
    fragment_delay: str = "10-20",
    tail_fragment_enabled: bool = False,
    multiplex_enabled: bool = False,
    multiplex_concurrency: int = 8,
) -> dict[str, Any]:
    if not isinstance(node.outbound, dict) or not node.outbound:
        raise ValueError("Выбранный сервер не содержит outbound JSON для xray sidecar.")
    if not str(node.outbound.get("protocol") or "").strip():
        raise ValueError("Выбранный сервер не содержит protocol для xray sidecar.")
    proxy_outbound = deepcopy(node.outbound)
    proxy_outbound["tag"] = "proxy"
    apply_xray_multiplex(
        proxy_outbound,
        enabled=multiplex_enabled,
        concurrency=multiplex_concurrency,
    )
    stream_settings = proxy_outbound.get("streamSettings")
    if not isinstance(stream_settings, dict):
        stream_settings = {}
        proxy_outbound["streamSettings"] = stream_settings
    sockopt = stream_settings.get("sockopt")
    if not isinstance(sockopt, dict):
        sockopt = {}
        stream_settings["sockopt"] = sockopt
    sockopt["dialerProxy"] = _APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG

    config = {
        "log": {"loglevel": "warning"},
        "inbounds": [
            {
                "tag": _APP_XRAY_SIDECAR_RELAY_INBOUND_TAG,
                "protocol": "socks",
                "listen": PROXY_HOST,
                "port": relay_port,
                "settings": {
                    "auth": "password",
                    "accounts": [{"user": relay_username, "pass": relay_password}],
                    "udp": True,
                },
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls"],
                    "routeOnly": False,
                },
            }
        ],
        "outbounds": [
            proxy_outbound,
            {
                "tag": _APP_XRAY_SIDECAR_PROTECT_OUTBOUND_TAG,
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [
                        {
                            "address": PROXY_HOST,
                            "port": protect_port,
                            "method": _SS_PROTECT_METHOD,
                            "password": protect_password,
                        }
                    ]
                },
            },
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": [
                {
                    "type": "field",
                    "inboundTag": [_APP_XRAY_SIDECAR_RELAY_INBOUND_TAG],
                    "outboundTag": "proxy",
                }
            ],
        },
    }
    if enable_final_fragment:
        apply_xray_final_fragment(
            config,
            packets=fragment_packets,
            length=fragment_length,
            delay=fragment_delay,
            tail_fragment=tail_fragment_enabled,
        )
    return config


def _is_domain_name(value: str) -> bool:
    host = str(value or "").strip()
    if not host:
        return False
    try:
        ip_address(host)
    except ValueError:
        return True
    return False


def _ensure_proxy_server_bootstrap_contract(
    payload: dict[str, Any],
    proxy_outbound: dict[str, Any],
    preferred_server: str,
) -> None:
    server = str(preferred_server or proxy_outbound.get("server") or "").strip()
    if not server:
        return
    endpoint_cidr = _endpoint_ip_cidr(server)
    if endpoint_cidr:
        _ensure_tun_route_exclude_addresses(payload, [endpoint_cidr])
        _ensure_direct_ip_route(payload, endpoint_cidr)
        return

    # Domain-based proxy servers must resolve through bootstrap-dns, otherwise
    # proxy-dns can recurse into the proxy outbound before the tunnel is ready.
    proxy_outbound["domain_resolver"] = "bootstrap-dns"
    _ensure_direct_domain_route(payload, server)


def _ensure_endpoint_server_bootstrap_contract(payload: dict[str, Any], endpoint: dict[str, Any]) -> None:
    endpoint_type = str(endpoint.get("type") or "").strip().lower()
    hosts: list[str] = []
    if endpoint_type == "wireguard":
        endpoint["domain_resolver"] = "bootstrap-dns"
        for peer in endpoint.get("peers") or []:
            if isinstance(peer, dict):
                hosts.append(str(peer.get("address") or "").strip())
    elif endpoint_type == "warp":
        endpoint["domain_resolver"] = "bootstrap-dns"
        hosts.append("engage.cloudflareclient.com")

    endpoint_cidrs = [_endpoint_ip_cidr(host) for host in hosts]
    _ensure_tun_route_exclude_addresses(payload, [cidr for cidr in endpoint_cidrs if cidr])
    for host in hosts:
        endpoint_cidr = _endpoint_ip_cidr(host)
        if endpoint_cidr:
            _ensure_direct_ip_route(payload, endpoint_cidr)
        elif _is_domain_name(host):
            _ensure_direct_domain_route(payload, host)


def _endpoint_ip_cidr(value: str) -> str:
    try:
        address = ip_address(str(value or "").strip())
    except ValueError:
        return ""
    return f"{address}/{'128' if address.version == 6 else '32'}"


def _ensure_tun_route_exclude_addresses(payload: dict[str, Any], addresses: list[str]) -> None:
    normalized = _normalize_route_exclude_addresses(addresses)
    if not normalized:
        return
    inbounds = payload.get("inbounds")
    if not isinstance(inbounds, list):
        return
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        if str(inbound.get("type") or "").strip().lower() != "tun":
            continue
        existing = inbound.get("route_exclude_address")
        values = existing if isinstance(existing, list) else []
        inbound["route_exclude_address"] = _normalize_route_exclude_addresses(
            [str(item) for item in values] + normalized
        )


def _ensure_direct_domain_route(payload: dict[str, Any], server: str) -> None:
    server = str(server or "").strip()
    if not server:
        return

    route = _ensure_dict(payload, "route")
    rules = _ensure_list(route, "rules")
    direct_rule = {"domain": [server], "action": "route", "outbound": "direct"}

    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        domain_value = rule.get("domain")
        if isinstance(domain_value, list) and server in [str(item) for item in domain_value]:
            rules[index] = direct_rule
            return

    insert_index = 0
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        if rule.get("action") == "sniff" or rule.get("protocol") == "dns":
            insert_index = index + 1
            continue
        break
    rules.insert(insert_index, direct_rule)


def _ensure_direct_ip_route(payload: dict[str, Any], cidr: str) -> None:
    cidr = str(cidr or "").strip()
    if not cidr:
        return

    route = _ensure_dict(payload, "route")
    rules = _ensure_list(route, "rules")
    direct_rule = {"ip_cidr": [cidr], "action": "route", "outbound": "direct"}

    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        ip_value = rule.get("ip_cidr")
        if isinstance(ip_value, list) and cidr in [str(item) for item in ip_value]:
            rules[index] = direct_rule
            return

    insert_index = 0
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        if rule.get("action") == "sniff" or rule.get("protocol") == "dns":
            insert_index = index + 1
            continue
        break
    rules.insert(insert_index, direct_rule)


def _ensure_hybrid_protect_route(payload: dict[str, Any]) -> None:
    route = _ensure_dict(payload, "route")
    rules = _ensure_list(route, "rules")
    protect_rule = {
        "inbound": [_APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG],
        "action": "route",
        "outbound": "direct",
    }
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        inbound_value = rule.get("inbound")
        if isinstance(inbound_value, list) and _APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG in [str(item) for item in inbound_value]:
            rules[index] = protect_rule
            return
    rules.insert(0, protect_rule)


def _ensure_singbox_discord_proxy_contract(payload: dict[str, Any], *, enabled: bool) -> None:
    inbounds = _ensure_list(payload, "inbounds")
    route = _ensure_dict(payload, "route")
    rules = _ensure_list(route, "rules")

    inbounds[:] = [
        inbound
        for inbound in inbounds
        if not (
            isinstance(inbound, dict)
            and str(inbound.get("tag") or "") == _APP_DISCORD_PROXY_INBOUND_TAG
        )
    ]
    rules[:] = [rule for rule in rules if not _is_singbox_discord_proxy_rule(rule)]

    if not enabled:
        return

    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        try:
            port = int(inbound.get("listen_port") or inbound.get("port") or 0)
        except (TypeError, ValueError):
            port = 0
        if port == int(DEFAULT_DISCORD_SOCKS_PORT):
            tag = str(inbound.get("tag") or "").strip() or "<no tag>"
            raise ValueError(
                f"sing-box inbound `{tag}` already uses Discord proxy port {DEFAULT_DISCORD_SOCKS_PORT}."
            )

    inbounds.append(
        {
            "type": "socks",
            "tag": _APP_DISCORD_PROXY_INBOUND_TAG,
            "listen": PROXY_HOST,
            "listen_port": int(DEFAULT_DISCORD_SOCKS_PORT),
        }
    )
    rules.insert(
        _singbox_discord_rule_insert_index(rules),
        {
            "inbound": [_APP_DISCORD_PROXY_INBOUND_TAG],
            "action": "route",
            "outbound": "proxy",
        },
    )


def _is_singbox_discord_proxy_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    inbound = rule.get("inbound")
    if isinstance(inbound, str):
        return inbound == _APP_DISCORD_PROXY_INBOUND_TAG
    if isinstance(inbound, list):
        return _APP_DISCORD_PROXY_INBOUND_TAG in [str(item) for item in inbound]
    return False


def _dns_strategy(value: str) -> str:
    strategy = str(value or "").strip().lower().replace("-", "_")
    return strategy if strategy in {"prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"} else "prefer_ipv4"


def _build_dns_server(tag: str, server: str, server_type: str, strategy: str) -> dict[str, Any]:
    dns_type = str(server_type or "").strip().lower()
    address = str(server or "").strip() or ("8.8.8.8" if tag == "proxy-dns" else "1.1.1.1")
    if dns_type not in {"udp", "tcp", "tls", "https"}:
        dns_type = "https" if tag == "proxy-dns" else "udp"
    payload: dict[str, Any] = {
        "tag": tag,
        "type": dns_type,
        "server": address,
    }
    if dns_type == "https":
        payload.setdefault("server_port", 443)
        payload.setdefault("path", "/dns-query")
    elif dns_type == "tls":
        payload.setdefault("server_port", 853)
    elif dns_type in {"udp", "tcp"}:
        payload.setdefault("server_port", 53)
    return payload


def _set_dns_server_dial_contract(server: dict[str, Any], *, detour: str, resolver: str = "") -> None:
    server["detour"] = detour
    if _is_domain_name(str(server.get("server") or "")):
        server["domain_resolver"] = resolver or "system-dns"
    else:
        server.pop("domain_resolver", None)


def _normalize_route_exclude_addresses(values: list[str] | tuple[str, ...]) -> list[str]:
    normalized: list[str] = []
    for raw in values:
        value = str(raw or "").strip()
        if not value:
            continue
        try:
            # sing-box accepts CIDR entries here; plain IPs are normalized to /32 or /128.
            if "/" in value:
                normalized.append(str(ip_network(value, strict=False)))
            else:
                address = ip_address(value)
                normalized.append(f"{address}/{'128' if address.version == 6 else '32'}")
        except ValueError:
            continue
    return sorted(dict.fromkeys(normalized))


def _singbox_discord_rule_insert_index(rules: list[Any]) -> int:
    index = 0
    while index < len(rules):
        rule = rules[index]
        if not isinstance(rule, dict):
            break
        if rule.get("inbound"):
            index += 1
            continue
        if rule.get("action") in {"sniff", "hijack-dns", "route-options"}:
            index += 1
            continue
        break
    return index


def _ensure_singbox_metrics_contract(payload: dict[str, Any]) -> None:
    experimental = _ensure_dict(payload, "experimental")
    clash_api = _ensure_dict(experimental, "clash_api")
    clash_api["external_controller"] = f"127.0.0.1:{SINGBOX_CLASH_API_PORT}"


def _ensure_singbox_tun_runtime_contract(
    payload: dict[str, Any],
    *,
    routing: RoutingSettings | None = None,
    enable_final_fragment: bool = True,
) -> None:
    """Patch app-owned runtime fields for raw sing-box configs.

    The source document may keep a placeholder or stale interface name, but the
    runtime launch should always use a fresh xftun-prefixed adapter name. This
    avoids collisions during reconnect/apply while Windows is still releasing
    the previous wintun interface.
    """
    inbounds = payload.get("inbounds")
    has_tun = False
    if isinstance(inbounds, list):
        for inbound in inbounds:
            if not isinstance(inbound, dict):
                continue
            if str(inbound.get("type") or "").strip().lower() != "tun":
                continue
            has_tun = True
            inbound["interface_name"] = _generate_tun_interface_name()
            inbound["address"] = ["172.18.0.1/30"]
            inbound["mtu"] = 1500
            inbound["auto_route"] = True
            inbound["strict_route"] = False
            inbound["stack"] = "mixed"
            excludes = _normalize_route_exclude_addresses(
                routing.tun_route_exclude_address if routing is not None else []
            )
            if excludes:
                inbound["route_exclude_address"] = excludes
            else:
                inbound.pop("route_exclude_address", None)
            inbound.pop("sniff", None)
    if not has_tun:
        return

    route = _ensure_dict(payload, "route")
    route["auto_detect_interface"] = True
    use_builtin_dns = _singbox_uses_builtin_dns(routing)
    direct_strategy = _dns_strategy(routing.dns_bootstrap_strategy if routing is not None else "")
    route["default_domain_resolver"] = {"server": "bootstrap-dns", "strategy": direct_strategy}
    _ensure_singbox_dns_runtime_contract(payload, routing=routing)
    rules = _ensure_list(route, "rules")
    _ensure_singbox_tun_base_rules(
        rules,
        enable_final_fragment=enable_final_fragment,
        dns_hijack=(use_builtin_dns and (routing.dns_hijack_enabled if routing is not None else True)),
    )


def _ensure_singbox_dns_runtime_contract(payload: dict[str, Any], *, routing: RoutingSettings | None = None) -> None:
    dns = payload.get("dns")
    if not isinstance(dns, dict):
        dns = {}
        payload["dns"] = dns
    dns["independent_cache"] = True
    direct_server = routing.dns_bootstrap_server if routing is not None else "1.1.1.1"
    direct_type = routing.dns_bootstrap_type if routing is not None else "udp"
    direct_strategy = _dns_strategy(routing.dns_bootstrap_strategy if routing is not None else "")
    proxy_server = routing.dns_proxy_server if routing is not None else "8.8.8.8"
    proxy_type = routing.dns_proxy_type if routing is not None else "https"
    proxy_strategy = _dns_strategy(routing.dns_proxy_strategy if routing is not None else "")
    use_builtin_dns = _singbox_uses_builtin_dns(routing)
    fake_enabled = use_builtin_dns and (bool(routing.dns_fake_enabled) if routing is not None else False)

    dns["strategy"] = direct_strategy
    servers = dns.setdefault("servers", [])
    if not isinstance(servers, list):
        servers = []
        dns["servers"] = servers
    if use_builtin_dns:
        _replace_or_append_tagged(servers, "system-dns", {"tag": "system-dns", "type": "local"})
        bootstrap_dns = _build_dns_server("bootstrap-dns", direct_server, direct_type, direct_strategy)
        _set_dns_server_dial_contract(bootstrap_dns, detour="direct")
        direct_dns = _build_dns_server("direct-dns", direct_server, direct_type, direct_strategy)
        _set_dns_server_dial_contract(direct_dns, detour="direct")
        _replace_or_append_tagged(servers, "bootstrap-dns", bootstrap_dns)
        _replace_or_append_tagged(servers, "direct-dns", direct_dns)
        proxy_dns = _build_dns_server("proxy-dns", proxy_server, proxy_type, proxy_strategy)
        _set_dns_server_dial_contract(proxy_dns, detour="proxy", resolver="bootstrap-dns")
    else:
        system_dns = {"tag": "bootstrap-dns", "type": "local"}
        _replace_or_append_tagged(servers, "bootstrap-dns", system_dns)
        _replace_or_append_tagged(servers, "direct-dns", {"tag": "direct-dns", "type": "local"})
        proxy_dns = {"tag": "proxy-dns", "type": "local"}
    _replace_or_append_tagged(servers, "proxy-dns", proxy_dns)
    if fake_enabled:
        _replace_or_append_tagged(
            servers,
            "fake-dns",
            {
                "tag": "fake-dns",
                "type": "fakeip",
                "inet4_range": "198.18.0.0/15",
                "inet6_range": "fc00::/18",
            },
        )
    else:
        servers[:] = [
            server
            for server in servers
            if not (isinstance(server, dict) and str(server.get("tag") or "") == "fake-dns")
        ]

    for server in servers:
        if not isinstance(server, dict):
            continue
        tag = str(server.get("tag") or "")
        server_type = str(server.get("type") or "").strip().lower()
        address = str(server.get("server") or "").strip().lower()
        if tag == "proxy-dns" and server_type == "tcp" and address in {"8.8.8.8", "8.8.4.4"}:
            # TCP/53 through some proxy nodes times out and spams sing-box logs.
            # v2rayN defaults remote DNS to encrypted DNS; migrate old runtime
            # templates to DoH while keeping the same tag used by routing rules.
            server.clear()
            server.update(
                {
                    "tag": "proxy-dns",
                    "type": "https",
                    "server": "dns.google",
                    "server_port": 443,
                    "path": "/dns-query",
                    "domain_resolver": "bootstrap-dns",
                    "detour": "proxy",
                }
            )
            continue


def _singbox_uses_builtin_dns(routing: RoutingSettings | None) -> bool:
    return str(routing.dns_mode if routing is not None else "builtin").strip().lower() == "builtin"


def _ensure_singbox_tun_base_rules(
    rules: list[Any],
    *,
    enable_final_fragment: bool = True,
    dns_hijack: bool = True,
) -> None:
    base_rules = [{"action": "sniff"}]
    if dns_hijack:
        base_rules.append(
            {
            "type": "logical",
            "mode": "or",
            "action": "hijack-dns",
            "rules": [
                {"port": 53},
                {"protocol": "dns"},
            ],
            }
        )
    if enable_final_fragment:
        base_rules.append(
            {
                "protocol": ["tls"],
                "action": "route-options",
                "tls_record_fragment": True,
            }
        )
    base_rules.extend(
        [
            {
                "network": "udp",
                "port": 443,
                "action": "reject",
            },
            {
                "network": "udp",
                "port": [135, 137, 138, 139, 5353],
                "action": "reject",
            },
            {
                "ip_cidr": ["224.0.0.0/3", "ff00::/8"],
                "action": "reject",
            },
        ]
    )
    rules[:] = [rule for rule in rules if not _is_singbox_tun_base_rule(rule)]
    rules[0:0] = base_rules


def _remove_singbox_tls_fragment_rule(payload: dict[str, Any]) -> None:
    route = payload.get("route")
    if not isinstance(route, dict):
        return
    rules = route.get("rules")
    if not isinstance(rules, list):
        return
    rules[:] = [
        rule
        for rule in rules
        if not _is_app_singbox_tls_fragment_rule(rule)
    ]


def _is_singbox_tun_base_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    action = str(rule.get("action") or "")
    if action == "sniff":
        return True
    if action == "hijack-dns":
        return True
    if _is_app_singbox_tls_fragment_rule(rule):
        return True
    if rule.get("protocol") in ("dns", ["dns"]):
        return True
    if rule.get("port") in (53, [53]) and action in {"hijack-dns", ""}:
        return True
    if action == "reject" and rule.get("network") == "udp" and rule.get("port") == 443:
        return True
    if action == "reject" and rule.get("network") == "udp" and rule.get("port") == [135, 137, 138, 139, 5353]:
        return True
    return action == "reject" and rule.get("ip_cidr") == ["224.0.0.0/3", "ff00::/8"]


def _is_app_singbox_tls_fragment_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    return (
        str(rule.get("action") or "") == "route-options"
        and rule.get("protocol") == ["tls"]
        and rule.get("tls_record_fragment") is True
        and set(rule) <= {"protocol", "action", "tls_record_fragment"}
    )


def _validate_runtime_dns_contract(payload: dict[str, Any]) -> None:
    dns = payload.get("dns")
    server_tags: set[str] = set()
    if isinstance(dns, dict):
        for server in dns.get("servers") or []:
            if not isinstance(server, dict):
                continue
            tag = str(server.get("tag") or "").strip()
            if tag:
                server_tags.add(tag)

    missing_refs: list[str] = []

    def require_dns_tag(tag: str, owner: str) -> None:
        if not tag or tag in server_tags:
            return
        missing_refs.append(f"{owner} -> {tag}")

    route = payload.get("route")
    if isinstance(route, dict):
        require_dns_tag(_extract_dns_server_tag(route.get("default_domain_resolver")), "route.default_domain_resolver")

    if isinstance(dns, dict):
        require_dns_tag(_extract_dns_server_tag(dns.get("final")), "dns.final")
        for index, rule in enumerate(dns.get("rules") or []):
            if not isinstance(rule, dict):
                continue
            require_dns_tag(_extract_dns_server_tag(rule.get("server")), f"dns.rules[{index}].server")

    for index, outbound in enumerate(payload.get("outbounds") or []):
        if not isinstance(outbound, dict):
            continue
        require_dns_tag(
            _extract_dns_server_tag(outbound.get("domain_resolver")),
            f"outbounds[{index}].domain_resolver",
        )

    if not missing_refs:
        return

    details = "; ".join(dict.fromkeys(missing_refs))
    raise ValueError(
        "В sing-box конфиге отсутствует DNS-сервер с нужным tag. "
        f"Проверьте раздел dns.servers: {details}. "
        "Обычно для стандартного шаблона должны существовать теги `bootstrap-dns` и `proxy-dns`."
    )


def _find_proxy_outbound_index(outbounds: Any) -> int | None:
    if not isinstance(outbounds, list):
        return None
    for index, outbound in enumerate(outbounds):
        if isinstance(outbound, dict) and str(outbound.get("tag") or "") == "proxy":
            return index
    return None


def _config_has_proxy_outbound(payload: Any) -> bool:
    return _find_proxy_outbound_index(payload.get("outbounds") if isinstance(payload, dict) else None) is not None


def _replace_or_append_tagged(items: list[Any], tag: str, payload: dict[str, Any]) -> None:
    for index, item in enumerate(items):
        if isinstance(item, dict) and str(item.get("tag") or "") == tag:
            items[index] = payload
            return
    items.append(payload)


def _is_singbox_endpoint(payload: Any) -> bool:
    if not isinstance(payload, dict):
        return False
    return str(payload.get("type") or "").strip().lower() in {"warp", "wireguard"}


def _ensure_dict(parent: dict[str, Any], key: str) -> dict[str, Any]:
    value = parent.get(key)
    if isinstance(value, dict):
        return value
    created: dict[str, Any] = {}
    parent[key] = created
    return created


def _ensure_list(parent: dict[str, Any], key: str) -> list[Any]:
    value = parent.get(key)
    if isinstance(value, list):
        return value
    created: list[Any] = []
    parent[key] = created
    return created


def _extract_dns_server_tag(value: Any) -> str:
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, dict):
        return str(value.get("server") or "").strip()
    return ""


def _find_free_port(
    *,
    preferred: int,
    port_range: range | None = None,
    excluded: set[int] | None = None,
) -> int:
    excluded = excluded or set()
    candidates: list[int] = []
    if preferred > 0:
        candidates.append(preferred)
    if port_range is None:
        port_range = range(preferred, preferred + 100)
    for port in port_range:
        if port not in candidates:
            candidates.append(port)
    for port in candidates:
        if port <= 0 or port in excluded:
            continue
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind((PROXY_HOST, port))
                return port
            except OSError:
                continue
    raise RuntimeError(f"No free TCP port available near {preferred}")


def _generate_ss_password(length: int = 24) -> str:
    _, password = generate_local_proxy_credentials(prefix="protect", password_length=length)
    return password


def _generate_tun_interface_name() -> str:
    return f"xftun{secrets.token_hex(3)}"


def _format_json_error_message(text: str, exc: json.JSONDecodeError) -> str:
    lines = text.splitlines()
    line = lines[exc.lineno - 1] if 0 < exc.lineno <= len(lines) else ""
    caret = ""
    if line:
        caret = "\n" + (" " * max(0, exc.colno - 1)) + "^"
    return f"Ошибка синтаксиса JSON: {exc.msg} (строка {exc.lineno}, столбец {exc.colno})\n{line}{caret}".rstrip()
