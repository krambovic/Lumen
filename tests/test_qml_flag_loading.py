from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).parents[1]
QML_DIR = ROOT / "xray_fluent" / "qml_app" / "qml"


def test_server_flags_keep_a_fallback_visible_until_svg_is_ready() -> None:
    nodes_qml = (QML_DIR / "NodesPage.qml").read_text(encoding="utf-8")

    assert "flagImg.status === Image.Ready" in nodes_qml
    assert "visible: flagBox.imageReady" in nodes_qml
    assert "visible: flagBox.showEmojiFallback" in nodes_qml
    assert "visible: flagBox.hasShapeFallback && nodeRow.flagOrient" in nodes_qml


def test_dashboard_flag_keeps_emoji_visible_until_svg_is_ready() -> None:
    dashboard_qml = (QML_DIR / "DashboardPage.qml").read_text(encoding="utf-8")

    assert "readonly property bool imageReady: hasSource && flagImg.status === Image.Ready" in dashboard_qml
    assert "visible: flagBox.imageReady" in dashboard_qml
    assert "visible: !flagBox.imageReady && flagBox.hasEmoji" in dashboard_qml
