from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.qml_app import main_qml


class _Window:
    def __init__(self) -> None:
        self.calls: list[object] = []

    def setVisible(self, value: bool) -> None:
        self.calls.append(("setVisible", value))

    def showNormal(self) -> None:
        self.calls.append("showNormal")

    def show(self) -> None:
        self.calls.append("show")

    def raise_(self) -> None:
        self.calls.append("raise")

    def requestActivate(self) -> None:
        self.calls.append("requestActivate")

    def winId(self) -> int:
        return 1234


class _User32:
    def __init__(self) -> None:
        self.calls: list[tuple] = []

    def ShowWindow(self, *args) -> int:
        self.calls.append(("ShowWindow", *args))
        return 1

    def BringWindowToTop(self, *args) -> int:
        self.calls.append(("BringWindowToTop", *args))
        return 1

    def SetForegroundWindow(self, *args) -> int:
        self.calls.append(("SetForegroundWindow", *args))
        return 1


def test_activate_window_restores_tray_window_and_uses_win32_foreground(monkeypatch) -> None:
    window = _Window()
    user32 = _User32()
    monkeypatch.setattr(main_qml.sys, "platform", "win32")
    monkeypatch.setattr(main_qml.ctypes, "windll", SimpleNamespace(user32=user32))

    main_qml._activate_window(window)

    assert ("setVisible", True) in window.calls
    assert "showNormal" in window.calls
    assert "requestActivate" in window.calls
    assert ("ShowWindow", 1234, 9) in user32.calls
    assert ("BringWindowToTop", 1234) in user32.calls
    assert ("SetForegroundWindow", 1234) in user32.calls


def test_unavailable_primary_displays_actionable_message(monkeypatch) -> None:
    calls: list[tuple] = []
    user32 = SimpleNamespace(MessageBoxW=lambda *args: calls.append(args))
    monkeypatch.setattr(main_qml.sys, "platform", "win32")
    monkeypatch.setattr(main_qml.ctypes, "windll", SimpleNamespace(user32=user32))

    main_qml._show_single_instance_unavailable()

    assert len(calls) == 1
    assert "Диспетчер задач" in calls[0][1]
