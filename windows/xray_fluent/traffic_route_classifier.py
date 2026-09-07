"""Conservative route classification from *types*, never outbound tag names.

Clash chains may be leaf-first and may contain arbitrary selector labels.
Only route metadata is retained; imported outbound credentials are not copied.
"""
from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from types import MappingProxyType
from typing import Any

_DIRECT_TYPES = frozenset({"direct", "freedom"})
_PROXY_TYPES = frozenset({
    "socks", "socks5", "http", "https", "shadowsocks", "shadowsocksr",
    "ss", "ssr", "vmess", "vless", "trojan", "hysteria", "hysteria2",
    "hy2", "tuic", "wireguard", "amneziawg", "awg", "ssh", "tor",
    "naive", "naiveproxy", "anytls", "shadowtls", "masque", "openvpn",
})
_GROUP_TYPES = frozenset({
    "selector", "select", "urltest", "fallback", "loadbalance", "relay",
})


def _type_name(value: Any) -> str:
    return str(value or "").lower().replace("-", "").replace("_", "").replace(" ", "")


def _tags(value: Any) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)):
        return ()
    return tuple(dict.fromkeys(v for v in value if isinstance(v, str) and v))


def _detour_tag(item: Mapping[str, Any]) -> str:
    proxy = item.get("proxySettings")
    stream = item.get("streamSettings")
    sockopt = stream.get("sockopt") if isinstance(stream, Mapping) else None
    return str(item.get("detour")
               or (proxy.get("tag") if isinstance(proxy, Mapping) else "")
               or (sockopt.get("dialerProxy") if isinstance(sockopt, Mapping) else "") or "")


@dataclass(frozen=True, slots=True)
class OutboundInfo:
    kind: str
    choices: tuple[str, ...] = ()
    selected: str = ""
    detour: str = ""


class RouteClassifier:
    """Immutable graph accepting Clash /proxies or runtime type metadata.

    A runtime configuration's ``default`` is NOT a current selector choice.
    ``now`` is used only when the chain provides no actual selected member.
    """

    def __init__(self, document: Mapping[str, Any] | None = None) -> None:
        document = document or {}
        source = document.get("proxies", document)
        if isinstance(document.get("outbounds"), (list, tuple)):
            records = list(document["outbounds"]) + list(document.get("endpoints") or ())
            source = {r.get("tag"): r for r in records if isinstance(r, Mapping) and r.get("tag")}
        nodes: dict[str, OutboundInfo] = {}
        if isinstance(source, Mapping):
            for tag, item in source.items():
                if not isinstance(tag, str) or not isinstance(item, Mapping):
                    continue
                nodes[tag] = OutboundInfo(
                    kind=_type_name(item.get("type") or item.get("protocol")),
                    choices=_tags(item.get("all") or item.get("outbounds")),
                    selected=str(item.get("now") or ""),
                    detour=_detour_tag(item),
                )
        self.nodes = MappingProxyType(nodes)

    def classify(self, chains: Sequence[str] | None) -> str:
        if not isinstance(chains, (list, tuple)) or not chains:
            return "unknown"
        if any(not isinstance(tag, str) or not tag or tag not in self.nodes for tag in chains):
            return "unknown"
        chain = frozenset(chains)
        routes = {self._resolve(tag, chain, frozenset()) for tag in chains}
        return next(iter(routes)) if len(routes) == 1 else "unknown"

    def _resolve(self, tag: str, chain: frozenset[str], visiting: frozenset[str]) -> str:
        node = self.nodes.get(tag)
        if node is None or tag in visiting or len(visiting) >= 32:
            return "unknown"
        visiting = visiting | {tag}
        if node.kind in _DIRECT_TYPES:
            return self._resolve(node.detour, chain, visiting) if node.detour else "direct"
        if node.kind in _PROXY_TYPES:
            if node.detour and self._resolve(node.detour, chain, visiting) == "unknown":
                return "unknown"
            return "proxy"
        if node.kind not in _GROUP_TYPES:
            return "unknown"
        # The connection's recorded chain beats a selector's *current* choice:
        # a hot-switch must not relabel bytes on an older open connection.
        members = tuple(member for member in node.choices if member in chain)
        if not members:
            if node.selected:
                if node.choices and node.selected not in node.choices:
                    return "unknown"
                members = (node.selected,)
            else:
                members = node.choices
        if not members:
            return "unknown"
        routes = {self._resolve(member, chain, visiting) for member in members}
        return next(iter(routes)) if len(routes) == 1 else "unknown"


def classify_route(chains: Sequence[str] | None, outbounds: Mapping[str, Any] | None = None) -> str:
    return RouteClassifier(outbounds).classify(chains)
