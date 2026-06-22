from __future__ import annotations

import json
import socket
import time
from pathlib import Path

import pytest

from xray_fluent.engines.singbox.runtime_planner import (
    classify_node_for_singbox,
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
        server="203.0.113.1",
        outbound={
            "protocol": "vless",
            "settings": {
                "vnext": [
                    {
                        "address": "203.0.113.1",
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


def _xhttp_node(protocol: str) -> Node:
    user = {
        "id": "00000000-0000-0000-0000-000000000000",
        "encryption": "none",
    }
    if protocol == "vmess":
        user = {"id": user["id"], "alterId": 0, "security": "auto"}
    return Node(
        scheme=protocol,
        server="203.0.113.1",
        port=443,
        outbound={
            "protocol": protocol,
            "settings": {
                "vnext": [
                    {
                        "address": "203.0.113.1",
                        "port": 443,
                        "users": [user],
                    }
                ]
            },
            "streamSettings": {
                "network": "xhttp",
                "security": "tls",
                "tlsSettings": {"serverName": "server.example.com"},
                "xhttpSettings": {"path": "/", "mode": "auto"},
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


def test_tun_runtime_uses_stable_low_mtu_v2rayn_defaults() -> None:
    config = _plan(RoutingSettings(mode="global", tun_default_outbound="proxy"))
    inbound = _tun_inbound(config)

    assert inbound["interface_name"] == "singbox_tun"
    assert inbound["address"] == ["172.18.0.1/30"]
    assert inbound["mtu"] == 1280
    assert inbound["stack"] == "gvisor"
    assert inbound["auto_route"] is True
    assert inbound["strict_route"] is False


def test_disabled_tls_fragment_is_not_added_to_tun_rules() -> None:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    config = plan_singbox_runtime(
        document,
        _node(),
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
        enable_final_fragment=False,
    ).singbox_config

    assert not any(rule.get("tls_record_fragment") for rule in config["route"]["rules"])


def test_vmess_xhttp_uses_xray_sidecar_but_vless_xhttp_stays_native() -> None:
    assert classify_node_for_singbox(_xhttp_node("vmess")) == "hybrid_xray_sidecar"
    assert classify_node_for_singbox(_xhttp_node("vless")) == "native_singbox"


def test_vmess_xhttp_hybrid_plan_preserves_original_xray_outbound() -> None:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    plan = plan_singbox_runtime(
        document,
        _xhttp_node("vmess"),
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
        enable_final_fragment=False,
    )

    assert plan.is_hybrid
    assert plan.xray_sidecar is not None
    proxy = next(outbound for outbound in plan.singbox_config["outbounds"] if outbound.get("tag") == "proxy")
    xray_proxy = next(outbound for outbound in plan.xray_sidecar.config["outbounds"] if outbound.get("tag") == "proxy")
    assert proxy["type"] == "socks"
    assert xray_proxy["protocol"] == "vmess"
    assert xray_proxy["streamSettings"]["network"] == "xhttp"


def test_system_dns_mode_uses_physical_adapter_dns_without_local_recursion() -> None:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    config = plan_singbox_runtime(
        document,
        _node(),
        routing=RoutingSettings(
            mode="global",
            dns_mode="system",
            dns_hijack_enabled=True,
            dns_fake_enabled=True,
            tun_default_outbound="proxy",
        ),
        system_dns_servers=("192.0.2.53",),
    ).singbox_config

    rules = config["route"]["rules"]
    servers = _dns_servers(config)

    assert [rule for rule in rules if rule.get("action") == "hijack-dns"] == [
        {"port": 53, "action": "hijack-dns"}
    ]
    for tag in ("bootstrap-dns", "direct-dns", "proxy-dns"):
        assert servers[tag]["type"] == "udp"
        assert servers[tag]["server"] == "192.0.2.53"
        assert servers[tag]["detour"] == "direct"
    assert "fake-dns" not in servers


def test_system_dns_mode_falls_back_to_explicit_bootstrap_server() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="system",
            dns_bootstrap_server="1.1.1.1",
            dns_bootstrap_type="udp",
            tun_default_outbound="proxy",
        )
    )

    servers = _dns_servers(config)

    assert servers["proxy-dns"]["type"] == "udp"
    assert servers["proxy-dns"]["server"] == "1.1.1.1"
    assert servers["proxy-dns"]["detour"] == "direct"


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
    assert config["dns"]["final"] == "proxy-dns"
    assert any(
        rule.get("server") == "fake-dns"
        and rule.get("query_type") == ["A", "AAAA"]
        for rule in config["dns"]["rules"]
    )
    assert config["log"]["level"] == "info"


def test_builtin_dns_always_hijacks_the_tun_dns_peer() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            dns_mode="builtin",
            dns_hijack_enabled=False,
            tun_default_outbound="proxy",
        )
    )

    hijack_rules = [rule for rule in config["route"]["rules"] if rule.get("action") == "hijack-dns"]

    assert hijack_rules == [{"port": 53, "action": "hijack-dns"}]


def test_builtin_fake_dns_keeps_real_dns_final_for_direct_default() -> None:
    config = _plan(
        RoutingSettings(
            mode="direct",
            dns_mode="builtin",
            dns_hijack_enabled=True,
            dns_fake_enabled=True,
            tun_default_outbound="direct",
        )
    )

    assert config["route"]["final"] == "direct"
    assert config["dns"]["final"] == "bootstrap-dns"
    assert any(rule.get("server") == "fake-dns" for rule in config["dns"]["rules"])


def test_builtin_fake_dns_keeps_service_dns_rules_ahead_of_fakeip() -> None:
    config = _plan(
        RoutingSettings(
            mode="global",
            service_routes={"youtube": "direct"},
            dns_mode="builtin",
            dns_fake_enabled=True,
            tun_default_outbound="proxy",
        )
    )

    rules = config["dns"]["rules"]
    service_index = next(
        index
        for index, rule in enumerate(rules)
        if rule.get("server") == "bootstrap-dns"
        and "youtube.com" in rule.get("domain_suffix", [])
    )
    fake_index = next(index for index, rule in enumerate(rules) if rule.get("server") == "fake-dns")

    assert service_index < fake_index


@pytest.mark.parametrize("dns_type", ["udp", "tcp", "tls", "https"])
def test_builtin_dns_uses_independent_resolver_for_dns_server_hostnames(dns_type: str) -> None:
    text = json.dumps(_base_config())
    document = parse_singbox_document(Path("test.json"), text)
    config = plan_singbox_runtime(
        document,
        _node(),
        routing=RoutingSettings(
            mode="global",
            dns_mode="builtin",
            dns_bootstrap_server="resolver.example.com",
            dns_bootstrap_type=dns_type,
            dns_proxy_server="proxied-resolver.example.com",
            dns_proxy_type=dns_type,
            tun_default_outbound="proxy",
        ),
        system_dns_servers=("192.0.2.53",),
    ).singbox_config

    servers = _dns_servers(config)

    assert servers["system-dns"]["type"] == "udp"
    assert servers["system-dns"]["server"] == "192.0.2.53"
    assert servers["system-dns"]["detour"] == "direct"
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


def test_native_tun_excludes_resolved_domain_proxy_endpoint_from_tun_routes(monkeypatch: pytest.MonkeyPatch) -> None:
    def fake_getaddrinfo(host: str, *_args, **_kwargs):
        assert host == "vpn.example.com"
        return [
            (socket.AF_INET, socket.SOCK_STREAM, 0, "", ("203.0.113.10", 0)),
            (socket.AF_INET6, socket.SOCK_STREAM, 0, "", ("2001:db8::10", 0, 0, 0)),
        ]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _node_with_server("vpn.example.com"),
    )

    rules = config["route"]["rules"]
    inbound = _tun_inbound(config)
    proxy = next(outbound for outbound in config["outbounds"] if outbound.get("tag") == "proxy")

    assert "203.0.113.10/32" in inbound["route_exclude_address"]
    assert "2001:db8::10/128" in inbound["route_exclude_address"]
    assert proxy["server"] == "203.0.113.10"
    assert "domain_resolver" not in proxy
    assert proxy["tls"]["server_name"] == "vpn.example.com"
    assert any(rule.get("outbound") == "direct" and "203.0.113.10/32" in rule.get("ip_cidr", []) for rule in rules)
    assert any(
        rule.get("outbound") == "direct"
        and "vpn.example.com" in rule.get("domain", [])
        for rule in rules
    )


def test_endpoint_resolution_is_reused_within_cache_ttl(monkeypatch: pytest.MonkeyPatch) -> None:
    import xray_fluent.engines.singbox.runtime_planner as planner

    calls = 0

    def fake_getaddrinfo(host: str, *_args, **_kwargs):
        nonlocal calls
        calls += 1
        return [(socket.AF_INET, socket.SOCK_STREAM, 0, "", ("203.0.113.25", 0))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)
    with planner._endpoint_dns_cache_lock:
        planner._endpoint_dns_cache.clear()

    assert planner._resolve_endpoint_addresses("cache.example.com") == ["203.0.113.25"]
    assert planner._resolve_endpoint_addresses("cache.example.com") == ["203.0.113.25"]
    assert calls == 1


def test_tun_runtime_does_not_reject_quic_globally() -> None:
    config = _plan(
        RoutingSettings(
            mode="rule",
            dns_mode="system",
            direct_domains=["2ip.ru"],
            tun_default_outbound="direct",
        ),
        _node_with_server("vpn.example.com"),
    )

    rules = config["route"]["rules"]
    assert any(
        rule.get("outbound") == "direct" and "2ip.ru" in rule.get("domain_suffix", [])
        for rule in rules
    )
    assert not any(
        rule.get("action") == "reject" and rule.get("network") == "udp" and rule.get("port") == 443
        for rule in rules
    )


def test_native_tun_keeps_domain_proxy_endpoint_route_when_resolution_fails(monkeypatch: pytest.MonkeyPatch) -> None:
    def fail_getaddrinfo(*_args, **_kwargs):
        raise socket.gaierror()

    monkeypatch.setattr(socket, "getaddrinfo", fail_getaddrinfo)

    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _node_with_server("vpn.example.com"),
    )

    rules = config["route"]["rules"]
    inbound = _tun_inbound(config)
    proxy = next(outbound for outbound in config["outbounds"] if outbound.get("tag") == "proxy")

    assert "route_exclude_address" not in inbound
    assert proxy["server"] == "vpn.example.com"
    assert proxy["domain_resolver"] == "bootstrap-dns"
    assert any(
        rule.get("outbound") == "direct"
        and "vpn.example.com" in rule.get("domain", [])
        for rule in rules
    )


def test_native_tun_does_not_block_startup_on_slow_endpoint_dns(monkeypatch: pytest.MonkeyPatch) -> None:
    def slow_getaddrinfo(*_args, **_kwargs):
        time.sleep(5)
        return []

    monkeypatch.setattr(socket, "getaddrinfo", slow_getaddrinfo)
    started = time.monotonic()

    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _node_with_server("vpn.example.com"),
    )

    assert time.monotonic() - started < 2
    proxy = next(outbound for outbound in config["outbounds"] if outbound.get("tag") == "proxy")
    assert proxy["server"] == "vpn.example.com"
    assert proxy["domain_resolver"] == "bootstrap-dns"


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


def test_endpoint_tun_excludes_resolved_domain_peer_from_tun_routes(monkeypatch: pytest.MonkeyPatch) -> None:
    def fake_getaddrinfo(host: str, *_args, **_kwargs):
        assert host == "wg.example.com"
        return [(socket.AF_INET, socket.SOCK_STREAM, 0, "", ("198.51.100.20", 0))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)

    config = _plan(
        RoutingSettings(mode="global", dns_mode="builtin", tun_default_outbound="proxy"),
        _wireguard_node("wg.example.com"),
    )

    rules = config["route"]["rules"]
    route_excludes = _tun_inbound(config)["route_exclude_address"]

    assert "198.51.100.20/32" in route_excludes
    assert any(
        rule.get("outbound") == "direct"
        and "198.51.100.20/32" in rule.get("ip_cidr", [])
        for rule in rules
    )
    assert any(
        rule.get("outbound") == "direct"
        and "wg.example.com" in rule.get("domain", [])
        for rule in rules
    )
