from __future__ import annotations

from types import SimpleNamespace

import pytest

from PyQt6.QtCore import QUrl

from xray_fluent.models import Node
from xray_fluent.qml_app.bridge.app_bridge import AppBridge


class _Signal:
    def __init__(self) -> None:
        self.calls: list[tuple] = []

    def emit(self, *args) -> None:
        self.calls.append(args)


def _node(node_id: str) -> Node:
    return Node(
        id=node_id,
        name=node_id,
        scheme="vless",
        server=f"{node_id}.example.com",
        port=443,
        link=f"vless://{node_id}",
        outbound={"protocol": "vless"},
    )


def test_dropped_config_path_accepts_local_yaml_url(tmp_path) -> None:
    path = tmp_path / "warp.yaml"
    path.write_text("proxies: []\n", encoding="utf-8")

    assert AppBridge._dropped_config_path(QUrl.fromLocalFile(str(path))) == path

    unsupported = tmp_path / "notes.md"
    unsupported.write_text("text", encoding="utf-8")
    assert AppBridge._dropped_config_path(QUrl.fromLocalFile(str(unsupported))) is None

    openvpn = tmp_path / "provider.ovpn"
    openvpn.write_text("client\nremote vpn.example.com 1194\n", encoding="utf-8")
    assert AppBridge._dropped_config_path(QUrl.fromLocalFile(str(openvpn))) == openvpn


def test_dropped_file_is_highlighted_without_changing_active_node(tmp_path) -> None:
    path = tmp_path / "server.txt"
    path.write_text("server payload", encoding="utf-8")
    active = _node("active")
    controller = SimpleNamespace(
        state=SimpleNamespace(nodes=[active], selected_node_id=active.id),
    )

    def import_nodes_from_text(_text: str, *, group=None):
        assert group == "Manual"
        controller.state.nodes.append(_node("imported"))
        return 1, []

    controller.import_nodes_from_text = import_nodes_from_text
    imported_signal = _Signal()
    toast_signal = _Signal()
    bridge = SimpleNamespace(
        _filter_group="Manual",
        _dropped_config_path=AppBridge._dropped_config_path,
        controller=controller,
        nodeImported=imported_signal,
        toast=toast_signal,
    )

    AppBridge.importNodeFiles(bridge, [QUrl.fromLocalFile(str(path))])

    assert controller.state.selected_node_id == "active"
    assert imported_signal.calls == [("imported",)]
    assert toast_signal.calls[0][0] == "success"


def test_clipboard_file_urls_are_imported_as_config_files(monkeypatch, tmp_path) -> None:
    first = tmp_path / "awg3-first.conf"
    second = tmp_path / "awg3-second.conf"
    first.write_text("[Interface]\n", encoding="utf-8")
    second.write_text("[Interface]\n", encoding="utf-8")
    urls = [QUrl.fromLocalFile(str(first)), QUrl.fromLocalFile(str(second))]

    class _MimeData:
        def hasUrls(self) -> bool:
            return True

        def urls(self) -> list[QUrl]:
            return urls

    class _Clipboard:
        def mimeData(self) -> _MimeData:
            return _MimeData()

        def text(self) -> str:
            return "\n".join(url.toString() for url in urls)

    import xray_fluent.qml_app.bridge.app_bridge as app_bridge_module

    monkeypatch.setattr(
        app_bridge_module,
        "QGuiApplication",
        SimpleNamespace(clipboard=lambda: _Clipboard()),
    )
    imported: list[list[QUrl]] = []
    bridge = SimpleNamespace(
        _dropped_config_path=AppBridge._dropped_config_path,
        importNodeFiles=lambda values: imported.append(list(values)),
    )

    AppBridge.importClipboard(bridge)

    assert imported == [urls]


@pytest.mark.parametrize("text", [
    "https://sub.example/token",
    "https://sub.example:8443/token",
    "http://sub.example/?token=abc",
    "https://sub.example",
    "happ://crypt/example",
    "incy://crypt1/example",
])
def test_clipboard_subscription_uses_background_import(monkeypatch, text):
    import xray_fluent.qml_app.bridge.app_bridge as module

    clipboard = SimpleNamespace(mimeData=lambda: None, text=lambda: text)
    monkeypatch.setattr(module, "QGuiApplication", SimpleNamespace(clipboard=lambda: clipboard))
    imported = []
    bridge = SimpleNamespace(importSubscription=imported.append)
    AppBridge.importClipboard(bridge)
    assert imported == [text]


def test_clipboard_subscription_batch_is_deduplicated(monkeypatch):
    import xray_fluent.qml_app.bridge.app_bridge as module

    clipboard = SimpleNamespace(mimeData=lambda: None, text=lambda: "https://sub.example/a\nhttps://sub.example/b\nhttps://sub.example/a")
    monkeypatch.setattr(module, "QGuiApplication", SimpleNamespace(clipboard=lambda: clipboard))
    imported = []
    AppBridge.importClipboard(SimpleNamespace(importSubscriptions=imported.append))
    assert imported == ["https://sub.example/a\nhttps://sub.example/b"]


@pytest.mark.parametrize("text", ["http://user:pass@proxy.example:8080", "https://proxy.example:443", "vless://uuid@node.example:443"])
def test_clipboard_proxy_links_remain_node_imports(monkeypatch, text):
    import xray_fluent.qml_app.bridge.app_bridge as module

    clipboard = SimpleNamespace(mimeData=lambda: None, text=lambda: text)
    monkeypatch.setattr(module, "QGuiApplication", SimpleNamespace(clipboard=lambda: clipboard))
    imported = []
    bridge = SimpleNamespace(
        _filter_group="Manual", controller=SimpleNamespace(
            state=SimpleNamespace(nodes=[]),
            import_nodes_from_text=lambda value, **kwargs: (imported.append(value) or 0, []),
        ), toast=_Signal(),
    )
    AppBridge.importClipboard(bridge)
    assert imported == [text]
