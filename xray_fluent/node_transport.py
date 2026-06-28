from __future__ import annotations

from typing import Any

from .models import Node


_UDP_PROTOCOLS = {"awg", "hysteria", "hysteria2", "masque", "tuic", "warp", "wireguard"}
_TCP_PROTOCOLS = {"http", "shadowsocks", "socks", "trojan", "vless", "vmess"}


def node_transport(node: Node) -> str:
    """Return a compact transport label for server lists and sorting."""
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    stream = outbound.get("streamSettings")
    if isinstance(stream, dict):
        network = _normalize_transport(stream.get("network"))
        if network:
            return network

    native = outbound.get("singbox")
    if isinstance(native, dict):
        native_type = str(native.get("type") or "").strip().lower()
        transport = native.get("transport")
        if isinstance(transport, str) and native_type == "mieru":
            return f"MIERU/{transport.upper()}"
        if native_type in {"mieru", "masque"}:
            return native_type.upper()
        if isinstance(transport, dict):
            transport_type = _normalize_transport(transport.get("type"))
            if transport_type:
                return transport_type

    protocol = str(outbound.get("protocol") or node.scheme or "").strip().lower()
    if protocol in _UDP_PROTOCOLS:
        return "UDP"
    if protocol in {"mieru", "singbox_config"}:
        return protocol.upper()
    if protocol in _TCP_PROTOCOLS:
        return "TCP"
    return "—"


def _normalize_transport(value: Any) -> str:
    network = str(value or "").strip().lower()
    aliases = {"raw": "TCP", "http": "H2"}
    if network in aliases:
        return aliases[network]
    return network.upper() if network else ""
