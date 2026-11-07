import io
import json
import zipfile
from types import SimpleNamespace

from xray_fluent import diagnostics as diag


def test_runtime_versions_survive_export_and_upload_filter(tmp_path, monkeypatch):
    runtime = {"architecture": "AMD64", "windows_version": "10.0.26100",
               "engines": {"xray": "26.2.6", "sing-box": "1.12.0-beta.30"}}
    monkeypatch.setattr(diag, "collect_runtime_context", lambda state: runtime)
    path = diag.export_diagnostics(tmp_path / "out.zip", SimpleNamespace(), [], {})
    with zipfile.ZipFile(io.BytesIO(diag.sanitize_diagnostic_zip(path.read_bytes()))) as archive:
        assert json.loads(archive.read("meta.json"))["runtime"] == runtime


def test_runtime_filter_removes_personal_information():
    runtime = diag._sanitize_runtime({"architecture": "private-machine", "windows_version": "private-host",
        "hostname": "private-host", "engines": {"xray": "C:/private-user/xray.exe", "sing-box": "1.12.0", "path": "private"}})
    assert runtime == {"engines": {"xray": "unknown", "sing-box": "1.12.0"}}


def test_collect_runtime_uses_configured_engines_without_returning_paths(monkeypatch):
    from xray_fluent.engines import xray, singbox
    calls = []
    monkeypatch.setattr(xray, "get_xray_version", lambda path: (calls.append(path), "Xray 26.2.6 (Xray, Penetrates Everything.)")[1])
    monkeypatch.setattr(singbox, "get_singbox_version", lambda path: (calls.append(path), "1.12.0-beta.30")[1])
    state = SimpleNamespace(settings=SimpleNamespace(xray_path="private-xray", singbox_path="private-singbox"))
    result = diag.collect_runtime_context(state)
    assert calls == ["private-xray", "private-singbox"]
    assert result["engines"] == {"xray": "26.2.6", "sing-box": "1.12.0-beta.30"}
    assert "private" not in json.dumps(result)
