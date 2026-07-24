from __future__ import annotations

from pathlib import Path

from xray_fluent import app_updater, core_resource_updater, http_utils
from xray_fluent.engines.xray import core_updater


class _Response:
    def __init__(self, chunks: list[bytes | Exception]) -> None:
        self._chunks = list(chunks)
        self.headers = {"Content-Length": "4"}
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *_args) -> None:
        self.close()

    def read(self, *_args) -> bytes:
        if not self._chunks:
            return b""
        value = self._chunks.pop(0)
        if isinstance(value, Exception):
            raise value
        return value

    def close(self) -> None:
        self.closed = True


def test_http_open_retries_direct_when_proxy_open_fails(monkeypatch) -> None:
    direct_response = _Response([b"ok"])

    class _ProxyOpener:
        def open(self, *_args, **_kwargs):
            raise ConnectionResetError("proxy failed")

    monkeypatch.setattr(http_utils, "build_proxy_opener", lambda _url: _ProxyOpener())
    monkeypatch.setattr(http_utils, "urlopen_direct", lambda *_args, **_kwargs: direct_response)

    assert http_utils.urlopen_proxy_first(
        "https://example.test/update",
        proxy_url="http://127.0.0.1:10809",
    ) is direct_response


def test_app_update_check_retries_direct_after_proxy_read_failure(monkeypatch) -> None:
    calls: list[str | None] = []

    def fake_open(_request, *, timeout, proxy_url):
        del timeout
        calls.append(proxy_url)
        if proxy_url:
            return _Response([ConnectionResetError("proxy read failed")])
        return _Response([b'{"tag_name":"v2.0.0"}'])

    monkeypatch.setattr(app_updater, "urlopen_proxy_first", fake_open)
    checker = app_updater.UpdateChecker(proxy_url="http://127.0.0.1:10809")

    assert checker._fetch_json("https://example.test/release") == {"tag_name": "v2.0.0"}
    assert calls == ["http://127.0.0.1:10809", None]


def test_xray_download_restarts_direct_after_partial_proxy_file(monkeypatch, tmp_path: Path) -> None:
    calls: list[str | None] = []

    def fake_open(_request, *, timeout, proxy_url):
        del timeout
        calls.append(proxy_url)
        if proxy_url:
            return _Response([b"bad", ConnectionResetError("proxy read failed")])
        return _Response([b"good", b""])

    monkeypatch.setattr(core_updater, "urlopen_proxy_first", fake_open)
    destination = tmp_path / "xray.zip"

    core_updater._download_file(
        "https://example.test/xray.zip",
        destination,
        proxy_url="http://127.0.0.1:10809",
    )

    assert destination.read_bytes() == b"good"
    assert calls == ["http://127.0.0.1:10809", None]


def test_resource_download_restarts_direct_after_partial_proxy_file(monkeypatch, tmp_path: Path) -> None:
    calls: list[str | None] = []

    def fake_open(_request, *, timeout, proxy_url):
        del timeout
        calls.append(proxy_url)
        if proxy_url:
            return _Response([b"bad", ConnectionResetError("proxy read failed")])
        return _Response([b"good", b""])

    monkeypatch.setattr(core_resource_updater, "urlopen_proxy_first", fake_open)
    destination = tmp_path / "resource.bin"

    core_resource_updater._download_direct(
        "https://example.test/resource.bin",
        destination,
        proxy_url="http://127.0.0.1:10809",
    )

    assert destination.read_bytes() == b"good"
    assert calls == ["http://127.0.0.1:10809", None]
