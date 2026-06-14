"""Entry point for the Qt Quick (QML) frontend"""
from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path


def _enable_gpu_friendly_defaults() -> None:
    os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Universal")
    os.environ.setdefault("QSG_RENDER_LOOP", "threaded")
    os.environ.setdefault("QSG_RHI_BACKEND", "opengl")


def _install_message_filter() -> None:
    """Silence the benign ``qt.qpa.mime: Retrying to obtain clipboard.`` line.

    What it is: a Qt-on-Windows *warning* (not an error) emitted by the Windows
    clipboard backend (``qwindowsmime``). When Qt tries to READ the system
    clipboard - here that happens on every paste/«import» (Ctrl+V →
    ``AppBridge.importClipboard`` calls ``QGuiApplication.clipboard().text()``)
    and also when Qt automatically probes paste-availability - it must call the
    Win32 ``OpenClipboard()``. Windows only lets ONE process own the clipboard
    at a time, so if something else is momentarily holding it (clipboard
    managers / Win+V history, Punto Switcher, Ditto, a browser copying, etc.)
    the open is refused and Qt retries a few times, logging this line each
    attempt. It is completely harmless and its timing is dictated by whatever
    third-party clipboard software happens to be touching the clipboard, which
    is why it appears "at random" on some launches and never on others.

    Rather than leave the noise in the console we install a Qt message handler
    that drops just this one category/message and forwards everything else
    unchanged to stderr (so real warnings/errors are still visible).
    """
    try:
        from PyQt6.QtCore import (
            QtMsgType,
            qInstallMessageHandler,
        )
    except Exception:
        return

    def _handler(msg_type, context, message) -> None:
        text = message or ""
        if "Retrying to obtain clipboard" in text or (
            "qt.qpa.mime" in text and "clipboard" in text
        ):
            return
        try:
            sys.stderr.write(text + "\n")
        except Exception:
            pass

    qInstallMessageHandler(_handler)


def _theme_name(bridge) -> str:
    try:
        return str(bridge.themeName)
    except Exception:
        return "system"


def _resolve_dark(app, theme_name: str) -> bool:
    """Mirror Main.qml resolveDark(): explicit light/dark, else follow system."""
    if theme_name == "dark":
        return True
    if theme_name == "light":
        return False
    try:
        from PyQt6.QtCore import Qt
        return app.styleHints().colorScheme() == Qt.ColorScheme.Dark
    except Exception:
        return False


def _apply_mica(window, dark: bool) -> None:
    """Enable the Windows 11 Mica backdrop + matching title-bar colour"""
    if sys.platform != "win32":
        return
    try:
        import ctypes

        hwnd = int(window.winId())
        dwm = ctypes.windll.dwmapi

        DWMWA_USE_IMMERSIVE_DARK_MODE = 20
        DWMWA_USE_IMMERSIVE_DARK_MODE_OLD = 19  # builds < 18985
        DWMWA_MICA_EFFECT = 1029                # Win11 21H2 (build 22000)
        DWMWA_SYSTEMBACKDROP_TYPE = 38          # Win11 22H2+ (build 22621)
        DWMSBT_MAINWINDOW = 2                   # Mica

        # 1) Dark / light title bar (try both the modern and legacy attribute)
        dark_flag = ctypes.c_int(1 if dark else 0)
        for attr in (DWMWA_USE_IMMERSIVE_DARK_MODE,
                     DWMWA_USE_IMMERSIVE_DARK_MODE_OLD):
            dwm.DwmSetWindowAttribute(
                hwnd, attr,
                ctypes.byref(dark_flag), ctypes.sizeof(dark_flag),
            )

        # 2) Extend the frame into the ENTIRE client area
        class _Margins(ctypes.Structure):
            _fields_ = [
                ("cxLeftWidth", ctypes.c_int),
                ("cxRightWidth", ctypes.c_int),
                ("cyTopHeight", ctypes.c_int),
                ("cyBottomHeight", ctypes.c_int),
            ]

        margins = _Margins(-1, -1, -1, -1)
        dwm.DwmExtendFrameIntoClientArea(hwnd, ctypes.byref(margins))

        # 3) Enable Mica using the API that matches the running Windows build
        build = sys.getwindowsversion().build
        if build >= 22621:
            backdrop = ctypes.c_int(DWMSBT_MAINWINDOW)
            dwm.DwmSetWindowAttribute(
                hwnd, DWMWA_SYSTEMBACKDROP_TYPE,
                ctypes.byref(backdrop), ctypes.sizeof(backdrop),
            )
        elif build >= 22000:
            enable = ctypes.c_int(1)
            dwm.DwmSetWindowAttribute(
                hwnd, DWMWA_MICA_EFFECT,
                ctypes.byref(enable), ctypes.sizeof(enable),
            )
        else:
            print("Mica backdrop unavailable: requires Windows 11",
                  file=sys.stderr)
    except Exception as exc: 
        print(f"Mica backdrop unavailable: {exc}", file=sys.stderr)


