//! Owner-drawn tray context menu: the QSS of `QmlTray._apply_menu_style`
//! (palette, 12 px left / 28 px right item padding, 210 px minimum width,
//! 1 px separators with 2 px insets) painted into a native popup menu, so
//! keyboard navigation, submenus and dismissal stay native.

/// One row of the popup. `id == 0` is a separator.
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
}

/// `_apply_menu_style` colours as 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub background: u32,
    pub text: u32,
    pub selected: u32,
    pub disabled: u32,
    pub separator: u32,
}

impl Palette {
    pub fn for_theme(dark: bool) -> Self {
        if dark {
            Self { background: 0x242424, text: 0xFFFFFF, selected: 0x343434, disabled: 0x9A9A9A, separator: 0x3D3D3D }
        } else {
            Self { background: 0xFFFFFF, text: 0x1A1A1A, selected: 0xF0F0F0, disabled: 0x757575, separator: 0xE5E5E5 }
        }
    }
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
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, CreateSolidBrush, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject,
        DrawTextW, ExcludeClipRect, FillRect, GetDC, GetDeviceCaps, GetTextExtentPoint32W, HFONT, HGDIOBJ,
        LOGPIXELSY, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
    use windows::Win32::UI::Controls::{
        DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_SELECTED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        GetCursorPos, HMENU, MENUINFO, MF_DISABLED, MF_GRAYED, MF_OWNERDRAW, MF_POPUP,
        MIM_APPLYTOSUBMENUS, MIM_BACKGROUND, PostMessageW,
        RegisterClassExW, SetForegroundWindow, SetMenuInfo, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenuEx, WINDOW_EX_STYLE, WM_DRAWITEM, WM_MEASUREITEM, WM_NULL, WNDCLASSEXW, WS_EX_TOOLWINDOW,
        WS_POPUP,
    };
    use windows::core::{PCSTR, PCWSTR};

    use super::{MenuEntry, Palette};

    struct Row {
        label: Vec<u16>,
        separator: bool,
        submenu: bool,
    }

    struct State {
        owner: Option<HWND>,
        font: Option<HFONT>,
        scale: f32,
        palette: Palette,
        rows: Vec<Row>,
    }

