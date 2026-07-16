from __future__ import annotations

import json
import pytest

from xray_fluent.application import node_service
from xray_fluent.link_parser import MAX_IMPORT_BYTES, parse_links_text, validate_node_outbound


def test_happ_premium_headers_and_body_directives_are_preserved() -> None:
    metadata = node_service._extract_subscription_metadata(
        {
            "profile-title": "Lumen Premium",
            "providerid": "ABCD1234",
            "support-url": "https://support.example",
            "fragmentation-enable": "1",
            "mux-enable": "true",
        },
        "Happ Windows",
    )
    cleaned, body = node_service._extract_happ_body_metadata(
        "#subscription-autoconnect: 1\n#ping-type proxy\n"
        "vless://00000000-0000-0000-0000-000000000001@one.example:443"
        "?encryption=none&type=tcp&security=none#one"
    )

    merged = node_service._merge_subscription_info(metadata, body)
    assert merged["profileTitle"] == "Lumen Premium"
    assert merged["providerId"] == "ABCD1234"
    assert merged["premiumFeatures"] == {
        "fragmentation-enable": "1",
        "mux-enable": "true",
        "subscription-autoconnect": "1",
        "ping-type": "proxy",
    }
    assert cleaned.startswith("vless://")


def test_happ_server_description_is_displayed_separately_from_name() -> None:
    text = (
        "vless://00000000-0000-0000-0000-000000000001@one.example:443"
        "?encryption=none&type=tcp&security=none#Premium%20NL"
        "?serverDescription=SGFwcCB0aGUgYmVzdA=="
    )

    nodes, errors = parse_links_text(text)

    assert errors == []
    assert nodes[0].name == "Premium NL"
    assert nodes[0].description == "Happ the best"


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


def test_clash_yaml_imports_authenticated_socks5_and_http_proxies() -> None:
    text = """
proxies:
  - name: SOCKS relay
    type: socks5
    server: socks.example
    port: 1080
    username: socks-user
    password: socks-pass
    udp: true
  - name: HTTP relay
    type: http
    server: http.example
    port: 8080
    username: http-user
    password: http-pass
"""

    nodes, errors = parse_links_text(text)

    assert errors == []
    assert [(node.scheme, node.server, node.port) for node in nodes] == [
        ("socks", "socks.example", 1080),
        ("http", "http.example", 8080),
    ]
    assert nodes[0].outbound == {
        "protocol": "socks",
        "settings": {
            "servers": [
                {
                    "address": "socks.example",
                    "port": 1080,
                    "users": [{"user": "socks-user", "pass": "socks-pass"}],
                }
            ]
        },
    }
    assert nodes[1].outbound["settings"]["servers"][0]["users"] == [
        {"user": "http-user", "pass": "http-pass"}
    ]


def test_subscription_tls_eof_retries_same_profile_direct(monkeypatch) -> None:
    calls = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        calls.append((profile, headers["User-Agent"], headers["X-Hwid"], direct))
        if len(calls) == 1:
            raise OSError("<urlopen error TLS/SSL connection has been closed (EOF) (_ssl.c:1010)>")
        return (
            "vless://00000000-0000-0000-0000-000000000001@one.example:443?encryption=none&type=tcp&security=none#one",
            {"clientProfile": profile},
        )

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="",
        use_real_hwid=False,
    )

    assert errors == []
    assert "one.example" in text
    assert info["clientProfile"] == "Happ Windows"
    assert info["networkPath"] == "direct"
    assert calls[:2] == [
        (
            "Happ Windows",
            node_service.HAPP_WINDOWS_USER_AGENT,
            node_service.DEFAULT_SUBSCRIPTION_HWID,
            True,
        ),
        (
            "Happ Windows",
            node_service.HAPP_WINDOWS_USER_AGENT,
            node_service.DEFAULT_SUBSCRIPTION_HWID,
            True,
        ),
    ]


def test_subscription_uses_custom_user_agent_and_converter_first(monkeypatch) -> None:
    calls = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        calls.append((url, profile, headers["User-Agent"], headers["X-Hwid"], direct))
        return (
            "vless://00000000-0000-0000-0000-000000000001@one.example:443"
            "?encryption=none&type=tcp&security=none#one",
            {"clientProfile": profile},
        )

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path?id=1",
        user_agent="CustomClient/2.0",
        hwid="custom-device-id",
        use_real_hwid=False,
        converter_url="https://converter.example/sub?url={url}",
    )

    assert errors == []
    assert "one.example" in text
    assert info["clientProfile"] == "Custom"
    assert calls == [
        (
            "https://converter.example/sub?url=https%3A%2F%2Fsub.example%2Fpath%3Fid%3D1",
            "Custom",
                "CustomClient/2.0",
                "custom-device-id",
                True,
        )
    ]


def test_subscription_uses_real_windows_hwid_by_default(monkeypatch) -> None:
    calls = []

    monkeypatch.setattr(node_service, "_windows_machine_hwid", lambda: "real-machine-guid")

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = False):
        calls.append((profile, headers["X-Hwid"], direct))
        return (
            "vless://00000000-0000-0000-0000-000000000001@one.example:443"
            "?encryption=none&type=tcp&security=none#one",
            {"clientProfile": profile},
        )

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    _text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="manual-device-id",
    )

    assert errors == []
    assert info["clientProfile"] == "Happ Windows"
    assert calls == [("Happ Windows", "real-machine-guid", True)]


