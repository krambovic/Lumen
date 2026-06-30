from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import hashlib
from ipaddress import ip_address, ip_network
import json
import socket
import threading
import time
from pathlib import Path
from typing import Any

from ...application.runtime_security import (
    clamp_singbox_local_inbounds,
    generate_local_proxy_credentials,
    strip_singbox_proxy_inbounds,
)
from ...constants import (
    DATA_DIR,
    DEFAULT_DISCORD_SOCKS_PORT,
    DEFAULT_HTTP_PORT,
    DEFAULT_SOCKS_PORT,
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
_APP_TUN_MIXED_INBOUND_TAG = "socks-in"
_APP_TUN_HTTP_INBOUND_TAG = "http-in"
_ENDPOINT_DNS_CACHE_TTL_SECONDS = 300.0
_DEFAULT_DIRECT_DNS_SERVER = "8.8.8.8"
_DEFAULT_DIRECT_DNS_TYPE = "udp"
_DEFAULT_PROXY_DNS_SERVER = "8.8.8.8"
_DEFAULT_PROXY_DNS_TYPE = "https"
_SINGBOX_DHCP_AUTO_DNS = "dhcp://auto"
_KNOWN_DOH_IP_HOSTS = {
    "1.0.0.1": "cloudflare-dns.com",
    "1.1.1.1": "cloudflare-dns.com",
    "8.8.4.4": "dns.google",
    "8.8.8.8": "dns.google",
    "9.9.9.9": "dns.quad9.net",
    "149.112.112.112": "dns.quad9.net",
}
_BROWSER_DOH_DOMAIN_SUFFIXES = [
    "cloudflare-dns.com",
    "dns.adguard.com",
    "dns.alidns.com",
    "dns.cloudflare.com",
    "dns.google",
    "dns.nextdns.io",
    "dns.quad9.net",
    "doh.dns.sb",
    "doh.opendns.com",
    "doh.pub",
    "doh.sb",
    "mozilla.cloudflare-dns.com",
    "one.one.one.one",
]
_endpoint_dns_cache: dict[tuple[str, int], tuple[float, tuple[str, ...]]] = {}
_endpoint_dns_cache_lock = threading.Lock()


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
    if _node_is_full_singbox_config(node):
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
    system_dns_servers: tuple[str, ...] = (),
) -> SingboxRuntimePlan:
    if _node_is_full_singbox_config(node):
        runtime_config = deepcopy((node.outbound or {}).get("singbox_config") or {})
        strip_singbox_proxy_inbounds(runtime_config)
        _ensure_tun_inbound(runtime_config)
        _ensure_singbox_metrics_contract(runtime_config)
        _ensure_singbox_tun_runtime_contract(
            runtime_config,
            routing=routing,
            enable_final_fragment=enable_final_fragment,
            system_dns_servers=system_dns_servers,
        )
        _ensure_full_config_proxy_alias(runtime_config)
        if routing is not None:
            apply_singbox_gui_routing(runtime_config, routing)
        _ensure_singbox_discord_proxy_contract(runtime_config, enabled=discord_proxy_enabled)
        clamp_singbox_local_inbounds(runtime_config)
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

    runtime_config = deepcopy(document.payload)
    strip_singbox_proxy_inbounds(runtime_config)
    _ensure_singbox_metrics_contract(runtime_config)
    _ensure_singbox_tun_runtime_contract(
        runtime_config,
        routing=routing,
        enable_final_fragment=enable_final_fragment,
        system_dns_servers=system_dns_servers,
    )

    outbounds = runtime_config.get("outbounds")
    proxy_index = _find_proxy_outbound_index(outbounds)
    if proxy_index is None:
        if routing is not None:
            apply_singbox_gui_routing(runtime_config, routing)
        _ensure_singbox_discord_proxy_contract(runtime_config, enabled=discord_proxy_enabled)
        clamp_singbox_local_inbounds(runtime_config)
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
        clamp_singbox_local_inbounds(plan.singbox_config)
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
    clamp_singbox_local_inbounds(runtime_config)
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
    clamp_singbox_local_inbounds(runtime_config)
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
    stream_settings = outbound.get("streamSettings")
    if isinstance(stream_settings, dict):
        network = str(stream_settings.get("network") or "").strip().lower()
        # sing-box-extended accepts VMess + XHTTP configs, but its current
        # client closes the stream before the destination TLS handshake with
        # Xray-compatible VMess XHTTP servers. Keep working VLESS/Trojan XHTTP
        # native and route only this combination through the Xray sidecar.
        if protocol == "vmess" and network == "xhttp":
            return True
        if protocol == "vless" and (network == "raw" or "finalmask" in stream_settings):
            return True
    return False