def _set_app_user_model_id() -> None:
    """Give Windows an explicit AppUserModelID.

    Without it the taskbar button is associated with the generic host process
    and shows the default Windows icon. Setting an explicit ID lets Windows
    use our own icon and group the taskbar button correctly. Must be called
    before the first window is created.
    """
    if sys.platform != "win32":
        return
    try:
        import ctypes
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
            "Lumen.LumenKVN"
        )
    except Exception:
        pass


def _cleanup_legacy_root_program_install() -> None:
    if sys.platform != "win32" or not getattr(sys, "frozen", False):
        return
    try:
        current_dir = Path(sys.executable).resolve().parent
        old_dir = Path("C:/Program")
        if current_dir == old_dir or not old_dir.is_dir():
            return
        if current_dir.name.lower() != "lumen kvn":
            return
        program_files = {
            Path(os.environ.get("ProgramW6432") or ""),
            Path(os.environ.get("ProgramFiles") or ""),
        }
        if not any(
            str(current_dir).lower().startswith(str(root).lower().rstrip("\\/") + "\\")
            for root in program_files
            if str(root)
        ):
            return
        if not (old_dir / "LumenKVN.exe").is_file():
            return
        script = (
            "$old='C:\\Program';"
            "Start-Sleep -Seconds 3;"
            "if ((Test-Path -LiteralPath (Join-Path $old 'LumenKVN.exe')) -and "
            "((Split-Path -Leaf $old) -ieq 'Program')) {"
            "Remove-Item -LiteralPath $old -Recurse -Force -ErrorAction SilentlyContinue"
            "}"
        )
        subprocess.Popen(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script],
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
    except Exception:
        pass


def _create_single_instance(app):
    """Return (server, is_primary). A second launch just activates the first window."""
    try:
        from PyQt6.QtCore import QIODevice
        from PyQt6.QtNetwork import QLocalServer, QLocalSocket
    except Exception:
        return None, True

    server_name = "LumenKVN.SingleInstance"
    relaunching = "--relaunch-as-admin" in sys.argv[1:]
    deadline = time.monotonic() + (5.0 if relaunching else 0.0)
    while True:
        socket = QLocalSocket()
        socket.connectToServer(server_name, QIODevice.OpenModeFlag.WriteOnly)
        if socket.waitForConnected(250):
            if relaunching and time.monotonic() < deadline:
                socket.abort()
                time.sleep(0.25)
                continue
            try:
                socket.write(b"activate")
                socket.flush()
                socket.waitForBytesWritten(250)
            except Exception:
                pass
            socket.disconnectFromServer()
            return None, False
        break

    try:
        QLocalServer.removeServer(server_name)
    except Exception:
        pass
    server = QLocalServer(app)
    if not server.listen(server_name):
        return None, True
    return server, True


def _activate_window(window) -> None:
    try:
        window.show()
        window.raise_()
        window.requestActivate()
    except Exception:
        pass


