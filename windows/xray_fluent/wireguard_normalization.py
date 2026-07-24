from __future__ import annotations

import base64
from copy import deepcopy
import ipaddress
import re
from typing import Any


def normalize_ip_prefix(value: Any) -> str:
    """Return a sing-box compatible IP prefix, adding a host mask when absent."""
    text = str(value or "").strip().strip('"').strip("'")
    if not text:
        return ""
    if "/" in text:
        try:
            return str(ipaddress.ip_interface(text))
        except ValueError:
            return text
    try:
        address = ipaddress.ip_address(text)
    except ValueError:
        return text
    return f"{address}/{32 if address.version == 4 else 128}"


def normalize_ip_prefixes(value: Any) -> list[str]:
    if isinstance(value, (list, tuple, set)):
        raw_items = list(value)
    else:
        raw_items = str(value or "").split(",")
    result: list[str] = []
    for item in raw_items:
        normalized = normalize_ip_prefix(item)
        if normalized and normalized not in result:
            result.append(normalized)
    return result


def normalize_wireguard_endpoint(endpoint: dict[str, Any]) -> dict[str, Any]:
    """Return the current sing-box-extended WireGuard endpoint representation.

    sing-box 1.13 removed the legacy WireGuard outbound.  The extended fork
    follows the new endpoint schema, but still receives old outbound-shaped
    profiles from providers.  Keep this conversion in one place so links,
    native JSON documents and saved nodes all produce the same runtime shape.
    """
    result = deepcopy(endpoint)
    endpoint_type = str(result.get("type") or "").strip().lower()
    if endpoint_type == "warp":
        result["system"] = False
        return result
    if endpoint_type not in {"wireguard", "awg"}:
        return result
    if endpoint_type == "awg":
        result["type"] = "wireguard"

    legacy_server = str(result.pop("server", "") or "").strip()
    legacy_port = _positive_int(result.pop("server_port", 0), 51820)
    legacy_public_key = str(result.pop("peer_public_key", "") or "").strip()
    legacy_pre_shared_key = str(result.pop("pre_shared_key", "") or "").strip()
    legacy_allowed_ips = result.pop("allowed_ips", None)
    legacy_reserved = _reserved_bytes(result.pop("reserved", None))

    local_address = result.pop("local_address", None)
    result["address"] = normalize_ip_prefixes(
        result.get("address") if result.get("address") not in (None, "", []) else local_address
    )
    # Lumen is a Windows TUN client.  A system WireGuard interface routes its
    # own handshake back into the catch-all TUN route; userspace mode is the
    # supported sing-box endpoint mode for this topology.
    result["system"] = False

    peers = result.get("peers")
    if not isinstance(peers, list) or not peers:
        if legacy_server or legacy_public_key:
            peers = [
                {
                    "address": legacy_server,
                    "port": legacy_port,
                    "public_key": legacy_public_key,
                    "pre_shared_key": legacy_pre_shared_key,
                    "allowed_ips": legacy_allowed_ips or ["0.0.0.0/0", "::/0"],
                }
            ]
        else:
            peers = []

    if isinstance(peers, list):
        normalized_peers: list[Any] = []
        for peer in peers:
            if not isinstance(peer, dict):
                normalized_peers.append(peer)
                continue
            normalized_peer = deepcopy(peer)
            peer_endpoint = str(normalized_peer.pop("endpoint", "") or "").strip()
            peer_server = str(
                normalized_peer.pop("server", "")
                or normalized_peer.get("address")
                or ""
            ).strip()
            peer_port = _positive_int(
                normalized_peer.pop("server_port", 0)
                or normalized_peer.get("port"),
                51820,
            )
            if peer_endpoint:
                endpoint_host, endpoint_port = _split_endpoint(peer_endpoint)
                peer_server = endpoint_host or peer_server
                peer_port = endpoint_port or peer_port
            normalized_peer["address"] = peer_server
            normalized_peer["port"] = peer_port
            if not normalized_peer.get("public_key"):
                normalized_peer["public_key"] = str(
                    normalized_peer.pop("peer_public_key", "") or ""
                ).strip()
            normalized_peer["allowed_ips"] = normalize_ip_prefixes(
                normalized_peer.get("allowed_ips") or ["0.0.0.0/0", "::/0"]
            )
            peer_reserved = _reserved_bytes(normalized_peer.pop("reserved", None))
            if peer_reserved:
                if len(peers) == 1 and _is_cloudflare_warp_peer(peer_server):
                    legacy_reserved = peer_reserved
                else:
                    raise ValueError(
                        "sing-box extended 2.5.x supports reserved bytes only through a WARP endpoint"
                    )
            normalized_peers.append(normalized_peer)
        result["peers"] = normalized_peers

    if legacy_reserved:
        first_peer = result["peers"][0] if result.get("peers") else {}
        peer_server = str(first_peer.get("address") or legacy_server or "")
        if not _is_cloudflare_warp_peer(peer_server):
            raise ValueError(
                "sing-box extended 2.5.x supports reserved bytes only for Cloudflare WARP profiles"
            )
        warp_endpoint: dict[str, Any] = {
            "type": "warp",
            "tag": str(result.get("tag") or "proxy"),
            "system": False,
            "udp_timeout": str(result.get("udp_timeout") or "5m0s"),
            "profile": {
                "detour": "direct",
                "private_key": str(result.get("private_key") or ""),
            },
            "reserved": legacy_reserved,
        }
        keepalive = first_peer.get("persistent_keepalive_interval")
        if keepalive not in (None, ""):
            warp_endpoint["persistent_keepalive_interval"] = int(keepalive)
        for key in (
            "listen_port",
            "workers",
            "preallocated_buffers_per_pool",
            "disable_pauses",
            "amnezia",
            "domain_resolver",
            "detour",
        ):
            if result.get(key) not in (None, "", [], {}):
                warp_endpoint[key] = deepcopy(result[key])
        return warp_endpoint
    return result


