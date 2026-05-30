from __future__ import annotations

import ntpath
import os
import re
from pathlib import Path

from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtWidgets import (
    QAbstractItemView,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    FluentIcon as FIF,
    MessageBox,
    PrimaryToolButton,
    PrimaryPushButton,
    PushButton,
    SettingCard,
    SettingCardGroup,
    SmoothScrollArea,
    SubtitleLabel,
    SwitchButton,
    TableWidget,
    TransparentToolButton,
)

from ..models import RoutingSettings
from ..process_presets import PROCESS_PRESETS
from ..routing_presets import (
    ROUTING_PRESET_BLOCKED,
    ROUTING_PRESET_EXCEPT_RU,
    ROUTING_PRESET_GLOBAL,
    build_routing_preset,
)
from ..service_presets import SERVICE_PRESETS
from .table_scroll import tune_fluent_table_scroll, tune_plain_scroll_area

_ACTIONS = [
    ("Прямой", "direct"),
    ("Прокси", "proxy"),
    ("Блокировка", "block"),
]
_ACTION_LABELS = {data: label for label, data in _ACTIONS}
_ACTION_DATA = {label: data for label, data in _ACTIONS}

_SERVICE_ACTIONS = [
    ("Прокси", "proxy"),
    ("Прямой", "direct"),
    ("Блокировка", "block"),
]

_PROCESS_MATCH_NAME = "name"
_PROCESS_MATCH_PATH = "path"
_PROCESS_MATCH_PATH_REGEX = "path_regex"
_PROCESS_MATCH_ROLE = int(Qt.ItemDataRole.UserRole)
_PROCESS_VALUE_ROLE = _PROCESS_MATCH_ROLE + 1
_PROCESS_LABEL_ROLE = _PROCESS_MATCH_ROLE + 2