def _node_is_full_singbox_config(node: Node | None) -> bool:
    outbound = node.outbound if node is not None else None
    if not isinstance(outbound, dict):
        return False
    return str(outbound.get("protocol") or node.scheme or "").strip().lower() == "singbox_config" and isinstance(
        outbound.get("singbox_config"),
        dict,
    )


def _ensure_full_config_proxy_alias(config: dict[str, Any]) -> None:
    outbounds = config.get("outbounds")
    if not isinstance(outbounds, list):
        return
    tags = {
        str(outbound.get("tag") or "")
        for outbound in outbounds
        if isinstance(outbound, dict)
    }
    if "proxy" in tags:
        return
    route = config.get("route") if isinstance(config.get("route"), dict) else {}
    preferred = str(route.get("final") or "").strip()
    ignored = {"", "direct", "block", "dns"}
    if not preferred or preferred in ignored or preferred not in tags:
        for outbound in outbounds:
            if not isinstance(outbound, dict):
                continue
            tag = str(outbound.get("tag") or "").strip()
            outbound_type = str(outbound.get("type") or "").strip().lower()
            if tag and tag not in ignored and outbound_type not in ignored:
                preferred = tag
                break
    if not preferred or preferred in ignored:
        return
    outbounds.append(
        {
            "type": "selector",
            "tag": "proxy",
            "outbounds": [preferred],
            "default": preferred,
            "interrupt_exist_connections": True,
        }
    )


def _ensure_tun_inbound(config: dict[str, Any]) -> None:
    inbounds = _ensure_list(config, "inbounds")
    if any(isinstance(inbound, dict) and str(inbound.get("type") or "").strip().lower() == "tun" for inbound in inbounds):
        return
    inbounds.insert(
        0,
        {
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "singbox_tun",
        },
    )


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
    if _is_dhcp_auto_dns(host):
        return False
    try:
        ip_address(host)
    except ValueError:
        return True
    return False


def _is_dhcp_auto_dns(value: str) -> bool:
    return str(value or "").strip().lower() == _SINGBOX_DHCP_AUTO_DNS


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
    endpoint_addresses = _resolve_endpoint_addresses(server)
    endpoint_cidrs = [_endpoint_ip_cidr(address) for address in endpoint_addresses]
    if endpoint_cidrs:
        _pin_proxy_outbound_to_endpoint_ip(proxy_outbound, server, endpoint_addresses[0])
        _ensure_tun_route_exclude_addresses(payload, endpoint_cidrs)
        for cidr in endpoint_cidrs:
            _ensure_direct_ip_route(payload, cidr)
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

    resolved_by_host: dict[str, list[str]] = {}
    endpoint_cidrs: list[str] = []
    for host in hosts:
        cidr = _endpoint_ip_cidr(host)
        if cidr:
            endpoint_cidrs.append(cidr)
        elif _is_domain_name(host):
            resolved_by_host[host] = _resolve_endpoint_ip_cidrs(host)
            endpoint_cidrs.extend(resolved_by_host[host])
    _ensure_tun_route_exclude_addresses(payload, [cidr for cidr in endpoint_cidrs if cidr])
    for host in hosts:
        endpoint_cidr = _endpoint_ip_cidr(host)
        if endpoint_cidr:
            _ensure_direct_ip_route(payload, endpoint_cidr)
        elif _is_domain_name(host):
            for cidr in resolved_by_host.get(host, []):
                _ensure_direct_ip_route(payload, cidr)
            _ensure_direct_domain_route(payload, host)


