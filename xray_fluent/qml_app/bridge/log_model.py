"""Structured, filterable ring-buffer model for the QML logs page."""
from __future__ import annotations

from collections import deque

from PyQt6.QtCore import QAbstractListModel, QModelIndex, QSortFilterProxyModel, Qt, pyqtProperty, pyqtSignal, pyqtSlot

from ...log_utils import LogEntry, parse_log_line


class LogModel(QAbstractListModel):
    LineRole = Qt.ItemDataRole.UserRole + 1
    LevelRole = Qt.ItemDataRole.UserRole + 2
    TimeRole = Qt.ItemDataRole.UserRole + 3
    SourceRole = Qt.ItemDataRole.UserRole + 4
    DetailsRole = Qt.ItemDataRole.UserRole + 5
    ActionIdRole = Qt.ItemDataRole.UserRole + 6
    ActionLabelRole = Qt.ItemDataRole.UserRole + 7

    _ROLE_NAMES = {
        LineRole: b"line",
        LevelRole: b"level",
        TimeRole: b"time",
        SourceRole: b"source",
        DetailsRole: b"details",
        ActionIdRole: b"actionId",
        ActionLabelRole: b"actionLabel",
    }

    def __init__(self, max_lines: int = 4000, parent=None) -> None:
        super().__init__(parent)
        self._max_lines = int(max_lines)
        self._lines: deque[LogEntry] = deque(maxlen=self._max_lines)

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:
        if parent.isValid():
            return 0
        return len(self._lines)

    def roleNames(self):
        return dict(self._ROLE_NAMES)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._lines)):
            return None
        entry = self._lines[index.row()]
        if role == self.LineRole:
            return entry.message
        if role == self.LevelRole:
            return entry.level
        if role == self.TimeRole:
            return entry.timestamp
        if role == self.SourceRole:
            return entry.source
        if role == self.DetailsRole:
            return entry.details
        if role == self.ActionIdRole:
            return entry.action_id
        if role == self.ActionLabelRole:
            return entry.action_label
        return None

    @pyqtSlot(str)
    def append_line(self, line: str) -> None:
        if len(self._lines) >= self._max_lines:
            self.beginRemoveRows(QModelIndex(), 0, 0)
            self._lines.popleft()
            self.endRemoveRows()
        row = len(self._lines)
        self.beginInsertRows(QModelIndex(), row, row)
        self._lines.append(parse_log_line(line))
        self.endInsertRows()

    def clear(self) -> None:
        self.beginResetModel()
        self._lines.clear()
        self.endResetModel()



class LogFilterModel(QSortFilterProxyModel):
    filterChanged = pyqtSignal()

    def __init__(self, source: LogModel, parent=None) -> None:
        super().__init__(parent)
        self.setSourceModel(source)
        self._level = "all"
        self._search = ""
        self.setDynamicSortFilter(True)

    @pyqtProperty(str, notify=filterChanged)
    def levelFilter(self) -> str:
        return self._level

    @pyqtSlot(str)
    def setLevelFilter(self, value: str) -> None:
        value = (value or "all").strip().lower()
        if value not in {"all", "error", "warning", "success", "info"}:
            value = "all"
        if value == self._level:
            return
        self._level = value
        self.invalidateFilter()
        self.filterChanged.emit()

    @pyqtSlot(str)
    def setSearchText(self, value: str) -> None:
        value = (value or "").strip().lower()
        if value == self._search:
            return
        self._search = value
        self.invalidateFilter()

    @pyqtSlot()
    def clear(self) -> None:
        source = self.sourceModel()
        if isinstance(source, LogModel):
            source.clear()

    def filterAcceptsRow(self, source_row: int, source_parent: QModelIndex) -> bool:
        source = self.sourceModel()
        index = source.index(source_row, 0, source_parent)
        level = str(source.data(index, LogModel.LevelRole) or "info")
        if self._level == "error" and level != "error":
            return False
        if self._level == "warning" and level != "warning":
            return False
        if self._level == "success" and level != "success":
            return False
        if self._level == "info" and level != "info":
            return False
        if self._search:
            haystack = " ".join(
                str(source.data(index, role) or "")
                for role in (LogModel.LineRole, LogModel.DetailsRole, LogModel.SourceRole)
            ).lower()
            if self._search not in haystack:
                return False
        return True
