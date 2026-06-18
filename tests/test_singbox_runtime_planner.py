from __future__ import annotations

import json
from pathlib import Path

import pytest

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


def _node_with_server(server: str) -> Node:
    node = _node()
    node.server = server
    node.outbound["settings"]["vnext"][0]["address"] = server
    return node


def _wireguard_node(server: str) -> Node:
    return Node(
        scheme="wireguard",
        server=server,
        outbound={
            "protocol": "wireguard",
            "singbox": {
                "type": "wireguard",
                "tag": "proxy",
                "address": ["10.0.0.2/32"],
                "private_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "peers": [
                    {
                        "address": server,
                        "port": 51820,
                        "public_key": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=",
                        "allowed_ips": ["0.0.0.0/0"],
                    }
                ],
            },
        },
    )


def _plan(routing: RoutingSettings, node: Node | None = None) -> dict:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    return plan_singbox_runtime(document, node or _node(), routing=routing).singbox_config


def _dns_servers(config: dict) -> dict[str, dict]:
    return {
        str(server.get("tag")): server
        for server in config["dns"]["servers"]
        if isinstance(server, dict)
    }


def _tun_inbound(config: dict) -> dict:
    return next(inbound for inbound in config["inbounds"] if inbound.get("type") == "tun")


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
    assert servers["bootstrap-dns"]["detour"] == "direct"
    assert servers["direct-dns"]["detour"] == "direct"
    assert servers["proxy-dns"]["detour"] == "proxy"
    assert servers["fake-dns"]["type"] == "fakeip"


@pytest.mark.parametrize("dns_type", ["udp", "tcp", "tls", "https"])
def test_builtin_dns_uses_independent_resolver_for_dns_server_hostnames(dns_type: str) -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="builtin",
            dns_bootstrap_server="resolver.example.com",
            dns_bootstrap_type=dns_type,
            dns_proxy_server="proxied-resolver.example.com",
            dns_proxy_type=dns_type,
            tun_default_outbound="proxy",
        )
    )

    servers = _dns_servers(config)

    assert servers["system-dns"]["type"] == "local"
    assert servers["bootstrap-dns"]["detour"] == "direct"
    assert servers["bootstrap-dns"]["domain_resolver"] == "system-dns"
    assert servers["direct-dns"]["detour"] == "direct"
    assert servers["direct-dns"]["domain_resolver"] == "system-dns"
    assert servers["proxy-dns"]["detour"] == "proxy"
    assert servers["proxy-dns"]["domain_resolver"] == "bootstrap-dns"


@pytest.mark.parametrize("dns_type", ["udp", "tcp", "tls", "https"])
def test_builtin_dns_does_not_add_resolver_for_ip_dns_servers(dns_type: str) -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="builtin",
            dns_bootstrap_server="1.1.1.1",
            dns_bootstrap_type=dns_type,
            dns_proxy_server="8.8.8.8",
            dns_proxy_type=dns_type,
            tun_default_outbound="proxy",
        )
    )

    servers = _dns_servers(config)

    assert servers["bootstrap-dns"]["detour"] == "direct"
    assert "domain_resolver" not in servers["bootstrap-dns"]
    assert servers["direct-dns"]["detour"] == "direct"
    assert "domain_resolver" not in servers["direct-dns"]
    assert servers["proxy-dns"]["detour"] == "proxy"
    if dns_type == "tcp":
        assert servers["proxy-dns"]["server"] == "dns.google"
        assert servers["proxy-dns"]["domain_resolver"] == "bootstrap-dns"
    else:
        assert "domain_resolver" not in servers["proxy-dns"]


@pytest.mark.parametrize(
    ("server", "cidr"),
    [
        ("95.128.157.251", "95.128.157.251/32"),
        ("2001:db8::2208", "2001:db8::2208/128"),
    ],
)
def test_native_tun_excludes_ip_proxy_endpoint_from_tun_routes(server: str, cidr: str) -> None:
    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _node_with_server(server),
    )

    rules = config["route"]["rules"]
    route_excludes = _tun_inbound(config)["route_exclude_address"]

    assert cidr in route_excludes
    assert any(rule.get("outbound") == "direct" and cidr in rule.get("ip_cidr", []) for rule in rules)


def test_native_tun_preserves_user_route_excludes_when_adding_endpoint_exclude() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="builtin",
            tun_default_outbound="proxy",
            tun_route_exclude_address=["192.168.0.0/16"],
        ),
        _node_with_server("95.128.157.251"),
    )

    route_excludes = _tun_inbound(config)["route_exclude_address"]

    assert "192.168.0.0/16" in route_excludes
    assert "95.128.157.251/32" in route_excludes


def test_native_tun_keeps_domain_proxy_endpoint_as_bootstrap_domain_route() -> None:
    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _node_with_server("vpn.example.com"),
    )

    rules = config["route"]["rules"]
    inbound = _tun_inbound(config)

    assert "route_exclude_address" not in inbound
    assert any(
        rule.get("outbound") == "direct"
        and "vpn.example.com" in rule.get("domain", [])
        for rule in rules
    )


def test_endpoint_tun_excludes_ip_peer_from_tun_routes() -> None:
    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _wireguard_node("46.17.101.82"),
    )

    rules = config["route"]["rules"]
    route_excludes = _tun_inbound(config)["route_exclude_address"]

    assert "46.17.101.82/32" in route_excludes
    assert any(
        rule.get("outbound") == "direct"
        and "46.17.101.82/32" in rule.get("ip_cidr", [])
        for rule in rules
    )
