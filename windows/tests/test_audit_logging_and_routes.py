from __future__ import annotations

from urllib.error import HTTPError
from urllib.request import Request

import pytest

from xray_fluent.bounded_logs import LogRing
from xray_fluent import route_leases
from xray_fluent.http_redirect_policy import SafeRedirectHandler
from xray_fluent.qml_app.bridge.log_model import LogModel
from xray_fluent.secret_scrubber import scrub_text


def test_log_ring_is_bounded_without_replacing_the_container() -> None:
    ring = LogRing[str](maxlen=3)
    identity = id(ring)
    for value in ("one", "two", "three", "four"):
        ring.append(value)
    assert id(ring) == identity
    assert list(ring) == ["two", "three", "four"]
    assert ring[-2:] == ["three", "four"]
    assert ring.dropped == 1


def test_log_model_appends_bursts_and_evicts_in_one_bounded_batch() -> None:
    model = LogModel(max_lines=3)
    model.append_lines(["INFO: one", "INFO: two"])
    model.append_lines(["WARNING: three", "ERROR: four"])
    assert model.rowCount() == 3
    messages = [model.entry_at(index).message for index in range(3)]
    assert [message.rsplit(" ", 1)[-1] for message in messages] == ["two", "three", "four"]


def test_scrubber_removes_plain_and_markdown_escaped_share_links() -> None:
    secret = "private-token-that-must-not-survive"
    for separator in ("://", r"\://", r":\/\/"):
        source = f"failed: masque{separator}{secret}@profile.example?token={secret}"
        cleaned = scrub_text(source)
        assert secret not in cleaned
        assert "[REDACTED URI]" in cleaned


def test_scrubber_removes_authorization_and_password_values() -> None:
    cleaned = scrub_text("Authorization: Bearer abc.def.ghi\npassword=top secret, status=failed")
    assert "abc.def.ghi" not in cleaned
    assert "top secret" not in cleaned
    assert "status=failed" in cleaned


def test_preexisting_route_is_shared_but_never_deleted() -> None:
    key = ("192.0.2.1", "198.51.100.4", 9)
    calls: list[str] = []
    assert route_leases.acquire_route(
        key,
        lambda: calls.append("create") or True,
        lambda: True,
        lambda: calls.append("delete"),
    )
    assert route_leases.acquire_route(
        key,
        lambda: calls.append("create-again") or True,
        lambda: False,
        lambda: calls.append("delete-again"),
    )
    route_leases.release_route(key)
    route_leases.release_route(key)
    assert calls == []
    assert key not in route_leases._leases


def test_owned_route_is_deleted_only_after_last_lease() -> None:
    key = ("192.0.2.1", "198.51.100.5", 9)
    calls: list[str] = []
    assert route_leases.acquire_route(
        key,
        lambda: calls.append("create") or True,
        lambda: False,
        lambda: calls.append("delete"),
    )
    assert route_leases.acquire_route(key, lambda: False, lambda: False, lambda: None)
    route_leases.release_route(key)
    assert calls == ["create"]
    route_leases.release_route(key)
    assert calls == ["create", "delete"]
    assert key not in route_leases._leases


def test_failed_owned_route_cleanup_does_not_poison_future_leases() -> None:
    key = ("192.0.2.1", "198.51.100.6", 9)

    def fail_delete() -> None:
        raise OSError("injected cleanup failure")

    assert route_leases.acquire_route(key, lambda: True, lambda: False, fail_delete)
    with pytest.raises(OSError, match="cleanup failure"):
        route_leases.release_route(key)
    assert key not in route_leases._leases
    assert route_leases.acquire_route(key, lambda: True, lambda: False, lambda: None)
    route_leases.release_route(key)


def test_https_redirect_cannot_downgrade_or_embed_credentials() -> None:
    handler = SafeRedirectHandler()
    request = Request("https://provider.example/sub", headers={"User-Agent": "Lumen"})
    for target in (
        "http://provider.example/sub",
        "https://user:password@provider.example/sub",
        "file:///tmp/subscription",
    ):
        with pytest.raises(HTTPError):
            handler.redirect_request(request, None, 302, "Found", {}, target)


def test_cross_origin_redirect_strips_identifying_headers() -> None:
    handler = SafeRedirectHandler()
    request = Request(
        "https://provider.example/sub",
        headers={
            "User-Agent": "Lumen",
            "X-Hwid": "device-secret",
            "Authorization": "Bearer subscription-secret",
            "If-None-Match": "private-etag",
        },
    )
    redirected = handler.redirect_request(
        request, None, 302, "Found", {}, "https://cdn.example/config",
    )
    assert redirected is not None
    lowered = {key.lower(): value for key, value in redirected.header_items()}
    assert lowered.get("user-agent") == "Lumen"
    assert "x-hwid" not in lowered
    assert "authorization" not in lowered
    assert "if-none-match" not in lowered
