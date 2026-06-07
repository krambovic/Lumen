"""Ring-buffer log model for the QML logs page"""
from __future__ import annotations

from collections import deque

from PyQt6.QtCore import QAbstractListModel, QModelIndex, Qt


class LogModel(QAbstractListModel):
    LineRole = Qt.ItemDataRole.UserRole + 1
    LevelRole = Qt.ItemDataRole.UserRole + 2

    _ROLE_NAMES = {LineRole: b"line", LevelRole: b"level"}

    def __init__(self, max_lines: int = 4000, parent=None) -> None:
        super().__init__(parent)
        self._max_lines = int(max_lines)
        self._lines: deque[str] = deque(maxlen=self._max_lines)

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:
        if parent.isValid():
            return 0
        return len(self._lines)

    def roleNames(self):
        return dict(self._ROLE_NAMES)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._lines)):
            return None
        line = self._lines[index.row()]
        if role == self.LineRole:
            return line
        if role == self.LevelRole:
            return self._classify(line)
        return None

    def append_line(self, line: str) -> None:
        if len(self._lines) >= self._max_lines:
            self.beginRemoveRows(QModelIndex(), 0, 0)
            self._lines.popleft()
            self.endRemoveRows()
        row = len(self._lines)
        self.beginInsertRows(QModelIndex(), row, row)
        self._lines.append(line)
        self.endInsertRows()

    def clear(self) -> None:
        self.beginResetModel()
        self._lines.clear()
        self.endResetModel()

    @staticmethod
    def _classify(line: str) -> str:
        low = line.lower()
        if "error" in low or "ошибк" in low or "fail" in low:
            return "error"
        if "warn" in low or "вниман" in low:
            return "warning"
        if "[test] ok" in low or "success" in low or "успеш" in low:
            return "success"
        return "info"
