from __future__ import annotations

import pytest

from xray_fluent import happ_crypto
from xray_fluent.happ_crypto import (
    HappDecryptError,
    decrypt_happ_link,
    is_happ_crypt_link,
)

from happ_encrypt_fixtures import encrypt_crypt, encrypt_crypt5

_SUB_URL = "https://panel.example/api/sub/USERTOKEN?name=Test%20VPN&extra=" + "x" * 300
_CONFIG_LIST = (
    "vless://00000000-0000-0000-0000-000000000001@one.example:443"
    "?encryption=none&type=tcp&security=none#One\n"
    "vless://00000000-0000-0000-0000-000000000002@two.example:443"
    "?encryption=none&type=tcp&security=none#Two"
)


@pytest.mark.parametrize("mode", [1, 2, 3, 4])
def test_crypt1_to_4_round_trip(mode: int) -> None:
    link = encrypt_crypt(_SUB_URL, mode)
    assert is_happ_crypt_link(link)
    assert decrypt_happ_link(link) == _SUB_URL


def test_crypt5_round_trip() -> None:
    link = encrypt_crypt5(_SUB_URL)
    assert is_happ_crypt_link(link)
    assert decrypt_happ_link(link) == _SUB_URL


def test_crypt5_round_trip_all_markers() -> None:
    # Every embedded marker key must be usable for decryption.
    for marker in list(happ_crypto.CRYPT5_MARKER_KEYS_B64)[:5]:
        link = encrypt_crypt5("https://x.example/s/" + marker, marker=marker)
        assert decrypt_happ_link(link) == "https://x.example/s/" + marker


def test_crypt_can_wrap_inline_config_list() -> None:
    link = encrypt_crypt(_CONFIG_LIST, 2)
    assert decrypt_happ_link(link) == _CONFIG_LIST


@pytest.mark.parametrize(
    "value,expected",
    [
        ("happ://crypt/AAAA", True),
        ("happ://crypt5/AAAA", True),
        ("HAPP://CRYPT2/AAAA", True),
        ("happ://add/vless://x", False),
        ("https://example.com", False),
        ("vless://uuid@host:443", False),
        ("", False),
        (None, False),
    ],
)
def test_is_happ_crypt_link(value, expected) -> None:
    assert is_happ_crypt_link(value) is expected


def test_unknown_crypt5_marker_raises(monkeypatch) -> None:
    # A structurally valid crypt5 link whose marker is not in the embedded table
    # must be rejected rather than silently mis-decrypted.
    marker = next(iter(happ_crypto.CRYPT5_MARKER_KEYS_B64))
    link = encrypt_crypt5("https://x.example/s", marker=marker)
    monkeypatch.setattr(
        happ_crypto,
        "CRYPT5_MARKER_KEYS_B64",
        {m: k for m, k in happ_crypto.CRYPT5_MARKER_KEYS_B64.items() if m != marker},
    )
    happ_crypto._crypt5_keys.clear()
    with pytest.raises(HappDecryptError):
        decrypt_happ_link(link)


def test_crypt5_too_short_payload_raises() -> None:
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("happ://crypt5/AAAA")


def test_malformed_rsa_payload_raises() -> None:
    import base64

    junk = base64.b64encode(b"not-a-real-rsa-block" * 10).decode()
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("happ://crypt2/" + junk)


def test_non_happ_link_raises() -> None:
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("https://example.com/sub")


def test_empty_payload_raises() -> None:
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("happ://crypt/")
