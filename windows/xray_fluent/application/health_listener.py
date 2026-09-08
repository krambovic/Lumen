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
