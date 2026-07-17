from __future__ import annotations


def fit_window_geometry(
    width: int,
    height: int,
    x: int,
    y: int,
    minimum_width: int,
    minimum_height: int,
    available_x: int,
    available_y: int,
    available_width: int,
    available_height: int,
    *,
    position_saved: bool,
) -> dict[str, int]:
    """Fit a restored window into one monitor's usable desktop area."""
    aw = max(1, int(available_width))
    ah = max(1, int(available_height))
    ax = int(available_x)
    ay = int(available_y)

    min_width = min(aw, max(1, int(minimum_width)))
    min_height = min(ah, max(1, int(minimum_height)))
    fitted_width = max(min_width, min(max(1, int(width)), aw))
    fitted_height = max(min_height, min(max(1, int(height)), ah))

    if position_saved:
        fitted_x = max(ax, min(int(x), ax + aw - fitted_width))
        fitted_y = max(ay, min(int(y), ay + ah - fitted_height))
    else:
        fitted_x = ax + max(0, (aw - fitted_width) // 2)
        fitted_y = ay + max(0, (ah - fitted_height) // 2)

    return {
        "x": fitted_x,
        "y": fitted_y,
        "width": fitted_width,
        "height": fitted_height,
    }
