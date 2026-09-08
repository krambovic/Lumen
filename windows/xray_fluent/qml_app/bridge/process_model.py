"""Per-process traffic stats model for the Dashboard page"""
from __future__ import annotations

from typing import Any, Sequence

from PyQt6.QtCore import QAbstractListModel, QModelIndex, Qt, pyqtSlot


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
    UnknownBytesRole = Qt.ItemDataRole.UserRole + 12
    DownloadTotalRole = Qt.ItemDataRole.UserRole + 13
    UploadTotalRole = Qt.ItemDataRole.UserRole + 14

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
        UnknownBytesRole: b"unknownBytes",
        DownloadTotalRole: b"downloadTotal",
        UploadTotalRole: b"uploadTotal",
    }

    _CHANGED_ROLES = list(_ROLE_NAMES.keys())

    def __init__(self, parent=None) -> None:
        super().__init__(parent)
        self._rows: list[dict[str, Any]] = []
        self._sort_key = "total"
        self._sort_ascending = False

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
            return row.get("route", "unknown")
        if role == self.UnknownBytesRole:
            return float(row.get("unknown_bytes", 0.0))
        if role == self.DownloadTotalRole:
            return float(row.get("download_total", 0.0))
        if role == self.UploadTotalRole:
            return float(row.get("upload_total", 0.0))
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

    @pyqtSlot(str, bool)
    def set_sort(self, key: str, ascending: bool) -> None:
        normalized_key = key if key in {"download", "upload", "total"} else "total"
        normalized_ascending = bool(ascending)
        if (normalized_key, normalized_ascending) == (self._sort_key, self._sort_ascending):
            return
        self._sort_key = normalized_key
        self._sort_ascending = normalized_ascending
        self.beginResetModel()
        self._sort_rows(self._rows)
        self.endResetModel()

    def _sort_rows(self, rows: list[dict[str, Any]]) -> None:
        field = {"download": "download_total", "upload": "upload_total", "total": "total"}[
            self._sort_key
        ]
        rows.sort(key=lambda row: str(row["name"]).casefold())
        rows.sort(key=lambda row: float(row[field]), reverse=not self._sort_ascending)

    def _normalize(self, stats: Sequence[Any]) -> list[dict[str, Any]]:
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
                    "download_total": download,
                    "upload_total": upload,
                    "pid": int(_get(item, "pid", default=0)),
                    "proxy_bytes": float(_get(item, "proxy_bytes", default=0.0)),
                    "direct_bytes": float(_get(item, "direct_bytes", default=0.0)),
                    "unknown_bytes": float(_get(item, "unknown_bytes", default=max(
                        0.0, total - float(_get(item, "proxy_bytes", default=0.0))
                        - float(_get(item, "direct_bytes", default=0.0)),
                    ))),
                    "connections": int(_get(item, "connections", default=0)),
                    "total_connections": int(_get(item, "total_connections", default=0)),
                    "top_host": str(_get(item, "top_host", "host", default="")),
                    "total": total,
                    "route": str(_get(item, "route", default="unknown")),
                }
            )
        # The collector normally sends the busiest processes first, but not every
        # metrics backend guarantees that order. Keep the selected UI order stable.
        self._sort_rows(rows)
        return rows
