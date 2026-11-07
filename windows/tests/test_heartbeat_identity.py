from __future__ import annotations

import json
import threading
from contextlib import nullcontext
from types import SimpleNamespace
from uuid import UUID

import pytest

from xray_fluent import constants, data_paths, diagnostics_uploader as uploader


@pytest.mark.parametrize("existing_id", [None, "7a8a8892f5334c0ea8ce6158056ee5ad", "7a8a8892-f533-4c0e-a8ce-6158056ee5ad"])
def test_heartbeat_sends_persistent_identity_after_transport_filtering(tmp_path, monkeypatch, existing_id):
    monkeypatch.setattr(constants, "DATA_DIR", tmp_path)
    monkeypatch.setattr(data_paths, "_install_id_cache", None)
    if existing_id:
        (tmp_path / "install_id").write_text(existing_id, encoding="utf-8")
    requests = []
    dispatched = threading.Event()

    def capture(request, **kwargs):
        requests.append(request)
        dispatched.set()
        return nullcontext()

    monkeypatch.setattr(uploader.urllib.request, "build_opener", lambda *args: SimpleNamespace(open=capture))
    uploader.set_uploads_enabled(True)
    try:
        for _ in range(2):
            # Simulate a fresh process loading the ID from disk.
            monkeypatch.setattr(data_paths, "_install_id_cache", None)
            dispatched.clear()
            sender = uploader.HeartbeatSender("https://diagnostics.example.test/api/ingest", app_version="2.1.3")
            try:
                assert dispatched.wait(2), "heartbeat did not reach the HTTP transport"
            finally:
                sender.stop()
        saved_id = (tmp_path / "install_id").read_text(encoding="utf-8")
        UUID(saved_id)
        if existing_id:
            assert saved_id == existing_id
        assert len(requests) == 2
        for request in requests:
            payload = json.loads(request.data)
            assert payload == {
                "kind": "heartbeat", "app_version": "2.1.3", "platform": "windows",
                "install_id": saved_id, "ts": payload["ts"],
            }
            assert type(payload["ts"]) is int and payload["ts"] > 0
    finally:
        uploader.set_uploads_enabled(False)


@pytest.mark.parametrize("install_id", [None, "", "../../private-path", "private-device-name"])
def test_heartbeat_filter_rejects_invalid_identity(install_id):
    with pytest.raises(ValueError, match="install_id"):
        uploader._safe_payload({"kind": "heartbeat", "install_id": install_id, "ts": 1})


def test_disabled_heartbeat_does_not_generate_identity(tmp_path, monkeypatch):
    monkeypatch.setattr(constants, "DATA_DIR", tmp_path)
    monkeypatch.setattr(data_paths, "_install_id_cache", None)
    monkeypatch.setattr(uploader, "_send", lambda *args, **kwargs: pytest.fail("telemetry is disabled"))
    uploader.set_uploads_enabled(False)
    sender = uploader.HeartbeatSender("https://diagnostics.example.test/api/ingest")
    sender.stop()
    assert not (tmp_path / "install_id").exists()
