from __future__ import annotations

from xray_fluent.qml_app.window_geometry import fit_window_geometry


def _fit(**overrides: int | bool) -> dict[str, int]:
    values: dict[str, int | bool] = {
        "width": 1280,
        "height": 720,
        "x": 40,
        "y": 20,
        "minimum_width": 640,
        "minimum_height": 360,
        "available_x": 0,
        "available_y": 0,
        "available_width": 1920,
        "available_height": 1040,
        "position_saved": True,
    }
    values.update(overrides)
    return fit_window_geometry(**values)  # type: ignore[arg-type]


def test_oversized_window_is_reduced_to_available_desktop() -> None:
    fitted = _fit(available_width=1093, available_height=680)

    assert fitted == {"x": 0, "y": 0, "width": 1093, "height": 680}


def test_saved_position_is_clamped_above_taskbar() -> None:
    fitted = _fit(y=700)

    assert fitted["y"] == 320
    assert fitted["height"] == 720


def test_unsaved_window_is_centered_in_available_area() -> None:
    fitted = _fit(x=-1, y=-1, position_saved=False)

    assert fitted == {"x": 320, "y": 160, "width": 1280, "height": 720}


def test_negative_secondary_monitor_coordinates_are_preserved() -> None:
    fitted = _fit(
        x=-1800,
        y=50,
        available_x=-1920,
        available_width=1920,
        available_height=1080,
    )

    assert fitted["x"] == -1800
    assert fitted["y"] == 50


def test_tiny_available_area_wins_over_minimum_window_size() -> None:
    fitted = _fit(available_width=500, available_height=300)

    assert fitted["width"] == 500
    assert fitted["height"] == 300
