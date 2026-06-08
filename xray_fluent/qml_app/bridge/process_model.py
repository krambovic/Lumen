"""Per-process traffic stats model for the Dashboard page"""
from __future__ import annotations

from typing import Any, Sequence

from PyQt6.QtCore import QAbstractListModel, QModelIndex, Qt


def _get(d: Any, *keys: str, default: Any = 0) -> Any:
    # The worker may hand us either plain mappings or dataclass instances
    # (ProcessTrafficSnapshot has slots=True, so read via getattr, not __dict__).
    for key in keys:
        if isinstance(d, dict):
            value = d.get(key)
        else:
            value = getattr(d, key, None)
        if value is not None:
            return value
    return default


class ProcessModel(QAbstractListModel):
    NameRole = Qt.ItemDataRole.UserRole + 1
    DownRole = Qt.ItemDataRole.UserRole + 2
    UpRole = Qt.ItemDataRole.UserRole + 3
    PidRole = Qt.ItemDataRole.UserRole + 4
    ProxyBytesRole = Qt.ItemDataRole.UserRole + 5
    DirectBytesRole = Qt.ItemDataRole.UserRole + 6
    ConnectionsRole = Qt.ItemDataRole.UserRole + 7
    TotalConnectionsRole = Qt.ItemDataRole.UserRole + 8
    TopHostRole = Qt.ItemDataRole.UserRole + 9
    TotalRole = Qt.ItemDataRole.UserRole + 10
    RouteRole = Qt.ItemDataRole.UserRole + 11

    _ROLE_NAMES = {
        NameRole: b"name",
        DownRole: b"downBps",
        UpRole: b"upBps",
        PidRole: b"pid",
        ProxyBytesRole: b"proxyBytes",
        DirectBytesRole: b"directBytes",
        ConnectionsRole: b"connections",
        TotalConnectionsRole: b"totalConnections",
        TopHostRole: b"topHost",
        TotalRole: b"total",
        RouteRole: b"route",
    }

    _CHANGED_ROLES = list(_ROLE_NAMES.keys())

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._rows: list[dict[str, Any]] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:
        if parent.isValid():
            return 0
        return len(self._rows)

    def roleNames(self):
        return dict(self._ROLE_NAMES)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._rows)):
            return None
        row = self._rows[index.row()]
        if role == self.NameRole:
            return row.get("name", "")
        if role == self.DownRole:
            return float(row.get("down", 0.0))
        if role == self.UpRole:
            return float(row.get("up", 0.0))
        if role == self.PidRole:
            return int(row.get("pid", 0))
        if role == self.ProxyBytesRole:
            return float(row.get("proxy_bytes", 0.0))
        if role == self.DirectBytesRole:
            return float(row.get("direct_bytes", 0.0))
        if role == self.ConnectionsRole:
            return int(row.get("connections", 0))
        if role == self.TotalConnectionsRole:
            return int(row.get("total_connections", 0))
        if role == self.TopHostRole:
            return row.get("top_host", "")
        if role == self.TotalRole:
            return float(row.get("total", 0.0))
        if role == self.RouteRole:
            return row.get("route", "direct")
        return None

    def set_stats(self, stats: Sequence[Any]) -> None:
        normalized = self._normalize(stats)
        # Same length: update in place (cheapest, the common case).
        if len(normalized) == len(self._rows):
            changed_top = None
            changed_bottom = None
            for i, new in enumerate(normalized):
                if new != self._rows[i]:
                    self._rows[i] = new
                    changed_top = i if changed_top is None else changed_top
                    changed_bottom = i
            if changed_top is not None:
                self.dataChanged.emit(
                    self.index(changed_top, 0),
                    self.index(changed_bottom, 0),
                    self._CHANGED_ROLES,
                )
            return
        # Length changed: reset (process list rarely changes size).
        self.beginResetModel()
        self._rows = normalized
        self.endResetModel()

    @staticmethod
    def _normalize(stats: Sequence[Any]) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        for item in stats or []:
            upload = float(_get(item, "upload", default=0.0))
            download = float(_get(item, "download", default=0.0))
            total = float(_get(item, "total", default=0.0)) or (upload + download)
            rows.append(
                {
                    "name": str(_get(item, "name", "process", "image", "exe", default="")),
                    "down": float(_get(item, "down_bps", "down", "rx", "down_speed", default=0.0)),
                    "up": float(_get(item, "up_bps", "up", "tx", "up_speed", default=0.0)),
                    "pid": int(_get(item, "pid", default=0)),
                    "proxy_bytes": float(_get(item, "proxy_bytes", default=0.0)),
                    "direct_bytes": float(_get(item, "direct_bytes", default=0.0)),
                    "connections": int(_get(item, "connections", default=0)),
                    "total_connections": int(_get(item, "total_connections", default=0)),
                    "top_host": str(_get(item, "top_host", "host", default="")),
                    "total": total,
                    "route": str(_get(item, "route", default="direct")),
                }
            )
        return rows
