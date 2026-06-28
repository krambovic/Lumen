from __future__ import annotations

import json

from xray_fluent.application import node_service
from xray_fluent.link_parser import parse_links_text, validate_node_outbound


def test_unsupported_app_placeholder_is_rejected() -> None:
    text = (
        "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1"
        "?encryption=none&type=tcp&security=none#Приложение%20не%20поддерживается"
    )

    nodes, errors = parse_links_text(text)

    assert errors == []
    assert len(nodes) == 1
    assert validate_node_outbound(nodes[0]) is not None


def test_singbox_subscription_imports_all_proxy_outbounds() -> None:
    payload = {
        "outbounds": [
            {"type": "selector", "tag": "proxy", "outbounds": ["one", "two"]},
            {"type": "direct", "tag": "direct"},
            {"type": "vless", "tag": "one", "server": "one.example", "server_port": 443, "uuid": "u1"},
            {"type": "trojan", "tag": "two", "server": "two.example", "server_port": 443, "password": "p2"},
        ]
    }

    nodes, errors = parse_links_text(json.dumps(payload))

    assert errors == []
    assert [(node.scheme, node.server, node.port, node.name) for node in nodes] == [
        ("vless", "one.example", 443, "proxy"),
        ("vless", "one.example", 443, "one"),
        ("trojan", "two.example", 443, "two"),
    ]


def test_singbox_selector_imports_autoselect_as_full_config_node() -> None:
    payload = {
        "outbounds": [
            {"type": "urltest", "tag": "Автовыбор сервера", "outbounds": ["hy-one"], "interval": "3m"},
            {
                "type": "hysteria",
                "tag": "hy-one",
                "server": "nl42.7geo7.ru",
                "server_port": 1450,
                "auth_str": "secret",
            },
            {"type": "direct", "tag": "direct"},
        ],
    }

    nodes, errors = parse_links_text(json.dumps(payload, ensure_ascii=False))

    assert errors == []
    auto = nodes[0]
    assert (auto.name, auto.scheme, auto.server, auto.port) == (
        "Автовыбор сервера",
        "hysteria",
        "nl42.7geo7.ru",
        1450,
    )
    assert auto.outbound["protocol"] == "singbox_config"
    assert auto.outbound["singbox_config"]["route"]["final"] == "Автовыбор сервера"


def test_happ_xray_subscription_imports_full_configs() -> None:
    payload = [
        {
            "remarks": "Auto",
            "outbounds": [
                {
                    "tag": "proxy",
                    "protocol": "hysteria",
                    "settings": {"address": "hy.example", "port": 1450, "version": 2},
                },
                {"tag": "direct", "protocol": "freedom"},
            ],
        },
        {
            "remarks": "VLESS XHTTP",
            "outbounds": [
                {
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": {
                        "vnext": [
                            {
                                "address": "xhttp.example",
                                "port": 1489,
                                "users": [{"id": "u1", "encryption": "none"}],
                            }
                        ]
                    },
                    "streamSettings": {"network": "xhttp", "security": "none"},
                }
            ],
        },
    ]

    nodes, errors = parse_links_text(json.dumps(payload))

    assert errors == []
    assert [(node.scheme, node.server, node.port, node.name) for node in nodes] == [
        ("hysteria", "hy.example", 1450, "Auto"),
        ("vless", "xhttp.example", 1489, "VLESS XHTTP"),
    ]


def test_clash_yaml_subscription_imports_proxies() -> None:
    text = """
mixed-port: 7890
proxies:
  - name: ws-one
    type: vless
    server: one.example
    port: 443
    uuid: 00000000-0000-0000-0000-000000000001
    network: ws
    tls: true
    servername: one.example
    ws-opts:
      path: /ws
      headers:
        Host: one.example
  - name: hy2-two
    type: hysteria2
    server: two.example
    port: 8443
    password: secret
"""

    nodes, errors = parse_links_text(text)

    assert errors == []
    assert [(node.scheme, node.server, node.port, node.name) for node in nodes] == [
        ("vless", "one.example", 443, "ws-one"),
        ("hysteria2", "two.example", 8443, "hy2-two"),
    ]


def test_subscription_tls_eof_retries_same_profile_direct(monkeypatch) -> None:
    calls = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        calls.append((profile, direct))
        if not direct:
            raise OSError("<urlopen error TLS/SSL connection has been closed (EOF) (_ssl.c:1010)>")
        return (
            "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp&security=none#one",
            {"clientProfile": profile},
        )

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload("https://sub.example/path")

    assert errors == []
    assert "one.example" in text
    assert info["clientProfile"] == "Happ Windows"
    assert info["networkPath"] == "direct"
    assert calls[:2] == [("Happ Windows", False), ("Happ Windows", True)]


def test_subscription_metadata_accepts_common_button_headers() -> None:
    info = node_service._extract_subscription_metadata(
        {
            "profile-title": "VPN",
            "support-link": "https://support.example",
            "panel-url": "https://panel.example",
            "telegram-link": "https://t.me/example",
        },
        "Happ Windows",
    )

    assert info["profileTitle"] == "VPN"
    assert info["supportUrl"] == "https://support.example"
    assert info["profileUrl"] == "https://panel.example"
    assert info["telegramUrl"] == "https://t.me/example"
