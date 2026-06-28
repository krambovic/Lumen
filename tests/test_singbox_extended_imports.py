from __future__ import annotations

import json
from pathlib import Path

from xray_fluent.engines.singbox.config_builder import build_singbox_outbound
from xray_fluent.engines.singbox.runtime_planner import parse_singbox_document, plan_singbox_runtime
from xray_fluent.link_parser import parse_links_text
from xray_fluent.models import RoutingSettings


def _base_config() -> dict:
    return {
        "inbounds": [{"type": "tun", "tag": "tun-in"}],
        "outbounds": [
            {"type": "direct", "tag": "proxy"},
            {"type": "direct", "tag": "direct"},
            {"type": "block", "tag": "block"},
        ],
        "route": {"rules": [], "final": "direct"},
        "dns": {"servers": [{"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"}]},
    }


def test_mkcp_transport_is_converted_for_singbox_extended() -> None:
    nodes, errors = parse_links_text(
        "vless://00000000-0000-0000-0000-000000000000@example.com:443"
        "?type=kcp&headerType=wireguard&seed=secret&congestion=false&mtu=1350#mkcp"
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")

    assert outbound["transport"]["type"] == "mkcp"
    assert outbound["transport"]["header_type"] == "wireguard"
    assert outbound["transport"]["seed"] == "secret"
    assert outbound["transport"]["congestion"] is False
    assert outbound["transport"]["mtu"] == 1350


def test_mieru_native_json_import_is_tun_only_singbox_outbound() -> None:
    nodes, errors = parse_links_text(
        json.dumps(
            {
                "type": "mieru",
                "tag": "proxy",
                "server": "example.com",
                "server_port": 27017,
                "transport": "TCP",
                "username": "user",
                "password": "pass",
            }
        )
    )

    assert errors == []
    assert nodes[0].scheme == "mieru"
    assert build_singbox_outbound(nodes[0], tag="proxy")["type"] == "mieru"


def test_mierus_link_imports_as_mieru_extended_outbound() -> None:
    nodes, errors = parse_links_text(
        "mierus://user:pass@example.com:27017?transport=TCP&multiplexing=MULTIPLEXING_LOW#secure-mieru"
    )

    assert errors == []
    assert nodes[0].scheme == "mieru"
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert outbound["type"] == "mieru"
    assert outbound["server"] == "example.com"
    assert outbound["server_port"] == 27017
    assert outbound["transport"] == "TCP"
    assert outbound["username"] == "user"
    assert outbound["password"] == "pass"
    assert outbound["multiplexing"] == "MULTIPLEXING_LOW"


def test_full_provider_config_is_preserved_for_tun_runtime() -> None:
    provider_config = {
        "inbounds": [{"type": "mixed", "tag": "mixed-in", "listen_port": 7897}],
        "outbounds": [
            {"type": "direct", "tag": "direct"},
            {"type": "urltest", "tag": "auto", "outbounds": ["direct"], "use_all_providers": True},
        ],
        "providers": [
            {
                "type": "remote",
                "tag": "sub",
                "url": "https://example.com/sub.txt",
                "user_agent": "sing-box",
            }
        ],
        "route": {"final": "auto"},
        "dns": {"servers": [{"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"}]},
    }
    nodes, errors = parse_links_text(json.dumps(provider_config))
    assert errors == []
    assert nodes[0].scheme == "singbox_config"

    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    planned = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
    ).singbox_config

    assert planned["providers"] == provider_config["providers"]
    assert any(outbound.get("tag") == "proxy" for outbound in planned["outbounds"])
