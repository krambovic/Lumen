"""Prevent Qt's fallback text context menu from leaking into the QML UI."""

from __future__ import annotations

from PyQt6.QtCore import QEvent, QObject
from PyQt6.QtQuick import QQuickItem, QQuickWindow


class QmlNativeContextMenuFilter(QObject):
    """Consume native context-menu requests sent to Qt Quick objects.

    Lumen opens its styled text menu from the QML right-button handler.  Qt can
    emit a separate ``ContextMenu`` event for the same click, especially when a
    menu is already open.  If that event reaches ``QQuickTextInput`` or
    ``QQuickTextEdit``, Qt displays its built-in platform menu on top of ours.
    Mouse-button events are deliberately left untouched.
    """

    def eventFilter(self, watched: QObject, event: QEvent) -> bool:  # noqa: N802
        if event.type() == QEvent.Type.ContextMenu and isinstance(
            watched, (QQuickItem, QQuickWindow)
        ):
            event.accept()
            return True
        return super().eventFilter(watched, event)
