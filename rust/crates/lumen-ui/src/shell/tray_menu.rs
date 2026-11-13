//! Tray context menu: a port of the styled `QMenu` in `QmlTray`.
//!
//! A native popup menu cannot be styled past its items (Windows draws the
//! frame, the padding and the square corners itself), so the menu is its own
//! popup window painted with the `_apply_menu_style` stylesheet: 1 px border,
//! 6 px padding, 34 px items with 12 px / 28 px text insets, 13 px separator
//! rows, 210 px minimum width, and the routing submenu 6 px to the right of
//! its row.

/// One row of the popup. `id == 0` is a separator or a submenu parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: u32,
    pub label: String,
    pub enabled: bool,
    pub submenu: Vec<MenuEntry>,
}

impl MenuEntry {
    pub fn item(id: u32, label: impl Into<String>, enabled: bool) -> Self {
        Self { id, label: label.into(), enabled, submenu: Vec::new() }
    }

    pub fn separator() -> Self {
        Self { id: 0, label: String::new(), enabled: false, submenu: Vec::new() }
    }

    pub fn submenu(label: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self { id: 0, label: label.into(), enabled: !entries.is_empty(), submenu: entries }
    }

    fn is_separator(&self) -> bool {
        self.id == 0 && self.submenu.is_empty() && self.label.is_empty()
    }

    /// A row the pointer or the keyboard can select.
    fn selectable(&self) -> bool {
        !self.is_separator() && self.enabled
    }
}

/// `_apply_menu_style` colours as 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub background: u32,
    pub text: u32,
    pub border: u32,
    pub selected: u32,
    pub disabled: u32,
    pub separator: u32,
}

impl Palette {
    pub fn for_theme(dark: bool) -> Self {
        if dark {
            Self {
                background: 0x242424,
                text: 0xFFFFFF,
                border: 0x3B3B3B,
                selected: 0x343434,
                disabled: 0x9A9A9A,
                separator: 0x3D3D3D,
            }
        } else {
            Self {
                background: 0xFFFFFF,
                text: 0x1A1A1A,
                border: 0xD6D6D6,
                selected: 0xF0F0F0,
                disabled: 0x757575,
                separator: 0xE5E5E5,
            }
        }
    }
}

// Stylesheet metrics in logical pixels.
const BORDER: f32 = 1.0;
const PADDING: f32 = 6.0;
const ITEM_HEIGHT: f32 = 34.0; // min-height 24 + padding 5 + 5
const SEPARATOR_HEIGHT: f32 = 13.0; // height 1 + margin 6 + 6
const SEPARATOR_INSET: f32 = 2.0;
const TEXT_LEFT: f32 = 12.0;
const TEXT_RIGHT: f32 = 28.0;
const MIN_WIDTH: f32 = 210.0;
/// `actionGeometry.topRight() + QPoint(6, 0)`.
const SUBMENU_GAP: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    top: i32,
    height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Layout {
    width: i32,
    height: i32,
    rows: Vec<Row>,
}

fn px(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}

fn layout(entries: &[MenuEntry], scale: f32, text_width: impl Fn(&str) -> i32) -> Layout {
    let frame = px(BORDER + PADDING, scale);
    let widest = entries
        .iter()
        .filter(|entry| !entry.is_separator())
        .map(|entry| text_width(&entry.label))
        .max()
        .unwrap_or(0);
    let width = (widest + px(TEXT_LEFT + TEXT_RIGHT, scale) + 2 * frame).max(px(MIN_WIDTH, scale));
    let mut top = frame;
    let rows = entries
        .iter()
        .map(|entry| {
            let height = px(
                if entry.is_separator() { SEPARATOR_HEIGHT } else { ITEM_HEIGHT },
                scale,
            );
            let row = Row { top, height };
            top += height;
            row
        })
        .collect();
    Layout { width, height: top + frame, rows }
}

/// Next selectable row after `from` in `step` direction, wrapping around.
fn step_selection(entries: &[MenuEntry], from: Option<usize>, step: isize) -> Option<usize> {
    let count = entries.len() as isize;
    if count == 0 {
        return None;
    }
    let mut index = match from {
        Some(index) => index as isize,
        None if step > 0 => -1,
        None => count,
    };
    for _ in 0..count {
        index = (index + step).rem_euclid(count);
        if entries[index as usize].selectable() {
            return Some(index as usize);
        }
    }
    None
}

#[cfg(windows)]
pub use imp::popup;

