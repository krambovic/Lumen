from __future__ import annotations

from copy import deepcopy
import ipaddress
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
    """Normalize WireGuard/AWG fields accepted by sing-box endpoint schema."""
    result = deepcopy(endpoint)
    endpoint_type = str(result.get("type") or "").strip().lower()
    if endpoint_type not in {"wireguard", "awg"}:
        return result
    if endpoint_type == "awg":
        result["type"] = "wireguard"

    result["address"] = normalize_ip_prefixes(result.get("address"))
    peers = result.get("peers")
    if isinstance(peers, list):
        normalized_peers: list[Any] = []
        for peer in peers:
            if not isinstance(peer, dict):
                normalized_peers.append(peer)
                continue
            normalized_peer = deepcopy(peer)
            normalized_peer["allowed_ips"] = normalize_ip_prefixes(
                normalized_peer.get("allowed_ips")
            )
            normalized_peers.append(normalized_peer)
        result["peers"] = normalized_peers
    return result


def normalize_singbox_wireguard_endpoints(payload: dict[str, Any]) -> None:
    endpoints = payload.get("endpoints")
    if not isinstance(endpoints, list):
        return
    for index, endpoint in enumerate(endpoints):
        if isinstance(endpoint, dict):
            endpoints[index] = normalize_wireguard_endpoint(endpoint)
