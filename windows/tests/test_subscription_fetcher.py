from __future__ import annotations

import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import sys
import threading

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


def test_proxy_tun_mode_uses_local_proxy_without_starting_direct_helper(monkeypatch) -> None:
    captured = {}

    class _ProxyOpener:
        def open(self, _request, *, timeout: float):
            assert timeout == 7.0
            return _FakeResponse()

        def close(self) -> None:
            captured["closed"] = True

    def fake_build_opener(*handlers):
        captured["handlers"] = handlers
        return _ProxyOpener()

    monkeypatch.setattr(
        fetcher,
        "_fetcher_command",
        lambda: (_ for _ in ()).throw(AssertionError("direct helper must not start")),
    )
    monkeypatch.setattr(fetcher.urllib.request, "build_opener", fake_build_opener)

    result = fetcher.fetch_subscription_http(
        "https://example.com/sub",
        {"User-Agent": "Lumen"},
        timeout=7.0,
        max_bytes=1024,
        use_proxy_tun=True,
        proxy_url="http://127.0.0.1:10808",
    )

    proxy_handler = next(
        handler for handler in captured["handlers"]
        if isinstance(handler, fetcher.urllib.request.ProxyHandler)
    )
    assert proxy_handler.proxies == {
        "http": "http://127.0.0.1:10808",
        "https": "http://127.0.0.1:10808",
    }
    assert result.transport == "lumen-proxy"
    assert captured["closed"] is True


def test_proxy_tun_mode_really_sends_subscription_request_through_local_proxy(monkeypatch) -> None:
    requests: list[str] = []

    class _ProxyHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            requests.append(self.path)
            body = b"vless://from-local-proxy"
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Profile-Title", "Proxy path")
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, _format: str, *_args) -> None:
            return None

    server = ThreadingHTTPServer(("127.0.0.1", 0), _ProxyHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    monkeypatch.setattr(
        fetcher,
        "_fetcher_command",
        lambda: (_ for _ in ()).throw(AssertionError("direct helper must not start")),
    )
    try:
        result = fetcher.fetch_subscription_http(
            "http://subscription.invalid/profile",
            {"User-Agent": "Lumen"},
            timeout=3.0,
            max_bytes=1024,
            use_proxy_tun=True,
            proxy_url=f"http://127.0.0.1:{server.server_port}",
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2.0)

    assert requests == ["http://subscription.invalid/profile"]
    assert result.body == b"vless://from-local-proxy"
    assert result.headers["profile-title"] == "Proxy path"
    assert result.transport == "lumen-proxy"


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
