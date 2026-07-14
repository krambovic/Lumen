from pathlib import Path


QML_DIR = Path(__file__).parents[1] / "xray_fluent" / "qml_app" / "qml"


def test_shared_fluent_dialog_defines_common_surface_and_actions() -> None:
    source = (QML_DIR / "FluentDialog.qml").read_text(encoding="utf-8")

    assert "color: Theme.flyout" in source
    assert "border.color: Theme.flyoutBorder" in source
    assert "Overlay.modal: Rectangle" in source
    assert 'kind: "ghost"' in source
    assert 'kind: "accent"' in source


def test_pages_do_not_fall_back_to_native_dialog_buttons() -> None:
    qml_sources = {
        path.name: path.read_text(encoding="utf-8")
        for path in QML_DIR.glob("*.qml")
    }

    assert all("standardButtons:" not in source for source in qml_sources.values())
    assert "component FluentDialog:" not in qml_sources["RoutingPage.qml"]
    assert qml_sources["RoutingPage.qml"].count("FluentDialog {") >= 5
    assert "FluentDialog {\n        id: resetSettingsDialog" in qml_sources["SettingsPage.qml"]
    assert "FluentDialog {\n        id: qrDialog" in qml_sources["NodesPage.qml"]
