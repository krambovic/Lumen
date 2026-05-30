from __future__ import annotations

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import QAbstractItemView, QAbstractScrollArea


def _disable_fluent_smooth_scroll(widget: object) -> None:
    scroll_delegate = getattr(widget, "scrollDelagate", None)
    if scroll_delegate is None:
        return

    smooth_mode = getattr(
        getattr(scroll_delegate, "verticalSmoothScroll", None),
        "__class__",
        object,
    ).__init__.__globals__.get("SmoothMode")
    if smooth_mode is not None:
        for attr in ("verticalSmoothScroll", "horizonSmoothScroll"):
            smooth = getattr(scroll_delegate, attr, None)
            if smooth is not None:
                smooth.setSmoothMode(smooth_mode.NO_SMOOTH)
    for attr in ("vScrollBar", "hScrollBar"):
        bar = getattr(scroll_delegate, attr, None)
        if bar is not None and hasattr(bar, "setScrollAnimation"):
            bar.setScrollAnimation(0)


def tune_fluent_table_scroll(table: QAbstractItemView, *, disable_hover: bool = False) -> None:
    """Keep Fluent scrollbars, but remove slow wheel inertia for large tables."""
    _disable_fluent_smooth_scroll(table)

    table.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
    table.setHorizontalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
    table.setAutoScroll(False)
    table.setAttribute(Qt.WidgetAttribute.WA_Hover, False)
    table.viewport().setAttribute(Qt.WidgetAttribute.WA_Hover, False)

    if disable_hover:
        table.setMouseTracking(False)
        table.viewport().setMouseTracking(False)
        try:
            table.entered.disconnect()
        except TypeError:
            pass


def tune_plain_scroll_area(scroll_area: QAbstractScrollArea) -> None:
    """Remove qfluentwidgets inertial scrolling from dense pages and text panes."""
    _disable_fluent_smooth_scroll(scroll_area)
    scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    scroll_area.setAttribute(Qt.WidgetAttribute.WA_Hover, False)
    scroll_area.viewport().setAttribute(Qt.WidgetAttribute.WA_Hover, False)
