//! Window geometry restore (port of `qml_app/window_geometry.py`) plus the
//! monitor lookup that replaced `QGuiApplication.screens()` in
//! `AppBridge.fitWindowGeometry`.

use serde_json::{Map, Value, json};

/// Fit a restored window into one monitor's usable desktop area.
///
/// Ported 1:1 from `fit_window_geometry`; the tests in
/// `windows/tests/test_window_geometry.py` assert these exact numbers.
pub fn fit_window_geometry(
    width: i64,
    height: i64,
    x: i64,
    y: i64,
    minimum_width: i64,
    minimum_height: i64,
    available_x: i64,
    available_y: i64,
    available_width: i64,
    available_height: i64,
    position_saved: bool,
) -> Map<String, Value> {
    let aw = available_width.max(1);
    let ah = available_height.max(1);
    let ax = available_x;
    let ay = available_y;

    let min_width = aw.min(minimum_width.max(1));
    let min_height = ah.min(minimum_height.max(1));
    let fitted_width = min_width.max(width.clamp(1, aw));
    let fitted_height = min_height.max(height.clamp(1, ah));

    let (fitted_x, fitted_y) = if position_saved {
        (
            ax.max((ax + aw - fitted_width).min(x)),
            ay.max((ay + ah - fitted_height).min(y)),
        )
    } else {
        (
            ax + ((aw - fitted_width) / 2).max(0),
            ay + ((ah - fitted_height) / 2).max(0),
        )
    };

    Map::from_iter([
        ("x".to_string(), json!(fitted_x)),
        ("y".to_string(), json!(fitted_y)),
        ("width".to_string(), json!(fitted_width)),
        ("height".to_string(), json!(fitted_height)),
    ])
}

/// A monitor's usable work area (excludes the taskbar), in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorArea {
    pub fn primary() -> Option<MonitorArea> {
        monitor_work_area(primary_monitor().ok()?)
    }
}

#[cfg(windows)]
pub fn monitor_work_area(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<MonitorArea> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let rc = info.rcWork;
            return Some(MonitorArea {
                x: rc.left,
                y: rc.top,
                width: rc.right - rc.left,
                height: rc.bottom - rc.top,
            });
        }
    }
    None
}

#[cfg(not(windows))]
pub fn monitor_work_area(_monitor: ()) -> Option<MonitorArea> {
    None
}

#[cfg(windows)]
pub fn primary_monitor() -> Result<windows::Win32::Graphics::Gdi::HMONITOR, ()> {
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};

    Ok(unsafe { MonitorFromPoint(Default::default(), MONITOR_DEFAULTTOPRIMARY) })
}

#[cfg(not(windows))]
pub fn primary_monitor() -> Result<(), ()> {
    Err(())
}

#[cfg(windows)]
mod enumerate {
    use super::MonitorArea;
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::core::BOOL;

    /// Enumerate every monitor's work area. `QGuiApplication.screens()` order
    /// is not guaranteed to match, but the caller only compares intersection
    /// areas, so order does not matter.
    pub fn all_work_areas() -> Vec<MonitorArea> {
        let mut areas: Vec<MonitorArea> = Vec::new();
        // SAFETY: EnumDisplayMonitors only hands back HMONITOR/HDC handles and
        // the caller data pointer, which points at the live `areas` Vec kept
        // alive for the whole synchronous call.
        unsafe {
            let data = &mut areas as *mut Vec<MonitorArea>;
            let _ = EnumDisplayMonitors(None, None, Some(monitor_enum_proc), LPARAM(data as isize));
        }
        areas
    }

    unsafe extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let areas = unsafe { &mut *(lparam.0 as *mut Vec<MonitorArea>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(hmonitor, &mut info) }.as_bool() {
            let rc = info.rcWork;
            areas.push(MonitorArea {
                x: rc.left,
                y: rc.top,
                width: rc.right - rc.left,
                height: rc.bottom - rc.top,
            });
        }
        true.into()
    }
}

#[cfg(not(windows))]
mod enumerate {
    use super::MonitorArea;
    pub fn all_work_areas() -> Vec<MonitorArea> {
        Vec::new()
    }
}

pub use enumerate::all_work_areas;

fn intersection_area(a: &MonitorArea, rect: (i64, i64, i64, i64)) -> i64 {
    let (rx, ry, rw, rh) = rect;
    let left = i64::from(a.x).max(rx);
    let top = i64::from(a.y).max(ry);
    let right = (i64::from(a.x) + i64::from(a.width)).min(rx + rw);
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(ry + rh);
    (right - left).max(0) * (bottom - top).max(0)
}

