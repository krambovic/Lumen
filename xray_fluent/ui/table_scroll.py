from __future__ import annotations

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import QAbstractItemView, QAbstractScrollArea

try:  # Qt ships QScroller on Windows; keep the helper harmless if bindings differ.
    from PyQt6.QtWidgets import QScroller
except Exception:  # pragma: no cover - depends on the installed Qt build
    QScroller = None  # type: ignore[assignment]


def _scroll_delegates(widget: object) -> list[object]:
    delegates: list[object] = []
    for attr in ("scrollDelagate", "scrollDelegate", "_scrollDelegate"):
        delegate = getattr(widget, attr, None)
        if delegate is not None and delegate not in delegates:
            delegates.append(delegate)
    return delegates


def _disable_fluent_smooth_scroll(widget: object) -> None:
    for scroll_delegate in _scroll_delegates(widget):
        smooth_mode = None
        for attr in ("verticalSmoothScroll", "horizonSmoothScroll"):
            smooth = getattr(scroll_delegate, attr, None)
            init = getattr(getattr(smooth, "__class__", object), "__init__", None)
            smooth_mode = getattr(init, "__globals__", {}).get("SmoothMode") if init else None
            if smooth_mode is not None:
                break

        no_smooth = getattr(smooth_mode, "NO_SMOOTH", None) if smooth_mode is not None else None
        for attr in ("verticalSmoothScroll", "horizonSmoothScroll"):
            smooth = getattr(scroll_delegate, attr, None)
            if smooth is None or not hasattr(smooth, "setSmoothMode"):
                continue
            try:
                smooth.setSmoothMode(no_smooth if no_smooth is not None else 0)
            except Exception:
                pass

        for attr in ("vScrollBar", "hScrollBar"):
            bar = getattr(scroll_delegate, attr, None)
            if bar is None:
                continue
            if hasattr(bar, "setScrollAnimation"):
                try:
                    bar.setScrollAnimation(0)
                except Exception:
                    pass
            animation = getattr(bar, "scrollAni", None)
            if animation is not None and hasattr(animation, "setDuration"):
                try:
                    animation.setDuration(0)
                except Exception:
                    pass


def _disable_kinetic_scroller(scroll_area: QAbstractScrollArea) -> None:
    if QScroller is None:
        return
    for target in (scroll_area, scroll_area.viewport()):
        try:
            QScroller.ungrabGesture(target)
        except Exception:
            pass


def tune_fluent_table_scroll(table: QAbstractItemView, *, disable_hover: bool = False) -> None:
    """Keep Fluent scrollbars, but remove slow wheel inertia for large tables."""
    _disable_fluent_smooth_scroll(table)
    _disable_kinetic_scroller(table)

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
    _disable_kinetic_scroller(scroll_area)
    scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
    scroll_area.setAttribute(Qt.WidgetAttribute.WA_Hover, False)
    scroll_area.viewport().setAttribute(Qt.WidgetAttribute.WA_Hover, False)
