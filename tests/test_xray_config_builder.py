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
            "tls://1.1.1.1",
            "localhost",
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
    assert [entry["address"] for entry in dns["servers"][:2]] == [
        "https://dns.google/dns-query",
        "https://cloudflare-dns.com/dns-query",
    ]
    assert "geosite:ru-blocked" in dns["servers"][0]["domains"]
    assert "geosite:lumen-exclude" in dns["servers"][2]["domains"]


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
