"""GPU-friendly list model for VPN server nodes.

This replaces the old QAbstractTableModel + QTableView combination (which was a
major source of jank: per-cell ``data()`` calls re-decoding flag icons, no
delegate recycling, smooth-scroll disabled). A QAbstractListModel feeding a QML
``ListView`` gets free delegate recycling and scene-graph batching, so scrolling
stays smooth at the display refresh rate.
"""
from __future__ import annotations

from typing import Iterable

from PyQt6.QtCore import QAbstractListModel, QModelIndex, Qt, pyqtSlot

from ...models import Node
from ...country_flags import detect_country, get_flag_emoji, get_flag_svg_data_uri, _STRIPES as _FLAG_STRIPES
from ...application.node_runtime_service import is_native_singbox_only_node


class NodeListModel(QAbstractListModel):
    # Custom roles exposed to QML delegates.
    IdRole = Qt.ItemDataRole.UserRole + 1
    NameRole = Qt.ItemDataRole.UserRole + 2
    SchemeRole = Qt.ItemDataRole.UserRole + 3
    ServerRole = Qt.ItemDataRole.UserRole + 4
    PortRole = Qt.ItemDataRole.UserRole + 5
    GroupRole = Qt.ItemDataRole.UserRole + 6
    TagsRole = Qt.ItemDataRole.UserRole + 7
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

    _ROLE_NAMES = {
        IdRole: b"nodeId",
        NameRole: b"name",
        SchemeRole: b"scheme",
        ServerRole: b"server",
        PortRole: b"port",
        GroupRole: b"group",
        TagsRole: b"tags",
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
    }

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._nodes: list[Node] = []
        self._selected_id: str | None = None
        self._index_by_id: dict[str, int] = {}
        self._speed_progress: dict[str, int] = {}
        self._country_cache: dict[str, str] = {}
        self._allow_native_singbox_only = False

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
            return node.name or node.server or "(\u0431\u0435\u0437 \u0438\u043c\u0435\u043d\u0438)"
        if role == self.SchemeRole:
            return (node.scheme or "").upper()
        if role == self.ServerRole:
            return node.server or ""
        if role == self.PortRole:
            return int(node.port or 0)
        if role == self.GroupRole:
            return node.group or ""
        if role == self.TagsRole:
            return list(node.tags or [])
        if role == self.PingRole:
            return -1 if node.ping_ms is None else int(node.ping_ms)
        if role == self.SpeedRole:
            return -1.0 if node.speed_mbps is None else float(node.speed_mbps)
        if role == self.AliveRole:
            return bool(node.is_alive)
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
            return self._allow_native_singbox_only or not is_native_singbox_only_node(node)
        if role == self.SelectedRole:
            return node.id == self._selected_id
        if role == self.SpeedProgressRole:
            return int(self._speed_progress.get(node.id, -1))
        return None

    # ── Country detection (cached per node id) ──────────────────
    def _country_for(self, node: Node) -> str:
        code = (node.country_code or "").upper()
        if code:
            return code
        cached = self._country_cache.get(node.id)
        if cached is None:
            cached = detect_country(node.name or "", node.server or "")
            self._country_cache[node.id] = cached
        return cached

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
        self.beginResetModel()
        self._nodes = list(nodes)
        self._selected_id = selected_id
        self._index_by_id = {n.id: i for i, n in enumerate(self._nodes)}
        self._speed_progress.clear()
        live_ids = set(self._index_by_id)
        self._country_cache = {
            nid: cc for nid, cc in self._country_cache.items() if nid in live_ids
        }
        self.endResetModel()

    def set_selected(self, selected_id: str | None) -> None:
        if selected_id == self._selected_id:
            return
        previous = self._selected_id
        self._selected_id = selected_id
        for nid in (previous, selected_id):
            self._emit_row_changed(nid, [self.SelectedRole])

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
        self._emit_row_changed(node_id, [self.PingRole])

    def update_speed(self, node_id: str, speed_mbps: float | None) -> None:
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        self._nodes[row].speed_mbps = speed_mbps
        self._speed_progress.pop(node_id, None)
        self._emit_row_changed(node_id, [self.SpeedRole, self.SpeedProgressRole])

    def update_alive(self, node_id: str, is_alive: bool) -> None:
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        self._nodes[row].is_alive = is_alive
        self._emit_row_changed(node_id, [self.AliveRole])

    def update_speed_progress(self, node_id: str, percent: int) -> None:
        if node_id not in self._index_by_id:
            return
        self._speed_progress[node_id] = int(percent)
        self._emit_row_changed(node_id, [self.SpeedProgressRole])

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
                "name": node.name or node.server or "(\u0431\u0435\u0437 \u0438\u043c\u0435\u043d\u0438)",
                "server": node.server or "",
                "group": node.group or "",
                "tags": list(node.tags or []),
            }
        return None

    # ── helpers ─────────────────────────────────────────────────
    def _emit_row_changed(self, node_id: str | None, roles: list[int]) -> None:
        if not node_id:
            return
        row = self._index_by_id.get(node_id)
        if row is None:
            return
        idx = self.index(row, 0)
        self.dataChanged.emit(idx, idx, roles)
