from __future__ import annotations

import pytest

from xray_fluent.application import node_service
from xray_fluent.incy_crypt import (
    IncyDecryptError,
    decrypt_incy_link,
    is_incy_crypt_link,
    is_incy_link,
)


INCY_LINK = (
    "incy://crypt1/"
    "AAECAwQFBgcICQoLNyIQL3rDwRZqnyoD8pGKSKPC6cwYYSGTRieILcbBXDI2_rVLW0ACi9IVK63oUXhNjJAgucw3__4DCG5s9js"
)
EXPECTED_URL = "https://example.com/sub?id=incy-test"
OFFICIAL_EXAMPLE = (
    "incy://crypt1/"
    "FZEVXuV39UEX1yHB3nkrgdPdrJ3syVxcQm_Y-lY0oKWAT5yRn00xe6ohg06aVWjWRrGJ7BAeEzuoFzv8XBosLnqnqqCMbnAJmR7EN2hII4Yyql1FtWlLlLs"
)


def test_detects_and_decrypts_public_incy_format() -> None:
    assert is_incy_link(INCY_LINK)
    assert is_incy_crypt_link(INCY_LINK.upper().replace(INCY_LINK[14:].upper(), INCY_LINK[14:]))
    assert decrypt_incy_link(INCY_LINK) == EXPECTED_URL


def test_decrypts_official_incy_documentation_example() -> None:
    assert decrypt_incy_link(OFFICIAL_EXAMPLE) == "https://incsub.myincteam.org/vTyt0xVE-aAjHv8T"


def test_rejects_empty_invalid_and_tampered_payloads() -> None:
    for link in ("incy://crypt1/", "incy://crypt1/not-base64!", INCY_LINK[:-1] + "A"):
        with pytest.raises(IncyDecryptError):
            decrypt_incy_link(link)


def test_subscription_fetch_decrypts_incy_before_network_request(monkeypatch) -> None:
    calls: list[str] = []
    node = (
        "vless://00000000-0000-0000-0000-000000000001@one.example:443"
        "?encryption=none&type=tcp&security=none#one"
    )

    def fake_fetch(url: str, _profile: str, _headers: dict, **_kwargs):
        calls.append(url)
        return node, {}

    monkeypatch.setattr(node_service, "_fetch_subscription_with_headers", fake_fetch)

    text, _info, errors = node_service.fetch_subscription_payload(
        INCY_LINK, hwid="", use_real_hwid=False
    )

    assert errors == []
    assert "one.example" in text
    assert calls == [EXPECTED_URL]


def test_subscription_fetch_reports_incy_authentication_failure_without_network(monkeypatch) -> None:
    monkeypatch.setattr(
        node_service,
        "_fetch_subscription_with_headers",
        lambda *_args, **_kwargs: pytest.fail("network must not be used"),
    )

    text, _info, errors = node_service.fetch_subscription_payload(INCY_LINK[:-1] + "A")

    assert text == ""
    assert errors and errors[0].startswith("Incy:")


def test_encrypted_subscription_source_is_not_replaced_by_provider_metadata() -> None:
    provider_info = {
        "premiumFeatures": {
            "new-url": "https://migrated.example/sub",
            "new-domain": "migrated.example",
        }
    }

    assert node_service._premium_subscription_url(INCY_LINK, provider_info) == INCY_LINK
    happ_link = "happ://crypt5/encrypted-source"
    assert node_service._premium_subscription_url(happ_link, provider_info) == happ_link
