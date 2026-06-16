"""System tray icon for the QML frontend (background-run support)"""
from __future__ import annotations

import logging

from PyQt6.QtCore import QObject
from PyQt6.QtGui import QAction, QIcon
from PyQt6.QtWidgets import QMenu, QSystemTrayIcon

from ..constants import APP_ICON_PATH, APP_NAME
from .toast import show_toast
from ..i18n import tr

_log = logging.getLogger("xray_fluent")


# Минимальный тёмный стиль меню трея под общий Fluent-вид программы.
_TRAY_MENU_QSS = (
    "QMenu { background-color: #1b1b1f; color: #f2f2f3; "
    "border: 1px solid #3a3a40; border-radius: 8px; padding: 4px; } "
    "QMenu::item { padding: 6px 28px 6px 14px; border-radius: 6px; } "
    "QMenu::item:selected { background-color: #2c2c33; } "
    "QMenu::item:disabled { color: #6f6f78; } "
    "QMenu::separator { height: 1px; background: #3a3a40; margin: 4px 8px; }"
)


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

        menu.addAction(self._action_show)
        menu.addAction(self._action_connect)
        menu.addAction(self._action_next)
        menu.addSeparator()
        self._menu_routing = QMenu(tr("Маршрутизация"), menu)
        self._menu_routing.setStyleSheet(_TRAY_MENU_QSS)
        menu.addMenu(self._menu_routing)
        menu.addSeparator()
        menu.addAction(self._action_admin)
        menu.addSeparator()
        menu.addAction(self._action_quit)
        menu.setStyleSheet(_TRAY_MENU_QSS)
        self._menu = menu
        self._tray.setContextMenu(menu)

        self._tray.activated.connect(self._on_activated)
        menu.aboutToShow.connect(self._refresh_actions)
        # Подменю маршрутизации перестраиваем только перед показом самого
        # подменю, чтобы не очищать пункты уже открытого меню (это роняло прогу).
        self._menu_routing.aboutToShow.connect(self._build_routing_menu)
        try:
            self._bridge.connectedChanged.connect(self._refresh_actions)
        except Exception:
            pass
        try:
            self._bridge.trayMessageRequested.connect(self.notify_hidden)
        except Exception:
            pass
        try:
            self._bridge.trayNotify.connect(self.notify_event)
        except Exception:
            pass
        try:
            self._bridge.selectionChanged.connect(self._update_tooltip)
        except Exception:
            pass

        self._refresh_actions()
        self._update_tooltip()
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
        try:
            self._on_activated_impl(reason)
        except Exception:
            _log.exception("[tray] _on_activated failed")

    def _on_activated_impl(self, reason) -> None:
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
        self._update_tooltip()

    def _build_routing_menu(self) -> None:
        # Колбэк вызывается Qt из C++ во время показа нативного меню трея.
        # Любое исключение здесь иначе роняет процесс без Python-трейсбэка.
        try:
            self._build_routing_menu_impl()
        except Exception:
            _log.exception("[tray] _build_routing_menu failed")

    def _build_routing_menu_impl(self) -> None:
        # Перестраиваем список при каждом открытии: дефолтные + кастомные в одном меню.
        menu = getattr(self, "_menu_routing", None)
        if menu is None:
            return
        menu.clear()
        menu.setStyleSheet(_TRAY_MENU_QSS)
        defaults = [
            ("global", tr("Глобально (всё через VPN)")),
            ("blocked", tr("Только блокировки")),
            ("except_ru", tr("Всё кроме РФ")),
        ]
        for preset_id, label in defaults:
            act = menu.addAction(label)
            act.triggered.connect(
                lambda _checked=False, pid=preset_id: self._apply_routing_preset(pid)
            )
        try:
            custom = list(self._bridge.customRoutingPresets or [])
        except Exception:
            custom = []
        if custom:
            menu.addSeparator()
            for preset in custom:
                if not isinstance(preset, dict):
                    continue
                pid = str(preset.get("id", ""))
                name = str(preset.get("name", ""))
                if not pid:
                    continue
                act = menu.addAction(name or tr("Пресет"))
                act.triggered.connect(
                    lambda _checked=False, p=pid: self._apply_custom_routing_preset(p)
                )

    def _apply_routing_preset(self, preset_id: str) -> None:
        try:
            self._bridge.applyRoutingPreset(preset_id)
        except Exception:
            pass

    def _apply_custom_routing_preset(self, preset_id: str) -> None:
        try:
            self._bridge.applyCustomRoutingPreset(preset_id)
        except Exception:
            pass

    def _current_node_name(self) -> str:
        try:
            return str(self._bridge.selectedNodeName or "")
        except Exception:
            return ""

    def _update_tooltip(self) -> None:
        # Мини-статус в трее: состояние подключения + текущий сервер.
        try:
            name = self._current_node_name()
            if self._connected():
                state = tr("Подключено")
                tip = f"{APP_NAME} — {state}"
                if name:
                    tip += f"\n{name}"
            else:
                tip = f"{APP_NAME} — {tr('Отключено')}"
            self._tray.setToolTip(tip)
        except Exception:
            pass

    def notify_event(self, title: str, message: str) -> None:
        # Балун только когда окно скрыто (в окне уже есть тост).
        if self._window_visible():
            return
        if show_toast(title or APP_NAME, message or ""):
            return
        try:
            self._tray.showMessage(
                title or APP_NAME,
                message or "",
                QSystemTrayIcon.MessageIcon.Information,
                4000,
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
