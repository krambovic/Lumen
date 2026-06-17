"""System tray icon for the QML frontend."""
from __future__ import annotations

from PyQt6.QtGui import QAction, QIcon
from PyQt6.QtWidgets import QMenu, QSystemTrayIcon
from PyQt6.QtCore import QObject

from ..constants import APP_ICON_PATH, APP_NAME
from ..i18n import tr
from .toast import show_toast


class QmlTray(QObject):
    """Owns the QSystemTrayIcon and bridges it to the window + AppBridge."""

    def __init__(self, app, window, bridge) -> None:
        super().__init__(window)
        self._app = app
        self._window = window
        self._bridge = bridge
        self._notified = False
        self._routing_actions: list[QAction] = []

        self._tray = QSystemTrayIcon(self)
        if APP_ICON_PATH.is_file():
            self._tray.setIcon(QIcon(str(APP_ICON_PATH)))
        else:
            self._tray.setIcon(app.windowIcon())
        self._tray.setToolTip(APP_NAME)

        self._menu = QMenu()
        self._apply_menu_style(self._menu)

        self._action_show = QAction(tr("Скрыть"), self._menu)
        self._action_show.triggered.connect(self._toggle_window)
        self._action_connect = QAction(tr("Подключить"), self._menu)
        self._action_connect.triggered.connect(self._toggle_connection)
        self._action_next = QAction(tr("Следующий сервер"), self._menu)
        self._action_next.triggered.connect(self._switch_next)
        self._action_admin = QAction(tr("Перезапустить от администратора"), self._menu)
        self._action_admin.triggered.connect(self._restart_admin)
        self._action_quit = QAction(tr("Выход"), self._menu)
        self._action_quit.triggered.connect(self._quit)

        self._menu.addAction(self._action_show)
        self._menu.addAction(self._action_connect)
        self._menu.addAction(self._action_next)
        self._routing_separator = self._menu.addSeparator()
        self._admin_separator = self._menu.addSeparator()
        self._menu.addAction(self._action_admin)
        self._menu.addSeparator()
        self._menu.addAction(self._action_quit)
        self._tray.setContextMenu(self._menu)

        self._tray.activated.connect(self._on_activated)
        self._menu.aboutToShow.connect(self._refresh_actions)
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

    def _apply_custom_routing(self, preset_id: str) -> None:
        try:
            self._bridge.applyCustomRoutingPreset(preset_id)
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

    def _connected(self) -> bool:
        try:
            return bool(self._bridge.controller.connected)
        except Exception:
            return False

    def _refresh_actions(self) -> None:
        try:
            self._action_show.setText(tr("Показать") if not self._window_visible() else tr("Скрыть"))
            self._action_connect.setText(tr("Отключить") if self._connected() else tr("Подключить"))
            self._action_next.setText(tr("Следующий сервер"))
            self._action_admin.setText(tr("Перезапустить от администратора"))
            self._action_quit.setText(tr("Выход"))
            self._refresh_routing_actions()
        except Exception:
            pass

    def _refresh_routing_actions(self) -> None:
        for action in self._routing_actions:
            try:
                self._menu.removeAction(action)
                action.deleteLater()
            except Exception:
                pass
        self._routing_actions = []

        header = QAction(tr("Маршрутизация"), self._menu)
        header.setEnabled(False)
        self._insert_routing_action(header)

        current = str(getattr(self._bridge, "routingMode", "") or "")
        presets = [
            ("global", tr("Все через VPN")),
            ("blocked", tr("Только заблокированное")),
            ("except_ru", tr("Все кроме РФ")),
        ]
        for preset_id, label in presets:
            action = QAction(label, self._menu)
            action.setCheckable(True)
            action.setChecked(
                (preset_id == "global" and current == "global")
                or (preset_id == "blocked" and current == "rule")
            )
            action.triggered.connect(lambda _checked=False, pid=preset_id: self._apply_default_routing(pid))
            self._insert_routing_action(action)

        custom = getattr(self._bridge, "customRoutingPresets", []) or []
        if custom:
            sep = QAction(self._menu)
            sep.setSeparator(True)
            self._insert_routing_action(sep)
        for item in custom:
            if not isinstance(item, dict):
                continue
            preset_id = str(item.get("id") or "")
            if not preset_id:
                continue
            name = str(item.get("name") or "") or tr("Пресет")
            action = QAction(name, self._menu)
            action.triggered.connect(lambda _checked=False, pid=preset_id: self._apply_custom_routing(pid))
            self._insert_routing_action(action)

    def _insert_routing_action(self, action: QAction) -> None:
        self._menu.insertAction(self._admin_separator, action)
        self._routing_actions.append(action)

    @staticmethod
    def _apply_menu_style(menu: QMenu) -> None:
        try:
            menu.setStyleSheet(
                """
                QMenu {
                    background-color: #202020;
                    color: #F3F3F3;
                    border: 1px solid #3A3A3A;
                    padding: 6px;
                }
                QMenu::item {
                    min-height: 24px;
                    padding: 5px 28px 5px 12px;
                    border-radius: 4px;
                }
                QMenu::item:selected {
                    background-color: #2D76D2;
                    color: #FFFFFF;
                }
                QMenu::item:disabled {
                    color: #9A9A9A;
                    background-color: transparent;
                }
                QMenu::separator {
                    height: 1px;
                    background: #3A3A3A;
                    margin: 6px 4px;
                }
                """
            )
        except Exception:
            pass

    def notify_hidden(self) -> None:
        if self._notified:
            return
        self._notified = True
        message = tr("Приложение свернуто в системный трей")
        if show_toast(APP_NAME, message):
            return
        try:
            self._tray.showMessage(APP_NAME, message, QSystemTrayIcon.MessageIcon.Information, 2000)
        except Exception:
            pass
