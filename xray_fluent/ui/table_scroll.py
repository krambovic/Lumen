from __future__ import annotations

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import QAbstractItemView, QAbstractScrollArea


def use_native_table_scroll(table: QAbstractItemView, *, disable_hover: bool = False) -> None:
    """Disable qfluentwidgets inertial scrolling for large data tables."""
    scroll_delegate = getattr(table, "scrollDelagate", None)
    if scroll_delegate is not None:
        table.viewport().removeEventFilter(scroll_delegate)
        for attr in ("vScrollBar", "hScrollBar"):
            bar = getattr(scroll_delegate, attr, None)
            if bar is not None:
                bar.hide()
                try:
                    bar.setForceHidden(True)
                except AttributeError:
                    pass

    table.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerItem)
    table.setHorizontalScrollMode(QAbstractItemView.ScrollMode.ScrollPerItem)
    QAbstractScrollArea.setVerticalScrollBarPolicy(table, Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    QAbstractScrollArea.setHorizontalScrollBarPolicy(table, Qt.ScrollBarPolicy.ScrollBarAsNeeded)

    if disable_hover:
        table.setMouseTracking(False)
        try:
            table.entered.disconnect()
        except TypeError:
            pass
