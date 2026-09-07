# Single probe contract: endpoint reachability is never profile health.
from dataclasses import dataclass

XRAY_PROTOCOLS = frozenset({"vless", "vmess", "trojan", "shadowsocks", "ss", "socks", "http"})
NATIVE_PROTOCOLS = frozenset({"awg", "wireguard", "warp", "hysteria", "hysteria2", "hy", "hy2", "tuic", "masque", "openvpn", "mieru", "naive", "anytls", "snell"})
UDP_PROTOCOLS = frozenset({"awg", "wireguard", "warp", "hysteria", "hysteria2", "hy", "hy2", "tuic", "masque"})
GROUP_TYPES = frozenset({"selector", "url-test", "urltest"})
NON_PROXY_TYPES = frozenset({"direct", "freedom", "block", "blackhole", "dns"})


def node_protocol(node) -> str:
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    return str(outbound.get("protocol") or outbound.get("type") or node.scheme or "").strip().lower()


def probe_backend(node) -> str:
    protocol = node_protocol(node)
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    if protocol == "xray_config":
        return "xray"
    if protocol == "singbox_config" or protocol in NATIVE_PROTOCOLS or (protocol in XRAY_PROTOCOLS and isinstance(outbound.get("singbox"), dict)):
        return "singbox"
    return "xray" if protocol in XRAY_PROTOCOLS else ""


def endpoint_method(node, requested: str) -> str:
    if requested == "icmp":
        return "icmp"
    protocol = node_protocol(node)
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    native = outbound.get("singbox", outbound)
    native = native if isinstance(native, dict) else {}
    transport = str(native.get("transport") or native.get("protocol") or "").lower()
    if protocol in UDP_PROTOCOLS or (protocol == "openvpn" and not transport.startswith("tcp")) or (protocol == "mieru" and transport == "udp"):
        # Unsolicited UDP datagrams cannot verify an authenticated handshake.
        return "icmp"
    return "tcping"


@dataclass(frozen=True, slots=True)
class ProbeCapability:
    supported: bool
    method: str
    backend: str = ""
    reason: str = ""


def probe_capability(node, method: str) -> ProbeCapability:
    backend = probe_backend(node)
    if method in {"real", "http", "speed"}:
        return ProbeCapability(bool(backend), "proxy", backend, "Нет совместимого адаптера профиля" if not backend else "")
    actual = endpoint_method(node, method)
    try:
        port_valid = 0 < int(node.port or 0) < 65536
    except (TypeError, ValueError):
        port_valid = False
    supported = bool(str(node.server or "").strip() and (actual == "icmp" or port_valid))
    return ProbeCapability(supported, actual + "_endpoint", reason="Нет адреса endpoint" if not supported else "")