def _endpoint_ip_cidr(value: str) -> str:
    try:
        address = ip_address(str(value or "").strip())
    except ValueError:
        return ""
    return f"{address}/{'128' if address.version == 6 else '32'}"


def _pin_proxy_outbound_to_endpoint_ip(proxy_outbound: dict[str, Any], original_server: str, endpoint_ip: str) -> None:
    if not endpoint_ip:
        return
    proxy_outbound["server"] = endpoint_ip
    proxy_outbound.pop("domain_resolver", None)
    tls = proxy_outbound.get("tls")
    if isinstance(tls, dict) and not str(tls.get("server_name") or "").strip():
        tls["server_name"] = original_server


def _resolve_endpoint_ip_cidrs(host: str) -> list[str]:
    return [_endpoint_ip_cidr(address) for address in _resolve_endpoint_addresses(host)]


def _resolve_endpoint_addresses(host: str) -> list[str]:
    if not _is_domain_name(host):
        return []
    normalized_host = str(host).strip().lower()
    cache_key = (normalized_host, id(socket.getaddrinfo))
    with _endpoint_dns_cache_lock:
        cached = _endpoint_dns_cache.get(cache_key)
        if cached and time.monotonic() - cached[0] < _ENDPOINT_DNS_CACHE_TTL_SECONDS:
            return list(cached[1])
    result: list[Any] = []
    completed = threading.Event()

    def resolve() -> None:
        try:
            result.extend(socket.getaddrinfo(normalized_host, None, 0, socket.SOCK_STREAM))
        except OSError:
            pass
        finally:
            addresses = _resolved_addresses(result)
            if addresses:
                with _endpoint_dns_cache_lock:
                    _endpoint_dns_cache[cache_key] = (time.monotonic(), tuple(addresses))
            completed.set()

    threading.Thread(target=resolve, name="tun-endpoint-resolver", daemon=True).start()
    if not completed.wait(1.2):
        return []

    return _resolved_addresses(result)


def _resolved_addresses(result: list[Any]) -> list[str]:
    addresses: list[str] = []
    for info in result:
        try:
            sockaddr = info[4]
            if not sockaddr:
                continue
            address = ip_address(str(sockaddr[0]))
        except (IndexError, TypeError, ValueError):
            continue
        addresses.append(str(address))
    return sorted(dict.fromkeys(addresses), key=lambda value: (ip_address(value).version, value))


def prime_endpoint_resolution(host: str) -> None:
    """Warm endpoint DNS without delaying application or connection startup."""
    if not _is_domain_name(host):
        return
    threading.Thread(
        target=_resolve_endpoint_addresses,
        args=(host,),
        name="tun-endpoint-prewarm",
        daemon=True,
    ).start()


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


def _ensure_singbox_tun_local_proxy_contract(payload: dict[str, Any]) -> None:
    """Keep v2rayN-style local proxy ports alive while TUN is running.

    Some browsers, Necko profiles, extensions and helper apps keep using
    127.0.0.1:10808/10809 even after the Windows system proxy is disabled for
    TUN. v2rayN keeps local proxy inbounds available in TUN mode, so do the same
    here instead of letting those requests time out outside sing-box.
    """
    inbounds = _ensure_list(payload, "inbounds")
    route = _ensure_dict(payload, "route")
    rules = _ensure_list(route, "rules")

    managed_tags = {_APP_TUN_MIXED_INBOUND_TAG, _APP_TUN_HTTP_INBOUND_TAG}
    inbounds[:] = [
        inbound
        for inbound in inbounds
        if not (isinstance(inbound, dict) and str(inbound.get("tag") or "").strip() in managed_tags)
    ]
    rules[:] = [rule for rule in rules if not _is_singbox_tun_local_proxy_rule(rule)]

    inbounds.extend(
        [
            {
                "type": "mixed",
                "tag": _APP_TUN_MIXED_INBOUND_TAG,
                "listen": PROXY_HOST,
                "listen_port": int(DEFAULT_SOCKS_PORT),
            },
            {
                "type": "http",
                "tag": _APP_TUN_HTTP_INBOUND_TAG,
                "listen": PROXY_HOST,
                "listen_port": int(DEFAULT_HTTP_PORT),
            },
        ]
    )
    rules.insert(
        _singbox_local_proxy_rule_insert_index(rules),
        {
            "inbound": [_APP_TUN_MIXED_INBOUND_TAG, _APP_TUN_HTTP_INBOUND_TAG],
            "action": "route",
            "outbound": "proxy",
        },
    )