    thread_local! {
        static STATE: RefCell<State> = const {
            RefCell::new(State {
                owner: None,
                font: None,
                scale: 1.0,
                palette: Palette { background: 0, text: 0, selected: 0, disabled: 0, separator: 0 },
                rows: Vec::new(),
            })
        };
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn colorref(rgb: u32) -> COLORREF {
        COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF))
    }

    /// uxtheme `SetPreferredAppMode` (ordinal 135) + `FlushMenuThemes`
    /// (136): the popup frame and its rounded border follow the app theme
    /// instead of the system one.
    fn set_menu_theme(dark: bool) {
        unsafe {
            let Ok(module) = LoadLibraryW(PCWSTR(wide("uxtheme.dll").as_ptr())) else {
                return;
            };
            if let Some(set_mode) = GetProcAddress(module, PCSTR(135usize as *const u8)) {
                let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
                // 2 = ForceDark, 3 = ForceLight.
                set_mode(if dark { 2 } else { 3 });
            }
            if let Some(flush) = GetProcAddress(module, PCSTR(136usize as *const u8)) {
                let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
                flush();
            }
        }
    }

    unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_MEASUREITEM => {
                let item = unsafe { &mut *(lparam.0 as *mut MEASUREITEMSTRUCT) };
                measure(item);
                LRESULT(1)
            }
            WM_DRAWITEM => {
                let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                draw(item);
                LRESULT(1)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn measure(item: &mut MEASUREITEMSTRUCT) {
        STATE.with(|state| {
            let state = state.borrow();
            let Some(row) = state.rows.get(item.itemData.wrapping_sub(1)) else {
                return;
            };
            let scale = state.scale;
            if row.separator {
                item.itemHeight = (13.0 * scale).round() as u32;
                item.itemWidth = 1;
                return;
            }
            let mut size = SIZE::default();
            unsafe {
                let hdc = GetDC(state.owner);
                let previous = state.font.map(|font| SelectObject(hdc, HGDIOBJ(font.0)));
                let text = &row.label[..row.label.len().saturating_sub(1)];
                let _ = GetTextExtentPoint32W(hdc, text, &mut size);
                if let Some(previous) = previous {
                    SelectObject(hdc, previous);
                }
                ReleaseDC(state.owner, hdc);
            }
            // Windows adds the submenu-arrow column to every item; `min-width:
            // 210px` covers the menu's own 6 px padding on both sides.
            let width = size.cx + ((12.0 + 28.0) * scale) as i32;
            let min_width = ((210.0 - 12.0) * scale) as i32;
            item.itemWidth = width.max(min_width) as u32;
            item.itemHeight = ((24.0 + 10.0) * scale).round() as u32;
        });
    }

    fn draw(item: &DRAWITEMSTRUCT) {
        STATE.with(|state| {
            let state = state.borrow();
            let Some(row) = state.rows.get(item.itemData.wrapping_sub(1)) else {
                return;
            };
            let palette = state.palette;
            let scale = state.scale;
            let hdc = item.hDC;
            let rect = item.rcItem;
            let disabled = (item.itemState.0 & (ODS_DISABLED.0 | ODS_GRAYED.0)) != 0;
            let selected = (item.itemState.0 & ODS_SELECTED.0) != 0 && !disabled && !row.separator;
            unsafe {
                let fill = CreateSolidBrush(colorref(if selected { palette.selected } else { palette.background }));
                FillRect(hdc, &rect, fill);
                let _ = DeleteObject(HGDIOBJ(fill.0));
                if row.separator {
                    let y = (rect.top + rect.bottom) / 2;
                    let inset = (2.0 * scale).round() as i32;
                    let line = RECT { left: rect.left + inset, top: y, right: rect.right - inset, bottom: y + (scale.round() as i32).max(1) };
                    let brush = CreateSolidBrush(colorref(palette.separator));
                    FillRect(hdc, &line, brush);
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                } else {
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, colorref(if disabled { palette.disabled } else { palette.text }));
                    let previous = state.font.map(|font| SelectObject(hdc, HGDIOBJ(font.0)));
                    let mut text_rect = RECT { left: rect.left + (12.0 * scale) as i32, ..rect };
                    let mut text = row.label[..row.label.len().saturating_sub(1)].to_vec();
                    DrawTextW(hdc, &mut text, &mut text_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
                    if let Some(previous) = previous {
                        SelectObject(hdc, previous);
                    }
                }
                if row.submenu {
                    // The label carries the QML "  ›"; keep Windows from
                    // drawing its own arrow over the item.
                    let _ = ExcludeClipRect(hdc, rect.left, rect.top, rect.right, rect.bottom);
                }
            }
        });
    }

    fn ensure_owner() -> Option<HWND> {
        if let Some(owner) = STATE.with(|state| state.borrow().owner) {
            return Some(owner);
        }
        unsafe {
            let module = GetModuleHandleW(PCWSTR::null()).ok()?;
            let instance = windows::Win32::Foundation::HINSTANCE(module.0);
            let class = wide("LumenTrayMenuOwner");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW | WINDOW_EX_STYLE(0),
                PCWSTR(class.as_ptr()),
                PCWSTR(class.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
            .ok()?;
            let hdc = GetDC(Some(owner));
            let scale = GetDeviceCaps(Some(hdc), LOGPIXELSY) as f32 / 96.0;
            ReleaseDC(Some(owner), hdc);
            // Qt's default QFont on Windows: Segoe UI 9 pt.
            let font = CreateFontW(
                -((12.0 * scale).round() as i32),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                windows::Win32::Graphics::Gdi::DEFAULT_CHARSET,
                windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS,
                windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS,
                windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY,
                0,
                PCWSTR(wide("Segoe UI").as_ptr()),
            );
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.owner = Some(owner);
                state.font = Some(font);
                state.scale = scale.max(1.0);
            });
            Some(owner)
        }
    }

    fn append(menu: HMENU, entries: &[MenuEntry]) {
        for entry in entries {
            let index = STATE.with(|state| {
                let mut state = state.borrow_mut();
                state.rows.push(Row {
                    label: wide(&entry.label),
                    separator: entry.is_separator(),
                    submenu: !entry.submenu.is_empty(),
                });
                state.rows.len()
            });
            let data = PCWSTR(index as *const u16);
            unsafe {
                if !entry.submenu.is_empty() {
                    let Ok(sub) = CreatePopupMenu() else {
                        continue;
                    };
                    append(sub, &entry.submenu);
                    let _ = AppendMenuW(menu, MF_OWNERDRAW | MF_POPUP, sub.0 as usize, data);
                } else if entry.is_separator() {
                    let _ = AppendMenuW(menu, MF_OWNERDRAW | MF_DISABLED, 0, data);
                } else {
                    let flags = if entry.enabled { MF_OWNERDRAW } else { MF_OWNERDRAW | MF_GRAYED };
                    let _ = AppendMenuW(menu, flags, entry.id as usize, data);
                }
            }
        }
    }

    /// Show the menu at the cursor (`QMenu.popup(QCursor.pos())`) and return
    /// the chosen entry id.
    pub fn popup(entries: &[MenuEntry], dark: bool) -> Option<u32> {
        let owner = ensure_owner()?;
        set_menu_theme(dark);
        let palette = Palette::for_theme(dark);
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.palette = palette;
            state.rows.clear();
        });
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            append(menu, entries);
            let background = CreateSolidBrush(colorref(palette.background));
            let info = MENUINFO {
                cbSize: std::mem::size_of::<MENUINFO>() as u32,
                fMask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS,
                hbrBack: background,
                ..Default::default()
            };
            let _ = SetMenuInfo(menu, &info);
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            // Without the foreground the popup would not close on an outside
            // click (TrackPopupMenu remarks).
            let _ = SetForegroundWindow(owner);
            let chosen = TrackPopupMenuEx(
                menu,
                (TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON).0,
                cursor.x,
                cursor.y,
                owner,
                None,
            );
            let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            let _ = DeleteObject(HGDIOBJ(background.0));
            let chosen = chosen.0 as u32;
            (chosen != 0).then_some(chosen)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_qss() {
        assert_eq!(Palette::for_theme(true).background, 0x242424);
        assert_eq!(Palette::for_theme(true).selected, 0x343434);
        assert_eq!(Palette::for_theme(false).text, 0x1A1A1A);
        assert_eq!(Palette::for_theme(false).separator, 0xE5E5E5);
    }

    #[test]
    fn separators_and_submenus_are_told_apart() {
        assert!(MenuEntry::separator().is_separator());
        assert!(!MenuEntry::submenu("Routing", vec![MenuEntry::item(7, "a", true)]).is_separator());
        assert!(!MenuEntry::item(3, "Quit", true).is_separator());
    }
}