def main(argv: list[str] | None = None) -> int:
    _set_app_user_model_id()
    _cleanup_legacy_root_program_install()
    _enable_gpu_friendly_defaults()
    _install_message_filter()

    from PyQt6.QtCore import QUrl
    from PyQt6.QtGui import QIcon
    from PyQt6.QtQml import QQmlApplicationEngine, qmlRegisterSingletonInstance
    from PyQt6.QtWidgets import QApplication

    from ..constants import APP_ICON_PATH, APP_NAME
    from .bridge import AppBridge

    QApplication.setApplicationName(APP_NAME)

    from PyQt6.QtGui import QSurfaceFormat
    _fmt = QSurfaceFormat.defaultFormat()
    _fmt.setAlphaBufferSize(8)
    QSurfaceFormat.setDefaultFormat(_fmt)

    from PyQt6.QtQuick import QQuickWindow
    QQuickWindow.setDefaultAlphaBuffer(True)

    app = QApplication(argv if argv is not None else sys.argv)
    single_server, is_primary = _create_single_instance(app)
    if not is_primary:
        return 0

    if APP_ICON_PATH.is_file():
        app.setWindowIcon(QIcon(str(APP_ICON_PATH)))

    bridge = AppBridge()
    bridge._single_instance_server = single_server
    bridge.load()

    qmlRegisterSingletonInstance("App", 1, 0, "App", bridge)

    engine = QQmlApplicationEngine()
    qml_dir = Path(__file__).resolve().parent / "qml"
    engine.addImportPath(str(qml_dir))
    engine.load(QUrl.fromLocalFile(str(qml_dir / "Main.qml")))

    if not engine.rootObjects():
        print("Failed to load Main.qml", file=sys.stderr)
        return 1

    window = engine.rootObjects()[0]

    if single_server is not None:
        def _on_second_instance() -> None:
            while single_server.hasPendingConnections():
                conn = single_server.nextPendingConnection()
                if conn is not None:
                    conn.disconnectFromServer()
            _activate_window(window)

        single_server.newConnection.connect(_on_second_instance)

    # Apply the icon directly to the top-level QQuickWindow. Setting it only on
    # the QApplication doesn't reliably reach the first-shown QML window's
    # taskbar button on Windows — the icon would otherwise stay the generic
    # default until the window was hidden to tray and re-shown. Setting it on
    # the QWindow itself makes the correct taskbar icon appear on first launch.
    if APP_ICON_PATH.is_file():
        try:
            window.setIcon(QIcon(str(APP_ICON_PATH)))
        except Exception:
            pass

    def _refresh_backdrop() -> None:
        _apply_mica(window, _resolve_dark(app, _theme_name(bridge)))

    from PyQt6.QtCore import QTimer

    _recomposited = {"done": False}

    def _force_recomposite() -> None:
        try:
            w = window.width()
            h = window.height()
            window.setWidth(w + 1)
            window.setHeight(h + 1)

            def _revert() -> None:
                try:
                    window.setWidth(w)
                    window.setHeight(h)
                except Exception:
                    pass
                _refresh_backdrop()

            QTimer.singleShot(50, _revert)
        except Exception:
            pass
        _refresh_backdrop()

    def _on_first_frame() -> None:
        if _recomposited["done"]:
            return
        _recomposited["done"] = True
        QTimer.singleShot(0, _force_recomposite)

    _refresh_backdrop()
    try:
        window.frameSwapped.connect(_on_first_frame)
    except Exception:
        # Fall back to deferred timers if the signal is unavailable.
        QTimer.singleShot(0, _force_recomposite)
        QTimer.singleShot(200, _force_recomposite)
    try:
        bridge.settingsChanged.connect(_refresh_backdrop)
    except Exception:
        pass

    from PyQt6.QtWidgets import QSystemTrayIcon
    from .tray import QmlTray

    tray_available = QSystemTrayIcon.isSystemTrayAvailable()
    bridge.set_tray_available(tray_available)
    app.setQuitOnLastWindowClosed(not tray_available)
    tray = QmlTray(app, window, bridge) if tray_available else None
    app.aboutToQuit.connect(bridge.shutdown)

    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
