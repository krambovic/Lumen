"""System tray icon for the QML frontend (background-run support)"""
from __future__ import annotations

from PyQt6.QtCore import QObject
from PyQt6.QtGui import QAction, QIcon
from PyQt6.QtWidgets import QMenu, QSystemTrayIcon

from ..constants import APP_ICON_PATH, APP_NAME
from .toast import show_toast


class QmlTray(QObject):
    """Owns the QSystemTrayIcon and bridges it to the window + AppBridge."""

    def __init__(self, app, window, bridge) -> None:
        super().__init__(window)
        self._app = app
        self._window = window
        self._bridge = bridge
        self._notified = False

        self._tray = QSystemTrayIcon(self)
        if APP_ICON_PATH.is_file():
            self._tray.setIcon(QIcon(str(APP_ICON_PATH)))
        else:
            self._tray.setIcon(app.windowIcon())
        self._tray.setToolTip(APP_NAME)

        menu = QMenu()
        self._action_show = QAction("Скрыть", menu)
        self._action_show.triggered.connect(self._toggle_window)
        self._action_connect = QAction("Подключить", menu)
        self._action_connect.triggered.connect(self._toggle_connection)
        self._action_next = QAction("Следующий сервер", menu)
        self._action_next.triggered.connect(self._switch_next)
        self._action_admin = QAction("Перезапустить от администратора", menu)
        self._action_admin.triggered.connect(self._restart_admin)
        self._action_quit = QAction("Выход", menu)
        self._action_quit.triggered.connect(self._quit)

        menu.addAction(self._action_show)
        menu.addAction(self._action_connect)
        menu.addAction(self._action_next)
        menu.addSeparator()
        menu.addAction(self._action_admin)
        menu.addSeparator()
        menu.addAction(self._action_quit)
        self._menu = menu
        self._tray.setContextMenu(menu)

        self._tray.activated.connect(self._on_activated)
        menu.aboutToShow.connect(self._refresh_actions)
        try:
            self._bridge.connectedChanged.connect(self._refresh_actions)
        except Exception:
            pass
        try:
            self._bridge.trayMessageRequested.connect(self.notify_hidden)
        except Exception:
            pass

        self._refresh_actions()
        self._tray.show()

    # ── window visibility ──────────────────────────────────
    def _window_visible(self) -> bool:
        try:
            return bool(self._window.isVisible())
        except Exception:
            return True

    def _show_window(self) -> None:
        try:
            self._window.show()
            self._window.raise_()
            self._window.requestActivate()
        except Exception:
            pass
        self._refresh_actions()

    def _hide_window(self) -> None:
        try:
            self._window.hide()
        except Exception:
            pass
        self._refresh_actions()

    def _toggle_window(self) -> None:
        if self._window_visible():
            self._hide_window()
        else:
            self._show_window()

    def _on_activated(self, reason) -> None:
        if reason in (
            QSystemTrayIcon.ActivationReason.Trigger,
            QSystemTrayIcon.ActivationReason.DoubleClick,
        ):
            self._toggle_window()

    # ── menu actions ────────────────────────────────────
    def _toggle_connection(self) -> None:
        try:
            self._bridge.toggleConnection()
        except Exception:
            pass

    def _switch_next(self) -> None:
        try:
            self._bridge.switchNext()
        except Exception:
            pass

    def _restart_admin(self) -> None:
        try:
            self._bridge._on_admin_relaunch()
        except Exception:
            pass

    def _quit(self) -> None:
        try:
            self._bridge.prepareQuit()
        except Exception:
            pass
        try:
            self._tray.hide()
        except Exception:
            pass
        self._app.quit()

    # ── state ─────────────────────────────────────────
    def _connected(self) -> bool:
        try:
            return bool(self._bridge.controller.connected)
        except Exception:
            return False

    def _refresh_actions(self) -> None:
        try:
            self._action_show.setText(
                "Показать" if not self._window_visible() else "Скрыть"
            )
        except Exception:
            pass
        try:
            self._action_connect.setText(
                "Отключить" if self._connected() else "Подключить"
            )
        except Exception:
            pass

    def notify_hidden(self) -> None:
        if self._notified:
            return
        self._notified = True
        if show_toast(APP_NAME, "Приложение свёрнуто в системный трей"):
            return
        try:
            self._tray.showMessage(
                APP_NAME,
                "Приложение свёрнуто в системный трей",
                QSystemTrayIcon.MessageIcon.Information,
                2000,
            )
        except Exception:
            pass
