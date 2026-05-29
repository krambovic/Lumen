from __future__ import annotations

from PyQt6.QtWidgets import QAbstractItemView


def tune_fluent_table_scroll(table: QAbstractItemView, *, disable_hover: bool = False) -> None:
    """Keep Fluent scrollbars, but remove slow wheel inertia for large tables."""
    scroll_delegate = getattr(table, "scrollDelagate", None)
    if scroll_delegate is not None:
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

    table.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerItem)
    table.setHorizontalScrollMode(QAbstractItemView.ScrollMode.ScrollPerItem)

    if disable_hover:
        table.setMouseTracking(False)
        try:
            table.entered.disconnect()
        except TypeError:
            pass
