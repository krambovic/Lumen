from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.qml_app import main_qml


class _NativeCall:
    def __init__(self, result: int = 1, effect=None) -> None:
        self.result = result
        self.effect = effect
        self.calls: list[tuple[object, ...]] = []
        self.argtypes = None
        self.restype = None

    def __call__(self, *args):
        self.calls.append(args)
        if self.effect is not None:
            self.effect(*args)
        return self.result


class _Window:
    def __init__(self, hwnd: int = 0x123456789) -> None:
        self._hwnd = hwnd

    def winId(self) -> int:
        return self._hwnd


def _install_windows_api(monkeypatch, *, build: int):
    set_dwm_attribute = _NativeCall(result=0)
    extend_frame = _NativeCall(result=0)
    set_composition = _NativeCall(result=1)
    set_window_theme = _NativeCall(result=0)
    redraw_window = _NativeCall(result=1)
    user32 = SimpleNamespace(
        SetWindowCompositionAttribute=set_composition,
        RedrawWindow=redraw_window,
    )
    api = SimpleNamespace(
        dwmapi=SimpleNamespace(
            DwmSetWindowAttribute=set_dwm_attribute,
            DwmExtendFrameIntoClientArea=extend_frame,
        ),
        user32=user32,
        uxtheme=SimpleNamespace(SetWindowTheme=set_window_theme),
    )
    monkeypatch.setattr(main_qml.sys, "platform", "win32")
    monkeypatch.setattr(main_qml.sys, "getwindowsversion", lambda: SimpleNamespace(build=build))
    monkeypatch.setattr(main_qml.ctypes, "windll", api)
    return set_dwm_attribute, extend_frame, set_composition, set_window_theme, redraw_window


def test_windows_10_frame_uses_legacy_dark_mode_fallback(monkeypatch) -> None:
    calls = _install_windows_api(monkeypatch, build=19045)
    set_dwm_attribute, extend_frame, set_composition, set_window_theme, redraw_window = calls

    main_qml._apply_mica(_Window(), True)

    assert [call[1] for call in set_dwm_attribute.calls] == [20]
    assert not extend_frame.calls
    assert set_composition.calls
    assert set_composition.calls[0][1]._obj.attribute == 26
    assert set_window_theme.calls[0][1] == "DarkMode_Explorer"
    assert redraw_window.calls
    assert redraw_window.calls[0][0].value == 0x123456789


def test_windows_11_keeps_native_border_and_backdrop_contract(monkeypatch) -> None:
    calls = _install_windows_api(monkeypatch, build=22621)
    set_dwm_attribute, extend_frame, set_composition, set_window_theme, redraw_window = calls

    main_qml._apply_mica(_Window(), True, "mica")

    assert [call[1] for call in set_dwm_attribute.calls] == [20, 34, 35, 36, 38]
    assert extend_frame.calls
    assert not set_composition.calls
    assert not set_window_theme.calls
    assert redraw_window.calls


def test_windows_vulkan_backend_falls_back_to_qwindowkit_safe_opengl(monkeypatch) -> None:
    monkeypatch.setattr(main_qml.sys, "platform", "win32")
    monkeypatch.setenv("QSG_RHI_BACKEND", "Vulkan")

    main_qml._enable_gpu_friendly_defaults()

    assert main_qml.os.environ["QSG_RHI_BACKEND"] == "opengl"


def test_windows_supported_backend_is_preserved(monkeypatch) -> None:
    monkeypatch.setattr(main_qml.sys, "platform", "win32")
    monkeypatch.setenv("QSG_RHI_BACKEND", "d3d11")

    main_qml._enable_gpu_friendly_defaults()

    assert main_qml.os.environ["QSG_RHI_BACKEND"] == "d3d11"


def test_frame_refresh_preserves_qwindowkit_windows10_margins(monkeypatch) -> None:
    calls = _install_windows_api(monkeypatch, build=19045)
    user32 = main_qml.ctypes.windll.user32
    for name in ("GetWindowRect", "ClientToScreen", "GetWindowLongW",
                 "SetWindowLongW", "SetWindowPos"):
        setattr(user32, name, _NativeCall(result=0))

    main_qml._refresh_custom_frame(_Window())

    assert user32.SetWindowPos.calls  # Reached the end, not a swallowed exception.
    assert not calls[1].calls  # QWindowKit's active/inactive margins survive.


def test_windows10_light_theme_reverses_dark_composition(monkeypatch) -> None:
    calls = _install_windows_api(monkeypatch, build=19045)
    values = []
    def capture(_hwnd, data):
        values.append(main_qml.ctypes.cast(
            data._obj.data, main_qml.ctypes.POINTER(main_qml.ctypes.c_long)
        ).contents.value)
    calls[2].effect = capture
    main_qml._apply_mica(_Window(), True)
    main_qml._apply_mica(_Window(), False)
    assert values == [1, 0]
    assert calls[3].calls[-1][1] == "Explorer"
