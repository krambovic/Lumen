"""Facade QObject that exposes the existing AppController to QML.

Design goals:
- **Reuse the backend as-is.** ``AppController`` and all engines/workers stay
  PyQt6 and are not modified. The bridge simply owns a controller instance and
  relays its signals to QML-friendly properties/signals.
- **Property bindings instead of imperative label updates.** QML binds to the
  properties below; when a controller signal arrives we update the property and
  Qt Quick repaints only the affected bindings on the render thread.
- **Models for collections.** Nodes/logs/process-stats go through dedicated
  list models (see siblings) for delegate recycling.
"""
from __future__ import annotations

from copy import deepcopy

from PyQt6.QtCore import QObject, QTimer, QUrl, pyqtProperty, pyqtSignal, pyqtSlot
from PyQt6.QtGui import QDesktopServices, QGuiApplication

from ...app_controller import AppController
from ...application.node_runtime_service import is_native_singbox_only_node, native_singbox_only_message
from ...constants import APP_NAME, APP_VERSION, SPEED_TEST_MAX_CONCURRENCY
from ...engines.singbox import get_singbox_version
from ...models import Node, RoutingSettings
from ...startup import is_process_elevated, relaunch_as_admin
from .log_model import LogModel
from .node_list_model import NodeListModel
from .process_model import ProcessModel