def _is_singbox_tun_local_proxy_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    inbound = rule.get("inbound")
    tags: set[str]
    if isinstance(inbound, str):
        tags = {inbound}
    elif isinstance(inbound, list):
        tags = {str(item) for item in inbound}
    else:
        return False
    return bool(tags & {_APP_TUN_MIXED_INBOUND_TAG, _APP_TUN_HTTP_INBOUND_TAG}) and str(rule.get("outbound") or "") == "proxy"


def _singbox_local_proxy_rule_insert_index(rules: list[Any]) -> int:
    index = 0
    while index < len(rules):
        rule = rules[index]
        if not isinstance(rule, dict):
            break
        inbound = rule.get("inbound")
        if inbound:
            index += 1
            continue
        if rule.get("action") == "sniff":
            index += 1
            continue
        break
    return index


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


def _is_default_direct_dns(routing: RoutingSettings | None) -> bool:
    if routing is None:
        return True
    return (
        str(routing.dns_bootstrap_server or "").strip().lower() == _DEFAULT_DIRECT_DNS_SERVER
        and str(routing.dns_bootstrap_type or "").strip().lower() == _DEFAULT_DIRECT_DNS_TYPE
    )


def _is_default_proxy_dns(routing: RoutingSettings | None) -> bool:
    if routing is None:
        return True
    return (
        str(routing.dns_proxy_server or "").strip().lower() == _DEFAULT_PROXY_DNS_SERVER
        and str(routing.dns_proxy_type or "").strip().lower() == _DEFAULT_PROXY_DNS_TYPE
    )


def _build_dns_server(tag: str, server: str, server_type: str, strategy: str) -> dict[str, Any]:
    dns_type = str(server_type or "").strip().lower()
    address = str(server or "").strip() or ("8.8.8.8" if tag == "proxy-dns" else "1.1.1.1")
    if _is_dhcp_auto_dns(address):
        return {
            "tag": tag,
            "type": "dhcp",
        }
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
    address = str(server.get("server") or "")
    if _is_domain_name(address):
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
    log = _ensure_dict(payload, "log")
    if str(log.get("level") or "").strip().lower() in {"", "warn", "warning", "error", "fatal", "panic"}:
        log["level"] = "info"
    log.setdefault("timestamp", True)
    experimental = _ensure_dict(payload, "experimental")
    cache_file = _ensure_dict(experimental, "cache_file")
    cache_path = DATA_DIR / "runtime" / "sing-box-cache.db"
    try:
        cache_path.parent.mkdir(parents=True, exist_ok=True)
    except OSError:
        pass
    cache_file["enabled"] = True
    cache_file["path"] = str(cache_path)
    cache_file["store_fakeip"] = True
    clash_api = _ensure_dict(experimental, "clash_api")
    clash_api["external_controller"] = f"127.0.0.1:{SINGBOX_CLASH_API_PORT}"