def test_subscription_rejects_hwid_with_newline(monkeypatch) -> None:
    monkeypatch.setattr(
        node_service,
        "_fetch_subscription_with_headers",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("request must not start")),
    )

    text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="device-id\r\nX-Injected: true",
        use_real_hwid=False,
    )

    assert text == ""
    assert info == {}
    assert errors == ["HWID не должен содержать переносы строк"]


def test_subscription_rejects_declared_oversized_body(monkeypatch) -> None:
    class _Response:
        headers = {"Content-Length": str(node_service.MAX_SUBSCRIPTION_BYTES + 1)}

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def read(self, _size=-1):
            raise AssertionError("oversized response body must not be read")

    class _Opener:
        def open(self, *_args, **_kwargs):
            return _Response()

    class _DirectOpener:
        def __enter__(self):
            return _Opener()

        def __exit__(self, *_args):
            return None

    monkeypatch.setattr(node_service, "DirectUrlOpener", _DirectOpener)

    with pytest.raises(RuntimeError, match="слишком большая"):
        node_service._fetch_subscription_with_headers(
            "https://sub.example/large",
            "Lumen",
            {"User-Agent": "test"},
        )


def test_import_file_size_limit_is_checked_before_read(tmp_path) -> None:
    path = tmp_path / "large.txt"
    with path.open("wb") as stream:
        stream.seek(MAX_IMPORT_BYTES)
        stream.write(b"x")

    nodes, errors = parse_links_text(str(path))

    assert nodes == []
    assert errors and "exceeds" in errors[0]


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


def test_happ_crypt5_1_link_reports_friendly_error(monkeypatch) -> None:
    import shutil
    from xray_fluent import happ_crypt_keys
    # Remove key and mock node to ensure it falls back and raises HappKeyUnavailableError
    monkeypatch.delitem(happ_crypt_keys.CRYPT5_KEYS_B64, "vdfzfoff", raising=False)
    monkeypatch.setattr(shutil, "which", lambda cmd: None)

    link = (
        "happ://crypt5/fzvd4oXqWHPd9ZJzbmZcpU3I20FsDc8WfLpIJg8yO6G9p/GbNqkmpD1avm2fTYWs"
        "JmVeKxs/zdzR8yugTK73iSH6DXZ+Z/U6KivYcEeNBtYcSrziaK5+PDLsBMsCL1qwyDpXGn3esHXxj9t"
        "XNE/t0mmHiJycS6n6B3TnrpXNsBcpEUgji9oORF46JK0i5xwpAXrDNqY/4hLaGJhK0X4hoFkyuqx8M1"
        "VKXabyVq9q0geu84PwTPH2FOeOh1rKmFNWTMMcOSPG2YjFg6phIgEpoks8fwystrTVWV3138pqmeMRw"
        "zYthQcatqxRRMsrcwGnhq4mymB813vPboFGHflMcyYT/hpWAz9WfPPWjldEfgMhLHiS0+mznmZsHY9n"
        "9ZFU8gMHDtbIJTirbukv6V2taTh6wan4a6FWKovf85mIO6iUYbpQE3Uz3czKldiBx/MEFfTA5/k9N3W"
        "C1MQG2LddZ6Vod6thWpwaN7/ZhgqoHflA1hoV0SDaQ0q+EWI+egMoFrsRs55E91r1yObG5uYw9OZ399"
        "Qtv3ecveX98YOF4k8cn0DLrYhm7iCrbpWwLeg4bCFIY9KTq+u1TAqNIKxMlm29Tb2tSMFu7zoypz+Ga"
        "cEl00y4lTHpm/FTQtbqHxSSz7GCVYepZXfJkxQkjMf9V53YyrYtsbGhw8mhUnHOtEg0L/kHldlqpRqG"
        "ctgvA1aA7OzpviIoyYv6BvqxblSBQrYIRZEj1WPE8P+rNodlI+6jMC16QFW/b2NWUtzuz7U8+slkCHd"
        "TV20hv+GZ6nIap41RKp41OPi5Un+PTkfGailpGazGInwecp8DXYuvudSxZqIeopf8YODcle1iWnSUJk"
        "urlnNP55jlmwCffr9c70mf7B+Q6OtMfb/f7rL8p3DjQLmzW/Cv+q0l2nCpqAxYM1+Nfos=ff"
    )

    text, _info, errors = node_service.fetch_subscription_payload(link)

    assert text == ""
    assert errors and errors[0].startswith("Happ:")
    assert "crypt5.1" in errors[0]


def test_happ_crypt_direct_config_payload(monkeypatch) -> None:
    # Если happ-ссылка расшифровывается не в URL, а в список конфигов —
    # он парсится напрямую, без сетевого запроса.
    vless = (
        "vless://00000000-0000-0000-0000-000000000001@one.example:443"
        "?encryption=none&type=tcp&security=none#one"
    )
    monkeypatch.setattr(node_service, "is_happ_crypt_link", lambda url: True)
    monkeypatch.setattr(node_service, "decrypt_happ_link", lambda url: vless)

    text, info, errors = node_service.fetch_subscription_payload("happ://crypt5/whatever")

    assert "one.example" in text
    assert errors == []
    assert info.get("clientProfile") == "Happ"
