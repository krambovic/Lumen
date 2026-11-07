import io
import json
import zipfile
from contextlib import nullcontext
from types import SimpleNamespace

from xray_fluent import diagnostics as diag, diagnostics_uploader as uploader


INSTALL_ID = "7a8a8892f5334c0ea8ce6158056ee5ad"


def test_error_batch_keeps_identity_through_http_transport(monkeypatch):
    requests = []
    monkeypatch.setattr(uploader, "get_install_id", lambda: INSTALL_ID)
    monkeypatch.setattr(uploader.urllib.request, "build_opener", lambda *args: SimpleNamespace(
        open=lambda request, **kwargs: (requests.append(request), nullcontext())[1]))
    uploader.set_uploads_enabled(True)
    handler = uploader.HttpDiagnosticsHandler("https://diagnostics.example.test")
    try:
        assert handler._post_events([{"msg": "password=secret", "private": "drop"}])
        payload = json.loads(requests[0].data)
        assert payload["install_id"] == INSTALL_ID
        assert payload["platform"] == "windows"
        assert "secret" not in payload["events"][0]["msg"]
        assert "private" not in payload["events"][0]
    finally:
        handler.close()
        uploader.set_uploads_enabled(False)


def test_export_and_repeated_zip_filtering_keep_identity(tmp_path, monkeypatch):
    monkeypatch.setattr(diag, "get_install_id", lambda: INSTALL_ID)
    path = diag.export_diagnostics(tmp_path / "out.zip", SimpleNamespace(), [], {})
    data = path.read_bytes()
    for _ in range(3):
        data = diag.sanitize_diagnostic_zip(data)
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            assert json.loads(archive.read("meta.json"))["install_id"] == INSTALL_ID
            assert archive.namelist() == ["meta.json"]


def test_metadata_does_not_preserve_arbitrary_identity_or_personal_fields():
    meta = diag._sanitize_meta({"install_id": "private-user", "hostname": "private-host"}, log_files=[])
    assert "install_id" not in meta
    assert "hostname" not in meta
