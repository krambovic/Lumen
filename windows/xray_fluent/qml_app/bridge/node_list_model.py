"""GPU-friendly list model for VPN server nodes.

This replaces the old QAbstractTableModel + QTableView combination (which was a
major source of jank: per-cell ``data()`` calls re-decoding flag icons, no
delegate recycling, smooth-scroll disabled). A QAbstractListModel feeding a QML
``ListView`` gets free delegate recycling and scene-graph batching, so scrolling
stays smooth at the display refresh rate.
"""
from __future__ import annotations

from typing import Iterable

from PyQt6.QtCore import QAbstractListModel, QModelIndex, QSortFilterProxyModel, Qt, QTimer

from ...models import Node
from ...country_flags import detect_country, get_flag_emoji, get_flag_svg_data_uri, _STRIPES as _FLAG_STRIPES
from ...application.node_runtime_service import is_native_singbox_only_node, node_supported_by_core
from ...i18n import tr
from ...node_transport import node_protocol_label, node_transport


class NodeListModel(QAbstractListModel):
    # Custom roles exposed to QML delegates.
    IdRole = Qt.ItemDataRole.UserRole + 1
    NameRole = Qt.ItemDataRole.UserRole + 2
    SchemeRole = Qt.ItemDataRole.UserRole + 3
    ServerRole = Qt.ItemDataRole.UserRole + 4
    PortRole = Qt.ItemDataRole.UserRole + 5
    GroupRole = Qt.ItemDataRole.UserRole + 6
    PingRole = Qt.ItemDataRole.UserRole + 8
    SpeedRole = Qt.ItemDataRole.UserRole + 9
    AliveRole = Qt.ItemDataRole.UserRole + 10
    CountryRole = Qt.ItemDataRole.UserRole + 11
    SelectedRole = Qt.ItemDataRole.UserRole + 12
    SpeedProgressRole = Qt.ItemDataRole.UserRole + 13
    FlagOrientRole = Qt.ItemDataRole.UserRole + 14
    FlagColorsRole = Qt.ItemDataRole.UserRole + 15
    LastUsedRole = Qt.ItemDataRole.UserRole + 16
    RuntimeSupportedRole = Qt.ItemDataRole.UserRole + 17
    FlagEmojiRole = Qt.ItemDataRole.UserRole + 18
    FlagSourceRole = Qt.ItemDataRole.UserRole + 19
    TransportRole = Qt.ItemDataRole.UserRole + 20
    TestedRole = Qt.ItemDataRole.UserRole + 21
    DescriptionRole = Qt.ItemDataRole.UserRole + 22
    PingingRole = Qt.ItemDataRole.UserRole + 23
    PingKindRole = Qt.ItemDataRole.UserRole + 24
    ProtocolRole = Qt.ItemDataRole.UserRole + 25

    _ROLE_NAMES = {
        IdRole: b"nodeId",
        NameRole: b"name",
        SchemeRole: b"scheme",
        ServerRole: b"server",
        PortRole: b"port",
        GroupRole: b"group",
        PingRole: b"ping",
        SpeedRole: b"speed",
        AliveRole: b"isAlive",
        CountryRole: b"country",
        SelectedRole: b"selected",
        SpeedProgressRole: b"speedProgress",
        FlagOrientRole: b"flagOrient",
        FlagColorsRole: b"flagColors",
        LastUsedRole: b"lastUsed",
        RuntimeSupportedRole: b"runtimeSupported",
        FlagEmojiRole: b"flagEmoji",
        FlagSourceRole: b"flagSource",
        TransportRole: b"transport",
        TestedRole: b"tested",
        DescriptionRole: b"description",
        PingingRole: b"pinging",
        PingKindRole: b"pingKind",
        ProtocolRole: b"protocol",
    }

    _FLUSH_INTERVAL_MS = 50

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._nodes: list[Node] = []
        self._selected_id: str | None = None
        self._index_by_id: dict[str, int] = {}
        self._speed_progress: dict[str, int] = {}
        self._pinging_ids: set[str] = set()
        self._country_cache: dict[str, tuple[tuple[str, str], str]] = {}
        self._allow_native_singbox_only = False
        self._core_preference = "auto"
        # Bulk ping/speed runs touch thousands of rows; one dataChanged per row
        # makes every proxy re-sort per row and freezes the UI.
        self._pending_roles: dict[str, set[int]] = {}
        self._flush_timer = QTimer(self)
        self._flush_timer.setSingleShot(True)
        self._flush_timer.setInterval(self._FLUSH_INTERVAL_MS)
        self._flush_timer.timeout.connect(self.flush_pending_changes)

    # ── Qt model API ────────────────────────────────────────────
    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:
        if parent.isValid():
            return 0
        return len(self._nodes)

    def roleNames(self):
        return dict(self._ROLE_NAMES)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._nodes)):
            return None
        node = self._nodes[index.row()]
        if role == self.IdRole:
            return node.id
        if role == self.NameRole:
            raw_name = node.name or node.server or tr("(без имени)")
            country = self._country_for(node)
            from .app_bridge import _server_display_name_without_country_prefix
            return _server_display_name_without_country_prefix(raw_name, country)
        if role == self.SchemeRole:
            return (node.scheme or "").upper()
        if role == self.DescriptionRole:
            description = (node.description or "").strip()
            if description:
                return description
            outbound = node.outbound if isinstance(node.outbound, dict) else {}
            protocol = str(outbound.get("protocol") or "").strip().lower()
            if (node.scheme or "").strip().lower() == "auto" or protocol == "xray_config":
                return tr("Автовыбор лучших серверов")
            return ""
        if role == self.TransportRole:
            return node_transport(node)
        if role == self.ServerRole:
            return node.server or ""
        if role == self.PortRole:
            return int(node.port or 0)
        if role == self.GroupRole:
            return node.group or ""
        if role == self.PingRole:
            return -1 if node.ping_ms is None else int(node.ping_ms)
        if role == self.ProtocolRole:
            return node_protocol_label(node)
        if role == self.PingKindRole:
            return node.ping_kind
        if role == self.PingingRole:
            return node.id in self._pinging_ids
        if role == self.SpeedRole:
            return -1.0 if node.speed_mbps is None else float(node.speed_mbps)
        if role == self.AliveRole:
            return bool(node.is_alive)
        if role == self.TestedRole:
            return node.is_alive is not None
        if role == self.CountryRole:
            return self._country_for(node).lower()
        if role == self.FlagEmojiRole:
            return get_flag_emoji(self._country_for(node))
        if role == self.FlagSourceRole:
            return get_flag_svg_data_uri(self._country_for(node))
        if role == self.FlagOrientRole:
            data = _FLAG_STRIPES.get(self._country_for(node))
            return data[0] if data else ""
        if role == self.FlagColorsRole:
            data = _FLAG_STRIPES.get(self._country_for(node))
            return list(data[1]) if data else []
        if role == self.LastUsedRole:
            return self._format_last_used(node.last_used_at)
        if role == self.RuntimeSupportedRole:
            return self._runtime_supported(node)
        if role == self.SelectedRole:
            return node.id == self._selected_id
        if role == self.SpeedProgressRole:
            return int(self._speed_progress.get(node.id, -1))
        return None

    # ── Country detection (cached per node id) ──────────────────
    def _country_for(self, node: Node) -> str:
        code = str(node.country_code or "").strip().upper()
        if code:
            return code
        # Nodes are mutable: identity alone cannot invalidate a cached miss.
        signature = (str(node.name or ""), str(node.server or ""))
        cached = self._country_cache.get(node.id)
        if cached is None or cached[0] != signature:
            cached = (signature, detect_country(*signature))
            self._country_cache[node.id] = cached
        return cached[1]

    @staticmethod
    def _format_last_used(value: str | None) -> str:
        if not value:
            return "\u2014"
        from datetime import datetime
        try:
            dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
            return dt.strftime("%Y-%m-%d %H:%M")
        except ValueError:
            return value

    # ── Bulk + incremental updates ──────────────────────────────
    def set_nodes(self, nodes: Iterable[Node], selected_id: str | None) -> None:
        incoming = list(nodes)
        new_ids = [node.id for node in incoming]
        structural = [node.id for node in self._nodes] != new_ids
        # A subscription replacement is one atomic model transaction, not a
        # remove/insert pair with intermediate IDs and ListView coordinates.
        if structural:
            self.beginResetModel()
        self._nodes = incoming
        self._selected_id = selected_id
        self._index_by_id = {node.id: i for i, node in enumerate(incoming)}
        live_ids = set(new_ids)
        self._speed_progress = {k: v for k, v in self._speed_progress.items() if k in live_ids}
        self._pinging_ids.intersection_update(live_ids)
        self._country_cache = {k: v for k, v in self._country_cache.items() if k in live_ids}
        if structural:
            self.endResetModel()
        elif incoming:
            self.dataChanged.emit(self.index(0, 0), self.index(len(incoming) - 1, 0), list(self._ROLE_NAMES))

    def set_selected(self, selected_id: str | None) -> None:
        if selected_id == self._selected_id:
            return
        previous = self._selected_id
        self._selected_id = selected_id
        for nid in (previous, selected_id):
            self._emit_row_changed(nid, [self.SelectedRole])

    def _runtime_supported(self, node: Node) -> bool:
        if not self._allow_native_singbox_only and is_native_singbox_only_node(node):
            return False
        return node_supported_by_core(node, self._core_preference)

    def set_core_preference(self, preference: str) -> None:
        """Dim servers the chosen main core cannot run."""
        value = str(preference or "auto")
        if value == self._core_preference:
            return
        self._core_preference = value
        if self._nodes:
            self.dataChanged.emit(
                self.index(0, 0),
                self.index(len(self._nodes) - 1, 0),
                [self.RuntimeSupportedRole],
            )

    def set_runtime_support(self, allow_native_singbox_only: bool) -> None:
        value = bool(allow_native_singbox_only)
        if value == self._allow_native_singbox_only:
            return
        self._allow_native_singbox_only = value
        if self._nodes:
            self.dataChanged.emit(
                self.index(0, 0),
                self.index(len(self._nodes) - 1, 0),
                [self.RuntimeSupportedRole],
            )

    def update_ping(self, node_id: str, ping_ms: int | None) -> None:
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        self._nodes[row].ping_ms = ping_ms
        self._pinging_ids.discard(node_id)
        self._queue_row_changed(
            node_id,
            (self.PingRole, self.AliveRole, self.TestedRole, self.PingingRole, self.PingKindRole),
        )

    def set_pinging_ids(self, node_ids: Iterable[str]) -> None:
        next_ids = {str(node_id) for node_id in node_ids if str(node_id)}
        changed = self._pinging_ids.symmetric_difference(next_ids)
        self._pinging_ids = next_ids
        for node_id in changed:
            self._queue_row_changed(node_id, (self.PingingRole,))

    def clear_pinging(self) -> None:
        self.set_pinging_ids(())

    def update_speed(self, node_id: str, speed_mbps: float | None) -> None:
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        self._nodes[row].speed_mbps = speed_mbps
        self._speed_progress.pop(node_id, None)
        self._queue_row_changed(node_id, (self.SpeedRole, self.SpeedProgressRole, self.TestedRole))

    def update_alive(self, node_id: str, is_alive: bool) -> None:
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        self._queue_row_changed(node_id, (self.AliveRole, self.TestedRole))

    def update_speed_progress(self, node_id: str, percent: int) -> None:
        if node_id not in self._index_by_id:
            return
        self._speed_progress[node_id] = int(percent)
        self._queue_row_changed(node_id, (self.SpeedProgressRole,))

    def node_id_at(self, row: int) -> str | None:
        if 0 <= row < len(self._nodes):
            return self._nodes[row].id
        return None

    def index_of_id(self, node_id: str) -> int:
        return self._index_by_id.get(node_id, -1)

    def node_row_at(self, row: int) -> dict | None:
        """Return the filterable fields of a row by index (for Ctrl+A respecting filters)."""
        if 0 <= row < len(self._nodes):
            node = self._nodes[row]
            return {
                "id": node.id,
                "name": node.name or node.server or tr("(без имени)"),
                "server": node.server or "",
                "group": node.group or "",
                "transport": node_transport(node),
                "pingKind": node.ping_kind or "",
            }
        return None

    def best_ping_node_id(self) -> str:
        """Return the visible, runnable node with the lowest measured ping."""
        best_id, best_ping = "", None
        for node in self._nodes:
            ping = node.ping_ms
            if ping is None or ping <= 0 or node.is_alive is False:
                continue
            if not self._runtime_supported(node):
                continue
            if best_ping is None or ping < best_ping:
                best_id, best_ping = node.id, ping
        return best_id

    # ── helpers ─────────────────────────────────────────────────
    def _queue_row_changed(self, node_id: str, roles: Iterable[int]) -> None:
        if node_id not in self._index_by_id:
            return
        self._pending_roles.setdefault(node_id, set()).update(roles)
        if not self._flush_timer.isActive():
            self._flush_timer.start()

    def flush_pending_changes(self) -> None:
        """Emit queued row changes as a few contiguous ranges."""
        self._flush_timer.stop()
        pending, self._pending_roles = self._pending_roles, {}
        rows = sorted(
            (row, roles)
            for node_id, roles in pending.items()
            if (row := self._index_by_id.get(node_id)) is not None
        )
        start = end = -1
        merged: set[int] = set()
        for row, roles in [*rows, (-2, set())]:
            if row == end + 1 and start >= 0:
                end = row
                merged |= roles
                continue
            if start >= 0:
                self.dataChanged.emit(self.index(start, 0), self.index(end, 0), sorted(merged))
            start = end = row
            merged = set(roles)

    def _emit_row_changed(self, node_id: str | None, roles: list[int]) -> None:
        if not node_id:
            return
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        idx = self.index(row, 0)
        self.dataChanged.emit(idx, idx, roles)


