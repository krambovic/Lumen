from __future__ import annotations

import json
import pytest

from xray_fluent.application import node_service
from xray_fluent.link_parser import MAX_IMPORT_BYTES, parse_links_text, validate_node_outbound
from xray_fluent.subscription_fetcher import _read_http_response


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


_REAL_LINK = (
    "vless://00000000-0000-0000-0000-000000000001@one.example:443"
    "?encryption=none&type=tcp&security=none#one"
)
_STUB_LINK = (
    "vless://00000000-0000-0000-0000-000000000002@stub.example:443"
    "?encryption=none&type=tcp&security=none#{name}"
)
# "Ваш клиент не поддерживается" и "Обновите приложение" в percent-encoding.
_STUB_NAMES = (
    "Your%20client%20is%20not%20supported",
    "Please%20update%20your%20app",
    "%D0%92%D0%B0%D1%88%20%D0%BA%D0%BB%D0%B8%D0%B5%D0%BD%D1%82%20%D0%BD%D0%B5%20"
    "%D0%BF%D0%BE%D0%B4%D0%B4%D0%B5%D1%80%D0%B6%D0%B8%D0%B2%D0%B0%D0%B5%D1%82%D1%81%D1%8F",
    "%D0%9E%D0%B1%D0%BD%D0%BE%D0%B2%D0%B8%D1%82%D0%B5%20"
    "%D0%BF%D1%80%D0%B8%D0%BB%D0%BE%D0%B6%D0%B5%D0%BD%D0%B8%D0%B5",
)


def test_lumen_client_profile_is_tried_before_happ() -> None:
    names = [name for name, _headers in node_service._SUBSCRIPTION_CLIENT_PROFILES]

    assert names[0] == "Lumen"
    assert names.index("Lumen") < names.index("Happ Windows")
    assert names.index("Lumen") < names.index("Happ")
    assert len(names) == len(set(names))


def test_lumen_user_agent_carries_the_real_app_version() -> None:
    from xray_fluent.constants import APP_VERSION

    assert node_service.LUMEN_SUBSCRIPTION_USER_AGENT == f"Lumen-Subscription/Windows-{APP_VERSION}"
    assert dict(node_service._SUBSCRIPTION_CLIENT_PROFILES)["Lumen"]["User-Agent"] == (
        node_service.LUMEN_SUBSCRIPTION_USER_AGENT
    )


def test_client_stub_is_detected_in_english_and_russian() -> None:
    for name in _STUB_NAMES:
        body = _STUB_LINK.format(name=name)
        nodes, errors = parse_links_text(body)
        assert errors == []
        # Сама заглушка выглядит как валидный сервер — отличаем её только по тексту.
        assert node_service._parsed_nodes_are_usable(nodes)
        assert node_service._looks_like_client_stub(body, nodes)
        assert not node_service._subscription_response_is_accepted(body, nodes)

    nodes, _errors = parse_links_text(_REAL_LINK)
    assert not node_service._looks_like_client_stub(_REAL_LINK, nodes)
    assert node_service._subscription_response_is_accepted(_REAL_LINK, nodes)


@pytest.mark.parametrize("stub_name", _STUB_NAMES)
def test_client_not_supported_stub_falls_back_to_happ(monkeypatch, stub_name) -> None:
    calls = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = True):
        calls.append(profile)
        if profile == "Lumen":
            return _STUB_LINK.format(name=stub_name), {"clientProfile": profile}
        return _REAL_LINK, {"clientProfile": profile}

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="",
        use_real_hwid=False,
    )

    assert errors == []
    assert "one.example" in text
    assert calls[0] == "Lumen"
    assert info["clientProfile"] == "Happ Windows"


def test_empty_body_advances_to_the_next_client_profile(monkeypatch) -> None:
    calls = []

    def fake_fetch(url: str, profile: str, headers: dict, *, direct: bool = True):
        calls.append(profile)
        if profile == "Lumen":
            return "", {"clientProfile": profile}
        return _REAL_LINK, {"clientProfile": profile}

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    _text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="",
        use_real_hwid=False,
    )

    assert errors == []
    assert calls == ["Lumen", "Happ Windows"]
    assert info["clientProfile"] == "Happ Windows"


