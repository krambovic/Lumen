from __future__ import annotations

import base64
import json
import sys

import pytest

from xray_fluent.direct_http import DirectNetworkUnavailable
import xray_fluent.subscription_fetcher as fetcher


class _FakeResponse:
    def __init__(self, body: bytes = b"subscription") -> None:
        self._body = body
        self._read = False
        self.headers = {"Content-Length": str(len(body)), "Profile-Title": "Example"}
        self.status = 200

    def __enter__(self):
        return self

    def __exit__(self, *_args) -> None:
        return None

    def read(self, _size: int = -1) -> bytes:
        if self._read:
            return b""
        self._read = True
        return self._body


def test_download_falls_back_to_process_direct_when_route_is_unavailable(monkeypatch) -> None:
    class _UnavailableDirectOpener:
        def __enter__(self):
            raise DirectNetworkUnavailable("no physical route")

        def __exit__(self, *_args) -> None:
            return None

    class _FallbackOpener:
        def open(self, _request, *, timeout: float):
            assert timeout == 7.0
            return _FakeResponse()

    monkeypatch.setattr(fetcher, "DirectUrlOpener", _UnavailableDirectOpener)
    monkeypatch.setattr(fetcher.urllib.request, "build_opener", lambda *_handlers: _FallbackOpener())

    result = fetcher._download_in_current_process(
        "https://example.com/sub",
        {"User-Agent": "Lumen"},
        timeout=7.0,
        max_bytes=1024,
        allow_process_direct_fallback=True,
    )

    assert result.body == b"subscription"
    assert result.headers["profile-title"] == "Example"
    assert result.status == 200
    assert result.transport == "process-direct"


def test_main_process_never_falls_through_into_tun_when_route_is_unavailable(monkeypatch) -> None:
    class _UnavailableDirectOpener:
        def __enter__(self):
            raise DirectNetworkUnavailable("no physical route")

        def __exit__(self, *_args) -> None:
            return None

    monkeypatch.setattr(fetcher, "DirectUrlOpener", _UnavailableDirectOpener)

    with pytest.raises(DirectNetworkUnavailable):
        fetcher._download_in_current_process(
            "https://example.com/sub",
            {},
            timeout=7.0,
            max_bytes=1024,
        )


def test_run_fetcher_process_waits_for_one_shot_child_and_decodes_response() -> None:
    encoded = base64.b64encode(b"child-result").decode("ascii")
    script = (
        "import json,sys,time; "
        "json.load(sys.stdin); "
        "time.sleep(0.35); "
        f"json.dump({{'ok':True,'body':'{encoded}','headers':{{'X-Test':'yes'}},"
        "'status':200,'transport':'process-direct'},sys.stdout)"
    )

    result = fetcher._run_fetcher_process(
        [sys.executable, "-c", script],
        "https://example.com/sub",
        {},
        timeout=2.0,
        max_bytes=1024,
        cancelled=None,
    )

    assert result.body == b"child-result"
    assert result.headers == {"x-test": "yes"}
    assert result.status == 200
    assert result.transport == "process-direct"


def test_run_fetcher_process_terminates_child_when_cancelled() -> None:
    script = "import json,sys,time; json.load(sys.stdin); time.sleep(30)"
    checks = 0

    def cancelled() -> bool:
        nonlocal checks
        checks += 1
        return checks >= 2

    with pytest.raises(fetcher.SubscriptionFetcherCancelled):
        fetcher._run_fetcher_process(
            [sys.executable, "-c", script],
            "https://example.com/sub",
            {},
            timeout=2.0,
            max_bytes=1024,
            cancelled=cancelled,
        )
