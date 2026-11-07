import base64
from http.client import parse_headers
import io
from types import SimpleNamespace
from urllib.error import HTTPError

import pytest

from xray_fluent.application.node_service import _decode_profile_header, _extract_subscription_metadata
from xray_fluent.subscription_fetcher import _not_modified_http_error, _read_http_response
from xray_fluent.subscription_presentation import present_subscription, text


TITLE = "Справочная служба Вашей сети"


def test_utf8_http_headers_reach_subscription_metadata_and_banner():
    raw = (f"Profile-Title: {TITLE}\r\nProfile-Description: Описание подписки\r\n"
           f"Announce: Обновите подписку\r\nBanner-Text: {TITLE}\r\n"
           f"Providerid: demo\r\nSub-Info-Text: {TITLE}\r\n"
           "Sub-Info-Button-Text: Поддержка\r\nSub-Info-Button-Link: https://example.test\r\n\r\n").encode("utf-8")
    response = SimpleNamespace(headers=parse_headers(io.BytesIO(raw)), read=io.BytesIO(b"body").read, status=200)
    _, headers, _ = _read_http_response(response, 1024)
    info = _extract_subscription_metadata(headers, "Happ Windows")
    assert info["profileTitle"] == TITLE
    assert info["profileDescription"] == "Описание подписки"
    assert info["announcement"] == "Обновите подписку"
    assert info["bannerText"] == TITLE
    presentation = present_subscription({"userinfo": info})
    assert presentation["banners"][0]["text"] == TITLE
    assert presentation["banners"][0]["button"] == "Поддержка"


@pytest.mark.parametrize("value", ["х", "Р", "🇷🇺 Моя подписка", "你好", "اشتراک", "Café", "Telegram"])
def test_correct_unicode_and_base64_stay_unchanged(value):
    encoded = "base64:" + base64.b64encode(value.encode()).decode().rstrip("=")
    assert _decode_profile_header(value) == value
    assert _decode_profile_header(encoded) == value
    assert text(value) == value
    assert text(encoded) == value


@pytest.mark.parametrize("value", ["х", "Р", TITLE])
def test_utf8_header_bytes_are_not_lost_by_whitespace_stripping(value):
    raw = f"Profile-Title: {value}\r\n\r\n".encode()
    response = SimpleNamespace(headers=parse_headers(io.BytesIO(raw)), read=lambda count: b"", status=200)
    _, headers, _ = _read_http_response(response, 1024)
    assert _extract_subscription_metadata(headers, "Lumen")["profileTitle"] == value


@pytest.mark.parametrize("encoding,value", [("latin-1", TITLE), ("cp1252", "Поддержка")])
def test_cached_banner_is_repaired_without_refresh(encoding, value):
    broken = value.encode().decode(encoding)
    stored = {"userinfo": {"providerId": "demo", "premiumFeatures": {"sub-info-text": broken}}}
    assert present_subscription(stored)["banners"][0]["text"] == value


def test_invalid_utf8_and_correct_legacy_text_are_not_lossily_replaced():
    for value in ("François", "München", "Ã invalid", "Привет", "� incomplete"):
        assert text(value) == value


def test_cached_double_encoding_is_repaired_once_and_stays_stable():
    value = TITLE
    for _ in range(2):
        value = value.encode().decode("latin-1")
    for _ in range(3):
        value = text(value)
        assert value == TITLE


def test_not_modified_headers_preserve_utf8_continuation_bytes():
    headers = parse_headers(io.BytesIO("Profile-Title: хР\r\n\r\n".encode()))
    response = _not_modified_http_error(HTTPError("https://example.test", 304, "Not Modified", headers, None), "direct-route")
    assert _extract_subscription_metadata(response.headers, "Lumen")["profileTitle"] == "хР"