class DashboardNodeProxy(QSortFilterProxyModel):
    """Dashboard view of the server list.

    Follows the Серверы page order when a sort is chosen there; with manual
    order it auto-sorts by ping (unknown pings last). Also owns the dashboard
    search so Ctrl+A can select exactly the rows that are shown.
    """

    _UNKNOWN_PING = 1 << 30

    def __init__(self, source: NodeListModel, sort_key, parent=None) -> None:
        super().__init__(parent)
        self._sort_key = sort_key
        self._text = ""
        self._ping_sorted: bool | None = None
        self.setSourceModel(source)
        self.setSortRole(NodeListModel.PingRole)
        self.setDynamicSortFilter(True)
        self.refresh_sort()

    def refresh_sort(self) -> None:
        ping_sorted = str(self._sort_key() or "manual") == "manual"
        if ping_sorted == self._ping_sorted:
            return
        self._ping_sorted = ping_sorted
        self.sort(0 if ping_sorted else -1)

    def set_filter_text(self, text: str) -> None:
        value = (text or "").strip().casefold()
        if value != self._text:
            self._text = value
            self.invalidateFilter()

    def node_ids(self) -> list[str]:
        return [str(self.data(self.index(row, 0), NodeListModel.IdRole) or "") for row in range(self.rowCount())]

    def filterAcceptsRow(self, source_row: int, source_parent: QModelIndex) -> bool:
        if not self._text:
            return True
        model = self.sourceModel()
        index = model.index(source_row, 0, source_parent)
        roles = (NodeListModel.NameRole, NodeListModel.ProtocolRole, NodeListModel.GroupRole,
                 NodeListModel.CountryRole, NodeListModel.ServerRole)
        haystack = " ".join(str(model.data(index, role) or "") for role in roles)
        return self._text in haystack.casefold()

    def lessThan(self, left: QModelIndex, right: QModelIndex) -> bool:
        # Called O(n log n) times per re-sort: read the nodes directly instead
        # of going through the role dispatch in data().
        nodes = self.sourceModel()._nodes
        a = nodes[left.row()].ping_ms
        b = nodes[right.row()].ping_ms
        a = a if isinstance(a, int) and a > 0 else self._UNKNOWN_PING
        b = b if isinstance(b, int) and b > 0 else self._UNKNOWN_PING
        if a != b:
            return a < b
        return left.row() < right.row()
