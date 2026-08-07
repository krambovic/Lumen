from __future__ import annotations

from xray_fluent.engines.xray.config_builder import build_xray_config
from xray_fluent.models import AppSettings, Node, RoutingSettings


def _node() -> Node:
    return Node(
        id="node",
        name="test",
        outbound={"protocol": "freedom", "settings": {}},
    )


def test_xray_builtin_dns_uses_advanced_settings() -> None:
    routing = RoutingSettings(
        dns_mode="builtin",
        dns_bootstrap_server="1.1.1.1",
        dns_bootstrap_servers=["1.1.1.1"],
        dns_bootstrap_type="tls",
        dns_bootstrap_strategy="prefer_ipv4",
        dns_proxy_server="dns.example.com",
        dns_proxy_type="https",
        dns_proxy_strategy="ipv6_only",
        dns_geo_check=False,
    )

    config = build_xray_config(_node(), routing, AppSettings())

    assert config["dns"] == {
        "servers": [
            "https://dns.example.com/dns-query",
        ],
        "queryStrategy": "UseIPv6",
    }


def test_xray_builtin_dns_supports_multiple_servers_cache_hosts_and_geo() -> None:
    routing = RoutingSettings(
        preset_id="blocked",
        dns_mode="builtin",
        dns_bootstrap_servers=["1.1.1.1", "8.8.8.8"],
        dns_bootstrap_type="udp",
        dns_proxy_servers=["dns.google", "cloudflare-dns.com"],
        dns_proxy_type="https",
        dns_parallel_query=True,
        dns_optimistic_cache=True,
        dns_geo_check=True,
        dns_hosts={"local.test": ["127.0.0.1", "::1"]},
    )

    dns = build_xray_config(_node(), routing, AppSettings())["dns"]

    assert dns["enableParallelQuery"] is True
    assert dns["serveStale"] is True
    assert dns["serveExpiredTTL"] == 86400
    assert dns["hosts"] == {"local.test": ["127.0.0.1", "::1"]}
    scoped = [entry for entry in dns["servers"] if isinstance(entry, dict)]
    assert {entry["address"] for entry in scoped} == {
        "1.1.1.1",
        "8.8.8.8",
        "https://dns.google/dns-query",
        "https://cloudflare-dns.com/dns-query",
    }
    assert any("geosite:ru-blocked" in entry["domains"] for entry in scoped)
    assert any("geosite:lumen-exclude" in entry["domains"] for entry in scoped)
    assert dns["servers"][-2:] == [
        "https://dns.google/dns-query",
        "https://cloudflare-dns.com/dns-query",
    ]


def test_xray_dns_can_disable_direct_resolvers_and_never_falls_back_to_windows() -> None:
    routing = RoutingSettings.from_dict(
        {
            "preset_id": "blocked",
            "dns_mode": "builtin",
            "dns_bootstrap_servers": [],
            "dns_proxy_servers": ["dns.example.com"],
        }
    )

    config = build_xray_config(_node(), routing, AppSettings())

    assert routing.dns_bootstrap_servers == []
    assert config["dns"]["servers"][-1] == "https://dns.example.com/dns-query"
    assert "localhost" not in config["dns"]["servers"]


def test_xray_local_proxy_auth_applies_to_mixed_and_http_inbounds() -> None:
    settings = AppSettings(
        proxy_auth_enabled=True,
        proxy_auth_username="alice",
        proxy_auth_password="secret",
    )

    config = build_xray_config(_node(), RoutingSettings(), settings)

    assert config["inbounds"][0]["settings"]["accounts"] == [
        {"user": "alice", "pass": "secret"}
    ]
    assert config["inbounds"][1]["settings"]["accounts"] == [
        {"user": "alice", "pass": "secret"}
    ]


def test_xray_dns_keeps_explicit_service_and_custom_routes_when_geo_check_is_off() -> None:
    routing = RoutingSettings(
        preset_id="except_ru",
        dns_mode="builtin",
        dns_geo_check=False,
        direct_domains=["direct.example"],
        proxy_domains=["proxy.example"],
        service_routes={"youtube": "direct"},
    )

    servers = build_xray_config(_node(), routing, AppSettings())["dns"]["servers"]
    proxy_domains = {
        domain
        for server in servers
        if isinstance(server, dict) and str(server.get("address", "")).startswith("https://")
        for domain in server.get("domains", [])
    }
    direct_domains = {
        domain
        for server in servers
        if isinstance(server, dict) and not str(server.get("address", "")).startswith("https://")
        for domain in server.get("domains", [])
    }

    assert "domain:proxy.example" in proxy_domains
    assert "domain:direct.example" in direct_domains
    assert "domain:youtube.com" in direct_domains
    assert "geosite:category-ru" not in direct_domains


def test_xray_tls_certificate_pin_stays_a_single_hex_string() -> None:
    node = _node()
    node.outbound = {
        "protocol": "vless",
        "settings": {},
        "streamSettings": {
            "security": "tls",
            "tlsSettings": {"pinnedPeerCertSha256": ["A" * 64]},
        },
    }

    config = build_xray_config(node, RoutingSettings(), AppSettings())

    assert config["outbounds"][0]["streamSettings"]["tlsSettings"][
        "pinnedPeerCertSha256"
    ] == "a" * 64
