"""Entry point for the Qt Quick (QML) frontend"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
import ctypes
from pathlib import Path


def _enable_gpu_friendly_defaults() -> None:
    os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Universal")
    os.environ.setdefault("QSG_RENDER_LOOP", "threaded")  # threaded: GPU-sync рендер, плавные анимации; краш трея (0xc000041d) устранён откатом routing submenu в 69f86e4
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
        if (
            "Retrying to obtain clipboard" in text
            or "Unable to obtain clipboard" in text
        ) or (
            "qt.qpa.mime" in text and "clipboard" in text
        ):
            return
        if text.startswith("QWindowsWindow::setGeometry: Unable to set geometry"):
            return
        try:
            import logging

            logger = logging.getLogger("xray_fluent.qt")
            if msg_type in (QtMsgType.QtCriticalMsg, QtMsgType.QtFatalMsg):
                logger.error(text)
            elif msg_type == QtMsgType.QtWarningMsg:
                logger.warning(text)
            else:
                logger.debug(text)
        except Exception:
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


def _apply_mica(window, dark: bool, backdrop_name: str = "mica") -> None:
    """Enable the selected Windows 11 backdrop + dark/light title-bar colour."""
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
        DWMSBT_NONE = 1                         # solid (no backdrop)
        DWMSBT_MAINWINDOW = 2                   # Mica
        DWMSBT_ACRYLIC = 3                      # Acrylic (transient)
        DWMWA_BORDER_COLOR = 34
        DWMWA_CAPTION_COLOR = 35
        DWMWA_TEXT_COLOR = 36
        DWMWA_COLOR_NONE = 0xFFFFFFFE

        backdrop_name = (backdrop_name or "mica").strip().lower()
        if backdrop_name == "acrylic":
            backdrop_name = "mica"
        if backdrop_name not in {"mica", "solid"}:
            backdrop_name = "mica"

        dark_flag = ctypes.c_int(1 if dark else 0)
        for attr in (DWMWA_USE_IMMERSIVE_DARK_MODE,
                     DWMWA_USE_IMMERSIVE_DARK_MODE_OLD):
            dwm.DwmSetWindowAttribute(
                hwnd, attr,
                ctypes.byref(dark_flag), ctypes.sizeof(dark_flag),
            )

        build = sys.getwindowsversion().build
        if build >= 22000:
            class _Margins(ctypes.Structure):
                _fields_ = [
                    ("cxLeftWidth", ctypes.c_int),
                    ("cxRightWidth", ctypes.c_int),
                    ("cyTopHeight", ctypes.c_int),
                    ("cyBottomHeight", ctypes.c_int),
                ]

            margins = _Margins(0, 0, 0, 0)
            dwm.DwmExtendFrameIntoClientArea(hwnd, ctypes.byref(margins))

            # Prevent Windows from drawing the user's accent colour as a 1px
            # active-window line above our custom title bar.
            border_color = ctypes.c_int(DWMWA_COLOR_NONE)
            dwm.DwmSetWindowAttribute(
                hwnd, DWMWA_BORDER_COLOR,
                ctypes.byref(border_color), ctypes.sizeof(border_color),
            )
            caption_color = ctypes.c_int(0x002C2631 if dark else 0x00F3F3F3)
            text_color = ctypes.c_int(0x00F3F3F3 if dark else 0x00202020)
            for attr, color in (
                (DWMWA_CAPTION_COLOR, caption_color),
                (DWMWA_TEXT_COLOR, text_color),
            ):
                dwm.DwmSetWindowAttribute(
                    hwnd, attr,
                    ctypes.byref(color), ctypes.sizeof(color),
                )

            if build >= 22621:
                backdrop_map = {
                    "mica": DWMSBT_MAINWINDOW,
                    "acrylic": DWMSBT_ACRYLIC,
                    "solid": DWMSBT_NONE,
                }
                backdrop = ctypes.c_int(backdrop_map[backdrop_name])
                dwm.DwmSetWindowAttribute(
                    hwnd, DWMWA_SYSTEMBACKDROP_TYPE,
                    ctypes.byref(backdrop), ctypes.sizeof(backdrop),
                )
            else:
                # Older Win11 only exposes Mica on/off.
                enable = ctypes.c_int(1 if backdrop_name != "solid" else 0)
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
    _register_aumid_toast_identity()
    _register_toast_protocol()


def _register_aumid_toast_identity() -> None:
    """Register a friendly DisplayName + icon for our AppUserModelID"""
    if sys.platform != "win32":
        return
    try:
        import winreg
        from ..constants import APP_ICON_PATH, APP_NAME
        key_path = r"Software\Classes\AppUserModelId\Lumen.LumenKVN"
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, key_path) as key:
            winreg.SetValueEx(key, "DisplayName", 0, winreg.REG_SZ, APP_NAME)
            if APP_ICON_PATH.is_file():
                winreg.SetValueEx(
                    key, "IconUri", 0, winreg.REG_SZ, str(APP_ICON_PATH)
                )
    except Exception:
        pass


def _register_toast_protocol() -> None:
    """Register the lumen-kvn: URL protocol so a toast click reopens the app"""
    if sys.platform != "win32" or not getattr(sys, "frozen", False):
        return
    try:
        import winreg
        exe = Path(sys.executable).resolve()
        with winreg.CreateKey(
            winreg.HKEY_CURRENT_USER, r"Software\Classes\lumen-kvn"
        ) as key:
            winreg.SetValueEx(
                key, None, 0, winreg.REG_SZ, "URL:Lumen KVN Protocol"
            )
            winreg.SetValueEx(key, "URL Protocol", 0, winreg.REG_SZ, "")
        with winreg.CreateKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Classes\lumen-kvn\shell\open\command",
        ) as key:
            winreg.SetValueEx(key, None, 0, winreg.REG_SZ, f'"{exe}" "%1"')
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


def _notify_primary_instance(server_name: str) -> bool:
    try:
        from PyQt6.QtCore import QIODevice
        from PyQt6.QtNetwork import QLocalSocket
    except Exception:
        return False
    socket = QLocalSocket()
    socket.connectToServer(server_name, QIODevice.OpenModeFlag.WriteOnly)
    if not socket.waitForConnected(250):
        socket.abort()
        return False
    try:
        socket.write(b"activate")
        socket.flush()
        socket.waitForBytesWritten(250)
    except Exception:
        pass
    socket.disconnectFromServer()
    return True


def _create_windows_single_instance_mutex() -> tuple[object | None, bool]:
    if sys.platform != "win32":
        return None, True
    try:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateMutexW.argtypes = (ctypes.c_void_p, ctypes.c_bool, ctypes.c_wchar_p)
        kernel32.CreateMutexW.restype = ctypes.c_void_p
        handle = kernel32.CreateMutexW(None, True, "Local\\LumenKVN.SingleInstance")
        if not handle:
            return None, True
        already_exists = ctypes.get_last_error() == 183
        return handle, not already_exists
    except Exception:
        return None, True


def _close_mutex_handle(handle: object | None) -> None:
    if not handle or sys.platform != "win32":
        return
    try:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CloseHandle.argtypes = (ctypes.c_void_p,)
        kernel32.CloseHandle(handle)
    except Exception:
        pass


def _acquire_single_instance_mutex(relaunching: bool) -> tuple[object | None, bool]:
    handle, owns = _create_windows_single_instance_mutex()
    if owns or not relaunching:
        return handle, owns
    deadline = time.monotonic() + 20.0
    while time.monotonic() < deadline:
        _close_mutex_handle(handle)
        time.sleep(0.1)
        handle, owns = _create_windows_single_instance_mutex()
        if owns:
            break
    return handle, owns


def _create_single_instance(app):
    """Return (server, is_primary). A second launch just activates the first window."""
    try:
        from PyQt6.QtCore import QLockFile
        from PyQt6.QtNetwork import QLocalServer
    except Exception:
        return None, True

    server_name = "LumenKVN.SingleInstance"
    relaunching = any(flag in sys.argv[1:] for flag in ("--relaunch-as-admin", "--relaunched"))

    mutex_handle, owns_mutex = _acquire_single_instance_mutex(relaunching)
    if not owns_mutex:
        _close_mutex_handle(mutex_handle)
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            if _notify_primary_instance(server_name):
                break
            time.sleep(0.1)
        return None, False

    lock = QLockFile(str(Path(tempfile.gettempdir()) / "LumenKVN.SingleInstance.lock"))
    lock.setStaleLockTime(30_000)
    deadline = time.monotonic() + (20.0 if relaunching else 0.5)

    while not lock.tryLock(0):
        if relaunching:
            lock.removeStaleLockFile()
        else:
            _notify_primary_instance(server_name)
        if time.monotonic() >= deadline:
            _close_mutex_handle(mutex_handle)
            return None, False
        time.sleep(0.1)

    try:
        QLocalServer.removeServer(server_name)
    except Exception:
        pass
    server = QLocalServer(app)
    if not server.listen(server_name):
        lock.unlock()
        _close_mutex_handle(mutex_handle)
        _notify_primary_instance(server_name)
        return None, False
    server._single_instance_lock = lock
    server._single_instance_mutex_handle = mutex_handle
    return server, True


def _activate_window(window) -> None:
    try:
        window.show()
        window.raise_()
        window.requestActivate()
    except Exception:
        pass


def _install_crash_guards() -> None:
    """Stop PyQt6 from turning a stray slot exception into a native abort.

    On Windows an unhandled Python exception raised inside a Qt slot invoked
    synchronously from the native message loop (for example while the tray
    context menu is shown) makes PyQt6 call abort() inside qwindows.dll, which
    crashes the whole app with STATUS_FATAL_USER_CALLBACK_EXCEPTION (0xc000041d).
    Installing a custom sys.excepthook makes PyQt log the traceback and keep
    running instead of aborting. faulthandler is armed so any genuine native
    fault still leaves a trace on disk.
    """
    import logging

    _log = logging.getLogger("xray_fluent")

    def _hook(exc_type, exc, tb):
        if issubclass(exc_type, KeyboardInterrupt):
            sys.__excepthook__(exc_type, exc, tb)
            return
        try:
            _log.error(
                "Unhandled exception (suppressed to avoid native abort)",
                exc_info=(exc_type, exc, tb),
            )
        except Exception:
            try:
                import traceback

                traceback.print_exception(exc_type, exc, tb)
            except Exception:
                pass

    sys.excepthook = _hook

    def _thread_hook(args) -> None:
        if issubclass(args.exc_type, SystemExit):
            return
        try:
            _log.error(
                "Unhandled exception in thread %r",
                getattr(args.thread, "name", None),
                exc_info=(args.exc_type, args.exc_value, args.exc_traceback),
            )
        except Exception:
            pass

    def _unraisable_hook(args) -> None:
        try:
            _log.error(
                "Unraisable exception in %r",
                args.object,
                exc_info=(args.exc_type, args.exc_value, args.exc_traceback),
            )
        except Exception:
            pass

    import threading

    threading.excepthook = _thread_hook
    sys.unraisablehook = _unraisable_hook

    try:
        import faulthandler

        from ..constants import LOG_DIR

        LOG_DIR.mkdir(parents=True, exist_ok=True)
        fh_path = LOG_DIR / "faulthandler.log"
        prev_path = LOG_DIR / "faulthandler.prev"
        if fh_path.is_file() and fh_path.stat().st_size > 0:
            try:
                if prev_path.exists():
                    prev_path.unlink()
                fh_path.rename(prev_path)
            except Exception:
                pass
        _fh = open(
            fh_path,
            "a",
            buffering=1,
            encoding="utf-8",
            errors="replace",
        )
        faulthandler.enable(file=_fh)
        globals()["_FAULTHANDLER_FILE"] = _fh
    except Exception:
        pass


def _load_bundled_fonts() -> None:
    try:
        from PyQt6.QtGui import QFontDatabase
        if getattr(sys, "frozen", False):
            font_path = Path(sys._MEIPASS) / "xray_fluent" / "qml_app" / "assets" / "fonts" / "SegoeIcons.ttf"
        else:
            font_path = Path(__file__).resolve().parent / "assets" / "fonts" / "SegoeIcons.ttf"
            
        if font_path.is_file():
            font_id = QFontDatabase.addApplicationFont(str(font_path))
            if font_id == -1:
                print(f"Failed to load bundled icon font: QFontDatabase returned -1", file=sys.stderr)
        else:
            print(f"Bundled icon font file not found at: {font_path}", file=sys.stderr)
    except Exception as exc:
        print(f"Failed to load bundled icon font: {exc}", file=sys.stderr)


def _attach_qwindowkit(window) -> None:
    if sys.platform != "win32":
        return
    try:
        import os
        import ctypes
        import PyQt6
        from PyQt6 import sip
        from PyQt6.QtQuick import QQuickItem

        if getattr(sys, "frozen", False):
            qwk_dir = os.path.join(sys._MEIPASS, "qwk")
        else:
            qwk_dir = os.path.join(os.path.dirname(__file__), "vendor", "qwk")
            qt_bin = os.path.join(os.path.dirname(PyQt6.__file__), "Qt6", "bin")
            if os.path.isdir(qt_bin) and hasattr(os, "add_dll_directory"):
                os.add_dll_directory(qt_bin)

        if os.path.isdir(qwk_dir) and hasattr(os, "add_dll_directory"):
            os.add_dll_directory(qwk_dir)

        qwkshim_path = os.path.join(qwk_dir, "qwkshim.dll")
        if not os.path.isfile(qwkshim_path):
            print(f"qwkshim.dll not found at: {qwkshim_path}", file=sys.stderr)
            return

        qwk = ctypes.CDLL(qwkshim_path)
        
        qwk.qwk_attach_by_hwnd.argtypes = [
            ctypes.c_ulonglong,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        ]
        qwk.qwk_attach_by_hwnd.restype = ctypes.c_int

        hwnd = int(window.winId())
        
        rc = qwk.qwk_attach_by_hwnd(
            hwnd,
            b"qwkTitleBar",
            b"qwkMinBtn",
            b"qwkMaxBtn",
            b"qwkCloseBtn",
        )
        if rc != 0:
            print(f"qwk_attach_by_hwnd rc={rc}", file=sys.stderr)
            
        window._qwk_lib = qwk
        window._qwk_attach_ok = (rc == 0)
        
    except Exception as exc:
        print(f"QWindowKit attachment failed: {exc}", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    _install_crash_guards()
    _set_app_user_model_id()
    _cleanup_legacy_root_program_install()
    _enable_gpu_friendly_defaults()
    _install_message_filter()

    from PyQt6.QtCore import QMetaObject, QTimer, QUrl
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
    _load_bundled_fonts()
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
    _attach_qwindowkit(window)
    # QWindowKit updates Qt's non-client-area geometry.  Restore the saved
    # size and install minimum constraints only after that setup is complete;
    # applying them while QML is still loading can leave a DPI-scaled blank
    # strip above the title bar and push the bottom of the scene off-screen.
    if not QMetaObject.invokeMethod(window, "applyStartupWindowGeometry"):
        window.setMinimumWidth(640)
        window.setMinimumHeight(360)
    start_in_tray = "--tray" in (argv if argv is not None else sys.argv)[1:]

    if single_server is not None:
        def _on_second_instance() -> None:
            while single_server.hasPendingConnections():
                conn = single_server.nextPendingConnection()
                if conn is not None:
                    conn.disconnectFromServer()
            _activate_window(window)

        single_server.newConnection.connect(_on_second_instance)

    if APP_ICON_PATH.is_file():
        try:
            window.setIcon(QIcon(str(APP_ICON_PATH)))
        except Exception:
            pass

    def _refresh_backdrop() -> None:
        _apply_mica(window, _resolve_dark(app, _theme_name(bridge)), bridge.uiBackdrop)

    def _schedule_backdrop_refresh(*_args) -> None:
        QTimer.singleShot(0, _refresh_backdrop)
        QTimer.singleShot(150, _refresh_backdrop)

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
        QTimer.singleShot(0, bridge.startDeferred)
        QTimer.singleShot(250, lambda: QMetaObject.invokeMethod(window, "beginBackgroundPageWarmup"))
        QTimer.singleShot(0, _force_recomposite)

    _refresh_backdrop()
    try:
        window.frameSwapped.connect(_on_first_frame)
    except Exception:
        QTimer.singleShot(0, _force_recomposite)
        QTimer.singleShot(200, _force_recomposite)
    QTimer.singleShot(750, bridge.startDeferred)
    QTimer.singleShot(1200, lambda: QMetaObject.invokeMethod(window, "beginBackgroundPageWarmup"))
    try:
        bridge.settingsChanged.connect(_refresh_backdrop)
    except Exception:
        pass
    for signal_name in ("visibilityChanged", "windowStateChanged"):
        signal = getattr(window, signal_name, None)
        if signal is not None:
            try:
                signal.connect(_schedule_backdrop_refresh)
            except Exception:
                pass

    from PyQt6.QtWidgets import QSystemTrayIcon
    from .tray import QmlTray

    tray_available = QSystemTrayIcon.isSystemTrayAvailable()
    bridge.set_tray_available(tray_available)
    app.setQuitOnLastWindowClosed(not tray_available)
    tray = QmlTray(app, window, bridge) if tray_available else None
    window.setVisible(not (start_in_tray and tray_available))
    app.aboutToQuit.connect(bridge.shutdown)

    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
