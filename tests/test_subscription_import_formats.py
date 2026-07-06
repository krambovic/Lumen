from __future__ import annotations

import json
import uuid

from xray_fluent.application import node_service
from xray_fluent.link_parser import parse_links_text, validate_node_outbound

from happ_encrypt_fixtures import encrypt_crypt, encrypt_crypt5

_ONE_CONFIG = (
    "vless://00000000-0000-0000-0000-000000000001@one.example:443"
    "?encryption=none&type=tcp&security=none#one"
)
_TWO_CONFIGS = _ONE_CONFIG + "\n" + (
    "vless://00000000-0000-0000-0000-000000000002@two.example:443"
    "?encryption=none&type=tcp&security=none#two"
)


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


# --------------------------------------------------------------------------- #
# HAPP: stable HWID + encrypted subscription links
# --------------------------------------------------------------------------- #
def test_happ_profiles_send_stable_uuid_hwid() -> None:
    profiles = dict(node_service._subscription_client_profiles())

    win_hwid = profiles["Happ Windows"]["X-Hwid"]
    uuid.UUID(win_hwid)  # raises unless a valid UUID
    assert win_hwid != "00000000-0000-4000-8000-000000000000"
    # Both HAPP profiles present the same device id.
    assert profiles["Happ"]["X-Hwid"] == win_hwid
    # Non-HAPP profiles do not carry a HWID header.
    assert "X-Hwid" not in profiles["Clash Meta"]


def test_happ_crypt_link_decrypts_to_url_and_fetches_with_hwid(monkeypatch) -> None:
    real_url = "https://panel.example/api/sub/TOKEN123"
    crypt_link = encrypt_crypt(real_url, mode=2)
    calls: list[tuple[str, str, dict]] = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        calls.append((url, profile, dict(headers)))
        return _TWO_CONFIGS, {"clientProfile": profile}

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(crypt_link)

    assert errors == []
    assert "one.example" in text and "two.example" in text
    # The decrypted real URL is what gets fetched — with a stable HWID header.
    fetched_url, profile, headers = calls[0]
    assert fetched_url == real_url
    assert profile == "Happ Windows"
    uuid.UUID(headers["X-Hwid"])


def test_happ_crypt_link_with_inline_configs_needs_no_network(monkeypatch) -> None:
    crypt_link = encrypt_crypt(_TWO_CONFIGS, mode=3)

    def boom(*args, **kwargs):  # network must not be touched
        raise AssertionError("network fetch should not happen for inline configs")

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", boom)

    text, info, errors = node_service.fetch_subscription_payload(crypt_link)

    assert errors == []
    assert info.get("clientProfile") == "Happ (decrypted)"
    nodes, parse_errors = parse_links_text(text)
    assert [n.server for n in nodes] == ["one.example", "two.example"]


def test_happ_crypt5_link_decrypts_and_fetches(monkeypatch) -> None:
    real_url = "https://p.example/sub/abc"
    crypt_link = encrypt_crypt5(real_url)
    captured: list[str] = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        captured.append(url)
        return _ONE_CONFIG, {"clientProfile": profile}

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(crypt_link)

    assert errors == []
    assert captured == [real_url]
    assert "one.example" in text


def test_happ_crypt_decrypt_failure_reports_error(monkeypatch) -> None:
    def boom(*args, **kwargs):
        raise AssertionError("must not fetch when decryption fails")

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", boom)

    import base64

    junk = base64.b64encode(b"garbage-block" * 12).decode()
    text, info, errors = node_service.fetch_subscription_payload("happ://crypt2/" + junk)

    assert text == ""
    assert errors and "HAPP" in errors[0]


def test_subscription_body_that_is_encrypted_happ_link_is_expanded() -> None:
    crypt_body = encrypt_crypt(_TWO_CONFIGS, mode=1)
    expanded = node_service._maybe_expand_happ_body(crypt_body)
    nodes, errors = parse_links_text(expanded)
    assert [n.server for n in nodes] == ["one.example", "two.example"]


def test_parse_links_text_decrypts_inline_happ_config_link() -> None:
    crypt_link = encrypt_crypt(_TWO_CONFIGS, mode=4)
    nodes, errors = parse_links_text(crypt_link)
    assert errors == []
    assert [n.server for n in nodes] == ["one.example", "two.example"]


def test_parse_links_text_hints_when_happ_link_is_a_subscription_url() -> None:
    crypt_link = encrypt_crypt("https://panel.example/sub/token", mode=2)
    nodes, errors = parse_links_text(crypt_link)
    assert nodes == []
    assert errors and "подписк" in errors[0].lower()
