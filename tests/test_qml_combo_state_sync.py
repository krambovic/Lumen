from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import textwrap


ROOT = Path(__file__).parents[1]
QML_DIR = ROOT / "xray_fluent" / "qml_app" / "qml"


def test_fluent_combo_reapplies_bound_index_after_model_population() -> None:
    script = textwrap.dedent(
        f"""
        import sys
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
        Lumen.FluentCombo {{
            boundIndex: 1
            model: ["direct", "proxy"]
        }}
        ''',
            QUrl(),
        )
        if component.isError():
            raise RuntimeError("\\n".join(error.toString() for error in component.errors()))
        combo = component.create()
        if combo is None:
            raise RuntimeError("\\n".join(error.toString() for error in component.errors()))
        for _ in range(5):
            app.processEvents()
        assert combo.property("count") == 2
        assert combo.property("currentIndex") == 1
        """
    )
    env = os.environ.copy()
    env.setdefault("QT_QPA_PLATFORM", "offscreen")
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


def test_dns_and_routing_state_use_model_safe_combo_selection() -> None:
    settings_qml = (QML_DIR / "SettingsPage.qml").read_text(encoding="utf-8")
    routing_qml = (QML_DIR / "RoutingPage.qml").read_text(encoding="utf-8")

    assert "boundIndex: Math.max(0, values.indexOf(App.dnsMode))" in settings_qml
    assert "boundIndex: page.idxIn(page.svcActionKeys, modelData.action)" in routing_qml
    assert "boundIndex: page.idxIn(page.procActionKeys, modelData.action)" in routing_qml
    assert routing_qml.count("boundIndex: 1") >= 3
