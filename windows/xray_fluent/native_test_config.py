# Isolated sing-box test listener; never creates TUN or a system proxy.
from copy import deepcopy
from .engines.singbox.config_builder import build_singbox_outbound
from .probe_capabilities import GROUP_TYPES, NON_PROXY_TYPES, node_protocol


def build_native_test_config(node, port: int) -> dict:
    if node_protocol(node) == "singbox_config":
        config = deepcopy(node.outbound.get("singbox_config"))
        if not isinstance(config, dict):
            raise ValueError("Full sing-box profile is missing")
        outbounds = config.get("outbounds", [])
        endpoints = config.get("endpoints", [])
        all_items = [item for item in [*outbounds, *endpoints] if isinstance(item, dict)]
        final = str((config.get("route") or {}).get("final") or "")
        if not final:
            if not all_items:
                raise ValueError("No profile outbounds")
            final = str(all_items[0].get("tag") or "")
        _validate_proxy_group(all_items, final)
    else:
        outbound = build_singbox_outbound(node, tag="speed-proxy")
        final = "speed-proxy"
        endpoint = outbound.get("type") in {"wireguard", "warp"}
        config = {"outbounds": [] if endpoint else [outbound]}
        if endpoint:
            config["endpoints"] = [outbound]
        config["dns"] = {"servers": [{"type": "local", "tag": "test-dns"}], "final": "test-dns"}
        config["route"] = {"default_domain_resolver": "test-dns"}
    # No provider listeners, shared cache, services or OS interfaces in tests.
    config["inbounds"] = [{"type": "mixed", "tag": "speed-http", "listen": "127.0.0.1", "listen_port": int(port)}]
    config["log"] = {"disabled": True}
    config.pop("experimental", None)
    config.pop("services", None)
    for item in config.get("endpoints", []):
        if isinstance(item, dict):
            item["system"] = False
    for item in config.get("outbounds", []):
        if isinstance(item, dict) and item.get("type") == "openvpn":
            item["system"] = False
    route = config.setdefault("route", {})
    route["rules"] = [{"inbound": ["speed-http"], "action": "route", "outbound": final}]
    route["final"] = final
    route.pop("auto_detect_interface", None)
    return config


def _validate_proxy_group(items: list[dict], root: str) -> None:
    by_tag = {str(item.get("tag") or ""): item for item in items}
    visiting, visited = set(), set()
    def visit(tag):
        if tag in visiting:
            raise ValueError("Cyclic profile selector")
        if tag in visited:
            return
        item = by_tag.get(tag)
        if item is None or str(item.get("type") or "") in NON_PROXY_TYPES:
            raise ValueError("A proxy test cannot use a missing/direct group member")
        visiting.add(tag)
        if str(item.get("type") or "") in GROUP_TYPES:
            members = item.get("outbounds")
            if not isinstance(members, list) or not members:
                raise ValueError("Empty proxy group")
            for member in members:
                visit(str(member))
        visiting.remove(tag)
        visited.add(tag)
    visit(root)