#[cfg(not(windows))]
pub fn popup(_entries: &[MenuEntry], _dark: bool) -> Option<u32> {
    None
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;

    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL, DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
        DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FillRect,
        FrameRect, GetDC, GetDeviceCaps, GetMonitorInfoW, GetTextExtentPoint32W, HDC, HFONT, HGDIOBJ,
        InvalidateRect, LOGPIXELSY, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
        OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_DOWN, VK_ESCAPE,
        VK_LBUTTON, VK_LEFT, VK_MBUTTON, VK_RBUTTON, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
        GetMessageW, GetWindowRect, KillTimer, MA_NOACTIVATE, MSG, PostMessageW, PostQuitMessage,
        RegisterClassExW, SW_SHOW, SW_SHOWNOACTIVATE, SetForegroundWindow, SetTimer, ShowWindow,
        TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP,
        WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NULL, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::PCWSTR;

    use super::{
        BORDER, Layout, MenuEntry, PADDING, Palette, SEPARATOR_INSET, SUBMENU_GAP, TEXT_LEFT, layout, px,
        step_selection,
    };

    const WM_MOUSELEAVE: u32 = 0x02A3;
    const OUTSIDE_CLICK_TIMER: usize = 1;

    struct Popup {
        hwnd: HWND,
        entries: Vec<MenuEntry>,
        layout: Layout,
        hot: Option<usize>,
    }

    impl Popup {
        fn row_at(&self, scale: f32, x: i32, y: i32) -> Option<usize> {
            let frame = px(BORDER + PADDING, scale);
            if x < frame || x >= self.layout.width - frame {
                return None;
            }
            self.layout
                .rows
                .iter()
                .position(|row| y >= row.top && y < row.top + row.height)
                .filter(|index| self.entries[*index].selectable())
        }
    }

    struct Session {
        palette: Palette,
        scale: f32,
        font: HFONT,
        main: Popup,
        sub: Option<Popup>,
        /// Row of `main` whose submenu is open.
        sub_row: Option<usize>,
        keyboard_in_sub: bool,
        result: Option<u32>,
        done: bool,
    }

    enum Input {
        Move(i32, i32),
        Leave,
        Click(i32, i32),
        Key(u16),
        Deactivated(HWND),
        OutsideClick,
    }

    enum Effect {
        Repaint(HWND),
        OpenSub { row: usize, select_first: bool },
        CloseSub,
        Finish,
    }

    thread_local! {
        static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    }

    impl Session {
        fn finish(&mut self, effects: &mut Vec<Effect>) {
            self.done = true;
            effects.push(Effect::Finish);
        }

        fn activate(&mut self, in_sub: bool, index: usize, effects: &mut Vec<Effect>) {
            let entries = if in_sub {
                match &self.sub {
                    Some(sub) => &sub.entries,
                    None => return,
                }
            } else {
                &self.main.entries
            };
            let Some(entry) = entries.get(index).filter(|entry| entry.selectable()) else {
                return;
            };
            if !entry.submenu.is_empty() {
                if self.sub_row != Some(index) {
                    effects.push(Effect::OpenSub { row: index, select_first: false });
                }
            } else {
                self.result = Some(entry.id);
                self.finish(effects);
            }
        }

        fn handle(&mut self, hwnd: HWND, input: Input) -> Vec<Effect> {
            let mut effects = Vec::new();
            let in_sub = self.sub.as_ref().is_some_and(|sub| sub.hwnd == hwnd);
            if !in_sub && self.main.hwnd != hwnd {
                return effects;
            }
            let scale = self.scale;
            match input {
                Input::Move(x, y) if in_sub => {
                    let sub = self.sub.as_mut().expect("submenu window");
                    let row = sub.row_at(scale, x, y);
                    if sub.hot != row {
                        sub.hot = row;
                        effects.push(Effect::Repaint(hwnd));
                    }
                    self.keyboard_in_sub = true;
                    // The parent row stays highlighted while its submenu is used.
                    if self.main.hot != self.sub_row {
                        self.main.hot = self.sub_row;
                        effects.push(Effect::Repaint(self.main.hwnd));
                    }
                }
                Input::Move(x, y) => {
                    let row = self.main.row_at(scale, x, y);
                    self.keyboard_in_sub = false;
                    if self.main.hot != row && (row.is_some() || self.sub.is_none()) {
                        self.main.hot = row;
                        effects.push(Effect::Repaint(hwnd));
                    }
                    if let Some(index) = row {
                        if !self.main.entries[index].submenu.is_empty() {
                            if self.sub_row != Some(index) {
                                effects.push(Effect::OpenSub { row: index, select_first: false });
                            }
                        } else if self.sub.is_some() {
                            // `action.hovered -> _hide_routing_menu`.
                            effects.push(Effect::CloseSub);
                        }
                    }
                }
                Input::Leave if in_sub => {
                    let sub = self.sub.as_mut().expect("submenu window");
                    if sub.hot.take().is_some() {
                        effects.push(Effect::Repaint(hwnd));
                    }
                }
                Input::Leave => {
                    let keep = if self.sub.is_some() { self.sub_row } else { None };
                    if self.main.hot != keep {
                        self.main.hot = keep;
                        effects.push(Effect::Repaint(hwnd));
                    }
                }
                Input::Click(x, y) => {
                    let popup = if in_sub { self.sub.as_ref().expect("submenu window") } else { &self.main };
                    if let Some(index) = popup.row_at(scale, x, y) {
                        self.activate(in_sub, index, &mut effects);
                    }
                }
                Input::Key(key) => self.key(key, &mut effects),
                Input::Deactivated(other) => {
                    let to_sub = self.sub.as_ref().is_some_and(|sub| sub.hwnd == other);
                    if !in_sub && !to_sub {
                        self.finish(&mut effects);
                    }
                }
                Input::OutsideClick => self.finish(&mut effects),
            }
            effects
        }

        fn key(&mut self, key: u16, effects: &mut Vec<Effect>) {
            let in_sub = self.keyboard_in_sub && self.sub.is_some();
            if key == VK_ESCAPE.0 {
                if self.sub.is_some() {
                    self.keyboard_in_sub = false;
                    effects.push(Effect::CloseSub);
                } else {
                    self.finish(effects);
                }
            } else if key == VK_DOWN.0 || key == VK_UP.0 {
                let step = if key == VK_DOWN.0 { 1 } else { -1 };
                let popup = if in_sub { self.sub.as_mut().expect("submenu window") } else { &mut self.main };
                popup.hot = step_selection(&popup.entries, popup.hot, step);
                effects.push(Effect::Repaint(popup.hwnd));
                if !in_sub && self.sub.is_some() && self.main.hot != self.sub_row {
                    effects.push(Effect::CloseSub);
                }
            } else if key == VK_RETURN.0 || key == VK_SPACE.0 || key == VK_RIGHT.0 {
                let popup = if in_sub { self.sub.as_ref().expect("submenu window") } else { &self.main };
                let Some(index) = popup.hot else {
                    return;
                };
                let parent = !popup.entries[index].submenu.is_empty();
                if parent && !in_sub {
                    self.keyboard_in_sub = true;
                    if self.sub_row != Some(index) {
                        effects.push(Effect::OpenSub { row: index, select_first: true });
                    } else if let Some(sub) = self.sub.as_mut() {
                        sub.hot = step_selection(&sub.entries, None, 1);
                        effects.push(Effect::Repaint(sub.hwnd));
                    }
                } else if key != VK_RIGHT.0 {
                    self.activate(in_sub, index, effects);
                }
            } else if key == VK_LEFT.0 && self.sub.is_some() {
                self.keyboard_in_sub = false;
                effects.push(Effect::CloseSub);
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn colorref(rgb: u32) -> COLORREF {
        COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF))
    }

    fn fill(hdc: HDC, rect: &RECT, rgb: u32) {
        unsafe {
            let brush = CreateSolidBrush(colorref(rgb));
            FillRect(hdc, rect, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }

    fn text_width(font: HFONT, text: &str) -> i32 {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            let hdc = GetDC(None);
            let previous = SelectObject(hdc, HGDIOBJ(font.0));
            let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
            SelectObject(hdc, previous);
            ReleaseDC(None, hdc);
        }
        size.cx
    }

    fn work_area(point: POINT) -> RECT {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
            let _ = GetMonitorInfoW(monitor, &mut info);
        }
        info.rcWork
    }

    fn paint(hwnd: HWND) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
        SESSION.with(|session| {
            let Ok(session) = session.try_borrow() else {
                return;
            };
            let Some(session) = session.as_ref() else {
                return;
            };
            let popup = match &session.sub {
                Some(sub) if sub.hwnd == hwnd => sub,
                _ if session.main.hwnd == hwnd => &session.main,
                _ => return,
            };
            let palette = session.palette;
            let scale = session.scale;
            let frame = px(BORDER + PADDING, scale);
            let Layout { width, height, rows } = &popup.layout;
            let client = RECT { left: 0, top: 0, right: *width, bottom: *height };
            fill(hdc, &client, palette.background);
            unsafe {
                let brush = CreateSolidBrush(colorref(palette.border));
                FrameRect(hdc, &client, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));
                SetBkMode(hdc, TRANSPARENT);
                let previous = SelectObject(hdc, HGDIOBJ(session.font.0));
                for (index, (entry, row)) in popup.entries.iter().zip(rows).enumerate() {
                    let rect = RECT {
                        left: frame,
                        top: row.top,
                        right: width - frame,
                        bottom: row.top + row.height,
                    };
                    if entry.is_separator() {
                        let inset = px(SEPARATOR_INSET, scale);
                        let y = row.top + (row.height - px(1.0, scale).max(1)) / 2;
                        let line = RECT {
                            left: rect.left + inset,
                            top: y,
                            right: rect.right - inset,
                            bottom: y + px(1.0, scale).max(1),
                        };
                        fill(hdc, &line, palette.separator);
                        continue;
                    }
                    if popup.hot == Some(index) && entry.enabled {
                        fill(hdc, &rect, palette.selected);
                    }
                    SetTextColor(hdc, colorref(if entry.enabled { palette.text } else { palette.disabled }));
                    // Qt centres the label one pixel lower than DT_VCENTER.
                    let nudge = px(1.0, scale);
                    let mut text_rect = RECT {
                        left: rect.left + px(TEXT_LEFT, scale),
                        top: rect.top + nudge,
                        bottom: rect.bottom + nudge,
                        ..rect
                    };
                    let mut text: Vec<u16> = entry.label.encode_utf16().collect();
                    DrawTextW(hdc, &mut text, &mut text_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
                }
                SelectObject(hdc, previous);
            }
        });
        unsafe {
            let _ = EndPaint(hwnd, &ps);
        }
    }

    fn dispatch(hwnd: HWND, input: Input) {
        let effects = SESSION.with(|session| {
            let Ok(mut session) = session.try_borrow_mut() else {
                return Vec::new();
            };
            session.as_mut().map(|session| session.handle(hwnd, input)).unwrap_or_default()
        });
        // Window calls re-enter the window procedure, so they run unborrowed.
        for effect in effects {
            match effect {
                Effect::Repaint(hwnd) => unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                },
                Effect::OpenSub { row, select_first } => open_sub(row, select_first),
                Effect::CloseSub => close_sub(),
                Effect::Finish => unsafe {
                    // Wake the modal loop in `popup`.
                    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                },
            }
        }
    }

    fn close_sub() {
        let (sub, main) = SESSION.with(|session| {
            let mut session = session.borrow_mut();
            let Some(session) = session.as_mut() else {
                return (None, None);
            };
            session.sub_row = None;
            (session.sub.take().map(|sub| sub.hwnd), Some(session.main.hwnd))
        });
        unsafe {
            if let Some(sub) = sub {
                let _ = DestroyWindow(sub);
            }
            if let Some(main) = main {
                let _ = InvalidateRect(Some(main), None, false);
            }
        }
    }

    fn open_sub(row: usize, select_first: bool) {
        close_sub();
        let Some((entries, font, scale, palette, main, row_top, main_width)) = SESSION.with(|session| {
            let session = session.borrow();
            let session = session.as_ref()?;
            let entries = session.main.entries.get(row)?.submenu.clone();
            Some((
                entries,
                session.font,
                session.scale,
                session.palette,
                session.main.hwnd,
                session.main.layout.rows.get(row)?.top,
                session.main.layout.width,
            ))
        }) else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let layout = layout(&entries, scale, |text| text_width(font, text));
        let mut origin = RECT::default();
        unsafe {
            let _ = GetWindowRect(main, &mut origin);
        }
        let frame = px(BORDER + PADDING, scale);
        // QRect::topRight() is the last pixel of the row, hence the -1.
        let mut x = origin.left + main_width - frame - 1 + px(SUBMENU_GAP, scale);
        let mut y = origin.top + row_top;
        let area = work_area(POINT { x: origin.left, y: origin.top });
        if x + layout.width > area.right {
            x = origin.left + frame - px(SUBMENU_GAP, scale) - layout.width;
        }
        y = y.min(area.bottom - layout.height).max(area.top);
        let Some(hwnd) = create_window(x, y, &layout, palette, true) else {
            return;
        };
        let hot = if select_first { step_selection(&entries, None, 1) } else { None };
        SESSION.with(|session| {
            if let Some(session) = session.borrow_mut().as_mut() {
                session.sub = Some(Popup { hwnd, entries, layout, hot });
                session.sub_row = Some(row);
                session.main.hot = Some(row);
            }
        });
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(main), None, false);
        }
    }

    fn point(lparam: LPARAM) -> (i32, i32) {
        let value = lparam.0 as u32;
        ((value & 0xFFFF) as i16 as i32, (value >> 16) as i16 as i32)
    }

    /// A press outside both popups closes the menu even when the popup could
    /// not take the foreground (no deactivation arrives then).
    fn pressed_outside() -> bool {
        let pressed = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0);
        if !pressed {
            return false;
        }
        let mut cursor = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut cursor);
        }
        let windows = SESSION.with(|session| {
            session
                .try_borrow()
                .ok()
                .and_then(|session| {
                    session
                        .as_ref()
                        .map(|session| [Some(session.main.hwnd), session.sub.as_ref().map(|sub| sub.hwnd)])
                })
                .unwrap_or([None, None])
        });
        !windows.into_iter().flatten().any(|hwnd| {
            let mut rect = RECT::default();
            unsafe {
                let _ = GetWindowRect(hwnd, &mut rect);
            }
            cursor.x >= rect.left && cursor.x < rect.right && cursor.y >= rect.top && cursor.y < rect.bottom
        })
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            // Clicks never change activation: the main popup is activated
            // when shown and the submenu must not take it away.
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE => {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut track);
                }
                let (x, y) = point(lparam);
                dispatch(hwnd, Input::Move(x, y));
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                dispatch(hwnd, Input::Leave);
                LRESULT(0)
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                let (x, y) = point(lparam);
                dispatch(hwnd, Input::Click(x, y));
                LRESULT(0)
            }
            WM_KEYDOWN => {
                dispatch(hwnd, Input::Key(wparam.0 as u16));
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == OUTSIDE_CLICK_TIMER => {
                if pressed_outside() {
                    dispatch(hwnd, Input::OutsideClick);
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    dispatch(hwnd, Input::Deactivated(HWND(lparam.0 as *mut _)));
                }
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn class_name() -> Vec<u16> {
        wide("LumenTrayMenu")
    }

    fn register_class() {
        thread_local! {
            static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        if REGISTERED.replace(true) {
            return;
        }
        unsafe {
            let Ok(module) = GetModuleHandleW(PCWSTR::null()) else {
                return;
            };
            let class = class_name();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(wnd_proc),
                hInstance: windows::Win32::Foundation::HINSTANCE(module.0),
                hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                    None,
                    windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
                )
                .unwrap_or_default(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        }
    }

    fn create_window(x: i32, y: i32, layout: &Layout, palette: Palette, submenu: bool) -> Option<HWND> {
        unsafe {
            let module = GetModuleHandleW(PCWSTR::null()).ok()?;
            let class = class_name();
            let mut ex_style = WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
            if submenu {
                ex_style |= WS_EX_NOACTIVATE;
            }
            let hwnd = CreateWindowExW(
                ex_style,
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WS_POPUP,
                x,
                y,
                layout.width,
                layout.height,
                None,
                None,
                Some(windows::Win32::Foundation::HINSTANCE(module.0)),
                None,
            )
            .ok()?;
            // Windows 11: the small popup radius and a border that follows it.
            // Older systems ignore both and keep the square painted border.
            let corner = DWMWCP_ROUNDSMALL;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&raw const corner).cast(),
                std::mem::size_of_val(&corner) as u32,
            );
            let border = colorref(palette.border);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                (&raw const border).cast(),
                std::mem::size_of_val(&border) as u32,
            );
            Some(hwnd)
        }
    }

    /// Show the menu at the cursor (`QMenu.popup(QCursor.pos())`) and return
    /// the chosen entry id. Runs a modal message loop like `QMenu.exec`.
    pub fn popup(entries: &[MenuEntry], dark: bool) -> Option<u32> {
        if SESSION.with(|session| session.borrow().is_some()) {
            return None;
        }
        register_class();
        let palette = Palette::for_theme(dark);
        let scale = unsafe {
            let hdc = GetDC(None);
            let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSY);
            ReleaseDC(None, hdc);
            (dpi as f32 / 96.0).max(1.0)
        };
        // Qt's default QFont on Windows: Segoe UI 9 pt.
        let font = unsafe {
            CreateFontW(
                -px(12.0, scale),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                PCWSTR(wide("Segoe UI").as_ptr()),
            )
        };
        let layout = layout(entries, scale, |text| text_width(font, text));
        let mut cursor = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut cursor);
        }
        // QMenu flips to the other side of the point when it does not fit.
        let area = work_area(cursor);
        let mut x = cursor.x;
        let mut y = cursor.y;
        if x + layout.width > area.right {
            x = cursor.x - layout.width;
        }
        if y + layout.height > area.bottom {
            y = cursor.y - layout.height;
        }
        x = x.min(area.right - layout.width).max(area.left);
        y = y.min(area.bottom - layout.height).max(area.top);

        let result = create_window(x, y, &layout, palette, false).and_then(|hwnd| {
            SESSION.with(|session| {
                *session.borrow_mut() = Some(Session {
                    palette,
                    scale,
                    font,
                    main: Popup { hwnd, entries: entries.to_vec(), layout, hot: None },
                    sub: None,
                    sub_row: None,
                    keyboard_in_sub: false,
                    result: None,
                    done: false,
                });
            });
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                SetTimer(Some(hwnd), OUTSIDE_CLICK_TIMER, 30, None);
                let mut msg = MSG::default();
                while !SESSION.with(|session| session.borrow().as_ref().is_none_or(|session| session.done)) {
                    if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        // Hand the quit request back to the application loop.
                        PostQuitMessage(msg.wParam.0 as i32);
                        break;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = KillTimer(Some(hwnd), OUTSIDE_CLICK_TIMER);
            }
            let session = SESSION.with(|session| session.borrow_mut().take());
            let session = session?;
            unsafe {
                if let Some(sub) = &session.sub {
                    let _ = DestroyWindow(sub.hwnd);
                }
                let _ = DestroyWindow(session.main.hwnd);
            }
            session.result
        });
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
        result.filter(|id| *id != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tray_entries() -> Vec<MenuEntry> {
        vec![
            MenuEntry::item(1, "Hide", true),
            MenuEntry::item(2, "Connect", true),
            MenuEntry::item(3, "Next", true),
            MenuEntry::submenu("Routing", vec![MenuEntry::item(100, "Global", true)]),
            MenuEntry::separator(),
            MenuEntry::item(4, "Admin", false),
            MenuEntry::separator(),
            MenuEntry::item(5, "Quit", true),
        ]
    }

    #[test]
    fn palette_matches_the_qss() {
        assert_eq!(Palette::for_theme(true).background, 0x242424);
        assert_eq!(Palette::for_theme(true).border, 0x3B3B3B);
        assert_eq!(Palette::for_theme(true).selected, 0x343434);
        assert_eq!(Palette::for_theme(false).text, 0x1A1A1A);
        assert_eq!(Palette::for_theme(false).separator, 0xE5E5E5);
    }

    /// The Qt menu measured 244 x 244 with a 190 px widest label and rows of
    /// 34, 34, 34, 34, 13, 34, 13 and 34 px.
    #[test]
    fn layout_matches_the_measured_qt_menu() {
        let layout = layout(&tray_entries(), 1.0, |_| 190);
        assert_eq!((layout.width, layout.height), (244, 244));
        let heights: Vec<i32> = layout.rows.iter().map(|row| row.height).collect();
        assert_eq!(heights, [34, 34, 34, 34, 13, 34, 13, 34]);
        assert_eq!(layout.rows[0].top, 7);
        // Short labels fall back to the 210 px minimum width.
        assert_eq!(super::layout(&tray_entries(), 1.0, |_| 40).width, 210);
    }

    #[test]
    fn keyboard_selection_skips_separators_and_disabled_rows() {
        let entries = tray_entries();
        assert_eq!(step_selection(&entries, None, 1), Some(0));
        assert_eq!(step_selection(&entries, Some(3), 1), Some(7));
        assert_eq!(step_selection(&entries, Some(7), 1), Some(0));
        assert_eq!(step_selection(&entries, Some(0), -1), Some(7));
        assert_eq!(step_selection(&entries, None, -1), Some(7));
    }

    #[test]
    fn separators_and_submenus_are_told_apart() {
        assert!(MenuEntry::separator().is_separator());
        assert!(!MenuEntry::submenu("Routing", vec![MenuEntry::item(7, "a", true)]).is_separator());
        assert!(!MenuEntry::item(3, "Quit", true).is_separator());
    }
}