class AppBridge(QObject):
    # ── notification signals ─────────────────────────────────────
    toast = pyqtSignal(str, str)              # (level, message)
    autoSwitch = pyqtSignal(str)              # (node name)
    bulkTaskProgress = pyqtSignal(str, int, int, bool)  # task, cur, total, done
    connectivityResult = pyqtSignal(bool, str, int)
    appUpdateState = pyqtSignal("QVariantMap")     # application updater
    xrayUpdateState = pyqtSignal("QVariantMap")    # Xray-core updater
    resourceUpdateState = pyqtSignal("QVariantMap")    # sing-box/geodata updater

    # ── property-change signals ──────────────────────────────────
    connectedChanged = pyqtSignal()
    transitionBusyChanged = pyqtSignal()
    runtimeChanged = pyqtSignal()
    metricsChanged = pyqtSignal()
    selectionChanged = pyqtSignal()
    routingChanged = pyqtSignal()
    settingsChanged = pyqtSignal()
    subscriptionsChanged = pyqtSignal()    # subscription list changed
    nodeFiltersChanged = pyqtSignal()      # distinct group/tag option lists changed
    lockedChanged = pyqtSignal()           # app lock/unlock state changed
    trayAvailableChanged = pyqtSignal()    # system tray availability resolved
    trayMessageRequested = pyqtSignal()    # ask the tray to show its balloon
    quittingChanged = pyqtSignal()         # real-exit flag flipped on quit

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._node_model = NodeListModel(self)
        self._log_model = LogModel(parent=self)
        self._process_model = ProcessModel(self)

        # Reuse the existing backend untouched.
        self.controller = AppController(self)
        self.controller.resource_update_result.connect(self._on_resource_update_result)
        self.controller.resource_update_progress.connect(self._on_resource_update_progress)

        # cached state for QML properties
        self._connected = False
        self._busy = False
        self._runtime_phase = ""
        self._runtime_message = ""
        self._down_bps = 0.0
        self._up_bps = 0.0
        self._latency_ms = -1
        self._selected_id = ""
        self._selected_name = ""
        self._selected_latency = -1
        self._routing_mode = "rule"
        self._tun_mode = False
        self._tun_engine = "singbox"
        self._proxy_enabled = False
        self._discord_proxy = False
        self._theme = "dark"
        self._language = "en"
        self._accent = "#0078D4"

        # Активные фильтры списка серверов (группа/тег/текст). Фильтруем в модели
        # (Python), а не скрывая QML-делегаты: иначе ListView неверно оценивает
        # contentHeight — список можно проскроллить ниже конца и он «дёргается».
        self._filter_group = ""
        self._filter_tag = ""
        self._filter_text = ""

        self._tray_available = False
        self._quitting = False

        # Таймер авто-обновления подписок (интервал берётся из настроек).
        self._sub_timer = QTimer(self)
        self._sub_timer.setSingleShot(False)
        self._sub_timer.timeout.connect(self._on_sub_auto_update)

        self._wire_controller()

    # ── lifecycle ──────────────────────────────────────────
    def load(self) -> None:
        """Load persisted state and push initial snapshots into QML."""
        try:
            self.controller.load()
        except Exception as exc:  # pragma: no cover - defensive
            self.toast.emit("error", f"Ошибка загрузки: {exc}")
        self._push_initial_snapshot()
        self.controller.auto_connect_if_needed()
        self._reconfigure_sub_timer()
        # Проверка обновлений при запуске (как в старой widgets-версии).
        # Тихо: ничего не показываем, если обновлений нет/ошибка; при наличии
        # обновления — обновляем страницу «Обновления» и шлём уведомление.
        try:
            if getattr(self.controller.state.settings, "check_updates", True):
                QTimer.singleShot(
                    2500, lambda: self._start_app_update_check(silent=True)
                )
        except Exception:
            pass
        # Разовое тихое обновление подписок при запуске. Таймер авто-обновления
        # стартует «с нуля» при каждом запуске, поэтому при частых перезапусках
        # подписка иначе могла бы никогда не обновиться. Не трогает VPN/прокси.
        try:
            if self.controller.state.subscriptions:
                QTimer.singleShot(3000, self._on_sub_auto_update)
        except Exception:
            pass

    def shutdown(self) -> None:
        try:
            self._sub_timer.stop()
        except Exception:
            pass
        try:
            self.controller.shutdown()
        except Exception:
            pass

    # ── Авто-обновление подписок ────────────────────────
    def _reconfigure_sub_timer(self) -> None:
        """Перезапускает таймер авто-обновления по текущим настройкам."""
        try:
            minutes = int(self.controller.state.settings.subscription_auto_update_minutes)
        except Exception:
            minutes = 240
        if minutes > 0:
            self._sub_timer.setInterval(minutes * 60 * 1000)
            self._sub_timer.start()
        else:
            self._sub_timer.stop()

    def _on_sub_auto_update(self) -> None:
        """Тихое фоновое обновление подписок. Не трогает VPN/прокси."""
        try:
            if getattr(self.controller, "locked", False):
                return
            if not self.controller.state.subscriptions:
                return
            added, _errors = self.controller.update_all_subscriptions()
            if added:
                self.toast.emit("info", self._localized_backend_message(f"Авто-обновление подписок: +{added} серверов"))
        except Exception:
            pass

    # ── Tray / background ───────────────────────────────────────
    def set_tray_available(self, value: bool) -> None:
        value = bool(value)
        if value != self._tray_available:
            self._tray_available = value
            self.trayAvailableChanged.emit()

    @pyqtProperty(bool, notify=trayAvailableChanged)
    def trayAvailable(self) -> bool:
        return self._tray_available

    @pyqtProperty(bool, notify=quittingChanged)
    def quitting(self) -> bool:
        return self._quitting

    @pyqtSlot()
    def prepareQuit(self) -> None:
        if not self._quitting:
            self._quitting = True
            self.quittingChanged.emit()

    @pyqtSlot()
    def notifyHiddenToTray(self) -> None:
        self.trayMessageRequested.emit()

    def _on_admin_relaunch(self) -> None:
        """Relaunch elevated when the controller asks for admin rights"""
        if is_process_elevated():
            return
        try:
            self.controller.save()
        except Exception:
            pass
        if not relaunch_as_admin():
            self.toast.emit("error", "Не удалось перезапустить Lumen KVN от имени администратора")
            return
        app = QGuiApplication.instance()
        if app is not None:
            app.quit()

    def _push_initial_snapshot(self) -> None:
        state = self.controller.state
        self._node_model.set_nodes(state.nodes, state.selected_node_id)
        self._on_selection_changed(self.controller.selected_node)
        self._on_routing_changed(state.routing)
        self._on_settings_changed(state.settings)
        self._on_connection_changed(self.controller.connected)

    # ── controller signal wiring ────────────────────────────────
    def _wire_controller(self) -> None:
        c = self.controller
        c.nodes_changed.connect(self._on_nodes_changed)
        c.selection_changed.connect(self._on_selection_changed)
        c.connection_changed.connect(self._on_connection_changed)
        c.connection_status_changed.connect(self._on_runtime_status)
        c.routing_changed.connect(self._on_routing_changed)
        c.settings_changed.connect(self._on_settings_changed)
        c.subscriptions_changed.connect(self._on_subscriptions_changed)
        c.transition_state_changed.connect(self._on_transition)
        c.status.connect(self._on_status_message)
        c.log_line.connect(self._log_model.append_line)
        c.ping_updated.connect(self._on_ping)
        c.speed_updated.connect(self._on_speed)
        c.speed_progress_updated.connect(self._node_model.update_speed_progress)
        c.bulk_task_progress.connect(self.bulkTaskProgress.emit)
        c.live_metrics_updated.connect(self._on_live_metrics)
        c.auto_switch_triggered.connect(self.autoSwitch.emit)
        c.connectivity_test_done.connect(self.connectivityResult.emit)
        c.lock_state_changed.connect(self._on_lock_state)
        c.admin_relaunch_requested.connect(self._on_admin_relaunch)

    def _english_ui(self) -> bool:
        return (self._language or "en").lower() != "ru"

    def _localized_backend_message(self, message: str) -> str:
        if not message or not self._english_ui():
            return message
        replacements = (
            ("Остановка VPN...", "Stopping VPN..."),
            ("Обновление уже выполняется", "Update is already running"),
            ("Обновление Xray уже выполняется", "Xray update is already running"),
            ("Обновление Xray...", "Updating Xray..."),
            ("Проверка обновлений Xray...", "Checking Xray updates..."),
            ("geoip.dat и geosite.dat обновлены", "geoip.dat and geosite.dat updated"),
            ("Новых серверов не найдено", "No new servers found"),
            ("Подписка удалена", "Subscription removed"),
        )
        for ru, en in replacements:
            if message == ru:
                return en
        prefix_replacements = (
            ("Подписка обновлена: ", "Subscription updated: "),
            ("Подписки обновлены: ", "Subscriptions updated: "),
            ("Импортировано серверов: ", "Servers imported: "),
            ("Авто-обновление подписок: +", "Subscription auto update: +"),
            ("Доступно обновление v", "Update available: v"),
            ("Не удалось обновить geoip/geosite: ", "Could not update geoip/geosite: "),
            ("Не удалось проверить sing-box: ", "Could not check sing-box: "),
            ("Не удалось обновить sing-box: ", "Could not update sing-box: "),
            ("Ошибка загрузки: ", "Download error: "),
            ("Ошибка установки: ", "Install error: "),
            ("Доступно обновление Xray: ", "Xray update available: "),
            ("Xray core обновлён до ", "Xray core updated to "),
            ("sing-box обновлен до ", "sing-box updated to "),
            ("sing-box обновлён до ", "sing-box updated to "),
            ("Доступен sing-box extended ", "sing-box extended available: "),
        )
        for ru, en in prefix_replacements:
            if message.startswith(ru):
                return en + message[len(ru):]
        if message.startswith("Xray core актуален (") and message.endswith(")"):
            return "Xray core is up to date " + message[len("Xray core актуален "):]
        if message.startswith("sing-box актуален (") and message.endswith(")"):
            return "sing-box is up to date " + message[len("sing-box актуален "):]
        return message

    def _on_status_message(self, level: str, message: str) -> None:
        self.toast.emit(level, self._localized_backend_message(message))

    # ── controller -> QML slots ─────────────────────────────────
    def _on_nodes_changed(self, nodes: list[Node]) -> None:
        self._apply_node_model()
        self.nodeFiltersChanged.emit()

    def _apply_node_model(self) -> None:
        """Re-push the controller's nodes into the model honouring the active
        sort key/direction chosen in the Серверы toolbar"""
        state = self.controller.state
        self._node_model.set_runtime_support(
            bool(state.settings.tun_mode and state.settings.tun_engine == "singbox")
        )
        self._node_model.set_nodes(
            self._sorted_nodes(self._filtered_nodes(state.nodes)),
            state.selected_node_id,
        )

    def _filtered_nodes(self, nodes: list[Node]) -> list[Node]:
        """Apply the active group/tag/text filter (mirrors the old QML rowVisible)."""
        group = self._filter_group
        tag = self._filter_tag
        text = (self._filter_text or "").strip().lower()
        if not group and not tag and not text:
            return list(nodes)
        result: list[Node] = []
        for n in nodes:
            if group and (n.group or "Default") != group:
                continue
            if tag and tag not in (n.tags or []):
                continue
            if text and not (
                text in (n.name or "").lower()
                or text in (n.server or "").lower()
                or text in (n.group or "").lower()
                or text in " ".join(n.tags or []).lower()
            ):
                continue
            result.append(n)
        return result

    def _sorted_nodes(self, nodes: list[Node]) -> list[Node]:
        items = list(nodes)
        key = getattr(self, "_sort_key", "manual")
        if key == "name":
            items.sort(key=lambda n: (n.name or n.server or "").lower())
        elif key == "group":
            items.sort(key=lambda n: (n.group or "").lower())
        elif key == "scheme":
            items.sort(key=lambda n: (n.scheme or "").lower())
        elif key == "ping":
            items.sort(key=lambda n: (n.ping_ms is None, n.ping_ms if n.ping_ms is not None else 1e12))
        elif key == "speed":
            # Higher speed first under ascending, unknown speeds last.
            items.sort(key=lambda n: (n.speed_mbps is None, -(n.speed_mbps or 0.0)))
        elif key == "last":
            items.sort(key=lambda n: (not n.last_used_at, n.last_used_at or ""))
        else:  # "manual"
            items.sort(key=lambda n: (n.sort_order if n.sort_order is not None else 0))
        if not getattr(self, "_sort_asc", True):
            items.reverse()
        return items

    def _on_selection_changed(self, node: Node | None) -> None:
        self._selected_id = node.id if node else ""
        self._selected_name = (node.name or node.server) if node else ""
        self._selected_latency = (
            -1 if (node is None or node.ping_ms is None) else int(node.ping_ms)
        )
        self._node_model.set_selected(self._selected_id or None)
        self.selectionChanged.emit()

    def _on_connection_changed(self, connected: bool) -> None:
        if connected == self._connected:
            return
        self._connected = bool(connected)
        if not connected:
            self._down_bps = self._up_bps = 0.0
            self._latency_ms = -1
            self.metricsChanged.emit()
        self.connectedChanged.emit()

    def _on_transition(self, busy: bool, _message: str) -> None:
        if busy == self._busy:
            return
        self._busy = bool(busy)
        self.transitionBusyChanged.emit()

    def _on_runtime_status(self, phase: str, message: str) -> None:
        self._runtime_phase = phase or ""
        self._runtime_message = message or ""
        self.runtimeChanged.emit()

    def _on_routing_changed(self, routing: RoutingSettings) -> None:
        self._routing_mode = routing.mode
        self.routingChanged.emit()

    def _on_settings_changed(self, settings) -> None:
        self._tun_mode = bool(settings.tun_mode)
        self._tun_engine = settings.tun_engine
        self._proxy_enabled = bool(settings.enable_system_proxy)
        self._discord_proxy = bool(getattr(settings, "discord_proxy_enabled", False))
        self._theme = settings.theme
        self._language = getattr(settings, "language", "en")
        self._accent = settings.accent_color or "#0078D4"
        self._node_model.set_runtime_support(bool(settings.tun_mode and settings.tun_engine == "singbox"))
        self.settingsChanged.emit()

    def _on_subscriptions_changed(self, _subscriptions=None) -> None:
        self.subscriptionsChanged.emit()

    def _on_ping(self, node_id: str, ping_ms) -> None:
        self._node_model.update_ping(node_id, ping_ms)
        if node_id == self._selected_id:
            self._selected_latency = -1 if ping_ms is None else int(ping_ms)
            self.selectionChanged.emit()

    def _on_speed(self, node_id: str, speed_mbps, is_alive: bool) -> None:
        self._node_model.update_speed(node_id, speed_mbps)
        self._node_model.update_alive(node_id, is_alive)

    def _on_live_metrics(self, payload: dict) -> None:
        self._down_bps = float(payload.get("down_bps") or 0.0)
        self._up_bps = float(payload.get("up_bps") or 0.0)
        latency = payload.get("latency_ms")
        self._latency_ms = int(latency) if isinstance(latency, int) else -1
        self.metricsChanged.emit()
        stats = payload.get("process_stats")
        if stats is not None:
            self._process_model.set_stats(stats)

    def _on_lock_state(self, locked: bool) -> None:
        if locked:
            self.toast.emit("warning", "Приложение заблокировано")
        self.lockedChanged.emit()

    # ── QML-invokable commands ──────────────────────────────────
    @pyqtSlot()
    def toggleConnection(self) -> None:
        self.controller.toggle_connection()

    @pyqtSlot(str)
    def selectNode(self, node_id: str) -> None:
        if node_id:
            node = self.controller._get_node_by_id(node_id)
            settings = self.controller.state.settings
            if is_native_singbox_only_node(node) and not (settings.tun_mode and settings.tun_engine == "singbox"):
                self.toast.emit("warning", native_singbox_only_message(node))
                return
            self.controller.set_selected_node(node_id)

    @pyqtSlot()
    def switchNext(self) -> None:
        self.controller.switch_next_node()

    @pyqtSlot()
    def switchPrev(self) -> None:
        self.controller.switch_prev_node()

    @pyqtSlot(str)
    def setRoutingMode(self, mode: str) -> None:
        routing = deepcopy(self.controller.state.routing)
        routing.mode = mode
        self.controller.update_routing(routing)

    @pyqtSlot(str)
    def applyRoutingPreset(self, preset_id: str) -> None:
        self.controller.apply_routing_preset(preset_id)

    @pyqtSlot(bool)
    def setTun(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.tun_mode = enabled
        if enabled:
            settings.enable_system_proxy = False
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setProxy(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.enable_system_proxy = enabled
        if enabled and settings.tun_mode:
            settings.tun_mode = False
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setDiscordProxy(self, enabled: bool) -> None:
        self.controller.set_discord_proxy_enabled(enabled)

    @pyqtSlot(str)
    def setTheme(self, name: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.theme = name
        self.controller.update_settings(settings)

    @pyqtSlot(str)
    def setLanguage(self, language: str) -> None:
        value = (language or "en").strip().lower()
        if value not in {"ru", "en"}:
            value = "en"
        settings = deepcopy(self.controller.state.settings)
        settings.language = value
        self.controller.update_settings(settings)

    @pyqtSlot(str)
    def setAccent(self, hex_color: str) -> None:
        if not hex_color:
            return
        settings = deepcopy(self.controller.state.settings)
        settings.accent_color = hex_color
        self.controller.update_settings(settings)

    @pyqtSlot(str)
    def setInterfaceMode(self, mode: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.interface_mode = mode
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setAlwaysRunAsAdmin(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.always_run_as_admin = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtProperty(bool, notify=settingsChanged)
    def alwaysRunAsAdmin(self) -> bool:
        try:
            return bool(self.controller.state.settings.always_run_as_admin)
        except Exception:
            return False

    @pyqtProperty(bool, constant=True)
    def isAdmin(self) -> bool:
        try:
            return bool(is_process_elevated())
        except Exception:
            return False

    # ── «Обновления» settings persistence ───────────────────────
    @pyqtSlot(str)
    def setReleaseChannel(self, channel: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.release_channel = "stable"
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setCheckUpdates(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.check_updates = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setAllowUpdates(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.allow_updates = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setXrayAutoUpdate(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.xray_auto_update = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtProperty(str, notify=settingsChanged)
    def releaseChannel(self) -> str:
        return "stable"

    @pyqtProperty(bool, notify=settingsChanged)
    def checkUpdates(self) -> bool:
        try:
            return bool(self.controller.state.settings.check_updates)
        except Exception:
            return True

    @pyqtProperty(bool, notify=settingsChanged)
    def allowUpdates(self) -> bool:
        try:
            return bool(self.controller.state.settings.allow_updates)
        except Exception:
            return True

    @pyqtProperty(bool, notify=settingsChanged)
    def xrayAutoUpdate(self) -> bool:
        try:
            return bool(self.controller.state.settings.xray_auto_update)
        except Exception:
            return False

    # ════════════════════════════════════════════════════════════
    # Settings
    # ════════════════════════════════════════════════════════════

    # ── Network ──────────────────────────────────────────────────
    @pyqtSlot(bool)
    def setProxyBypassLan(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.system_proxy_bypass_lan = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setReconnectOnNetworkChange(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.reconnect_on_network_change = bool(enabled)
        self.controller.update_settings(settings)

    # ── Auto-switch ──────────────────────────────────────────────
    @pyqtSlot(bool)
    def setAutoSwitch(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_switch_enabled = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtSlot(int)
    def setAutoSwitchThreshold(self, kbps: int) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_switch_threshold_kbps = max(1, int(kbps))
        self.controller.update_settings(settings)

    @pyqtSlot(int)
    def setAutoSwitchDelay(self, seconds: int) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_switch_delay_sec = max(1, int(seconds))
        self.controller.update_settings(settings)

    @pyqtSlot(int)
    def setAutoSwitchCooldown(self, seconds: int) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_switch_cooldown_sec = max(1, int(seconds))
        self.controller.update_settings(settings)

    # ── Core paths ───────────────────────────────────────────────
    @pyqtSlot(str)
    def setXrayPath(self, path: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.xray_path = (path or "").strip()
        self.controller.update_settings(settings)

    @pyqtSlot(str)
    def setSingboxPath(self, path: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.singbox_path = (path or "").strip()
        self.controller.update_settings(settings)

    @pyqtSlot(result=str)
    def browseXrayPath(self) -> str:
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            None, "Выберите xray.exe", "", "xray.exe (xray.exe);;Все файлы (*.*)"
        )
        if path:
            self.setXrayPath(path)
        return path or ""

    @pyqtSlot(result=str)
    def browseSingboxPath(self) -> str:
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            None, "Выберите sing-box.exe", "", "sing-box.exe (sing-box.exe);;Все файлы (*.*)"
        )
        if path:
            self.setSingboxPath(path)
        return path or ""

    @pyqtSlot(str)
    def setTunEngine(self, engine: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.tun_engine = "singbox"
        self.controller.update_settings(settings)

    # ── Startup ──────────────────────────────────────────────────
    @pyqtSlot(bool)
    def setLaunchOnStartup(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.launch_on_startup = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtSlot(bool)
    def setZapretAutostart(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.zapret_autostart = bool(enabled)
        self.controller.update_settings(settings)

    # ── Security: master password / auto-lock ────────────────────
    @pyqtSlot(str)
    def setMasterPassword(self, password: str) -> None:
        pw = (password or "").strip()
        if not pw:
            self.toast.emit("warning", "Введите пароль")
            return
        self.controller.set_master_password(pw)
        self.settingsChanged.emit()
        self.toast.emit("success", "Пароль установлен")

    @pyqtSlot(str, result=bool)
    def disableMasterPassword(self, password: str) -> bool:
        pw = (password or "").strip()
        if not pw:
            self.toast.emit("warning", "Введите текущий пароль")
            return False
        if not self.controller.unlock(pw):
            self.toast.emit("error", "Неверный пароль")
            return False
        self.controller.disable_master_password()
        self.settingsChanged.emit()
        self.toast.emit("info", "Мастер-пароль отключён")
        return True

    @pyqtSlot(int)
    def setAutoLockMinutes(self, minutes: int) -> None:
        value = max(1, min(120, int(minutes)))
        self.controller.state.security.auto_lock_minutes = value
        self.controller.save()
        self.settingsChanged.emit()

    # ── Data: encryption + backup ────────────────────────────────
    @pyqtSlot(str)
    def setEncryptionPassword(self, password: str) -> None:
        pw = (password or "").strip()
        if not pw:
            self.toast.emit("warning", "Введите пароль шифрования")
            return
        self.controller.set_data_passphrase(pw)
        self.settingsChanged.emit()

    @pyqtSlot()
    def disableEncryption(self) -> None:
        self.controller.clear_data_passphrase()
        self.settingsChanged.emit()

    @pyqtSlot()
    def exportBackup(self) -> None:
        from PyQt6.QtWidgets import QFileDialog
        from pathlib import Path
        path, _ = QFileDialog.getSaveFileName(
            None, "Сохранить резервную копию", "lumen-backup.json",
            "Резервная копия (*.json);;Все файлы (*.*)"
        )
        if not path:
            return
        try:
            self.controller.export_backup(Path(path))
            self.toast.emit("success", "Резервная копия сохранена")
        except Exception as exc:
            self.toast.emit("error", f"Ошибка экспорта: {exc}")

    @pyqtSlot()
    def importBackup(self) -> None:
        from PyQt6.QtWidgets import QFileDialog
        from pathlib import Path
        path, _ = QFileDialog.getOpenFileName(
            None, "Импорт резервной копии", "",
            "Резервная копия (*.json);;Все файлы (*.*)"
        )
        if not path:
            return
        try:
            self.controller.import_backup(Path(path))
            self.toast.emit("success", "Резервная копия импортирована")
        except Exception as exc:
            self.toast.emit("error", f"Ошибка импорта: {exc}")

    @pyqtSlot()
    @pyqtSlot('QVariantList')
    def pingNodes(self, ids: list | None = None) -> None:
        self.controller.ping_nodes(set(ids) if ids else None)

    @pyqtSlot()
    @pyqtSlot('QVariantList')
    def tcpingNodes(self, ids: list | None = None) -> None:
        self.controller.ping_nodes(set(ids) if ids else None, method="tcping")

    @pyqtSlot()
    @pyqtSlot('QVariantList')
    def realDelayNodes(self, ids: list | None = None) -> None:
        self.controller.ping_nodes(set(ids) if ids else None, method="real")

    @pyqtSlot()
    @pyqtSlot('QVariantList')
    def speedTestNodes(self, ids: list | None = None) -> None:
        self.controller.speed_test_nodes(set(ids) if ids else None)

    @pyqtSlot()
    @pyqtSlot('QVariantList')
    def downloadSpeedNodes(self, ids: list | None = None) -> None:
        self.controller.speed_test_nodes(set(ids) if ids else None)

    @pyqtSlot()
    def cancelSpeedTest(self) -> None:
        self.controller.cancel_speed_test()

    @pyqtSlot('QVariantList')
    def deleteNodes(self, ids: list) -> None:
        if ids:
            self.controller.remove_nodes(set(ids))

    @pyqtSlot(str, str)
    def reorderNode(self, node_id: str, direction: str) -> None:
        self.controller.reorder_nodes(node_id, direction)

    # ── node edit / bulk-edit / share-link ───────────────────────
    @pyqtSlot(str, result="QVariantMap")
    def nodeEditFields(self, node_id: str):
        """Flattened editable fields for the node-edit form (or {} if missing)."""
        node = self.controller._get_node_by_id(node_id) if node_id else None
        if node is None:
            return {}
        from .node_edit_helpers import load_node_edit_fields
        return load_node_edit_fields(node)

    @pyqtProperty("QVariantMap", constant=True)
    def nodeEditOptions(self):
        """Static combo option lists for the node-edit form (mirror the dialog)."""
        from .node_edit_helpers import FINGERPRINTS, FLOWS, NETWORKS, RAW_HEADERS, SECURITY
        return {
            "fingerprints": list(FINGERPRINTS),
            "networks": list(NETWORKS),
            "security": list(SECURITY),
            "flows": list(FLOWS),
            "rawHeaders": list(RAW_HEADERS),
        }

    @pyqtSlot(str, "QVariantMap")
    def saveNodeEdit(self, node_id: str, fields) -> None:
        """Rebuild the outbound from the form values and persist via update_node."""
        node = self.controller._get_node_by_id(node_id) if node_id else None
        if node is None:
            self.toast.emit("warning", "Сервер не найден")
            return
        from .node_edit_helpers import build_node_updates
        try:
            updates = build_node_updates(node, dict(fields or {}))
            self.controller.update_node(node_id, updates)
        except Exception as exc:  # noqa: BLE001 - surface failures as a toast
            self.toast.emit("error", f"Не удалось сохранить: {exc}")
            return
        self.toast.emit("success", "Сервер обновлён")

    @pyqtSlot("QVariantList", "QVariantMap")
    def bulkEditNodes(self, ids: list, operations) -> None:
        """Apply bulk group move / tag add+remove to the selected nodes."""
        node_ids = {str(i) for i in (ids or []) if i}
        if not node_ids:
            self.toast.emit("warning", "Не выбрано ни одного сервера")
            return
        ops = dict(operations or {})
        payload = {
            "group": str(ops.get("group", "") or "").strip(),
            "add_tags": [str(t).strip() for t in (ops.get("add_tags") or []) if str(t).strip()],
            "remove_tags": [str(t).strip() for t in (ops.get("remove_tags") or []) if str(t).strip()],
        }
        self.controller.bulk_update_nodes(node_ids, payload)
        self.toast.emit("success", f"Обновлено серверов: {len(node_ids)}")

    @pyqtSlot()
    @pyqtSlot(str)
    def copyNodeLink(self, node_id: str = "") -> None:
        """Copy a node's share link (vless://…) to the clipboard."""
        node = (
            self.controller._get_node_by_id(node_id) if node_id
            else self.controller.selected_node
        )
        link = (getattr(node, "link", "") or "").strip() if node is not None else ""
        if not link:
            self.toast.emit("warning", "У сервера нет ссылки для копирования")
            return
        clipboard = QGuiApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(link)
            self.toast.emit("success", "Ссылка скопирована в буфер обмена")

    @pyqtSlot(int, result="QVariantMap")
    def historyData(self, days: int = 30):
        """Aggregated traffic-history payload for the History tab.

        Mirrors ui/history_page._refresh(): summary totals plus formatted
        sessions / daily / per-process rows for the given period (in days).
        """
        from .history_helpers import build_history_payload
        try:
            storage = self.controller.traffic_history
        except Exception:
            storage = None
        try:
            return build_history_payload(storage, int(days) if days else 30)
        except Exception as exc:  # noqa: BLE001 - never break the QML binding
            self.toast.emit("error", f"Не удалось загрузить историю: {exc}")
            return build_history_payload(None, 30)

    # ── Updates tab ──────────────────────────────────────────────


    @pyqtSlot()
    def clearTrafficHistory(self) -> None:
        try:
            self.controller.traffic_history.clear()
        except Exception as exc:  # noqa: BLE001 - surface failures as a toast
            self.toast.emit("error", f"Не удалось очистить историю: {exc}")
            return
        self.toast.emit("success", "История очищена")
    @pyqtSlot(result="QVariantMap")
    def updatesInitialState(self):
        """Snapshot shown when the Updates tab first appears."""
        from ...engines.xray import get_xray_version
        try:
            version = get_xray_version(self.controller.state.settings.xray_path) or ""
        except Exception:
            version = ""
        try:
            singbox_version = get_singbox_version(self.controller.state.settings.singbox_path) or ""
        except Exception:
            singbox_version = ""
        return {"appVersion": APP_VERSION, "xrayVersion": version, "singboxVersion": singbox_version}

    # -- application updater --
    @pyqtSlot()
    def checkAppUpdate(self) -> None:
        self._start_app_update_check(silent=False)

    def _start_app_update_check(self, silent: bool = False) -> None:
        from ...app_updater import UpdateChecker
        if getattr(self, "_app_update_checker", None) is not None:
            return
        self._app_update_silent = silent
        if not silent:
            self.appUpdateState.emit({"phase": "checking"})
        checker = UpdateChecker(self, channel="stable", prefer_qml=False)
        self._app_update_checker = checker
        checker.result.connect(self._on_app_update_result)
        checker.error.connect(self._on_app_update_error)
        checker.finished.connect(self._clear_app_update_checker)
        checker.start()

    def _clear_app_update_checker(self) -> None:
        self._app_update_checker = None

    def _on_app_update_result(self, update) -> None:
        self._pending_app_update = update
        silent = getattr(self, "_app_update_silent", False)
        if update is None:
            if not silent:
                self.appUpdateState.emit({"phase": "uptodate"})
            return
        notes = (update.notes or "").strip()
        if len(notes) > 1200:
            notes = notes[:1200].rstrip() + "…"
        self.appUpdateState.emit({
            "phase": "available",
            "version": update.version,
            "notes": notes,
        })
        if silent:
            # При тихой проверке на старте уведомляем пользователя тостом.
            self.toast.emit("info", self._localized_backend_message(f"Доступно обновление v{update.version}"))

    def _on_app_update_error(self, message: str) -> None:
        if getattr(self, "_app_update_silent", False):
            return
        self.appUpdateState.emit({"phase": "error", "message": message})

    @pyqtSlot()
    def downloadAppUpdate(self) -> None:
        update = getattr(self, "_pending_app_update", None)
        if update is None:
            self.appUpdateState.emit({"phase": "error", "message": "Сначала проверьте обновления"})
            return
        if not getattr(self.controller.state.settings, "allow_updates", True):
            self.appUpdateState.emit({"phase": "error", "message": "Установка обновлений отключена в настройках"})
            return
        if getattr(self, "_app_update_downloader", None) is not None:
            return
        from ...app_updater import UpdateDownloader
        from ...constants import PROXY_HOST
        proxy_url = None
        try:
            if self.controller.connected:
                port = self.controller.get_effective_http_proxy_port()
                if port:
                    proxy_url = f"http://{PROXY_HOST}:{port}"
        except Exception:
            proxy_url = None
        self.appUpdateState.emit({"phase": "downloading", "percent": 0})
        downloader = UpdateDownloader(update, proxy_url=proxy_url, restart_in_tray=False, parent=self)
        self._app_update_downloader = downloader
        downloader.progress.connect(self._on_app_download_progress)
        downloader.status.connect(self._on_app_download_status)
        downloader.finished_ok.connect(self._on_app_download_ok)
        downloader.error.connect(self._on_app_download_error)
        downloader.finished.connect(self._clear_app_update_downloader)
        downloader.start()

    def _clear_app_update_downloader(self) -> None:
        self._app_update_downloader = None

    def _on_app_download_progress(self, percent: int) -> None:
        self.appUpdateState.emit({"phase": "downloading", "percent": int(percent)})

    def _on_app_download_status(self, message: str) -> None:
        self.appUpdateState.emit({"phase": "downloading", "message": message})

    def _on_app_download_ok(self) -> None:
        self.appUpdateState.emit({"phase": "ready", "message": "Обновление загружено. Перезапуск..."})
        # PowerShell-скрипт перезапуска ждёт выхода этого процесса перед заменой
        # файлов. Если не закрыться самим, он ждёт ~60с и лишь потом убивает процесс —
        # это и была минутная задержка. Закрываемся быстро (как _quit_for_update в widgets-версии),
        # чтобы перезапуск произошёл практически сразу.
        self.prepareQuit()
        QTimer.singleShot(1500, self._quit_for_update)

    def _quit_for_update(self) -> None:
        # app.aboutToQuit уже вызывает bridge.shutdown() → корректно останавливает VPN/Wintun.
        app = QGuiApplication.instance()
        if app is not None:
            app.quit()

    def _on_app_download_error(self, message: str) -> None:
        self.appUpdateState.emit({"phase": "error", "message": message})

    # -- Xray core updater --
    def _ensure_xray_update_wired(self) -> None:
        if getattr(self, "_xray_update_wired", False):
            return
        try:
            self.controller.xray_update_progress.connect(self._on_xray_update_progress)
            self.controller.xray_update_result.connect(self._on_xray_update_result)
            self._xray_update_wired = True
        except Exception:
            pass

    @pyqtSlot()
    def checkXrayUpdate(self) -> None:
        self._ensure_xray_update_wired()
        self.xrayUpdateState.emit({"phase": "checking", "percent": 0})
        try:
            self.controller.run_xray_core_update(False)
        except Exception as exc:  # noqa: BLE001
            self.xrayUpdateState.emit({"phase": "error", "message": str(exc)})

    @pyqtSlot()
    def updateXrayCore(self) -> None:
        self._ensure_xray_update_wired()
        self.xrayUpdateState.emit({"phase": "updating", "percent": 0})
        try:
            self.controller.run_xray_core_update(True)
        except Exception as exc:  # noqa: BLE001
            self.xrayUpdateState.emit({"phase": "error", "message": str(exc)})

    def _on_xray_update_progress(self, percent: int) -> None:
        self.xrayUpdateState.emit({"phase": "updating", "percent": int(percent)})

    def _on_xray_update_result(self, result) -> None:
        phase = {
            "up_to_date": "uptodate",
            "available": "available",
            "updated": "updated",
            "error": "error",
        }.get(getattr(result, "status", ""), "uptodate")
        version = getattr(result, "latest_version", "") or getattr(result, "current_version", "") or ""
        self.xrayUpdateState.emit({
            "phase": phase,
            "version": version,
            "message": self._localized_backend_message(getattr(result, "message", "") or ""),
            "percent": 100 if phase == "updated" else 0,
        })

    @pyqtSlot()
    def checkSingboxUpdate(self) -> None:
        self.resourceUpdateState.emit({"kind": "singbox", "phase": "checking", "percent": 0})
        try:
            self.controller.run_resource_update("singbox", apply_update=False)
        except Exception as exc:  # noqa: BLE001
            self.resourceUpdateState.emit({"kind": "singbox", "phase": "error", "message": str(exc)})

    @pyqtSlot()
    def updateSingboxCore(self) -> None:
        self.resourceUpdateState.emit({"kind": "singbox", "phase": "updating", "percent": 0})
        try:
            self.controller.run_resource_update("singbox", apply_update=True)
        except Exception as exc:  # noqa: BLE001
            self.resourceUpdateState.emit({"kind": "singbox", "phase": "error", "message": str(exc)})

    @pyqtSlot()
    def updateGeodataFiles(self) -> None:
        self.resourceUpdateState.emit({"kind": "geodata", "phase": "updating", "percent": 0})
        try:
            self.controller.run_resource_update("geodata", apply_update=True)
        except Exception as exc:  # noqa: BLE001
            self.resourceUpdateState.emit({"kind": "geodata", "phase": "error", "message": str(exc)})

    def _on_resource_update_progress(self, kind: str, percent: int) -> None:
        self.resourceUpdateState.emit({"kind": kind, "phase": "updating", "percent": int(percent)})

    def _on_resource_update_result(self, result) -> None:
        phase = {
            "up_to_date": "uptodate",
            "available": "available",
            "updated": "updated",
            "error": "error",
        }.get(getattr(result, "status", ""), "uptodate")
        payload = {
            "kind": getattr(result, "kind", ""),
            "phase": phase,
            "message": self._localized_backend_message(getattr(result, "message", "") or ""),
            "version": getattr(result, "latest_version", "") or getattr(result, "current_version", "") or "",
            "percent": 100 if phase == "updated" else 0,
        }
        self.resourceUpdateState.emit(payload)

    # ── Zapret tab ──────────────────────────────────────────────
    zapretState = pyqtSignal("QVariantMap")        # {running, preset, error}
    zapretPresetsChanged = pyqtSignal()            # preset list changed

    def _ensure_zapret_wired(self) -> None:
        if getattr(self, "_zapret_wired", False):
            return
        try:
            z = self.controller.zapret
            z.started.connect(self._on_zapret_started)
            z.stopped.connect(self._on_zapret_stopped)
            z.error.connect(self._on_zapret_error)
            self._zapret_wired = True
        except Exception:
            pass

    @pyqtSlot(result="QVariantList")
    def zapretPresets(self):
        self._ensure_zapret_wired()
        from .zapret_helpers import list_preset_maps
        return list_preset_maps()

    @pyqtSlot(result="QVariantMap")
    def zapretStatus(self):
        self._ensure_zapret_wired()
        try:
            running = bool(self.controller.zapret.running)
        except Exception:
            running = False
        preset = ""
        try:
            if running:
                preset = self.controller.state.settings.zapret_preset or ""
        except Exception:
            preset = ""
        return {"running": running, "preset": preset, "error": ""}

    @pyqtSlot(str)
    def startZapret(self, preset_name: str) -> None:
        name = (preset_name or "").strip()
        if not name:
            return
        self._ensure_zapret_wired()
        try:
            self.controller.state.settings.zapret_preset = name
            self.controller.save()
            self.controller.zapret.start(name)
        except Exception as exc:  # noqa: BLE001
            self.zapretState.emit({"running": False, "preset": "", "error": str(exc)})

    @pyqtSlot()
    def stopZapret(self) -> None:
        self._ensure_zapret_wired()
        try:
            self.controller.zapret.stop()
        except Exception as exc:  # noqa: BLE001
            self.zapretState.emit({"running": False, "preset": "", "error": str(exc)})

    @pyqtSlot(str, result=str)
    def readPreset(self, name: str) -> str:
        from ...zapret_manager import ZapretManager
        try:
            return ZapretManager.read_preset(name)
        except Exception:
            return ""

    @pyqtSlot(str, str, str)
    def savePreset(self, name: str, description: str, content: str) -> None:
        from ...zapret_manager import ZapretManager
        name = (name or "").strip()
        if not name:
            self.toast.emit("warning", "Укажите имя пресета")
            return
        if any(c in '\\/:*?"<>|' for c in name):
            self.toast.emit("warning", "Недопустимые символы в имени")
            return
        try:
            ZapretManager.save_preset(name, content, description or "")
            self.toast.emit("success", f"Пресет сохранён: {name}")
            self.zapretPresetsChanged.emit()
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Не удалось сохранить пресет: {exc}")

    @pyqtSlot(str)
    def deletePreset(self, name: str) -> None:
        from ...zapret_manager import ZapretManager
        if not name:
            return
        try:
            if self.controller.zapret.running and name == self.controller.state.settings.zapret_preset:
                self.controller.zapret.stop()
            ZapretManager.delete_preset(name)
            self.toast.emit("success", f"Пресет удалён: {name}")
            self.zapretPresetsChanged.emit()
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Не удалось удалить пресет: {exc}")

    @pyqtSlot(result=str)
    def importZapretPreset(self) -> str:
        from pathlib import Path
        from ...zapret_manager import ZapretManager
        from PyQt6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            None, "Импорт пресета", "", "Текстовые файлы (*.txt);;Все файлы (*)"
        )
        if not path:
            return ""
        try:
            info = ZapretManager.import_preset(Path(path))
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Не удалось импортировать: {exc}")
            return ""
        if info is None:
            self.toast.emit("warning", "Не удалось импортировать пресет")
            return ""
        self.toast.emit("success", f"Импортирован пресет: {info.name}")
        self.zapretPresetsChanged.emit()
        return info.name

    def _on_zapret_started(self) -> None:
        try:
            active = self.controller.state.settings.zapret_preset or ""
        except Exception:
            active = ""
        self.zapretState.emit({"running": True, "preset": active, "error": ""})

    def _on_zapret_stopped(self) -> None:
        self.zapretState.emit({"running": False, "preset": "", "error": ""})

    def _on_zapret_error(self, message: str) -> None:
        self.zapretState.emit({"running": False, "preset": "", "error": message})
        self.toast.emit("error", f"Zapret: {message}")

    # ── Configs (sing-box / xray raw editors) ──────────────────────
    def _config_state(self, core, *, text="", file_label=None, level="", message=""):
        from .configs_helpers import build_state
        return build_state(
            self.controller, core, text=text, file_label=file_label,
            status_level=level, status_message=message,
        )

    @pyqtSlot(str, result="QVariantMap")
    def loadConfig(self, core: str):
        if core not in ("singbox", "xray"):
            return self._config_state("singbox")
        try:
            path, text = getattr(self.controller, f"load_active_{core}_config_text")()
        except Exception as exc:  # noqa: BLE001
            return self._config_state(core, level="error", message=str(exc))
        return self._config_state(
            core, text=text, file_label=path.as_posix(),
            level="info", message=f"Открыта активная копия: {path.name}",
        )

    @pyqtSlot(str, str, result="QVariantMap")
    def selectConfig(self, core: str, relative_path: str):
        from .configs_helpers import sync_template_for_config
        if core not in ("singbox", "xray") or not relative_path:
            return self._config_state(core if core in ("singbox", "xray") else "singbox")
        try:
            path, text = getattr(self.controller, f"load_{core}_config_text")(relative_path)
            sync_template_for_config(self.controller, core, path)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return self._config_state(core, level="error", message=str(exc))
        self.toast.emit("success", f"Открыт конфиг: {path.name}")
        return self._config_state(
            core, text=text, file_label=path.as_posix(),
            level="info", message=f"Открыт конфиг: {path.name}",
        )

    @pyqtSlot(str, str, result="QVariantMap")
    def selectTemplate(self, core: str, relative_path: str):
        from pathlib import Path
        if core not in ("singbox", "xray") or not relative_path:
            return self._config_state(core if core in ("singbox", "xray") else "singbox")
        try:
            path, text = getattr(self.controller, f"import_{core}_template")(relative_path)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return self._config_state(core, level="error", message=str(exc))
        name = Path(relative_path).name
        self.toast.emit("success", f"Применён шаблон: {name}")
        return self._config_state(
            core, text=text, file_label=path.as_posix(),
            level="info", message=f"Применён шаблон: {name}",
        )

    @pyqtSlot(str, result="QVariantMap")
    def importTemplate(self, core: str):
        from pathlib import Path
        from PyQt6.QtWidgets import QFileDialog
        if core not in ("singbox", "xray"):
            return {"cancelled": True}
        title = "sing-box" if core == "singbox" else "xray"
        base_dir = str(getattr(self.controller, f"get_{core}_template_dir")())
        file_path, _ = QFileDialog.getOpenFileName(
            None, f"Импортировать {title} template", base_dir, "JSON files (*.json)"
        )
        if not file_path:
            return {"cancelled": True}
        try:
            path, text = getattr(self.controller, f"import_{core}_template")(file_path)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return self._config_state(core, level="error", message=str(exc))
        self.toast.emit("success", f"Импортирован template: {Path(file_path).name}")
        return self._config_state(
            core, text=text, file_label=path.as_posix(),
            level="info", message=f"Импортирован template и обновлена активная копия: {path.name}",
        )

    @pyqtSlot(str, result="QVariantMap")
    def resetConfig(self, core: str):
        if core not in ("singbox", "xray"):
            return self._config_state("singbox")
        try:
            ok, path, message = getattr(self.controller, f"reset_active_{core}_config_to_template")()
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return self._config_state(core, level="error", message=str(exc))
        if not ok or path is None:
            msg = message or "Сброс не выполнен"
            self.toast.emit("error", msg.splitlines()[0])
            return self._config_state(core, level="error", message=msg)
        try:
            loaded_path, text = getattr(self.controller, f"load_active_{core}_config_text")()
        except Exception as exc:  # noqa: BLE001
            return self._config_state(core, level="error", message=str(exc))
        self.toast.emit("success", message)
        return self._config_state(
            core, text=text, file_label=loaded_path.as_posix(),
            level="success", message=message,
        )

    @pyqtSlot(str, str, result="QVariantMap")
    def saveConfig(self, core: str, text: str):
        if core not in ("singbox", "xray"):
            return self._config_state("singbox", text=text)
        try:
            path = getattr(self.controller, f"save_{core}_config_text")(text)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return self._config_state(core, text=text, level="error", message=str(exc))
        self.toast.emit("success", f"Сохранено: {path.name}")
        return self._config_state(
            core, text=text, file_label=path.as_posix(),
            level="success", message=f"Сохранено: {path.name}",
        )

    @pyqtSlot(str, str, result="QVariantMap")
    def validateConfig(self, core: str, text: str):
        if core not in ("singbox", "xray"):
            return {"statusLevel": "error", "statusMessage": "Неизвестное ядро"}
        try:
            ok, message = getattr(self.controller, f"validate_{core}_json_text")(text)
        except Exception as exc:  # noqa: BLE001
            return {"statusLevel": "error", "statusMessage": str(exc)}
        if ok:
            self.toast.emit("success", "JSON корректен")
        return {"statusLevel": "success" if ok else "error", "statusMessage": message}

    @pyqtSlot(str, str, result="QVariantMap")
    def applyConfig(self, core: str, text: str):
        if core not in ("singbox", "xray"):
            return {"statusLevel": "error", "statusMessage": "Неизвестное ядро", "fileLabel": ""}
        try:
            ok, path, message = getattr(self.controller, f"apply_{core}_config_text")(text)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", str(exc).splitlines()[0])
            return {"statusLevel": "error", "statusMessage": str(exc), "fileLabel": ""}
        if not ok:
            msg = message or "Не удалось применить"
            self.toast.emit("error", msg.splitlines()[0])
            return {"statusLevel": "error", "statusMessage": msg, "fileLabel": ""}
        level = "info" if "Применяю" in (message or "") else "success"
        self.toast.emit(level, (message or "Применено").splitlines()[0])
        return {
            "statusLevel": level,
            "statusMessage": message or "",
            "fileLabel": path.as_posix() if path is not None else "",
        }

    @pyqtSlot()
    def importClipboard(self) -> None:
        clipboard = QGuiApplication.clipboard()
        text = clipboard.text().strip() if clipboard is not None else ""
        if not text:
            self.toast.emit("warning", "Буфер обмена пуст")
            return
        added, errors = self.controller.import_nodes_from_text(text)
        if added:
            self.toast.emit("success", f"Импортировано серверов: {added}")
        if errors:
            self.toast.emit("warning", "; ".join(errors[:2]))
        if not added and not errors:
            self.toast.emit("warning", "Новых серверов не импортировано")

    @pyqtSlot()
    def importNodeFile(self) -> None:
        from pathlib import Path
        from PyQt6.QtWidgets import QFileDialog

        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Импортировать сервер",
            "",
            "VPN configs (*.conf *.txt *.json);;All files (*.*)",
        )
        if not file_path:
            return
        try:
            text = Path(file_path).read_text(encoding="utf-8", errors="replace")
        except Exception as exc:
            self.toast.emit("error", f"Не удалось прочитать файл: {exc}")
            return
        added, errors = self.controller.import_nodes_from_text(text)
        if added:
            self.toast.emit("success", f"Импортировано серверов: {added}")
        if errors:
            self.toast.emit("warning", "; ".join(errors[:2]))
        if not added and not errors:
            self.toast.emit("warning", "Новых серверов не импортировано")

    @pyqtSlot()
    @pyqtSlot(str)
    def copyOutboundJson(self, node_id: str = "") -> None:
        payload = self.controller.export_node_outbound_json(node_id or None)
        self._copy_or_warn(payload)

    @pyqtSlot()
    @pyqtSlot(str)
    def copyRuntimeJson(self, node_id: str = "") -> None:
        payload = self.controller.export_runtime_config_json(node_id or None)
        self._copy_or_warn(payload)

    @pyqtSlot()
    @pyqtSlot(str)
    def saveOutboundJson(self, node_id: str = "") -> None:
        payload = self.controller.export_node_outbound_json(node_id or None)
        self._save_json_payload(payload, "outbound.json")

    @pyqtSlot()
    @pyqtSlot(str)
    def saveRuntimeJson(self, node_id: str = "") -> None:
        payload = self.controller.export_runtime_config_json(node_id or None)
        try:
            singbox = self.controller.is_singbox_editor_mode()
        except Exception:  # noqa: BLE001 - defensive, default to xray name
            singbox = False
        suggested = "singbox_config.json" if singbox else "xray_config.json"
        self._save_json_payload(payload, suggested)

    def _save_json_payload(self, payload: str | None, suggested_name: str) -> None:
        if not payload:
            self.toast.emit("warning", "Выберите сервер для экспорта")
            return
        from PyQt6.QtWidgets import QApplication, QFileDialog
        file_path, _ = QFileDialog.getSaveFileName(
            None, "Экспорт JSON", suggested_name, "JSON files (*.json)"
        )
        if file_path:
            try:
                with open(file_path, "w", encoding="utf-8") as fh:
                    fh.write(payload)
            except OSError as exc:
                self.toast.emit("error", f"Не удалось сохранить файл: {exc}")
                return
            self.toast.emit("success", f"JSON экспортирован: {file_path}")
            return
        clipboard = QApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(payload)
            self.toast.emit("info", "Экспорт отменён, JSON скопирован в буфер обмена")

    @pyqtSlot(str, bool)
    def setNodeSort(self, key: str, ascending: bool) -> None:
        """Set the active sort key/direction for the node list and re-push it."""
        self._sort_key = key or "manual"
        self._sort_asc = bool(ascending)
        self._apply_node_model()

    @pyqtSlot(str, str, str)
    def setNodeFilter(self, group: str, tag: str, text: str) -> None:
        """Set the active group/tag/text filter and re-push the node list so the
        model holds only visible rows (keeps ListView count/contentHeight correct)."""
        self._filter_group = group or ""
        self._filter_tag = tag or ""
        self._filter_text = text or ""
        self._apply_node_model()

    @pyqtProperty("QVariantList", notify=nodeFiltersChanged)
    def groupOptions(self) -> list:
        """Distinct group names across all nodes (for the Группа filter combo)."""
        seen: list[str] = []
        for node in self.controller.state.nodes:
            grp = node.group or "Default"
            if grp not in seen:
                seen.append(grp)
        seen.sort(key=str.lower)
        return seen

    @pyqtProperty("QVariantList", notify=nodeFiltersChanged)
    def tagOptions(self) -> list:
        """Distinct tags across all nodes (for the Теги filter combo)."""
        seen: list[str] = []
        for node in self.controller.state.nodes:
            for tag in (node.tags or []):
                if tag and tag not in seen:
                    seen.append(tag)
        seen.sort(key=str.lower)
        return seen

    @pyqtSlot()
    def exportDiagnostics(self) -> None:
        """Build a diagnostics zip and reveal it in the file manager."""
        try:
            path = self.controller.build_diagnostics()
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Не удалось собрать диагностику: {exc}")
            return
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(path.parent)))
        self.toast.emit("success", f"Диагностика сохранена: {path.name}")

    @pyqtSlot(int, result=str)
    def nodeIdAt(self, row: int) -> str:
        """Map a ListView row index to a node id (used for drag/shift range select)."""
        return self._node_model.node_id_at(row) or ""

    @pyqtSlot(int, result="QVariant")
    def nodeRowAt(self, row: int):
        """Return a row's filterable fields {id,name,server,group,tags} by index.

        Used by Ctrl+A so it can select only the rows that pass the current
        group/tag/text filter instead of every node in the model.
        """
        return self._node_model.node_row_at(row)

    @pyqtSlot()
    def lockNow(self) -> None:
        self.controller.lock()

    @pyqtSlot()
    def clearLogs(self) -> None:
        self._log_model.clear()

    @pyqtSlot(str)
    def testConnectivity(self, url: str) -> None:
        self.controller.test_connectivity(url or None)

    @pyqtSlot(str)
    def openUrl(self, url: str) -> None:
        """Open an external link in the user's default browser (About page)."""
        target = (url or "").strip()
        if not target:
            return
        QDesktopServices.openUrl(QUrl(target))

    def _copy_or_warn(self, payload: str | None) -> None:
        if not payload:
            self.toast.emit("warning", "Выберите сервер для экспорта")
            return
        clipboard = QGuiApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(payload)
            self.toast.emit("success", "JSON скопирован в буфер обмена")

    # ── QML-readable models ────────────────────────────────────
    @pyqtProperty(QObject, constant=True)
    def nodeModel(self) -> NodeListModel:
        return self._node_model

    @pyqtProperty(QObject, constant=True)
    def logModel(self) -> LogModel:
        return self._log_model

    @pyqtProperty(QObject, constant=True)
    def processModel(self) -> ProcessModel:
        return self._process_model

    @pyqtProperty(str, constant=True)
    def appName(self) -> str:
        return APP_NAME

    @pyqtProperty(str, constant=True)
    def appVersion(self) -> str:
        return APP_VERSION

    # ── QML-readable state properties ─────────────────────────────
    @pyqtProperty(bool, notify=connectedChanged)
    def connected(self) -> bool:
        return self._connected

    @pyqtProperty(bool, notify=transitionBusyChanged)
    def transitionBusy(self) -> bool:
        return self._busy

    @pyqtProperty(str, notify=runtimeChanged)
    def runtimePhase(self) -> str:
        return self._runtime_phase

    @pyqtProperty(str, notify=runtimeChanged)
    def runtimeMessage(self) -> str:
        return self._runtime_message

    @pyqtProperty(float, notify=metricsChanged)
    def downBps(self) -> float:
        return self._down_bps

    @pyqtProperty(float, notify=metricsChanged)
    def upBps(self) -> float:
        return self._up_bps

    @pyqtProperty(int, notify=metricsChanged)
    def latencyMs(self) -> int:
        return self._latency_ms

    @pyqtProperty(str, notify=selectionChanged)
    def selectedNodeId(self) -> str:
        return self._selected_id

    @pyqtProperty(str, notify=selectionChanged)
    def selectedNodeName(self) -> str:
        return self._selected_name

    @pyqtProperty(int, notify=selectionChanged)
    def selectedLatency(self) -> int:
        return self._selected_latency

    @pyqtProperty(str, notify=routingChanged)
    def routingMode(self) -> str:
        return self._routing_mode

    @pyqtProperty(bool, notify=settingsChanged)
    def tunMode(self) -> bool:
        return self._tun_mode

    @pyqtProperty(str, notify=settingsChanged)
    def tunEngine(self) -> str:
        return self._tun_engine

    @pyqtProperty(bool, notify=settingsChanged)
    def proxyEnabled(self) -> bool:
        return self._proxy_enabled

    @pyqtProperty(bool, notify=settingsChanged)
    def discordProxy(self) -> bool:
        return self._discord_proxy

    @pyqtProperty(str, notify=settingsChanged)
    def themeName(self) -> str:
        return self._theme

    @pyqtProperty(str, notify=settingsChanged)
    def language(self) -> str:
        return self._language

    @pyqtProperty(str, notify=settingsChanged)
    def effectiveLanguage(self) -> str:
        return self._language if self._language in {"ru", "en"} else "en"

    @pyqtProperty(str, notify=settingsChanged)
    def accentColor(self) -> str:
        return self._accent

    @pyqtProperty(bool, notify=settingsChanged)
    def compactMode(self) -> bool:
        try:
            return self.controller.state.settings.interface_mode == "compact"
        except Exception:
            return False

    # ── Network / auto-switch / paths / startup mirrors ──────────
    @pyqtProperty(bool, notify=settingsChanged)
    def proxyBypassLan(self) -> bool:
        try:
            return bool(self.controller.state.settings.system_proxy_bypass_lan)
        except Exception:
            return True

    @pyqtProperty(bool, notify=settingsChanged)
    def reconnectOnNetworkChange(self) -> bool:
        try:
            return bool(self.controller.state.settings.reconnect_on_network_change)
        except Exception:
            return True

    @pyqtProperty(bool, notify=settingsChanged)
    def autoSwitchEnabled(self) -> bool:
        try:
            return bool(self.controller.state.settings.auto_switch_enabled)
        except Exception:
            return True

    @pyqtProperty(int, notify=settingsChanged)
    def autoSwitchThreshold(self) -> int:
        try:
            return int(self.controller.state.settings.auto_switch_threshold_kbps)
        except Exception:
            return 50

    @pyqtProperty(int, notify=settingsChanged)
    def autoSwitchDelay(self) -> int:
        try:
            return int(self.controller.state.settings.auto_switch_delay_sec)
        except Exception:
            return 30

    @pyqtProperty(int, notify=settingsChanged)
    def autoSwitchCooldown(self) -> int:
        try:
            return int(self.controller.state.settings.auto_switch_cooldown_sec)
        except Exception:
            return 60

    @pyqtProperty(str, notify=settingsChanged)
    def xrayPath(self) -> str:
        try:
            from ...constants import XRAY_PATH_DEFAULT
            path = (self.controller.state.settings.xray_path or "").strip()
            return path or str(XRAY_PATH_DEFAULT)
        except Exception:
            return ""

    @pyqtProperty(str, notify=settingsChanged)
    def singboxPath(self) -> str:
        try:
            from ...constants import SINGBOX_PATH_DEFAULT
            return self.controller.state.settings.singbox_path or str(SINGBOX_PATH_DEFAULT)
        except Exception:
            return ""

    @pyqtProperty(bool, notify=settingsChanged)
    def launchOnStartup(self) -> bool:
        try:
            return bool(self.controller.state.settings.launch_on_startup)
        except Exception:
            return False

    @pyqtProperty(bool, notify=settingsChanged)
    def zapretAutostart(self) -> bool:
        try:
            return bool(self.controller.state.settings.zapret_autostart)
        except Exception:
            return False

    # ── Auto-connect / server-test settings ──────────────────────
    @pyqtSlot(bool)
    def setAutoConnectLast(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_connect_last = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtProperty(bool, notify=settingsChanged)
    def autoConnectLast(self) -> bool:
        try:
            return bool(self.controller.state.settings.auto_connect_last)
        except Exception:
            return True

    @pyqtSlot(bool)
    def setAutoConnectOnImport(self, enabled: bool) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.auto_connect_on_import = bool(enabled)
        self.controller.update_settings(settings)

    @pyqtProperty(bool, notify=settingsChanged)
    def autoConnectOnImport(self) -> bool:
        try:
            return bool(self.controller.state.settings.auto_connect_on_import)
        except Exception:
            return False

    @pyqtSlot(str)
    def setPingMethod(self, method: str) -> None:
        value = (method or "tcping").strip().lower()
        if value not in ("tcping", "icmp", "real"):
            value = "tcping"
        settings = deepcopy(self.controller.state.settings)
        settings.ping_method = value
        self.controller.update_settings(settings)

    @pyqtProperty(str, notify=settingsChanged)
    def pingMethod(self) -> str:
        try:
            return self.controller.state.settings.ping_method or "tcping"
        except Exception:
            return "tcping"

    @pyqtSlot(str)
    def setSpeedTestUrl(self, url: str) -> None:
        settings = deepcopy(self.controller.state.settings)
        settings.speed_test_url = (url or "").strip()
        self.controller.update_settings(settings)

    @pyqtProperty(str, notify=settingsChanged)
    def speedTestUrl(self) -> str:
        try:
            return self.controller.state.settings.speed_test_url or ""
        except Exception:
            return ""

    @pyqtSlot(int)
    def setSpeedTestConcurrency(self, value: int) -> None:
        try:
            count = int(value)
        except Exception:
            count = 0
        count = max(0, min(SPEED_TEST_MAX_CONCURRENCY, count))
        settings = deepcopy(self.controller.state.settings)
        settings.speed_test_concurrency = count
        self.controller.update_settings(settings)

    @pyqtProperty(int, notify=settingsChanged)
    def speedTestConcurrency(self) -> int:
        try:
            return int(self.controller.state.settings.speed_test_concurrency)
        except Exception:
            return 0

    @pyqtSlot(int)
    def setSubscriptionAutoUpdateMinutes(self, value: int) -> None:
        try:
            minutes = int(value)
        except Exception:
            minutes = 240
        minutes = max(0, min(1440, minutes))
        settings = deepcopy(self.controller.state.settings)
        settings.subscription_auto_update_minutes = minutes
        self.controller.update_settings(settings)
        self._reconfigure_sub_timer()

    @pyqtProperty(int, notify=settingsChanged)
    def subscriptionAutoUpdateMinutes(self) -> int:
        try:
            return int(self.controller.state.settings.subscription_auto_update_minutes)
        except Exception:
            return 240

    # ── Subscriptions ────────────────────────────────────────────
    @pyqtSlot(str)
    @pyqtSlot(str, str)
    def importSubscription(self, url: str, name: str = "") -> None:
        target = (url or "").strip()
        if not target:
            self.toast.emit("warning", "Введите ссылку на подписку")
            return
        try:
            added, errors = self.controller.import_subscription(target, (name or "").strip() or None)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Ошибка импорта подписки: {exc}")
            return
        if added:
            self.toast.emit("success", f"Импортировано серверов: {added}")
        if errors:
            self.toast.emit("warning", "; ".join(errors[:2]))
        if not added and not errors:
            self.toast.emit("info", "Новых серверов не найдено")

    @pyqtSlot(str)
    def updateSubscription(self, url: str) -> None:
        target = (url or "").strip()
        if not target:
            return
        try:
            added, errors = self.controller.update_subscription(target)
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Ошибка обновления подписки: {exc}")
            return
        self.toast.emit("success", self._localized_backend_message(f"Подписка обновлена: {added} серверов"))
        if errors:
            self.toast.emit("warning", "; ".join(errors[:2]))

    @pyqtSlot()
    def updateAllSubscriptions(self) -> None:
        try:
            added, errors = self.controller.update_all_subscriptions()
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Ошибка обновления подписок: {exc}")
            return
        self.toast.emit("success", self._localized_backend_message(f"Подписки обновлены: {added} серверов"))
        if errors:
            self.toast.emit("warning", "; ".join(errors[:2]))

    @pyqtSlot(str)
    @pyqtSlot(str, bool)
    def removeSubscription(self, url: str, delete_nodes: bool = True) -> None:
        target = (url or "").strip()
        if not target:
            return
        try:
            self.controller.remove_subscription(target, bool(delete_nodes))
        except Exception as exc:  # noqa: BLE001
            self.toast.emit("error", f"Ошибка удаления подписки: {exc}")
            return
        self.toast.emit("info", "Подписка удалена")

    @pyqtProperty("QVariantList", notify=subscriptionsChanged)
    def subscriptions(self) -> list:
        try:
            return [dict(item) for item in self.controller.state.subscriptions]
        except Exception:
            return []

    # ── Security / data mirrors ──────────────────────────────────
    @pyqtProperty(int, notify=settingsChanged)
    def autoLockMinutes(self) -> int:
        try:
            return int(self.controller.state.security.auto_lock_minutes)
        except Exception:
            return 15

    @pyqtProperty(bool, notify=settingsChanged)
    def masterPasswordEnabled(self) -> bool:
        try:
            return bool(self.controller.state.security.enabled)
        except Exception:
            return False

    @pyqtProperty(bool, notify=lockedChanged)
    def locked(self) -> bool:
        try:
            return bool(self.controller.locked)
        except Exception:
            return False

    @pyqtSlot(str, result=bool)
    def unlock(self, password: str) -> bool:
        try:
            return bool(self.controller.unlock(password or ""))
        except Exception:
            return False

    @pyqtProperty(bool, notify=settingsChanged)
    def encryptionActive(self) -> bool:
        try:
            return bool(self.controller.is_data_encrypted())
        except Exception:
            return False

    @pyqtProperty(bool, notify=routingChanged)
    def bypassLan(self) -> bool:
        return bool(self.controller.state.routing.bypass_lan)

    @pyqtProperty(str, notify=routingChanged)
    def dnsMode(self) -> str:
        return self.controller.state.routing.dns_mode

    @pyqtProperty(str, notify=routingChanged)
    def dnsBootstrapServer(self) -> str:
        return self.controller.state.routing.dns_bootstrap_server

    @pyqtProperty(str, notify=routingChanged)
    def dnsBootstrapType(self) -> str:
        return self.controller.state.routing.dns_bootstrap_type

    @pyqtProperty(str, notify=routingChanged)
    def dnsProxyServer(self) -> str:
        return self.controller.state.routing.dns_proxy_server

    @pyqtProperty(str, notify=routingChanged)
    def dnsProxyType(self) -> str:
        return self.controller.state.routing.dns_proxy_type

    @pyqtProperty(str, notify=routingChanged)
    def tunDefaultOutbound(self) -> str:
        return self.controller.state.routing.tun_default_outbound

    @pyqtProperty('QVariantList', notify=routingChanged)
    def processRules(self):
        return [dict(x) for x in self.controller.state.routing.process_rules]

    @pyqtProperty('QVariantList', notify=routingChanged)
    def domainRules(self):
        routing = self.controller.state.routing
        out: list[dict[str, str]] = []
        for addr in routing.direct_domains:
            out.append({"addr": addr, "action": "direct"})
        for addr in routing.proxy_domains:
            out.append({"addr": addr, "action": "proxy"})
        for addr in routing.block_domains:
            out.append({"addr": addr, "action": "block"})
        return out

    @pyqtProperty('QVariantList', notify=routingChanged)
    def serviceList(self):
        from ...service_presets import SERVICE_PRESETS
        routes = self.controller.state.routing.service_routes
        return [
            {
                "id": s.id,
                "name": s.name,
                "description": s.description,
                "defaultAction": s.default_action,
                "action": routes.get(s.id, "off"),
            }
            for s in SERVICE_PRESETS
        ]

    @pyqtProperty('QVariantList', notify=routingChanged)
    def processPresetList(self):
        from ...process_presets import PROCESS_PRESETS
        routes = self.controller.state.routing.process_preset_routes
        return [
            {
                "id": p.id,
                "name": p.name,
                "description": p.description,
                "defaultAction": p.default_action,
                "action": routes.get(p.id, "off"),
            }
            for p in PROCESS_PRESETS
        ]

    # ── Routing detail: commands ─────────────────────────────────
    def _mutate_routing(self, fn) -> None:
        routing = deepcopy(self.controller.state.routing)
        fn(routing)
        self.controller.update_routing(routing)

    @pyqtSlot(bool)
    def setBypassLan(self, enabled: bool) -> None:
        def apply(r: RoutingSettings) -> None:
            r.bypass_lan = bool(enabled)
        self._mutate_routing(apply)

    @pyqtSlot(str)
    def setDnsMode(self, mode: str) -> None:
        def apply(r: RoutingSettings) -> None:
            r.dns_mode = mode or "system"
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def setBootstrapDns(self, server: str, dns_type: str) -> None:
        def apply(r: RoutingSettings) -> None:
            if server:
                r.dns_bootstrap_server = server.strip()
            if dns_type:
                r.dns_bootstrap_type = dns_type
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def setProxyDns(self, server: str, dns_type: str) -> None:
        def apply(r: RoutingSettings) -> None:
            if server:
                r.dns_proxy_server = server.strip()
            if dns_type:
                r.dns_proxy_type = dns_type
        self._mutate_routing(apply)

    @pyqtSlot(str)
    def setTunDefaultOutbound(self, value: str) -> None:
        def apply(r: RoutingSettings) -> None:
            r.tun_default_outbound = value if value in ("proxy", "direct") else "direct"
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def setServiceRoute(self, service_id: str, action: str) -> None:
        def apply(r: RoutingSettings) -> None:
            routes = dict(r.service_routes)
            if not action or action == "off":
                routes.pop(service_id, None)
            else:
                routes[service_id] = action
            r.service_routes = routes
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def setProcessPresetRoute(self, preset_id: str, action: str) -> None:
        def apply(r: RoutingSettings) -> None:
            routes = dict(r.process_preset_routes)
            if not action or action == "off":
                routes.pop(preset_id, None)
            else:
                routes[preset_id] = action
            r.process_preset_routes = routes
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def addProcessRule(self, process: str, action: str) -> None:
        process = (process or "").strip()
        if not process:
            return
        def apply(r: RoutingSettings) -> None:
            rules = [dict(x) for x in r.process_rules if x.get("process") != process]
            rules.append({"process": process, "action": action or "proxy"})
            r.process_rules = rules
        self._mutate_routing(apply)

    @pyqtSlot(str)
    def removeProcessRule(self, process: str) -> None:
        def apply(r: RoutingSettings) -> None:
            r.process_rules = [dict(x) for x in r.process_rules if x.get("process") != process]
        self._mutate_routing(apply)

    @pyqtSlot(str, str)
    def addDomainRule(self, addr: str, action: str) -> None:
        addr = (addr or "").strip()
        if not addr:
            return
        def apply(r: RoutingSettings) -> None:
            r.direct_domains = [d for d in r.direct_domains if d != addr]
            r.proxy_domains = [d for d in r.proxy_domains if d != addr]
            r.block_domains = [d for d in r.block_domains if d != addr]
            if action == "direct":
                r.direct_domains = list(r.direct_domains) + [addr]
            elif action == "block":
                r.block_domains = list(r.block_domains) + [addr]
            else:
                r.proxy_domains = list(r.proxy_domains) + [addr]
        self._mutate_routing(apply)

    @pyqtSlot(str)
    def removeDomainRule(self, addr: str) -> None:
        def apply(r: RoutingSettings) -> None:
            r.direct_domains = [d for d in r.direct_domains if d != addr]
            r.proxy_domains = [d for d in r.proxy_domains if d != addr]
            r.block_domains = [d for d in r.block_domains if d != addr]
        self._mutate_routing(apply)

    @pyqtSlot(str)
    def importDomainRules(self, text: str) -> None:
        lines = [ln.strip() for ln in (text or "").splitlines() if ln.strip()]
        if not lines:
            self.toast.emit("warning", "Нет строк для импорта")
            return
        def apply(r: RoutingSettings) -> None:
            direct = list(r.direct_domains)
            proxy = list(r.proxy_domains)
            block = list(r.block_domains)
            for ln in lines:
                if "|" in ln:
                    addr, _, act = ln.partition("|")
                    addr = addr.strip()
                    act = act.strip().lower()
                else:
                    addr = ln
                    act = "proxy"
                if not addr:
                    continue
                for lst in (direct, proxy, block):
                    if addr in lst:
                        lst.remove(addr)
                if act == "direct":
                    direct.append(addr)
                elif act == "block":
                    block.append(addr)
                else:
                    proxy.append(addr)
            r.direct_domains = direct
            r.proxy_domains = proxy
            r.block_domains = block
        self._mutate_routing(apply)
        self.toast.emit("success", f"Импортировано правил: {len(lines)}")

    @pyqtSlot(result=str)
    def exportDomainRules(self) -> str:
        routing = self.controller.state.routing
        lines: list[str] = []
        for addr in routing.direct_domains:
            lines.append(f"{addr}|direct")
        for addr in routing.proxy_domains:
            lines.append(f"{addr}|proxy")
        for addr in routing.block_domains:
            lines.append(f"{addr}|block")
        payload = "\n".join(lines)
        if not payload:
            self.toast.emit("warning", "Нет правил для экспорта")
            return ""
        clipboard = QGuiApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(payload)
            self.toast.emit("success", "Правила скопированы в буфер обмена")
        return payload
