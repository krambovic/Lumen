from __future__ import annotations

import json
from pathlib import Path

from xray_fluent.engines.singbox.runtime_planner import (
    parse_singbox_document,
    plan_singbox_runtime,
)
from xray_fluent.models import Node, RoutingSettings


def _base_config() -> dict:
    return {
        "inbounds": [
            {
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "xftun",
            }
        ],
        "outbounds": [
            {"type": "direct", "tag": "proxy"},
            {"type": "direct", "tag": "direct"},
            {"type": "block", "tag": "block"},
        ],
        "route": {"rules": [], "final": "direct"},
        "dns": {
            "servers": [
                {"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"},
                {"tag": "proxy-dns", "type": "https", "server": "dns.google"},
                {"tag": "fake-dns", "type": "fakeip", "inet4_range": "198.18.0.0/15"},
            ],
            "final": "bootstrap-dns",
        },
    }


def _node() -> Node:
    return Node(
        scheme="vless",
        server="vpn.example.com",
        outbound={
            "protocol": "vless",
            "settings": {
                "vnext": [
                    {
                        "address": "vpn.example.com",
                        "port": 443,
                        "users": [{"id": "00000000-0000-0000-0000-000000000000"}],
                    }
                ]
            },
            "streamSettings": {"security": "tls"},
        },
    )


def _plan(routing: RoutingSettings) -> dict:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    return plan_singbox_runtime(document, _node(), routing=routing).singbox_config


def _dns_servers(config: dict) -> dict[str, dict]:
    return {
        str(server.get("tag")): server
        for server in config["dns"]["servers"]
        if isinstance(server, dict)
    }


def test_system_dns_mode_does_not_hijack_dns_or_force_remote_dns() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="system",
            dns_hijack_enabled=True,
            dns_fake_enabled=True,
            tun_default_outbound="proxy",
        )
    )

    rules = config["route"]["rules"]
    servers = _dns_servers(config)

    assert not any(rule.get("action") == "hijack-dns" for rule in rules)
    assert servers["bootstrap-dns"]["type"] == "local"
    assert servers["direct-dns"]["type"] == "local"
    assert servers["proxy-dns"]["type"] == "local"
    assert "fake-dns" not in servers


def test_builtin_dns_mode_keeps_tun_dns_hijack_contract() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="builtin",
            dns_hijack_enabled=True,
            dns_fake_enabled=True,
            tun_default_outbound="proxy",
        )
    )

    rules = config["route"]["rules"]
    servers = _dns_servers(config)

    assert any(rule.get("action") == "hijack-dns" for rule in rules)
    assert servers["bootstrap-dns"]["type"] == "udp"
    assert servers["proxy-dns"]["detour"] == "proxy"
    assert servers["fake-dns"]["type"] == "fakeip"
