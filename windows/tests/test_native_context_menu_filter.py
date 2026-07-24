from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_qml_native_context_menu_is_suppressed_without_blocking_mouse_events() -> None:
    script = r"""
from PyQt6.QtCore import QEvent
from PyQt6.QtGui import QGuiApplication
from PyQt6.QtQuick import QQuickItem, QQuickWindow

from xray_fluent.qml_app.native_context_menu_filter import QmlNativeContextMenuFilter

app = QGuiApplication([])
event_filter = QmlNativeContextMenuFilter(app)

for target in (QQuickItem(), QQuickWindow()):
    context_event = QEvent(QEvent.Type.ContextMenu)
    assert event_filter.eventFilter(target, context_event) is True
    assert context_event.isAccepted()
    assert event_filter.eventFilter(
        target, QEvent(QEvent.Type.MouseButtonPress)
    ) is False
"""
    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "offscreen"
    result = subprocess.run(
        [sys.executable, "-c", script],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert result.returncode == 0, result.stderr or result.stdout


def test_main_installs_qml_native_context_menu_filter() -> None:
    source = (ROOT / "xray_fluent" / "qml_app" / "main_qml.py").read_text(
        encoding="utf-8"
    )
    assert "native_context_menu_filter = QmlNativeContextMenuFilter(app)" in source
    assert "app.installEventFilter(native_context_menu_filter)" in source
