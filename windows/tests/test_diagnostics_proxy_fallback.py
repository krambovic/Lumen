import json
import ssl
import urllib.error
from contextlib import nullcontext
from types import SimpleNamespace

import pytest

from xray_fluent import diagnostics_uploader as uploader


BODY = json.dumps({"kind": "error-batch", "app_version": "2.1.3", "events": []}).encode()


@pytest.fixture(autouse=True)
def uploads():
    uploader.set_uploads_enabled(True)
    yield
    uploader.set_uploads_enabled(False)


def test_connect_failure_uses_configured_proxy_and_keeps_request(monkeypatch):
    routes, requests = [], []
    monkeypatch.setattr(uploader.urllib.request, "getproxies", lambda: {"https": "127.0.0.1:1080"})
    def build(proxy_handler, redirect_handler):
        routes.append(proxy_handler.proxies)
        assert isinstance(redirect_handler, uploader._NoRedirect)
        def open(request, **kwargs):
            requests.append(request)
            if len(routes) == 1:
                raise urllib.error.URLError(TimeoutError())
            return nullcontext()
        return SimpleNamespace(open=open)
    monkeypatch.setattr(uploader.urllib.request, "build_opener", build)
    assert uploader._send("https://diagnostics.example.test/api/ingest", BODY, "application/json", 1)
    assert routes == [{}, {"https": "http://127.0.0.1:1080"}]
    assert requests[0] is requests[1]


@pytest.mark.parametrize("failure", [
    None,
    urllib.error.HTTPError("https://diagnostics.example.test", 503, "unavailable", {}, None),
    TimeoutError("response timed out"),
    urllib.error.URLError(ssl.SSLCertVerificationError("invalid certificate")),
    urllib.error.URLError(ConnectionResetError()),
])
def test_no_fallback_after_success_or_ambiguous_failure(monkeypatch, failure):
    monkeypatch.setattr(uploader.urllib.request, "getproxies", lambda: pytest.fail("proxy must not be consulted"))
    def open(*args, **kwargs):
        if failure is not None:
            raise failure
        return nullcontext()
    monkeypatch.setattr(uploader.urllib.request, "build_opener", lambda *args: SimpleNamespace(open=open))
    assert uploader._send("https://diagnostics.example.test", BODY, "application/json", 1) is (failure is None)


def test_consent_revocation_prevents_proxy_fallback(monkeypatch):
    monkeypatch.setattr(uploader.urllib.request, "getproxies", lambda: pytest.fail("consent revoked"))
    def open(*args, **kwargs):
        uploader.set_uploads_enabled(False)
        raise urllib.error.URLError(ConnectionRefusedError())
    monkeypatch.setattr(uploader.urllib.request, "build_opener", lambda *args: SimpleNamespace(open=open))
    assert not uploader._send("https://diagnostics.example.test", BODY, "application/json", 1)


def test_loopback_upload_never_falls_back_to_external_proxy(monkeypatch):
    monkeypatch.setattr(uploader.urllib.request, "getproxies", lambda: pytest.fail("loopback must stay local"))
    def open(*args, **kwargs):
        raise urllib.error.URLError(ConnectionRefusedError())
    monkeypatch.setattr(uploader.urllib.request, "build_opener", lambda *args: SimpleNamespace(open=open))
    assert not uploader._send("http://127.0.0.1:3000", BODY, "application/json", 1)