class _ServiceRouteCard(SettingCard):
    """Setting card with action combo + switch for service routing."""

    changed = pyqtSignal()

    def __init__(self, icon, title, content, parent=None):
        super().__init__(icon, title, content, parent)
        self.action_combo = ComboBox(self)
        for label, data in _SERVICE_ACTIONS:
            self.action_combo.addItem(label, userData=data)
        self.action_combo.setMinimumWidth(120)

        self.switch = SwitchButton(self)
        self.switch.setOnText("Вкл")
        self.switch.setOffText("Выкл")

        self.hBoxLayout.addWidget(self.action_combo, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(12)
        self.hBoxLayout.addWidget(self.switch, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

        self.switch.checkedChanged.connect(self._on_changed)
        self.action_combo.currentIndexChanged.connect(self._on_changed)

    def _on_changed(self):
        self.action_combo.setEnabled(self.switch.isChecked())
        self.changed.emit()

    def set_state(self, enabled: bool, action: str = "proxy"):
        self.switch.setChecked(enabled)
        for i in range(self.action_combo.count()):
            if self.action_combo.itemData(i) == action:
                self.action_combo.setCurrentIndex(i)
                break
        self.action_combo.setEnabled(enabled)

    def get_state(self) -> tuple[bool, str]:
        return self.switch.isChecked(), self.action_combo.currentData() or "proxy"


class RoutingPage(QWidget):
    apply_requested = pyqtSignal(object)  # emits RoutingSettings

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setObjectName("routing")
        self._loading = False
        self._compact_mode = False
        self._tun_mode = False
        self._apply_pending = False
        self._apply_timer = QTimer(self)
        self._apply_timer.setSingleShot(True)
        self._apply_timer.setInterval(1500)
        self._apply_timer.timeout.connect(self._emit_apply)

        # --- Outer layout with scroll area ---
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        self._scroll = SmoothScrollArea(self)
        self._scroll.setWidgetResizable(True)
        self._scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self._scroll.setStyleSheet("QScrollArea { background: transparent; border: none; }")
        tune_plain_scroll_area(self._scroll)
        outer.addWidget(self._scroll)

        container = QWidget()
        container.setStyleSheet("QWidget { background: transparent; }")
        self._scroll.setWidget(container)

        root = QVBoxLayout(container)
        root.setContentsMargins(24, 20, 24, 20)
        root.setSpacing(12)

        root.addWidget(SubtitleLabel("Маршрутизация", container))

        apply_row = QHBoxLayout()
        self._apply_pending_label = CaptionLabel("Есть неприменённые изменения маршрутизации", container)
        self._apply_pending_label.setVisible(False)
        apply_row.addWidget(self._apply_pending_label)
        apply_row.addStretch(1)
        self.help_btn = PushButton(FIF.INFO, "Справка", container)
        apply_row.addWidget(self.help_btn)
        self.apply_routing_btn = PrimaryPushButton("Применить", container)
        self.apply_routing_btn.setEnabled(False)
        apply_row.addWidget(self.apply_routing_btn)
        root.addLayout(apply_row)

        # --- Header: mode, DNS, bypass LAN ---
        header = QGridLayout()
        header.setHorizontalSpacing(12)
        header.setVerticalSpacing(8)

        self._mode_label = BodyLabel("Поведение", container)
        header.addWidget(self._mode_label, 0, 0)
        self.mode_combo = ComboBox(container)
        self.mode_combo.addItem("Всё через VPN", userData="global")
        self.mode_combo.addItem("По моим правилам", userData="rule")
        self.mode_combo.addItem("Без VPN по умолчанию", userData="direct")
        header.addWidget(self.mode_combo, 0, 1)

        self._dns_label = BodyLabel("DNS", container)
        header.addWidget(self._dns_label, 1, 0)
        self.dns_combo = ComboBox(container)
        self.dns_combo.addItem("Системный DNS", userData="system")
        self.dns_combo.addItem("Встроенный DNS", userData="builtin")
        header.addWidget(self.dns_combo, 1, 1)

        self.bypass_switch = SwitchButton("Обход локальной сети", container)
        header.addWidget(self.bypass_switch, 2, 0, 1, 2)

        # --- TUN DNS settings (visible only in TUN mode) ---
        self._dns_tun_widget = QWidget(container)
        dns_grid = QGridLayout(self._dns_tun_widget)
        dns_grid.setContentsMargins(0, 4, 0, 0)
        dns_grid.setHorizontalSpacing(8)
        dns_grid.setVerticalSpacing(6)

        dns_grid.addWidget(CaptionLabel("Bootstrap DNS (direct):", container), 0, 0)
        self._dns_bootstrap_server = ComboBox(container)
        for label, ip in [("Cloudflare 1.1.1.1", "1.1.1.1"), ("Google 8.8.8.8", "8.8.8.8"),
                          ("Quad9 9.9.9.9", "9.9.9.9"), ("Яндекс 77.88.8.8", "77.88.8.8"),
                          ("OpenDNS 208.67.222.222", "208.67.222.222")]:
            self._dns_bootstrap_server.addItem(label, userData=ip)
        self._dns_bootstrap_server.setMinimumWidth(180)
        dns_grid.addWidget(self._dns_bootstrap_server, 0, 1)

        self._dns_bootstrap_type = ComboBox(container)
        for label, val in [("UDP", "udp"), ("TCP", "tcp"), ("DoT (TLS)", "tls"), ("DoH (HTTPS)", "https")]:
            self._dns_bootstrap_type.addItem(label, userData=val)
        dns_grid.addWidget(self._dns_bootstrap_type, 0, 2)

        dns_grid.addWidget(CaptionLabel("Proxy DNS (VPN):", container), 1, 0)
        self._dns_proxy_server = ComboBox(container)
        for label, ip in [("Google 8.8.8.8", "8.8.8.8"), ("Cloudflare 1.1.1.1", "1.1.1.1"),
                          ("Quad9 9.9.9.9", "9.9.9.9"), ("OpenDNS 208.67.222.222", "208.67.222.222")]:
            self._dns_proxy_server.addItem(label, userData=ip)
        self._dns_proxy_server.setMinimumWidth(180)
        dns_grid.addWidget(self._dns_proxy_server, 1, 1)

        self._dns_proxy_type = ComboBox(container)
        for label, val in [("TCP", "tcp"), ("DoT (TLS)", "tls"), ("DoH (HTTPS)", "https")]:
            self._dns_proxy_type.addItem(label, userData=val)
        dns_grid.addWidget(self._dns_proxy_type, 1, 2)

        self._dns_tun_widget.setVisible(False)
        header.addWidget(self._dns_tun_widget, 3, 0, 1, 2)

        root.addLayout(header)

        self._priority_info = CaptionLabel(
            "Сначала применяются приложения и папки, потом сервисы, потом домены и IP.",
            container,
        )
        root.addWidget(self._priority_info)

        preset_row = QHBoxLayout()
        preset_row.setSpacing(8)
        self.preset_global_btn = PushButton(FIF.GLOBE, "Всё через VPN", container)
        self.preset_global_btn.setToolTip("Весь трафик через выбранный сервер")
        preset_row.addWidget(self.preset_global_btn)

        self.preset_blocked_btn = PushButton(FIF.LINK, "Только заблокированное", container)
        self.preset_blocked_btn.setToolTip("Только заблокированные и выбранные сервисы через сервер, остальное напрямую")
        preset_row.addWidget(self.preset_blocked_btn)

        self.preset_except_ru_btn = PushButton(FIF.HOME, "Всё кроме РФ", container)
        self.preset_except_ru_btn.setToolTip("Российские сайты и IP напрямую, остальное через сервер")
        preset_row.addWidget(self.preset_except_ru_btn)
        preset_row.addStretch(1)
        root.addLayout(preset_row)

        # --- Process routing section ---
        root.addWidget(SubtitleLabel("Приложения", container))

        self.process_info = CaptionLabel(
            "Добавьте приложение или его папку, если нужно отправлять его через VPN или, наоборот, оставить напрямую.",
            container,
        )
        root.addWidget(self.process_info)

        # TUN default outbound selector
        tun_default_row = QHBoxLayout()
        self._tun_default_label = BodyLabel("По умолчанию (TUN):", container)
        tun_default_row.addWidget(self._tun_default_label)
        self.tun_default_combo = ComboBox(container)
        self.tun_default_combo.addItem("Через прокси", userData="proxy")
        self.tun_default_combo.addItem("Напрямую", userData="direct")
        self.tun_default_combo.setMinimumWidth(160)
        tun_default_row.addWidget(self.tun_default_combo)
        tun_default_row.addStretch(1)
        self._tun_default_row_widget = QWidget(container)
        self._tun_default_row_widget.setLayout(tun_default_row)
        root.addWidget(self._tun_default_row_widget)

        self.tun_default_info = CaptionLabel(
            "Что делать с трафиком процессов, не указанных в таблице ниже. "
            "«Через прокси» — весь трафик через VPN, исключения идут напрямую. "
            "«Напрямую» — только указанные процессы идут через VPN.",
            container,
        )
        root.addWidget(self.tun_default_info)

        # Hidden by default — shown only in TUN mode
        self._tun_default_row_widget.setVisible(False)
        self.tun_default_info.setVisible(False)

        # --- Process presets (quick-add app groups) ---
        self._process_presets_group = SettingCardGroup("Быстрый выбор приложений", container)
        self._process_preset_cards: dict[str, _ServiceRouteCard] = {}
        for preset in PROCESS_PRESETS:
            card = _ServiceRouteCard(
                preset.icon,
                preset.name,
                preset.description,
                parent=self._process_presets_group,
            )
            card.changed.connect(self._schedule_apply)
            self._process_presets_group.addSettingCard(card)
            self._process_preset_cards[preset.id] = card
        root.addWidget(self._process_presets_group)
        self._process_presets_group.setVisible(False)  # shown only in TUN mode

        self._process_container = QWidget(container)
        proc_layout = QVBoxLayout(self._process_container)
        proc_layout.setContentsMargins(0, 0, 0, 0)
        proc_layout.setSpacing(8)

        proc_toolbar = QHBoxLayout()
        self.add_proc_btn = PrimaryToolButton(FIF.FOLDER_ADD, container)
        self.add_proc_btn.setToolTip("Добавить точный .exe")
        proc_toolbar.addWidget(self.add_proc_btn)

        self.add_proc_folder_btn = TransparentToolButton(FIF.FOLDER, container)
        self.add_proc_folder_btn.setToolTip("Добавить папку приложения")
        proc_toolbar.addWidget(self.add_proc_folder_btn)

        self.del_proc_btn = TransparentToolButton(FIF.DELETE, container)
        self.del_proc_btn.setToolTip("Удалить выбранные")
        proc_toolbar.addWidget(self.del_proc_btn)

        proc_toolbar.addStretch(1)
        proc_layout.addLayout(proc_toolbar)

        self.proc_table = TableWidget(self._process_container)
        self.proc_table.setColumnCount(2)
        self.proc_table.setHorizontalHeaderLabels(["Приложение / папка", "Действие"])
        self.proc_table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self.proc_table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        self.proc_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.proc_table.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.proc_table.verticalHeader().setVisible(False)
        tune_fluent_table_scroll(self.proc_table, disable_hover=True)
        self.proc_table.setMinimumHeight(120)
        proc_layout.addWidget(self.proc_table)

        root.addWidget(self._process_container)

        self.proxy_warning = CaptionLabel(
            "В системном прокси полный матч по пути/папке недоступен: точный эффект есть только в TUN (sing-box).",
            container,
        )
        self.proxy_warning.setStyleSheet("color: #e6a700;")
        self.proxy_warning.setVisible(False)
        root.addWidget(self.proxy_warning)

        # --- Services section ---
        self._services_group = SettingCardGroup("Сервисы", container)
        self._service_cards: dict[str, _ServiceRouteCard] = {}

        for preset in SERVICE_PRESETS:
            card = _ServiceRouteCard(
                preset.icon,
                preset.name,
                preset.description,
                parent=self._services_group,
            )
            card.changed.connect(self._schedule_apply)
            self._services_group.addSettingCard(card)
            self._service_cards[preset.id] = card

        root.addWidget(self._services_group)

        # --- Rules table ---
        root.addWidget(SubtitleLabel("Домены и IP", container))

        rules_toolbar = QHBoxLayout()
        self.add_rule_btn = PrimaryToolButton(FIF.ADD, container)
        self.add_rule_btn.setToolTip("Добавить правило")
        rules_toolbar.addWidget(self.add_rule_btn)

        self.del_rule_btn = TransparentToolButton(FIF.DELETE, container)
        self.del_rule_btn.setToolTip("Удалить выбранные")
        rules_toolbar.addWidget(self.del_rule_btn)

        rules_toolbar.addSpacing(16)

        self.import_btn = TransparentToolButton(FIF.DOWNLOAD, container)
        self.import_btn.setToolTip("Импорт из файла")
        rules_toolbar.addWidget(self.import_btn)

        self.export_btn = TransparentToolButton(FIF.SHARE, container)
        self.export_btn.setToolTip("Экспорт в файл")
        rules_toolbar.addWidget(self.export_btn)

        rules_toolbar.addStretch(1)
        root.addLayout(rules_toolbar)

        self.rules_table = TableWidget(container)
        self.rules_table.setColumnCount(2)
        self.rules_table.setHorizontalHeaderLabels(["Адрес", "Действие"])
        self.rules_table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self.rules_table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        self.rules_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.rules_table.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.rules_table.verticalHeader().setVisible(False)
        tune_fluent_table_scroll(self.rules_table, disable_hover=True)
        self.rules_table.setMinimumHeight(180)
        root.addWidget(self.rules_table)

        root.addStretch(1)

        # --- Signals ---
        self.mode_combo.currentIndexChanged.connect(self._schedule_apply)
        self.dns_combo.currentIndexChanged.connect(self._schedule_apply)
        self._dns_bootstrap_server.currentIndexChanged.connect(self._schedule_apply)
        self._dns_bootstrap_type.currentIndexChanged.connect(self._schedule_apply)
        self._dns_proxy_server.currentIndexChanged.connect(self._schedule_apply)
        self._dns_proxy_type.currentIndexChanged.connect(self._schedule_apply)
        self.bypass_switch.checkedChanged.connect(self._schedule_apply)
        self.tun_default_combo.currentIndexChanged.connect(self._schedule_apply)
        self.add_rule_btn.clicked.connect(self._on_add_rule)
        self.del_rule_btn.clicked.connect(self._on_del_rules)
        self.import_btn.clicked.connect(self._on_import_rules)
        self.export_btn.clicked.connect(self._export_rules)
        self.add_proc_btn.clicked.connect(self._on_browse_exe)
        self.add_proc_folder_btn.clicked.connect(self._on_browse_process_folder)
        self.del_proc_btn.clicked.connect(self._on_del_procs)
        self.apply_routing_btn.clicked.connect(self._emit_apply)
        self.help_btn.clicked.connect(self._show_help)
        self.preset_global_btn.clicked.connect(self._apply_global_preset)
        self.preset_blocked_btn.clicked.connect(self._apply_blocked_preset)
        self.preset_except_ru_btn.clicked.connect(self._apply_except_ru_preset)
        self.rules_table.cellChanged.connect(self._schedule_apply)

    # --- Auto-apply ---

    def _schedule_apply(self) -> None:
        if not self._loading:
            self._set_apply_pending(True)
            self._apply_timer.start()

    # --- Public API ---

    def set_routing(self, routing: RoutingSettings) -> None:
        self._loading = True
        self._select_combo_value(self.mode_combo, routing.mode)
        self._select_combo_value(self.dns_combo, routing.dns_mode)
        self._select_combo_value(self._dns_bootstrap_server, routing.dns_bootstrap_server)
        self._select_combo_value(self._dns_bootstrap_type, routing.dns_bootstrap_type)
        self._select_combo_value(self._dns_proxy_server, routing.dns_proxy_server)
        self._select_combo_value(self._dns_proxy_type, routing.dns_proxy_type)
        self.bypass_switch.setChecked(routing.bypass_lan)
        self._select_combo_value(self.tun_default_combo, routing.tun_default_outbound)

        for svc_id, card in self._service_cards.items():
            if svc_id in routing.service_routes:
                card.set_state(True, routing.service_routes[svc_id])
            else:
                card.set_state(False, "proxy")

        for preset_id, card in self._process_preset_cards.items():
            if preset_id in routing.process_preset_routes:
                card.set_state(True, routing.process_preset_routes[preset_id])
            else:
                card.set_state(False, "proxy")

        rows: list[tuple[str, str]] = []
        for addr in routing.direct_domains:
            rows.append((addr, "direct"))
        for addr in routing.proxy_domains:
            rows.append((addr, "proxy"))
        for addr in routing.block_domains:
            rows.append((addr, "block"))
        rows.sort(key=lambda r: r[0].lower())

        self.rules_table.setUpdatesEnabled(False)
        self.rules_table.setRowCount(0)
        for addr, action in rows:
            self._add_rule_row(addr, action)
        self.rules_table.setUpdatesEnabled(True)

        self.proc_table.setUpdatesEnabled(False)
        self.proc_table.setRowCount(0)
        for pr in routing.process_rules:
            value = pr.get("process", "")
            action = pr.get("action", "proxy")
            match = self._normalize_process_match(pr.get("match", ""), value)
            label = pr.get("label", "")
            if value:
                self._add_process_row(value, action, match=match, label=label)
        self.proc_table.setUpdatesEnabled(True)

        self._loading = False
        self._set_apply_pending(False)

    def set_tun_mode(self, enabled: bool) -> None:
        self._tun_mode = bool(enabled)
        # Process routing works in both modes — show warning only in system proxy mode
        self._process_container.setEnabled(True)
        self.add_proc_btn.setEnabled(True)
        self.add_proc_folder_btn.setEnabled(True)
        self.del_proc_btn.setEnabled(True)
        self.proxy_warning.setVisible((not enabled) and (not self._compact_mode))
        # TUN default outbound + process presets + DNS settings only relevant in TUN mode
        self._tun_default_row_widget.setVisible(enabled and not self._compact_mode)
        self._dns_tun_widget.setVisible(enabled and not self._compact_mode)
        self.tun_default_info.setVisible(enabled and not self._compact_mode)
        self._process_presets_group.setVisible(enabled)

    def set_compact_mode(self, enabled: bool) -> None:
        self._compact_mode = bool(enabled)
        self._dns_label.setVisible(not self._compact_mode)
        self.dns_combo.setVisible(not self._compact_mode)
        self.bypass_switch.setVisible(not self._compact_mode)
        self._priority_info.setVisible(not self._compact_mode)
        self.process_info.setVisible(not self._compact_mode)
        self.import_btn.setVisible(not self._compact_mode)
        self.export_btn.setVisible(not self._compact_mode)
        self.proxy_warning.setVisible((not self._tun_mode) and (not self._compact_mode))
        self._tun_default_row_widget.setVisible(self._tun_mode and not self._compact_mode)
        self._dns_tun_widget.setVisible(self._tun_mode and not self._compact_mode)
        self.tun_default_info.setVisible(self._tun_mode and not self._compact_mode)

    # --- Built-in routing presets ---

    def _set_mode(self, mode: str) -> None:
        self._select_combo_value(self.mode_combo, mode)

    def _set_rules(self, direct: tuple[str, ...] = (), proxy: tuple[str, ...] = (), block: tuple[str, ...] = ()) -> None:
        self.rules_table.setUpdatesEnabled(False)
        self.rules_table.blockSignals(True)
        self.rules_table.setRowCount(0)
        for addr in direct:
            self._add_rule_row(addr, "direct")
        for addr in proxy:
            self._add_rule_row(addr, "proxy")
        for addr in block:
            self._add_rule_row(addr, "block")
        self.rules_table.blockSignals(False)
        self.rules_table.setUpdatesEnabled(True)

    def _set_services(self, enabled: bool, *, only_proxy_defaults: bool = False) -> None:
        for svc_id, card in self._service_cards.items():
            preset = next((p for p in SERVICE_PRESETS if p.id == svc_id), None)
            if not preset:
                card.set_state(False, "proxy")
                continue
            if only_proxy_defaults and preset.default_action != "proxy":
                card.set_state(False, "proxy")
            else:
                card.set_state(enabled, preset.default_action)

    def _set_process_presets(self, enabled: bool) -> None:
        for preset_id, card in self._process_preset_cards.items():
            preset = next((p for p in PROCESS_PRESETS if p.id == preset_id), None)
            card.set_state(enabled and preset is not None, preset.default_action if preset else "proxy")

    def _apply_preset(self, preset_id: str) -> None:
        routing = build_routing_preset(self._collect_current_routing(), preset_id)
        self.set_routing(routing)
        self.apply_requested.emit(routing)

    def _apply_global_preset(self) -> None:
        self._apply_preset(ROUTING_PRESET_GLOBAL)

    def _apply_blocked_preset(self) -> None:
        self._apply_preset(ROUTING_PRESET_BLOCKED)

    def _apply_except_ru_preset(self) -> None:
        self._apply_preset(ROUTING_PRESET_EXCEPT_RU)

    # --- Rules table helpers ---

    def _add_rule_row(self, addr: str = "", action: str = "proxy") -> None:
        row = self.rules_table.rowCount()
        self.rules_table.insertRow(row)

        addr_item = QTableWidgetItem(addr)
        addr_item.setFlags(addr_item.flags() | Qt.ItemFlag.ItemIsEditable)
        self.rules_table.setItem(row, 0, addr_item)

        combo = ComboBox()
        for label, data in _ACTIONS:
            combo.addItem(label, userData=data)
        self._select_combo_value(combo, action)
        combo.currentIndexChanged.connect(self._schedule_apply)
        self.rules_table.setCellWidget(row, 1, combo)

    def _on_add_rule(self) -> None:
        self._add_rule_row()

    def _on_del_rules(self) -> None:
        rows = sorted({idx.row() for idx in self.rules_table.selectedIndexes()}, reverse=True)
        for r in rows:
            self.rules_table.removeRow(r)
        if rows:
            self._schedule_apply()

    def _on_import_rules(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Импорт правил", "", "Text files (*.txt);;All files (*)"
        )
        if not path:
            return
        try:
            text = Path(path).read_text(encoding="utf-8")
        except Exception:
            return
        added = False
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "|" in line:
                addr, action = line.rsplit("|", 1)
                action = action.strip().lower()
                if action not in ("direct", "proxy", "block"):
                    action = "proxy"
            else:
                addr = line
                action = "proxy"
            self._add_rule_row(addr.strip(), action)
            added = True
        if added:
            self._schedule_apply()

    def _export_rules(self) -> None:
        path, _ = QFileDialog.getSaveFileName(
            self, "Экспорт правил", "rules.txt", "Text files (*.txt);;All files (*)"
        )
        if not path:
            return
        lines: list[str] = []
        for row in range(self.rules_table.rowCount()):
            item = self.rules_table.item(row, 0)
            combo = self.rules_table.cellWidget(row, 1)
            if item and combo:
                addr = item.text().strip()
                action = combo.currentData() or "proxy"
                if addr:
                    lines.append(f"{addr}|{action}")
        Path(path).write_text("\n".join(lines), encoding="utf-8")

    # --- Process table helpers ---

    @staticmethod
    def _normalize_process_match(match: str, value: str) -> str:
        normalized = str(match or "").strip().lower()
        if normalized in {_PROCESS_MATCH_NAME, _PROCESS_MATCH_PATH, _PROCESS_MATCH_PATH_REGEX}:
            return normalized
        lowered = str(value or "").strip().lower()
        if lowered.startswith("regex:"):
            return _PROCESS_MATCH_PATH_REGEX
        if "\\\\" in value or "\\" in value or "/" in value or (len(value) > 1 and value[1] == ":"):
            return _PROCESS_MATCH_PATH
        return _PROCESS_MATCH_NAME

    @staticmethod
    def _normalize_windows_path(path: str) -> str:
        normalized = os.path.normpath(path.strip())
        return normalized.replace("/", "\\")

    @classmethod
    def _format_process_label(cls, match: str, value: str, label: str = "") -> str:
        if label:
            return label
        if match == _PROCESS_MATCH_PATH_REGEX:
            return f"Папка: {value}"
        return value

    @staticmethod
    def _build_folder_process_regex(folder_path: str) -> tuple[str, str]:
        folder = RoutingPage._normalize_windows_path(folder_path).rstrip("\\")
        escaped = re.escape(folder)
        regex = rf"(?i)^{escaped}\\.*\.exe$"
        label = f"Папка: {folder}\\*.exe"
        return regex, label

    def _find_process_rule_row(self, match: str, value: str) -> int:
        for row in range(self.proc_table.rowCount()):
            item = self.proc_table.item(row, 0)
            if not item:
                continue
            existing_match = str(item.data(_PROCESS_MATCH_ROLE) or _PROCESS_MATCH_NAME)
            existing_value = str(item.data(_PROCESS_VALUE_ROLE) or item.text())
            if existing_match == match and existing_value.lower() == value.lower():
                return row
        return -1

    def _add_process_row(
        self,
        value: str = "",
        action: str = "proxy",
        *,
        match: str = _PROCESS_MATCH_NAME,
        label: str = "",
    ) -> None:
        row = self.proc_table.rowCount()
        self.proc_table.insertRow(row)

        process_item = QTableWidgetItem(self._format_process_label(match, value, label))
        process_item.setFlags(process_item.flags() & ~Qt.ItemFlag.ItemIsEditable)
        process_item.setData(_PROCESS_MATCH_ROLE, match)
        process_item.setData(_PROCESS_VALUE_ROLE, value)
        process_item.setData(_PROCESS_LABEL_ROLE, label)
        self.proc_table.setItem(row, 0, process_item)

        combo = ComboBox()
        for label, data in _ACTIONS:
            combo.addItem(label, userData=data)
        self._select_combo_value(combo, action)
        combo.currentIndexChanged.connect(self._schedule_apply)
        self.proc_table.setCellWidget(row, 1, combo)

    _PROTECTED_PROCESSES = {"xray.exe", "sing-box.exe", "tun2socks.exe"}

    def _on_browse_exe(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Выбрать приложение", "", "Executables (*.exe)"
        )
        if not path:
            return
        normalized_path = self._normalize_windows_path(path)
        name = ntpath.basename(normalized_path)
        if name.lower() in self._PROTECTED_PROCESSES:
            from qfluentwidgets import InfoBar, InfoBarPosition
            InfoBar.warning(
                title="Защищённый процесс",
                content=f"{name} всегда использует прямое подключение для предотвращения петли маршрутизации",
                parent=self,
                duration=4000,
                position=InfoBarPosition.TOP,
            )
            return
        if self._find_process_rule_row(_PROCESS_MATCH_PATH, normalized_path) >= 0:
            return
        self._add_process_row(normalized_path, "proxy", match=_PROCESS_MATCH_PATH)
        self._schedule_apply()

    def _on_browse_process_folder(self) -> None:
        folder = QFileDialog.getExistingDirectory(self, "Выбрать папку приложения")
        if not folder:
            return
        regex, label = self._build_folder_process_regex(folder)
        if self._find_process_rule_row(_PROCESS_MATCH_PATH_REGEX, regex) >= 0:
            return
        self._add_process_row(regex, "proxy", match=_PROCESS_MATCH_PATH_REGEX, label=label)
        self._schedule_apply()

    def _on_del_procs(self) -> None:
        rows = sorted({idx.row() for idx in self.proc_table.selectedIndexes()}, reverse=True)
        for r in rows:
            self.proc_table.removeRow(r)
        if rows:
            self._schedule_apply()

    # --- Emit ---

    def _collect_current_routing(self) -> RoutingSettings:
        mode = self.mode_combo.currentData() or "rule"
        dns_mode = self.dns_combo.currentData() or "system"
        dns_bootstrap_server = self._dns_bootstrap_server.currentData() or "1.1.1.1"
        dns_bootstrap_type = self._dns_bootstrap_type.currentData() or "udp"
        dns_proxy_server = self._dns_proxy_server.currentData() or "8.8.8.8"
        dns_proxy_type = self._dns_proxy_type.currentData() or "tcp"

        direct: list[str] = []
        proxy: list[str] = []
        block: list[str] = []

        for row in range(self.rules_table.rowCount()):
            item = self.rules_table.item(row, 0)
            combo = self.rules_table.cellWidget(row, 1)
            if not item or not combo:
                continue
            addr = item.text().strip()
            if not addr:
                continue
            action = combo.currentData() or "proxy"
            if action == "direct":
                direct.append(addr)
            elif action == "block":
                block.append(addr)
            else:
                proxy.append(addr)

        process_rules: list[dict[str, str]] = []
        for row in range(self.proc_table.rowCount()):
            item = self.proc_table.item(row, 0)
            combo = self.proc_table.cellWidget(row, 1)
            if not item or not combo:
                continue
            value = str(item.data(_PROCESS_VALUE_ROLE) or item.text()).strip()
            if not value:
                continue
            match = str(item.data(_PROCESS_MATCH_ROLE) or self._normalize_process_match("", value))
            label = str(item.data(_PROCESS_LABEL_ROLE) or "").strip()
            action = combo.currentData() or "proxy"
            entry = {"process": value, "action": action}
            if match != _PROCESS_MATCH_NAME:
                entry["match"] = match
            if label:
                entry["label"] = label
            process_rules.append(entry)

        # Collect service states
        service_routes: dict[str, str] = {}
        for svc_id, card in self._service_cards.items():
            enabled, action = card.get_state()
            if enabled:
                service_routes[svc_id] = action

        # Collect process preset states
        process_preset_routes: dict[str, str] = {}
        for preset_id, card in self._process_preset_cards.items():
            enabled, action = card.get_state()
            if enabled:
                process_preset_routes[preset_id] = action

        tun_default_outbound = self.tun_default_combo.currentData() or "direct"

        return RoutingSettings(
            mode=str(mode),
            bypass_lan=self.bypass_switch.isChecked(),
            direct_domains=direct,
            proxy_domains=proxy,
            block_domains=block,
            dns_mode=str(dns_mode),
            dns_bootstrap_server=str(dns_bootstrap_server),
            dns_bootstrap_type=str(dns_bootstrap_type),
            dns_proxy_server=str(dns_proxy_server),
            dns_proxy_type=str(dns_proxy_type),
            process_rules=process_rules,
            process_preset_routes=process_preset_routes,
            service_routes=service_routes,
            tun_default_outbound=str(tun_default_outbound),
        )

    def _emit_apply(self) -> None:
        routing = self._collect_current_routing()
        self._set_apply_pending(False)
        self.apply_requested.emit(routing)

    def _show_help(self) -> None:
        box = MessageBox(
            "Как работает маршрутизация",
            "Выберите готовый режим сверху или добавьте свои правила.\n\n"
            "«Всё через VPN» — весь трафик идёт через выбранный сервер.\n"
            "«Только заблокированное» — обычные сайты открываются напрямую, а популярные заблокированные сервисы идут через VPN.\n"
            "«Всё кроме РФ» — российские сайты и адреса идут напрямую, остальное через VPN.\n\n"
            "В разделе «Приложения» можно выбрать конкретную программу или папку программы. "
            "В разделе «Домены и IP» можно добавить сайт вроде example.com или адрес вроде 1.2.3.4.",
            self.window(),
        )
        box.yesButton.setText("Понятно")
        box.cancelButton.hide()
        box.exec()

    # --- Helpers ---

    def _set_apply_pending(self, pending: bool) -> None:
        self._apply_pending = pending
        if not pending and self._apply_timer.isActive():
            self._apply_timer.stop()
        self._apply_pending_label.setVisible(pending)
        self.apply_routing_btn.setEnabled(pending)

    @staticmethod
    def _select_combo_value(combo: ComboBox, value: str) -> None:
        for index in range(combo.count()):
            if combo.itemData(index) == value:
                combo.setCurrentIndex(index)
                return