/// `AppBridge.fitWindowGeometry`: pick the screen with the largest overlap of
/// the restored rect (falling back to `screenAt(x, y)` when there is no
/// overlap), then fit into its available geometry.
pub fn fit_window_geometry_on_screens(
    width: i64,
    height: i64,
    x: i64,
    y: i64,
    minimum_width: i64,
    minimum_height: i64,
) -> Map<String, Value> {
    let position_saved = x != -1 && y != -1;
    let screens = all_work_areas();
    let mut target_screen: Option<MonitorArea> = None;

    if position_saved && !screens.is_empty() {
        let restored = (x, y, width.max(1), height.max(1));
        let mut best: Option<(i64, &MonitorArea)> = None;
        for area in &screens {
            let overlap = intersection_area(area, restored);
            if best.map_or(true, |(best_overlap, _)| overlap > best_overlap) {
                best = Some((overlap, area));
            }
        }
        if let Some((overlap, screen)) = best {
            if overlap > 0 {
                target_screen = Some(*screen);
            }
        }
        if target_screen.is_none() {
            // screenAt(QPoint(x, y)): first screen containing the point.
            target_screen = screens.iter().copied().find(|a| {
                x >= a.x as i64
                    && x < (a.x + a.width) as i64
                    && y >= a.y as i64
                    && y < (a.y + a.height) as i64
            });
        }
    }

    let target_screen = target_screen.or_else(MonitorArea::primary);
    let Some(available) = target_screen else {
        return Map::from_iter([
            ("x".to_string(), json!(x.max(0))),
            ("y".to_string(), json!(y.max(0))),
            ("width".to_string(), json!(minimum_width.max(width))),
            ("height".to_string(), json!(minimum_height.max(height))),
        ]);
    };

    fit_window_geometry(
        width,
        height,
        x,
        y,
        minimum_width,
        minimum_height,
        available.x as i64,
        available.y as i64,
        available.width as i64,
        available.height as i64,
        position_saved,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(overrides: &[(&str, i64)]) -> Map<String, Value> {
        let mut values: Vec<(&str, i64)> = vec![
            ("width", 1280),
            ("height", 720),
            ("x", 40),
            ("y", 20),
            ("minimum_width", 640),
            ("minimum_height", 360),
            ("available_x", 0),
            ("available_y", 0),
            ("available_width", 1920),
            ("available_height", 1040),
        ];
        for (key, value) in overrides {
            values.retain(|(k, _)| *k != *key);
            values.push((key, *value));
        }
        let get = |key: &str| {
            values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| *v)
                .unwrap()
        };
        fit_window_geometry(
            get("width"),
            get("height"),
            get("x"),
            get("y"),
            get("minimum_width"),
            get("minimum_height"),
            get("available_x"),
            get("available_y"),
            get("available_width"),
            get("available_height"),
            true,
        )
    }

    fn ivalue(map: &Map<String, Value>, key: &str) -> i64 {
        map[key].as_i64().expect(key)
    }

    /// test_window_geometry.test_oversized_window_is_reduced_to_available_desktop
    #[test]
    fn oversized_window_is_reduced_to_available_desktop() {
        let fitted = fit(&[("available_width", 1093), ("available_height", 680)]);
        assert_eq!(ivalue(&fitted, "x"), 0);
        assert_eq!(ivalue(&fitted, "y"), 0);
        assert_eq!(ivalue(&fitted, "width"), 1093);
        assert_eq!(ivalue(&fitted, "height"), 680);
    }

    /// test_window_geometry.test_saved_position_is_clamped_above_taskbar
    #[test]
    fn saved_position_is_clamped_above_taskbar() {
        let fitted = fit(&[("y", 700)]);
        assert_eq!(ivalue(&fitted, "y"), 320);
        assert_eq!(ivalue(&fitted, "height"), 720);
    }

    /// test_window_geometry.test_unsaved_window_is_centered_in_available_area
    #[test]
    fn unsaved_window_is_centered_in_available_area() {
        let fitted = fit_window_geometry(1280, 720, -1, -1, 640, 360, 0, 0, 1920, 1040, false);
        assert_eq!(ivalue(&fitted, "x"), 320);
        assert_eq!(ivalue(&fitted, "y"), 160);
        assert_eq!(ivalue(&fitted, "width"), 1280);
        assert_eq!(ivalue(&fitted, "height"), 720);
    }

    /// test_window_geometry.test_negative_secondary_monitor_coordinates_are_preserved
    #[test]
    fn negative_secondary_monitor_coordinates_are_preserved() {
        let fitted =
            fit_window_geometry(1280, 720, -1800, 50, 640, 360, -1920, 0, 1920, 1080, true);
        assert_eq!(ivalue(&fitted, "x"), -1800);
        assert_eq!(ivalue(&fitted, "y"), 50);
    }

    /// test_window_geometry.test_tiny_available_area_wins_over_minimum_window_size
    #[test]
    fn tiny_available_area_wins_over_minimum_window_size() {
        let fitted = fit(&[("available_width", 500), ("available_height", 300)]);
        assert_eq!(ivalue(&fitted, "width"), 500);
        assert_eq!(ivalue(&fitted, "height"), 300);
    }

    /// The screen-picking helper used by `AppBridge.fitWindowGeometry`.
    #[test]
    fn intersection_area_matches_qrect_intersected() {
        let area = MonitorArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        assert_eq!(intersection_area(&area, (40, 20, 1280, 720)), 1280 * 720);
        assert_eq!(intersection_area(&area, (1900, 1000, 500, 500)), 20 * 40);
        assert_eq!(intersection_area(&area, (1920, 1040, 100, 100)), 0);
        let secondary = MonitorArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            intersection_area(&secondary, (-1800, 50, 1280, 720)),
            1280 * 720
        );
    }
}
