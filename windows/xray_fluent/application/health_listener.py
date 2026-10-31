# Dedicated authenticated health traffic, independent of split-routing rules.
from __future__ import annotations
import secrets
from typing import Any

PROXY_PROTOCOLS = frozenset({"vless", "vmess", "trojan", "shadowsocks", "socks", "http"})


def add_xray_health_listener(payload: dict[str, Any], *, port: int, outbound_tag: str = "proxy") -> str:
    # Runtime only. Caller has bound this outbound to the active node.
    matches = [o for o in payload.get("outbounds", []) if isinstance(o, dict) and o.get("tag") == outbound_tag]
    if len(matches) != 1 or matches[0].get("protocol") not in PROXY_PROTOCOLS:
        return ""
    if not isinstance(port, int) or isinstance(port, bool) or not 1 <= port <= 65535:
        return ""
    inbounds, routing = payload.get("inbounds"), payload.get("routing")
    if not isinstance(inbounds, list) or not isinstance(routing, dict) or not isinstance(routing.get("rules"), list):
        return ""
    if any(isinstance(i, dict) and str(i.get("port")) == str(port) for i in inbounds):
        return ""
    tag = "__app_health_" + secrets.token_hex(12)
    username, password = "lumen-health", secrets.token_urlsafe(32)
    inbounds.append({
        "tag": tag, "listen": "127.0.0.1", "port": port, "protocol": "http",
        "settings": {"accounts": [{"user": username, "pass": password}]},
        "sniffing": {"enabled": False},
    })
    routing["rules"].insert(0, {"type": "field", "inboundTag": [tag], "outboundTag": outbound_tag})
    return "http" + "://" + username + ":" + password + "@127.0.0.1:" + str(port)


def add_singbox_health_listener(payload: dict[str, Any], *, port: int, outbound_tag: str = "proxy") -> str:
    if not isinstance(port, int) or isinstance(port, bool) or not 1 <= port <= 65535:
        return ""
    from ..traffic_route_classifier import RouteClassifier
    graph = {o.get("tag"): o for o in [*payload.get("outbounds", []), *payload.get("endpoints", [])] if isinstance(o, dict)}
    # Selector targets can change, but this listener always follows the active
    # selector. Unknown/full imported graphs remain on API-only health checks.
    target = graph.get(outbound_tag, {})
    if target.get("type") == "selector":
        graph[outbound_tag] = {**target, "now": target.get("default"), "all": target.get("outbounds", [])}
    if RouteClassifier(graph).classify([outbound_tag]) != "proxy":
        return ""
    inbounds = payload.setdefault("inbounds", [])
    if any(i.get("listen_port") == port for i in inbounds if isinstance(i, dict)):
        return ""
    tag = "__app_health_" + secrets.token_hex(12)
    username, password = "lumen-health", secrets.token_urlsafe(32)
    inbounds.append({"type": "http", "tag": tag, "listen": "127.0.0.1", "listen_port": port,
                     "users": [{"username": username, "password": password}]})
    payload.setdefault("route", {}).setdefault("rules", []).insert(
        0, {"inbound": [tag], "action": "route", "outbound": outbound_tag},
    )
    return f"http://{username}:{password}@127.0.0.1:{port}"