def _ensure_singbox_tun_runtime_contract(
    payload: dict[str, Any],
    *,
    routing: RoutingSettings | None = None,
    enable_final_fragment: bool = True,
    system_dns_servers: tuple[str, ...] = (),
) -> None:
    """Patch app-owned runtime fields for raw sing-box configs.

    Keep the Windows adapter identity stable so route and DNS registration can
    be reused across reconnects. MTU and stack match v2rayN's current defaults.
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
            inbound["interface_name"] = "singbox_tun"
            inbound["address"] = ["172.18.0.1/30", "fdfe:dcba:9876::1/126"]
            inbound["mtu"] = 1280
            inbound["auto_route"] = True
            inbound.pop("route_address", None)
            inbound["strict_route"] = False
            inbound["stack"] = "gvisor"
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
    _ensure_singbox_dns_runtime_contract(
        payload,
        routing=routing,
        system_dns_servers=system_dns_servers,
    )
    rules = _ensure_list(route, "rules")
    _ensure_singbox_tun_base_rules(
        rules,
        enable_final_fragment=enable_final_fragment,
        builtin_dns=use_builtin_dns,
        dns_hijack_all=(routing.dns_hijack_enabled if routing is not None else True),
    )
    _ensure_singbox_tun_local_proxy_contract(payload)


def _ensure_singbox_dns_runtime_contract(
    payload: dict[str, Any],
    *,
    routing: RoutingSettings | None = None,
    system_dns_servers: tuple[str, ...] = (),
) -> None:
    dns = payload.get("dns")
    if not isinstance(dns, dict):
        dns = {}
        payload["dns"] = dns
    dns["independent_cache"] = True
    direct_server = routing.dns_bootstrap_server if routing is not None else _DEFAULT_DIRECT_DNS_SERVER
    direct_type = routing.dns_bootstrap_type if routing is not None else _DEFAULT_DIRECT_DNS_TYPE
    direct_strategy = _dns_strategy(routing.dns_bootstrap_strategy if routing is not None else "")
    proxy_server = routing.dns_proxy_server if routing is not None else _DEFAULT_PROXY_DNS_SERVER
    proxy_type = routing.dns_proxy_type if routing is not None else _DEFAULT_PROXY_DNS_TYPE
    proxy_strategy = _dns_strategy(routing.dns_proxy_strategy if routing is not None else "")
    use_builtin_dns = _singbox_uses_builtin_dns(routing)
    # TUN needs FakeIP as part of the runtime contract, not as an optional
    # cosmetic DNS mode. Without it, ECH-enabled sites can hide the hostname
    # from sniffing and fall into IP-only routing, which is exactly where
    # OpenAI/Anthropic/Gemini become hard to diagnose.
    fake_enabled = True
    direct_is_default = _is_default_direct_dns(routing)
    proxy_is_default = _is_default_proxy_dns(routing)
    runtime_direct_server = str(direct_server or "").strip()
    runtime_direct_type = str(direct_type or "").strip()
    runtime_proxy_server = str(proxy_server or "").strip()
    runtime_proxy_type = str(proxy_type or "").strip()
    if proxy_is_default:
        runtime_proxy_server = _DEFAULT_PROXY_DNS_SERVER
        runtime_proxy_type = _DEFAULT_PROXY_DNS_TYPE

    dns["strategy"] = direct_strategy
    dns["reverse_mapping"] = True
    servers = dns.setdefault("servers", [])
    if not isinstance(servers, list):
        servers = []
        dns["servers"] = servers
    system_server = next((str(item).strip() for item in system_dns_servers if str(item).strip()), "")
    if not system_server:
        system_server = runtime_direct_server if not _is_domain_name(str(runtime_direct_server)) else _DEFAULT_DIRECT_DNS_SERVER
    system_type = "udp" if system_dns_servers or _is_domain_name(str(runtime_direct_server)) else runtime_direct_type
    if use_builtin_dns:
        system_dns = _build_dns_server("system-dns", system_server, system_type, direct_strategy)
        _set_dns_server_dial_contract(system_dns, detour="direct")
        _replace_or_append_tagged(servers, "system-dns", system_dns)
        bootstrap_dns = _build_dns_server("bootstrap-dns", runtime_direct_server, runtime_direct_type, direct_strategy)
        _set_dns_server_dial_contract(bootstrap_dns, detour="direct")
        direct_dns = _build_dns_server("direct-dns", runtime_direct_server, runtime_direct_type, direct_strategy)
        _set_dns_server_dial_contract(direct_dns, detour="direct")
        _replace_or_append_tagged(servers, "bootstrap-dns", bootstrap_dns)
        _replace_or_append_tagged(servers, "direct-dns", direct_dns)
        proxy_dns = _build_dns_server("proxy-dns", runtime_proxy_server, runtime_proxy_type, proxy_strategy)
        _set_dns_server_dial_contract(proxy_dns, detour="proxy", resolver="bootstrap-dns")
    else:
        # In system mode keep direct/bootstrap DNS pinned to the physical
        # adapter snapshot captured before TUN starts. dhcp://auto can pick the
        # just-created virtual adapter on Windows and recurse into the tunnel.
        bootstrap_dns = _build_dns_server("bootstrap-dns", system_server, system_type, direct_strategy)
        direct_dns = _build_dns_server("direct-dns", system_server, system_type, direct_strategy)
        proxy_dns = _build_dns_server("proxy-dns", runtime_proxy_server, runtime_proxy_type, proxy_strategy)
        _set_dns_server_dial_contract(bootstrap_dns, detour="direct")
        _set_dns_server_dial_contract(direct_dns, detour="direct")
        _set_dns_server_dial_contract(proxy_dns, detour="proxy", resolver="bootstrap-dns")
        _replace_or_append_tagged(servers, "bootstrap-dns", bootstrap_dns)
        _replace_or_append_tagged(servers, "direct-dns", direct_dns)
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
        if tag == "proxy-dns" and server_type in {"tls", "https"} and address in _KNOWN_DOH_IP_HOSTS:
            # Remote/proxied DNS must use a hostname with a valid TLS identity.
            # DoH/DoT to a bare IP can silently break domains because the TLS
            # handshake has no provider hostname/SNI to present.
            server.clear()
            server.update(
                {
                    "tag": "proxy-dns",
                    "type": server_type,
                    "server": _KNOWN_DOH_IP_HOSTS[address],
                    "server_port": 443 if server_type == "https" else 853,
                    "domain_resolver": "bootstrap-dns",
                    "detour": "proxy",
                }
            )
            if server_type == "https":
                server["path"] = "/dns-query"
            continue

    _ensure_singbox_https_dns_reject_rule(dns)


# DNS query types 64 (SVCB) and 65 (HTTPS) carry ECH config and HTTP/3 hints.
# Answering them lets browsers encrypt the SNI (breaking sniff) and prefer QUIC.
# Rejecting them forces a clean fallback to A/AAAA (FakeIP) and TCP, which is how
# v2rayN-style TUN profiles keep ECH-enabled sites (OpenAI/Anthropic/Google)
# reachable.
_HTTPS_DNS_REJECT_RULE = {"query_type": ["HTTPS", "SVCB"], "action": "reject"}


def _is_https_dns_reject_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    if str(rule.get("action") or "") != "reject":
        return False
    query_type = rule.get("query_type")
    if not isinstance(query_type, list):
        return False
    return {str(item).strip().upper() for item in query_type} == {"HTTPS", "SVCB"}


def _is_browser_doh_dns_reject_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    if str(rule.get("action") or "") != "reject":
        return False
    suffixes = rule.get("domain_suffix")
    if not isinstance(suffixes, list):
        return False
    return [str(item) for item in suffixes] == list(_BROWSER_DOH_DOMAIN_SUFFIXES)


def _ensure_singbox_https_dns_reject_rule(dns: dict[str, Any]) -> None:
    rules = dns.get("rules")
    if not isinstance(rules, list):
        rules = []
        dns["rules"] = rules
    rules[:] = [
        rule
        for rule in rules
        if not _is_https_dns_reject_rule(rule)
        and not _is_browser_doh_dns_reject_rule(rule)
    ]
    rules.insert(
        0,
        {
            "domain_suffix": list(_BROWSER_DOH_DOMAIN_SUFFIXES),
            "action": "reject",
        },
    )
    rules.insert(0, deepcopy(_HTTPS_DNS_REJECT_RULE))

def _singbox_uses_builtin_dns(routing: RoutingSettings | None) -> bool:
    return str(routing.dns_mode if routing is not None else "builtin").strip().lower() == "builtin"


def _ensure_singbox_tun_base_rules(
    rules: list[Any],
    *,
    enable_final_fragment: bool = True,
    builtin_dns: bool = True,
    dns_hijack_all: bool = True,
) -> None:
    noisy_local_dns_rejects = [
        {
            "network": "udp",
            "port": [135, 137, 138, 139, 5353, 5355],
            "action": "reject",
        },
        {
            "ip_cidr": ["224.0.0.0/3", "ff00::/8"],
            "action": "reject",
        },
    ]
    # DNS hijack must be early and broad: some Windows/Chromium paths are
    # detected by sing-box as protocol=dns before a plain port rule matches.
    dns_hijack_rule = {
        "type": "logical",
        "mode": "or",
        "rules": [{"port": 53}, {"protocol": "dns"}],
        "action": "hijack-dns",
    }
    sniff_rule = {
        "action": "sniff",
        "timeout": "1s",
    }
    # Drop QUIC/HTTP3 coming from the local TUN so browsers fall back to TCP
    # HTTP/2. Scoped to tun-in only, so proxy UDP endpoints (WARP/AWG/Hysteria2)
    # that dial out from the outbound side are untouched.
    quic_reject_rule = {
        "inbound": ["tun-in"],
        "protocol": "quic",
        "action": "reject",
    }
    browser_doh_reject_rule = {
        "inbound": ["tun-in"],
        "domain_suffix": _BROWSER_DOH_DOMAIN_SUFFIXES,
        "action": "reject",
    }
    base_rules = [sniff_rule, quic_reject_rule, browser_doh_reject_rule, dns_hijack_rule, *noisy_local_dns_rejects]
    if enable_final_fragment:
        base_rules.append(
            {
                "protocol": ["tls"],
                "action": "route-options",
                "tls_fragment": True,
                "tls_fragment_fallback_delay": "500ms",
            }
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
    if (
        action == "reject"
        and rule.get("inbound") == ["tun-in"]
        and rule.get("protocol") == "quic"
    ):
        return True
    if _is_app_singbox_tls_fragment_rule(rule):
        return True
    if (
        action == "reject"
        and rule.get("inbound") == ["tun-in"]
        and rule.get("domain_suffix") == _BROWSER_DOH_DOMAIN_SUFFIXES
    ):
        return True
    if rule.get("protocol") in ("dns", ["dns"]):
        return True
    if rule.get("port") in (53, [53]) and action in {"hijack-dns", ""}:
        return True
    if action == "reject" and rule.get("network") == "udp" and rule.get("port") == 443:
        return True
    if action == "reject" and rule.get("network") == "udp" and rule.get("port") in (
        [135, 137, 138, 139, 5353],
        [135, 137, 138, 139, 5353, 5355],
    ):
        return True
    return action == "reject" and rule.get("ip_cidr") == ["224.0.0.0/3", "ff00::/8"]


def _is_app_singbox_tls_fragment_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    return (
        str(rule.get("action") or "") == "route-options"
        and rule.get("protocol") == ["tls"]
        and (rule.get("tls_record_fragment") is True or rule.get("tls_fragment") is True)
        and set(rule)
        <= {"protocol", "action", "tls_record_fragment", "tls_fragment", "tls_fragment_fallback_delay"}
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


def _format_json_error_message(text: str, exc: json.JSONDecodeError) -> str:
    lines = text.splitlines()
    line = lines[exc.lineno - 1] if 0 < exc.lineno <= len(lines) else ""
    caret = ""
    if line:
        caret = "\n" + (" " * max(0, exc.colno - 1)) + "^"
    return f"Ошибка синтаксиса JSON: {exc.msg} (строка {exc.lineno}, столбец {exc.colno})\n{line}{caret}".rstrip()