def normalize_singbox_wireguard_endpoints(payload: dict[str, Any]) -> None:
    """Normalize endpoints and migrate removed WireGuard outbounds in-place."""
    endpoints = payload.get("endpoints")
    had_endpoint_list = isinstance(endpoints, list)
    if not had_endpoint_list:
        endpoints = []

    normalized_endpoints: list[Any] = []
    for endpoint in endpoints:
        if isinstance(endpoint, dict):
            normalized_endpoints.append(normalize_wireguard_endpoint(endpoint))
        else:
            normalized_endpoints.append(endpoint)

    outbounds = payload.get("outbounds")
    if isinstance(outbounds, list):
        retained_outbounds: list[Any] = []
        for outbound in outbounds:
            outbound_type = (
                str(outbound.get("type") or "").strip().lower()
                if isinstance(outbound, dict)
                else ""
            )
            if outbound_type in {"wireguard", "awg"}:
                migrated = normalize_wireguard_endpoint(outbound)
                tag = str(migrated.get("tag") or "").strip()
                if tag:
                    normalized_endpoints = [
                        item
                        for item in normalized_endpoints
                        if not (
                            isinstance(item, dict)
                            and str(item.get("tag") or "").strip() == tag
                        )
                    ]
                normalized_endpoints.append(migrated)
            else:
                retained_outbounds.append(outbound)
        payload["outbounds"] = retained_outbounds

    if had_endpoint_list or normalized_endpoints:
        payload["endpoints"] = normalized_endpoints


def _positive_int(value: Any, fallback: int) -> int:
    try:
        parsed = int(value or 0)
    except (TypeError, ValueError):
        return fallback
    return parsed if parsed > 0 else fallback


def _split_endpoint(value: str) -> tuple[str, int]:
    text = str(value or "").strip()
    if not text:
        return "", 0
    if text.startswith("["):
        closing = text.find("]")
        if closing > 0:
            host = text[1:closing]
            remainder = text[closing + 1 :]
            return host, _positive_int(remainder[1:], 0) if remainder.startswith(":") else 0
    host, separator, port_text = text.rpartition(":")
    if separator and host and port_text.isdigit():
        return host, int(port_text)
    return text, 0


def _reserved_bytes(value: Any) -> list[int]:
    if value in (None, "", [], ()):
        return []
    if isinstance(value, (list, tuple)):
        parts = list(value)
    else:
        text = str(value).strip("[] ")
        parts = [part for part in re.split(r"[\s,;]+", text) if part]
        if len(parts) == 1:
            encoded = parts[0].replace("-", "+").replace("_", "/")
            try:
                decoded = base64.b64decode(
                    encoded + "=" * (-len(encoded) % 4),
                    validate=True,
                )
            except (ValueError, TypeError):
                decoded = b""
            if len(decoded) == 3:
                return list(decoded)
    try:
        parsed = [int(part) for part in parts]
    except (TypeError, ValueError):
        return []
    return parsed if len(parsed) == 3 and all(0 <= item <= 255 for item in parsed) else []


def _is_cloudflare_warp_peer(value: str) -> bool:
    host = str(value or "").strip().lower().strip(".")
    if host == "engage.cloudflareclient.com" or host.endswith(".cloudflareclient.com"):
        return True
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        return False
    return any(
        address in network
        for network in (
            ipaddress.ip_network("162.159.192.0/24"),
            ipaddress.ip_network("162.159.193.0/24"),
            ipaddress.ip_network("188.114.96.0/20"),
            ipaddress.ip_network("2606:4700:d0::/48"),
            ipaddress.ip_network("2606:4700:d1::/48"),
        )
    )
