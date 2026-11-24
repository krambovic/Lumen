"""Build the runtime-config parity fixture for the Rust test suite.

Every distinct server from ``parser_parity.json`` goes through the Python
runtime builders:

* ``build_xray_config`` (Xray proxy mode),
* ``build_singbox_outbound`` (native sing-box outbound),
* ``plan_singbox_runtime`` on the bundled default template (TUN and proxy
  mode),

and a representative subset of servers is crossed with every settings and
routing variant (presets, DNS modes, fragments, multiplex, Discord proxy, TUN
options, process rules, hot switch, IPv6 preference).

Outputs are big, so each case stores the SHA-256 of its canonical JSON
(``json.dumps(..., ensure_ascii=True, sort_keys=True, separators=(",", ":"))``)
after masking per-run values (Clash API secret, sidecar credentials). Run
with ``--case N`` to print one case's full Python output for debugging.

Run from the repository root::

    python rust/tools/parity/gen_config_corpus.py
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import ipaddress
import json
import os
import socket
import sys
import tempfile
from pathlib import Path
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[3]
_TEMP = tempfile.TemporaryDirectory(prefix="lumen-config-parity-")
PARITY_ROOT = Path(_TEMP.name).resolve()
os.environ["LOCALAPPDATA"] = str(PARITY_ROOT / "Local")
os.environ["APPDATA"] = str(PARITY_ROOT / "Roaming")
sys.path.insert(0, str(ROOT / "windows"))

from xray_fluent import geodata_resources  # noqa: E402
from xray_fluent.engines.singbox import runtime_planner  # noqa: E402
from xray_fluent.engines.singbox.config_builder import build_singbox_outbound  # noqa: E402
from xray_fluent.engines.singbox.runtime_planner import (  # noqa: E402
    parse_singbox_document,
    plan_singbox_runtime,
)
from xray_fluent.engines.xray.config_builder import build_xray_config  # noqa: E402
from xray_fluent.models import AppSettings, Node, RoutingSettings  # noqa: E402
from xray_fluent.routing_presets import BUILT_IN_ROUTING_PRESET_IDS, build_routing_preset  # noqa: E402

PARSER_FIXTURE = ROOT / "rust" / "crates" / "lumen-core" / "tests" / "fixtures" / "parser_parity.json"
FIXTURE = ROOT / "rust" / "crates" / "lumen-engines" / "tests" / "fixtures" / "config_parity.json"
TEMPLATE = ROOT / "windows" / "data" / "templates" / "sing-box" / "default.json"

# The planner checks only whether the files exist. It never opens the SRS
# payloads. The Russia bundle is present and other regional sets are absent.
BINARY_RULE_SETS = sorted(Path(source).name for source in geodata_resources.SINGBOX_BINARY_RULE_SETS.values())
geodata_resources.SINGBOX_RULE_SET_DIR = PARITY_ROOT / "core" / "rule-sets"
geodata_resources.SINGBOX_RULE_SET_DIR.mkdir(parents=True)
for filename in BINARY_RULE_SETS:
    (geodata_resources.SINGBOX_RULE_SET_DIR / filename).touch()

# Exercise IPv4, IPv6 and failed lookups with the same answers on both sides.
# These addresses are reserved for documentation and never contacted.
ENDPOINT_DNS = {
    "engage.cloudflareclient.com": ["192.0.2.10", "2001:db8::10"],
    "example.com": ["198.51.100.10", "203.0.113.10", "2001:db8::20"],
}


def fixture_getaddrinfo(host, port, family=0, type=0):
    answers = []
    for value in ENDPOINT_DNS.get(host, ()):
        ip = ipaddress.ip_address(value)
        if ip.version == 4:
            answers.append((socket.AF_INET, type, 0, "", (value, port or 0)))
        else:
            answers.append((socket.AF_INET6, type, 0, "", (value, port or 0, 0, 0)))
    return answers


runtime_planner.socket = Mock(wraps=socket)
runtime_planner.socket.getaddrinfo = fixture_getaddrinfo
runtime_planner._local_ipv4_addresses = lambda: set()
runtime_planner._ipv4_gateway_in_use = lambda _gateway: False

RELAY_PORT = 47001
PROTECT_PORT = 47002
PROTECT_PASSWORD = "parity-protect-password"


# ── inputs ───────────────────────────────────────────────────────────────────


def corpus_nodes() -> list[Node]:
    cases = json.loads(PARSER_FIXTURE.read_text(encoding="utf-8"))["cases"]
    seen: set[str] = set()
    nodes: list[Node] = []
    for case in cases:
        expected = case["expected"]
        payloads = expected.get("nodes") or ([expected["node"]] if "node" in expected else [])
        for payload in payloads:
            data = {key: value for key, value in payload.items() if not key.startswith("_")}
            key = json.dumps([data.get("scheme"), data.get("server"), data.get("port"), data.get("outbound")],
                             sort_keys=True)
            if key in seen:
                continue
            seen.add(key)
            data["id"] = f"node-{len(nodes):04d}"
            data["created_at"] = "2026-01-01T00:00:00+00:00"
            nodes.append(Node.from_dict(data))
    return nodes


def node_family(node: Node) -> tuple:
    outbound = node.outbound or {}
    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    singbox = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else {}
    transport = singbox.get("transport") if isinstance(singbox.get("transport"), dict) else {}
    tls = singbox.get("tls") if isinstance(singbox.get("tls"), dict) else {}
    return (
        str(outbound.get("protocol") or node.scheme),
        str(stream.get("network") or transport.get("type") or ""),
        str(stream.get("security") or ("tls" if tls.get("enabled") else "")),
        bool(singbox), "singbox_config" in outbound, "xray_config" in outbound,
    )


def representative(nodes: list[Node]) -> list[Node]:
    picked: dict[tuple, Node] = {}
    for node in nodes:
        picked.setdefault(node_family(node), node)
    return list(picked.values())


def routing_variants() -> list[tuple[str, dict]]:
    base = RoutingSettings()
    variants: list[tuple[str, RoutingSettings]] = [("default", base)]
    for preset in sorted(BUILT_IN_ROUTING_PRESET_IDS):
        variants.append((f"preset-{preset}", build_routing_preset(RoutingSettings(), preset)))
    custom = build_routing_preset(RoutingSettings(), "custom") if "custom" in BUILT_IN_ROUTING_PRESET_IDS else RoutingSettings()
    custom = copy.deepcopy(custom)
    custom.direct_domains = ["domain:example.ru", "full:direct.example", "geosite:category-ru", "10.0.0.0/8"]
    custom.proxy_domains = ["domain:youtube.com", "keyword:google", "regexp:^api\\.", "geosite:google"]
    custom.block_domains = ["domain:ads.example", "geosite:category-ads-all"]
    custom.process_rules = [{"process": "chrome.exe", "action": "proxy"},
                            {"process": "steam.exe", "action": "direct"},
                            {"process": "bad.exe", "action": "block"}]
    custom.process_preset_routes = {"telegram": "proxy", "windows_system": "direct"}
    custom.service_routes = {"youtube": "proxy", "steam": "direct", "discord": "proxy"}
    custom.tun_route_exclude_address = ["192.168.0.0/16", "fd00::/8"]
    custom.dns_hosts = {"router.local": ["192.168.1.1"], "dual.local": ["10.0.0.1", "fd00::1"]}
    variants.append(("custom-rules", custom))
    for name, changes in (
        ("dns-system", {"dns_mode": "system"}),
        ("dns-fake", {"dns_fake_enabled": True}),
        ("dns-no-hijack", {"dns_hijack_enabled": False}),
        ("dns-udp", {"dns_bootstrap_type": "udp", "dns_proxy_type": "tcp", "dns_proxy_server": "8.8.8.8"}),
        ("dns-tls", {"dns_bootstrap_type": "tls", "dns_proxy_type": "tls", "dns_bootstrap_server": "dns.google",
                     "dns_proxy_server": "one.one.one.one"}),
        ("dns-lists", {"dns_bootstrap_servers": ["1.1.1.1", "8.8.8.8"], "dns_proxy_servers": ["9.9.9.9", "dns.quad9.net"]}),
        ("dns-strategies", {"dns_bootstrap_strategy": "prefer_ipv6", "dns_proxy_strategy": "ipv6_only",
                            "dns_parallel_query": True, "dns_optimistic_cache": True, "dns_geo_check": False}),
        ("no-bypass-lan", {"bypass_lan": False}),
        ("tun-default-proxy", {"tun_default_outbound": "proxy"}),
    ):
        routing = copy.deepcopy(base)
        for key, value in changes.items():
            setattr(routing, key, value)
        variants.append((name, routing))
    try:
        from xray_fluent.routing_runtime import routing_with_ip_preference
    except ImportError:  # pragma: no cover - older layouts
        from xray_fluent.application.routing_service import routing_with_ip_preference  # type: ignore
    variants.append(("prefer-ipv6", routing_with_ip_preference(base, prefer_ipv6=True)))
    return [(name, routing.to_dict()) for name, routing in variants]


def settings_variants() -> list[tuple[str, dict]]:
    variants = [("default", {})]
    for name, changes in (
        ("fragment-xray", {"enable_xray_fragment": True, "fragment_packets": "1-3", "fragment_length": "10-20",
                           "fragment_delay": "5-10"}),
        ("fragment-tail", {"tail_fragment_enabled": True, "enable_final_fragment": True}),
        ("no-final-fragment", {"enable_final_fragment": False}),
        ("multiplex", {"multiplex_enabled": True, "multiplex_concurrency": 4}),
        ("discord", {"discord_proxy_enabled": True}),
        ("log-debug", {"log_level": "debug"}),
        ("ports", {"local_socks_port": 20808, "local_http_port": 20809}),
        ("proxy-auth", {"proxy_auth_enabled": True, "proxy_auth_username": "user", "proxy_auth_password": "pass"}),
        ("tun-options", {"tun_stack": "gvisor", "tun_mtu": 1500, "tun_strict_route": False,
                         "tun_endpoint_independent_nat": True, "tun_block_quic": False}),
        ("tun-system", {"tun_stack": "system"}),
        ("core-xray", {"core_preference": "xray"}),
        ("core-singbox", {"core_preference": "singbox"}),
    ):
        variants.append((name, changes))
    result = []
    for name, changes in variants:
        settings = AppSettings()
        for key, value in changes.items():
            setattr(settings, key, value)
        result.append((name, settings.to_dict()))
    return result


# ── replay ───────────────────────────────────────────────────────────────────


def canonical(value) -> str:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def normalize_paths(text: str) -> str:
    escaped_root = canonical(str(PARITY_ROOT) + os.sep)[1:-1]
    return text.replace(escaped_root, canonical("<parity-root>" + os.sep)[1:-1])


def digest(value) -> str:
    return hashlib.sha256(canonical(value).encode("ascii")).hexdigest()


def mask(text: str, secrets: list[str]) -> str:
    for index, secret in enumerate(secrets):
        if secret:
            text = text.replace(secret, f"<masked-{index}>")
    return text


def error_result(exc: Exception) -> dict:
    return {"error": str(exc)}


def run_xray(node: Node, routing: dict, settings: dict) -> dict:
    app_settings = AppSettings.from_dict(settings)
    try:
        config = build_xray_config(
            node,
            RoutingSettings.from_dict(routing),
            app_settings,
            socks_port=int(app_settings.local_socks_port),
            http_port=int(app_settings.local_http_port),
        )
    except Exception as exc:  # noqa: BLE001 - recorded as data
        return error_result(exc)
    return {"config": config}


def run_outbound(node: Node, settings: dict) -> dict:
    app_settings = AppSettings.from_dict(settings)
    try:
        outbound = build_singbox_outbound(
            node,
            tag="proxy",
            multiplex_enabled=app_settings.multiplex_enabled,
            multiplex_concurrency=app_settings.multiplex_concurrency,
        )
    except Exception as exc:  # noqa: BLE001
        return error_result(exc)
    return {"outbound": outbound}


_DOCUMENT = None


def document():
    global _DOCUMENT
    if _DOCUMENT is None:
        _DOCUMENT = parse_singbox_document(TEMPLATE, TEMPLATE.read_text(encoding="utf-8"))
    return _DOCUMENT


def run_plan(node: Node | None, routing: dict, settings: dict, *, tun_mode: bool,
             hot_switch_nodes: list[Node]) -> dict:
    app_settings = AppSettings.from_dict(settings)
    try:
        plan = plan_singbox_runtime(
            document(),
            node,
            routing=RoutingSettings.from_dict(routing),
            enable_final_fragment=app_settings.enable_final_fragment,
            fragment_packets=app_settings.fragment_packets,
            fragment_length=app_settings.fragment_length,
            fragment_delay=app_settings.fragment_delay,
            tail_fragment_enabled=app_settings.tail_fragment_enabled,
            multiplex_enabled=app_settings.multiplex_enabled,
            multiplex_concurrency=app_settings.multiplex_concurrency,
            discord_proxy_enabled=app_settings.discord_proxy_enabled,
            tun_strict_route=app_settings.tun_strict_route,
            tun_stack=app_settings.tun_stack,
            tun_mtu=app_settings.tun_mtu,
            tun_endpoint_independent_nat=app_settings.tun_endpoint_independent_nat,
            tun_block_quic=app_settings.tun_block_quic,
            local_socks_port=app_settings.local_socks_port,
            local_http_port=app_settings.local_http_port,
            proxy_auth_username=app_settings.proxy_auth_username if app_settings.proxy_auth_enabled else "",
            proxy_auth_password=app_settings.proxy_auth_password if app_settings.proxy_auth_enabled else "",
            preferred_relay_port=RELAY_PORT,
            preferred_protect_port=PROTECT_PORT,
            preferred_protect_password=PROTECT_PASSWORD,
            system_dns_servers=("192.168.1.1",),
            tun_mode=tun_mode,
            enable_hot_switch=bool(hot_switch_nodes),
            hot_switch_nodes=tuple(hot_switch_nodes),
            core_preference=getattr(app_settings, "core_preference", "auto"),
        )
    except Exception as exc:  # noqa: BLE001
        return error_result(exc)
    sidecar = plan.xray_sidecar
    secrets = [plan.clash_api_secret]
    if sidecar is not None:
        secrets += [sidecar.relay_username, sidecar.relay_password]
    payload = {
        "outcome": plan.outcome,
        "singbox_config": plan.singbox_config,
        "has_proxy_outbound": plan.has_proxy_outbound,
        "used_selected_node": plan.used_selected_node,
        "xray_sidecar": None if sidecar is None else {
            "relay_port": sidecar.relay_port,
            "protect_port": sidecar.protect_port,
            "protect_password": sidecar.protect_password,
            "config": sidecar.config,
        },
        "clash_api_selector": plan.clash_api_selector,
        "clash_api_node_signatures": [list(item) for item in plan.clash_api_node_signatures],
        "health_proxy_url": plan.health_proxy_url,
    }
    return json.loads(normalize_paths(mask(canonical(payload), secrets)))


def build_cases() -> list[dict]:
    nodes = corpus_nodes()
    subset = representative(nodes)
    routings = routing_variants()
    settings = settings_variants()
    default_routing, default_settings = routings[0][1], settings[0][1]
    cases: list[dict] = []

    def add(kind: str, node: Node | None, routing: tuple[str, dict], setting: tuple[str, dict], **extra) -> None:
        cases.append({"kind": kind, "node": None if node is None else node.to_dict(), "routing": routing[1],
                      "settings": setting[1], "label": f"{kind} {routing[0]} {setting[0]}", **extra})

    for node in nodes:
        add("xray", node, routings[0], settings[0])
        add("outbound", node, routings[0], settings[0])
        add("plan", node, routings[0], settings[0], tun_mode=True)
        add("plan", node, routings[0], settings[0], tun_mode=False)
    for node in subset:
        for setting in settings[1:]:
            add("xray", node, routings[0], setting)
            add("outbound", node, routings[0], setting)
            add("plan", node, routings[0], setting, tun_mode=True)
        for routing in routings[1:]:
            add("xray", node, routing, settings[0])
            add("plan", node, routing, settings[0], tun_mode=True)
            add("plan", node, routing, settings[0], tun_mode=False)
    add("plan", None, routings[0], settings[0], tun_mode=True)
    add("plan", None, routings[0], settings[0], tun_mode=False)
    hot = [node.id for node in subset[:12]]
    for node in subset[:12]:
        add("plan", node, routings[0], settings[0], tun_mode=True, hot_switch=hot)
    return cases, {node.id: node for node in nodes}


def replay(case: dict, by_id: dict[str, Node]) -> dict:
    node = Node.from_dict(case["node"]) if case["node"] else None
    if case["kind"] == "xray":
        return run_xray(node, case["routing"], case["settings"])
    if case["kind"] == "outbound":
        return run_outbound(node, case["settings"])
    hot = [Node.from_dict(by_id[node_id].to_dict()) for node_id in case.get("hot_switch", [])]
    return run_plan(node, case["routing"], case["settings"], tun_mode=case["tun_mode"], hot_switch_nodes=hot)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", type=int, help="print one case's full Python output")
    args = parser.parse_args()
    cases, by_id = build_cases()
    if args.case is not None:
        sys.stdout.reconfigure(encoding="utf-8")
        case = cases[args.case]
        print(json.dumps({"label": case["label"], "result": replay(case, by_id)}, ensure_ascii=False, indent=1))
        return 0
    # Nodes are stored once and referenced by id to keep the fixture small.
    node_table: dict[str, dict] = {}
    output = []
    for case in cases:
        result = replay(case, by_id)
        entry = {key: value for key, value in case.items() if key != "node"}
        if case["node"] is not None:
            node_table[case["node"]["id"]] = case["node"]
            entry["node"] = case["node"]["id"]
        else:
            entry["node"] = None
        entry["expected"] = {"error": result["error"]} if "error" in result else {"sha256": digest(result)}
        output.append(entry)
    routings = {}
    settings = {}
    for entry in output:
        routings.setdefault(canonical(entry["routing"]), entry["routing"])
        settings.setdefault(canonical(entry["settings"]), entry["settings"])
    routing_ids = {key: index for index, key in enumerate(routings)}
    settings_ids = {key: index for index, key in enumerate(settings)}
    for entry in output:
        entry["routing"] = routing_ids[canonical(entry["routing"])]
        entry["settings"] = settings_ids[canonical(entry["settings"])]
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text(json.dumps({
        "binary_rule_sets": BINARY_RULE_SETS,
        "endpoint_dns": ENDPOINT_DNS,
        "relay_port": RELAY_PORT, "protect_port": PROTECT_PORT, "protect_password": PROTECT_PASSWORD,
        "nodes": node_table, "routings": list(routings.values()), "settings": list(settings.values()),
        "cases": output,
    }, ensure_ascii=False) + "\n", encoding="utf-8")
    errors = sum(1 for entry in output if "error" in entry["expected"])
    print(f"{len(output)} cases ({errors} expected errors), {len(node_table)} nodes -> {FIXTURE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
