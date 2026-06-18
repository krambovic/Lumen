"""System tray icon for the QML frontend (background-run support)"""
from __future__ import annotations

from PyQt6.QtCore import QObject, QTimer
from PyQt6.QtGui import QAction, QIcon
from PyQt6.QtWidgets import QMenu, QSystemTrayIcon

from ..constants import APP_ICON_PATH, APP_NAME
from .toast import show_toast
from ..i18n import tr


class QmlTray(QObject):
    """Owns the QSystemTrayIcon and bridges it to the window + AppBridge."""

    def __init__(self, app, window, bridge) -> None:
        super().__init__(window)
        self._app = app
        self._window = window
        self._bridge = bridge
        self._notified = False
        self._routing_rebuild_scheduled = False

        self._tray = QSystemTrayIcon(self)
        if APP_ICON_PATH.is_file():
            self._tray.setIcon(QIcon(str(APP_ICON_PATH)))
        else:
            self._tray.setIcon(app.windowIcon())
        self._tray.setToolTip(APP_NAME)

        menu = QMenu()
        self._apply_menu_style(menu)
        self._action_show = QAction(tr("Скрыть"), menu)
        self._action_show.triggered.connect(self._toggle_window)
        self._action_connect = QAction(tr("Подключить"), menu)
        self._action_connect.triggered.connect(self._toggle_connection)
        self._action_next = QAction(tr("Следующий сервер"), menu)
        self._action_next.triggered.connect(self._switch_next)
        self._action_admin = QAction(tr("Перезапустить от администратора"), menu)
        self._action_admin.triggered.connect(self._restart_admin)
        self._action_quit = QAction(tr("Выход"), menu)
        self._action_quit.triggered.connect(self._quit)
        self._routing_menu = QMenu(tr("Маршрутизация"), menu)
        self._apply_menu_style(self._routing_menu)

        menu.addAction(self._action_show)
        menu.addAction(self._action_connect)
        menu.addAction(self._action_next)
        menu.addMenu(self._routing_menu)
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
        try:
            self._bridge.routingChanged.connect(self._schedule_routing_rebuild)
        except Exception:
            pass

        self._refresh_actions()
        self._rebuild_routing_menu()
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

    def _apply_default_routing(self, preset_id: str) -> None:
        try:
            self._bridge.applyRoutingPreset(preset_id)
        except Exception:
            pass
        self._schedule_routing_rebuild()

    def _apply_custom_routing(self, preset_id: str) -> None:
        try:
            self._bridge.applyCustomRoutingPreset(preset_id)
        except Exception:
            pass
        self._schedule_routing_rebuild()

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
                tr("Показать") if not self._window_visible() else tr("Скрыть")
            )
        except Exception:
            pass
        try:
            self._action_connect.setText(
                tr("Отключить") if self._connected() else tr("Подключить")
            )
        except Exception:
            pass
        try:
            self._action_next.setText(tr("Следующий сервер"))
            self._action_admin.setText(tr("Перезапустить от администратора"))
            self._action_quit.setText(tr("Выход"))
        except Exception:
            pass

    def _schedule_routing_rebuild(self) -> None:
        if self._routing_rebuild_scheduled:
            return
        self._routing_rebuild_scheduled = True
        QTimer.singleShot(0, self._rebuild_routing_menu)

    def _rebuild_routing_menu(self) -> None:
        self._routing_rebuild_scheduled = False
        try:
            self._routing_menu.clear()
            current = str(getattr(self._bridge, "routingMode", "") or "")
            presets = [
                ("global", tr("Всё через VPN")),
                ("blocked", tr("Только заблокированное")),
                ("except_ru", tr("Всё кроме РФ")),
            ]
            for preset_id, label in presets:
                action = QAction(label, self._routing_menu)
                action.setCheckable(True)
                action.setChecked(
                    (preset_id == "global" and current == "global")
                    or (preset_id == "blocked" and current == "rule")
                )
                action.triggered.connect(lambda _checked=False, pid=preset_id: self._apply_default_routing(pid))
                self._routing_menu.addAction(action)
            custom = list(getattr(self._bridge, "customRoutingPresets", []) or [])
            if custom:
                self._routing_menu.addSeparator()
            for item in custom:
                if not isinstance(item, dict):
                    continue
                preset_id = str(item.get("id") or "")
                if not preset_id:
                    continue
                name = str(item.get("name") or "") or tr("Пресет")
                action = QAction(name, self._routing_menu)
                action.triggered.connect(lambda _checked=False, pid=preset_id: self._apply_custom_routing(pid))
                self._routing_menu.addAction(action)
        except Exception:
            pass

    @staticmethod
    def _apply_menu_style(menu: QMenu) -> None:
        try:
            menu.setStyleSheet(
                "QMenu{background:#202020;color:#F3F3F3;border:1px solid #3A3A3A;padding:6px;}"
                "QMenu::item{min-height:24px;padding:5px 28px 5px 12px;border-radius:4px;}"
                "QMenu::item:selected{background:#2D76D2;color:#FFFFFF;}"
                "QMenu::separator{height:1px;background:#3A3A3A;margin:6px 4px;}"
            )
        except Exception:
            pass

    def notify_hidden(self) -> None:
        if self._notified:
            return
        self._notified = True
        if show_toast(APP_NAME, tr("Приложение свёрнуто в системный трей")):
            return
        try:
            self._tray.showMessage(
                APP_NAME,
                tr("Приложение свёрнуто в системный трей"),
                QSystemTrayIcon.MessageIcon.Information,
                2000,
            )
        except Exception:
            pass