def test_http_404_does_not_walk_the_client_profiles(monkeypatch) -> None:
    calls = []

    def fail_fetch(url: str, profile: str, headers: dict, **_kwargs):
        calls.append(profile)
        raise OSError("HTTP Error 404: Not Found")

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fail_fetch)

    text, _info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        hwid="",
        use_real_hwid=False,
    )

    assert text == ""
    assert calls == ["Lumen"]
    assert errors and "не найдена" in errors[0]


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
    assert info["clientProfile"] == "Lumen"
    assert info["networkPath"] == "direct"
    assert calls[:2] == [
        (
            "Lumen",
            node_service.LUMEN_SUBSCRIPTION_USER_AGENT,
            node_service.DEFAULT_SUBSCRIPTION_HWID,
            True,
        ),
        (
            "Lumen",
            node_service.LUMEN_SUBSCRIPTION_USER_AGENT,
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


def test_subscription_proxy_tun_mode_is_forwarded_to_http_fetch(monkeypatch) -> None:
    calls = []

    def fake_fetch(
        url: str,
        profile: str,
        headers: dict,
        *,
        direct: bool = True,
        proxy_url: str = "",
    ):
        calls.append((url, profile, direct, proxy_url))
        return (
            "vless://00000000-0000-0000-0000-000000000001@one.example:443"
            "?encryption=none&type=tcp&security=none#one",
            {"clientProfile": profile},
        )

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, info, errors = node_service.fetch_subscription_payload(
        "https://sub.example/path",
        use_proxy_tun=True,
        proxy_url="http://127.0.0.1:10809",
    )

    assert errors == []
    assert "one.example" in text
    assert info["networkPath"] == "proxy-tun"
    assert calls == [
        (
            "https://sub.example/path",
            "Lumen",
            False,
            "http://127.0.0.1:10809",
        )
    ]


def test_subscription_dns_failure_is_explained_and_suggests_proxy_tun(monkeypatch) -> None:
    def fail_fetch(*_args, **_kwargs):
        raise OSError("<urlopen error [Errno 11001] getaddrinfo failed>")

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fail_fetch)

    text, _info, errors = node_service.fetch_subscription_payload("https://blocked.example/sub")

    assert text == ""
    assert len(errors) == 1
    assert "ошибка DNS" in errors[0]
    assert "getaddrinfo" not in errors[0]
    assert "Загружать подписки через прокси/TUN" in errors[0]
    assert "Настройки → Подписки" in errors[0]


def test_subscription_network_errors_are_decoded_consistently() -> None:
    cases = {
        "<urlopen error timed out>": "превышено время ожидания",
        "<urlopen error [WinError 10054] forcibly closed by remote host>": "принудительно разорвано",
        "<urlopen error [WinError 10061] connection refused>": "отклонил соединение",
        "<urlopen error [WinError 10051] network is unreachable>": "прямой маршрут",
        "<urlopen error TLS/SSL connection has been closed (EOF) (_ssl.c:1010)>": "TLS-обмена",
        "HTTP Error 404: Not Found": "Подписка не найдена",
    }

    for raw, expected in cases.items():
        message = node_service._friendly_subscription_fetch_error(
            OSError(raw),
            use_proxy_tun=False,
        )
        assert message is not None
        assert expected in message


def test_subscription_error_does_not_suggest_enabling_proxy_tun_twice(monkeypatch) -> None:
    def fail_fetch(*_args, **_kwargs):
        raise TimeoutError("timed out")

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fail_fetch)

    _text, _info, errors = node_service.fetch_subscription_payload(
        "https://blocked.example/sub",
        use_proxy_tun=True,
    )

    assert len(errors) == 1
    assert "уже включена" in errors[0]
    assert "Попробуйте включить" not in errors[0]


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
    assert info["clientProfile"] == "Lumen"
    assert calls == [("Lumen", "real-machine-guid", True)]


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

        def read(self, _size=-1):
            raise AssertionError("oversized response body must not be read")

    with pytest.raises(RuntimeError, match="слишком большая"):
        _read_http_response(_Response(), node_service.MAX_SUBSCRIPTION_BYTES)


def test_import_file_size_limit_is_checked_before_read(tmp_path) -> None:
    path = tmp_path / "large.txt"
    with path.open("wb") as stream:
        stream.seek(MAX_IMPORT_BYTES)
        stream.write(b"x")

    nodes, errors = parse_links_text(str(path), allow_file_reference=True)

    assert nodes == []
    assert errors and "exceeds" in errors[0]


def test_file_reference_is_ignored_for_downloaded_subscription_bodies(tmp_path) -> None:
    path = tmp_path / "secret.txt"
    path.write_text("vless://00000000-0000-0000-0000-000000000001@one.example:443#one", encoding="utf-8")

    nodes, errors = parse_links_text(str(path))

    assert nodes == []
    assert errors and "unsupported scheme" in errors[0]


def test_unc_file_reference_is_never_opened(monkeypatch) -> None:
    def _fail(self):  # pragma: no cover - must not run
        raise AssertionError(f"UNC path must not be stat'ed: {self}")

    monkeypatch.setattr("pathlib.Path.is_file", _fail)

    for body in (
        "file://attacker.example.com/share/a.txt",
        "\\\\attacker.example.com\\share\\a.txt",
        "//attacker.example.com/share/a.txt",
    ):
        nodes, errors = parse_links_text(body, allow_file_reference=True)
        assert nodes == []
        assert errors


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


def test_crypt5_1_candidates_survive_a_skip_index_at_the_region_end() -> None:
    from xray_fluent.happ_crypt import HappKeyUnavailableError, _c51_candidates, decrypt_happ_link

    # payload[18:20] == '21' and 21 % 4 == 1, which used to index url_region[21]
    # out of range and discard every trailer-length candidate.
    payload = "A" * 18 + "21" + "B" * 21 + "C" * 684 + "D" * 10

    assert len(_c51_candidates(payload)) > 0
    with pytest.raises(HappKeyUnavailableError):
        decrypt_happ_link(f"happ://crypt5/{payload}")


def test_crypt5_1_short_payload_does_not_raise_index_error() -> None:
    from xray_fluent.happ_crypt import HappDecryptError, decrypt_happ_link

    with pytest.raises(HappDecryptError) as excinfo:
        decrypt_happ_link("happ://crypt5/abcdefgh")

    assert "string index out of range" not in str(excinfo.value)


def test_legacy_base64_shadowsocks_link_with_slash_is_imported() -> None:
    import base64

    blob = base64.b64encode(b"aes-256-gcm:pw?x@example.com:8388").decode("ascii")
    assert "/" in blob

    nodes, errors = parse_links_text(f"ss://{blob}#legacy")

    assert errors == []
    server = nodes[0].outbound["settings"]["servers"][0]
    assert (nodes[0].server, nodes[0].port) == ("example.com", 8388)
    assert (server["method"], server["password"]) == ("aes-256-gcm", "pw?x")


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
