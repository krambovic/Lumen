from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import textwrap


ROOT = Path(__file__).parents[1]
QML_DIR = ROOT / "xray_fluent" / "qml_app" / "qml"


def test_protocol_aware_node_edit_dialog_compiles() -> None:
    script = textwrap.dedent(
        f"""
        from PyQt6.QtCore import QObject, QUrl, pyqtProperty
        from PyQt6.QtGui import QGuiApplication
        from PyQt6.QtQml import QQmlComponent, QQmlEngine, qmlRegisterSingletonInstance

        class StubApp(QObject):
            @pyqtProperty(str, constant=True)
            def language(self):
                return "ru"

            @pyqtProperty("QVariantMap", constant=True)
            def translations(self):
                return {{}}

        app = QGuiApplication([])
        stub = StubApp()
        qmlRegisterSingletonInstance("App", 1, 0, "App", stub)
        engine = QQmlEngine()
        component = QQmlComponent(engine)
        component.setData(
            b'''import QtQuick
        import "{QML_DIR.as_uri()}" as Lumen
        Lumen.NodeEditDialog {{}}
        ''',
            QUrl(),
        )
        if component.isError():
            raise RuntimeError("\\n".join(error.toString() for error in component.errors()))
        dialog = component.create()
        if dialog is None:
            raise RuntimeError("\\n".join(error.toString() for error in component.errors()))
        """
    )
    env = os.environ.copy()
    env.setdefault("QT_QPA_PLATFORM", "offscreen")
    env.setdefault("QT_QUICK_CONTROLS_STYLE", "Basic")
    result = subprocess.run(
        [sys.executable, "-c", script],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_dialog_uses_dynamic_protocol_schema_instead_of_shared_static_fields() -> None:
    source = (QML_DIR / "NodeEditDialog.qml").read_text(encoding="utf-8")

    assert "model: dlg.protocolFields" in source
    assert "payload[key] = fieldValue(key" in source
    assert "fieldVisible(fieldSpec)" in source
    assert "contentHeight: formColumn.implicitHeight" in source
    assert 'spec.kind === "bool" ? switchEditor' in source
    assert 'spec.kind === "area" ? areaEditor' not in source
    assert "echoMode: TextInput.Normal" in source
    assert "TextInput.Password" not in source
