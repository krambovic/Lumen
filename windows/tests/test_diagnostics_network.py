import io
import json
import socket
import zipfile
from types import SimpleNamespace

from xray_fluent import diagnostics as diag, windows_network


def test_selected_network_probes_keep_only_status(tmp_path, monkeypatch):
    calls = []
    monkeypatch.setattr(diag, "_probe_tcp", lambda family, address: (calls.append((family, address)), family == socket.AF_INET)[1])
    monkeypatch.setattr(diag, "_system_proxy_status", lambda: (True, True))
    monkeypatch.setattr(windows_network, "query_windows_route_context", lambda: SimpleNamespace(tun_active=True))
    monkeypatch.setattr(diag.sys, "platform", "win32")
    monkeypatch.setattr(diag.socket, "has_ipv6", True)
    monkeypatch.setattr(diag, "collect_runtime_context", lambda state: {})
    path = diag.export_diagnostics(tmp_path / "out.zip", SimpleNamespace(), [], {"network": True})
    with zipfile.ZipFile(io.BytesIO(diag.sanitize_diagnostic_zip(path.read_bytes()))) as archive:
        network = json.loads(archive.read("meta.json"))["network"]
    assert len(calls) == 2
    assert network["probe_performed"] is True
    assert network["ipv4_internet"] is True
    assert network["ipv6_internet"] is False
    assert network["tun_active"] is True
    assert network["system_proxy_enabled"] is True
    assert network["system_pac_present"] is True
    assert network["probe_target"] == "cloudflare_tcp_443"
    assert not {"system_dns", "connected_adapters", "proxy_info"} & network.keys()


def test_network_probe_uses_tcp_timeout_and_closes_socket(monkeypatch):
    class Connection:
        def __enter__(self):
            return self
        def __exit__(self, *args):
            calls.append("closed")
        def settimeout(self, timeout):
            calls.append(timeout)
        def connect(self, target):
            calls.append(target)
            raise TimeoutError()
    calls = []
    monkeypatch.setattr(diag.socket, "socket", lambda *args: Connection())
    assert diag._probe_tcp(socket.AF_INET, "1.1.1.1") is False
    assert calls == [1.0, ("1.1.1.1", 443), "closed"]


def test_export_options_require_explicit_network_selection():
    from xray_fluent.qml_app.bridge.app_bridge import AppBridge
    calls = []
    bridge = SimpleNamespace(_start_diagnostics_export=lambda **kwargs: calls.append(kwargs))
    AppBridge.runDiagnosticsExport(bridge, {"network": "true"})
    AppBridge.runDiagnosticsExport(bridge, {"network": True})
    assert calls[0]["include"]["network"] is False
    assert calls[1]["include"]["network"] is True
