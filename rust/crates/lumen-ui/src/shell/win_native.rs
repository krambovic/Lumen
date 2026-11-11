//! Windows-native shell integration (port of the ctypes blocks and startup
//! helpers in `qml_app/main_qml.py`).
//!
//! Covers: explicit AppUserModelID, toast identity + `lumen:` protocol
//! registration, legacy install cleanup, the DWM frame/backdrop contract
//! (`_apply_mica`) and the hidden power-event window that replaces Qt's
//! `installNativeEventFilter(WindowsPowerEventFilter)`.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use lumen_core::constants::{APP_NAME, app_icon_path};

pub const APP_USER_MODEL_ID: &str = "Lumen.Lumen";

// DWM attribute values from `_apply_mica`.
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19; // builds < 18985
const DWMWA_MICA_EFFECT: u32 = 1029; // Win11 21H2 (build 22000)
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38; // Win11 22H2+ (build 22621)
const DWMSBT_NONE: u32 = 1;
const DWMSBT_MAINWINDOW: u32 = 2;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_TEXT_COLOR: u32 = 36;
const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
// RegOpenKeyExW access masks for the transparency probe below.
#[cfg(windows)]
const KEY_QUERY_VALUE: u32 = 0x0001;
// Caption/text COLORREFs (0x00BBGGRR), byte-identical to main_qml.py.
const CAPTION_DARK: u32 = 0x002C2631;
const CAPTION_LIGHT: u32 = 0x00F3F3F3;
const TEXT_DARK: u32 = 0x00F3F3F3;
const TEXT_LIGHT: u32 = 0x00202020;

// ── AppUserModelID + registration ────────────────────────────────────────

/// `_set_app_user_model_id()`: must run before the first window is created so
/// the taskbar groups Lumen with its own icon. Also registers the toast
/// identity and the `lumen:` protocol, exactly like the Python helper.
pub fn set_app_user_model_id() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        use windows::core::PCWSTR;
        let wide = to_wide(APP_USER_MODEL_ID);
        let _ = SetCurrentProcessExplicitAppUserModelID(PCWSTR(wide.as_ptr()));
    }
    register_aumid_toast_identity();
    register_toast_protocol();
}

/// `_register_aumid_toast_identity()`: friendly DisplayName + icon for the AUMID.
pub fn register_aumid_toast_identity() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_WRITE, RegCloseKey, RegCreateKeyExW,
        };

        let key_path = to_wide(r"Software\Classes\AppUserModelId\Lumen.Lumen");
        let mut key = HKEY::default();
        let opened = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                windows::core::PCWSTR(key_path.as_ptr()),
                None,
                windows::core::PCWSTR::null(),
                Default::default(),
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        };
        if opened.is_err() {
            return;
        }
        set_reg_string(key, "DisplayName", APP_NAME);
        let icon = app_icon_path();
        if icon.is_file() {
            set_reg_string(key, "IconUri", &icon.to_string_lossy());
        }
        unsafe {
            let _ = RegCloseKey(key);
        }
    }
}

/// `_protocol_launch_command()`: the Windows command/icon target for the
/// installed (packaged) launch layout. The Python source-tree branch pointed
/// at `run_qml.py` under the repo root; the Rust binary is the product
/// executable itself, so only the packaged command remains.
pub fn protocol_launch_command() -> Option<(String, std::path::PathBuf)> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    let canonical_exe = executable.with_file_name("Lumen.exe");
    let executable = if canonical_exe.is_file() {
        canonical_exe
    } else {
        executable
    };
    Some((
        format!("\"{}\" \"%1\"", executable.to_string_lossy()),
        executable,
    ))
}

/// `_register_toast_protocol()`: register deep links for installed and
/// portable launches. Returns the error text on failure (main_qml.py logs a
/// warning; here the caller does the same).
pub fn register_toast_protocol() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_WRITE, RegCloseKey, RegCreateKeyExW,
        };

        let Some((command, icon_target)) = protocol_launch_command() else {
            return Ok(());
        };
        let scheme = "lumen";

        let create = |sub: &str| -> Result<HKEY, String> {
            let path = to_wide(&format!(r"Software\Classes\{scheme}\{sub}"));
            let mut key = HKEY::default();
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    windows::core::PCWSTR(path.as_ptr()),
                    None,
                    windows::core::PCWSTR::null(),
                    Default::default(),
                    KEY_WRITE,
                    None,
                    &mut key,
                    None,
                )
            }
            .ok()
            .map_err(|e| e.to_string())?;
            Ok(key)
        };

        let root = create("")?;
        set_reg_string(root, "", "URL:Lumen Protocol");
        set_reg_string(root, "URL Protocol", "");
        unsafe {
            let _ = RegCloseKey(root);
        }

        let command_key = create(r"shell\open\command")?;
        set_reg_string(command_key, "", &command);
        unsafe {
            let _ = RegCloseKey(command_key);
        }

        if app_icon_path().is_file() {
            let icon_key = create("DefaultIcon")?;
            set_reg_string(
                icon_key,
                "",
                &format!("{},0", icon_target.to_string_lossy()),
            );
            unsafe {
                let _ = RegCloseKey(icon_key);
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[cfg(windows)]
fn set_reg_string(key: windows::Win32::System::Registry::HKEY, name: &str, value: &str) {
    use windows::Win32::System::Registry::{REG_SZ, RegSetValueExW};
    let name_wide = to_wide(name);
    let mut value_wide = to_wide(value);
    // RegSetValueExW expects a NUL-terminated buffer including the terminator.
    value_wide.push(0);
    unsafe {
        let _ = RegSetValueExW(
            key,
            windows::core::PCWSTR(name_wide.as_ptr()),
            None,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                value_wide.as_ptr().cast(),
                value_wide.len() * 2,
            )),
        );
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

// ── Legacy install cleanup ───────────────────────────────────────────────

/// `_cleanup_legacy_root_program_install()`: remove exact legacy product
/// directories left behind under the Program Files roots (frozen builds only).
pub fn cleanup_legacy_root_program_install() {
    #[cfg(windows)]
    {
        let Some(current_dir) = installed_directory() else {
            return;
        };
        std::thread::sleep(Duration::from_secs(3));
        let mut candidates = Vec::new();
        for key in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            let Some(root) = std::env::var_os(key)
                .and_then(|path| std::path::PathBuf::from(path).canonicalize().ok())
            else {
                continue;
            };
            for name in ["Lumen KVN", "LumenKVN", "lumen-kvn", "Lumen_KVN"] {
                candidates.push(root.join(name));
            }
        }
        if let Ok(root) = std::path::Path::new("C:/").canonicalize() {
            let legacy = root.join("Program");
            if ["Lumen.exe", "LumenKVN.exe"]
                .iter()
                .any(|name| legacy.join(name).is_file())
            {
                candidates.push(legacy);
            }
        }
        for candidate in candidates {
            if candidate != current_dir
                && candidate.is_dir()
                && candidate
                    .canonicalize()
                    .is_ok_and(|resolved| resolved == candidate)
            {
                let _ = std::fs::remove_dir_all(candidate);
            }
        }
    }
}

#[cfg(windows)]
fn installed_directory() -> Option<std::path::PathBuf> {
    use lumen_app::app_updater::{program_files_app_dir, registered_install_dir};
    let current = std::env::current_exe().ok()?.canonicalize().ok()?;
    let directory = current.parent()?;
    if ["portable", "portable.txt"]
        .iter()
        .any(|name| directory.join(name).is_file())
    {
        return None;
    }
    [registered_install_dir(), Some(program_files_app_dir())]
        .into_iter()
        .flatten()
        .filter_map(|path| path.canonicalize().ok())
        .find(|path| path == directory)
}

/// `_cleanup_legacy_executable_bridge()`: delete a stale `LumenKVN.exe` next
/// to the running executable once this process exits.
pub fn cleanup_legacy_executable_bridge() {
    #[cfg(windows)]
    {
        if installed_directory().is_none() {
            return;
        }
        let Ok(current_exe) = std::env::current_exe() else {
            return;
        };
        let legacy_exe = current_exe.with_file_name("LumenKVN.exe");
        if !legacy_exe.is_file() {
            return;
        }
        if current_exe != legacy_exe {
            let _ = std::fs::remove_file(&legacy_exe);
            return;
        }
        let escaped = legacy_exe.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Wait-Process -Id {} -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue",
            std::process::id(),
            escaped
        );
        spawn_hidden_powershell(&script);
    }
}

#[cfg(windows)]
fn spawn_hidden_powershell(script: &str) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn();
}

#[cfg(not(windows))]
fn spawn_hidden_powershell(_script: &str) {}

// ── Mica support detection (app_bridge.py is_mica_supported) ─────────────

/// Read one HKCU DWORD value (the shell needs `EnableTransparency` for the
/// Mica support probe and `AppsUseLightTheme` for the system color scheme).
#[cfg(windows)]
pub fn read_hkcu_dword(subkey: &str, value_name: &str) -> Option<u32> {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };

    let path = to_wide(subkey);
    let name = to_wide(value_name);
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(path.as_ptr()),
            None,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .ok()
        .ok()?;
    }
    let mut data: u32 = 0;
    let mut size = 4u32;
    let result = unsafe {
        RegQueryValueExW(
            key,
            windows::core::PCWSTR(name.as_ptr()),
            None,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result.ok().ok().map(|_| data)
}

/// `is_mica_supported()`: system supports the Mica backdrop and it is enabled.
pub fn is_mica_supported() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dwm::DwmIsCompositionEnabled;

        let Some((_, _, build)) = lumen_platform::os_info::windows_version() else {
            return false;
        };
        if build < 22000 {
            return false;
        }

        // 1. DWM composition
        if !unsafe { DwmIsCompositionEnabled() }.is_ok_and(|enabled| enabled.as_bool()) {
            return false;
        }

        // 2. High contrast mode
        #[repr(C)]
        struct HighContrastW {
            cb_size: u32,
            dw_flags: u32,
            lpsz_default_scheme: *mut u16,
        }
        const SPI_GETHIGHCONTRAST: u32 = 0x0042;
        const HCF_HIGHCONTRASTON: u32 = 0x0000_0001;
        let mut hc = HighContrastW {
            cb_size: std::mem::size_of::<HighContrastW>() as u32,
            dw_flags: 0,
            lpsz_default_scheme: std::ptr::null_mut(),
        };
        let user32 = unsafe { get_module_function("user32.dll", "SystemParametersInfoW") };
        if let Some(system_parameters_info) = user32 {
            type SystemParametersInfoFn =
                unsafe extern "system" fn(u32, u32, *mut std::ffi::c_void, u32) -> i32;
            let system_parameters_info: SystemParametersInfoFn =
                unsafe { std::mem::transmute(system_parameters_info) };
            let success = unsafe {
                system_parameters_info(
                    SPI_GETHIGHCONTRAST,
                    hc.cb_size,
                    &mut hc as *mut HighContrastW as *mut std::ffi::c_void,
                    0,
                )
            };
            if success != 0 && (hc.dw_flags & HCF_HIGHCONTRASTON) != 0 {
                return false;
            }
        }

        // 3. Transparency setting in the registry
        if read_hkcu_dword(
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "EnableTransparency",
        ) == Some(0)
        {
            return false;
        }

        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Resolve an exported function by module + name (used for the undocumented
/// `SetWindowCompositionAttribute` and uxtheme's `SetWindowTheme`).
#[cfg(windows)]
unsafe fn get_module_function(module: &str, name: &str) -> Option<usize> {
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::core::PCWSTR;

    let module_wide = to_wide(module);
    let mut name_bytes = name.as_bytes().to_vec();
    name_bytes.push(0); // NUL-terminated ANSI name for GetProcAddress
    let handle = unsafe { GetModuleHandleW(PCWSTR(module_wide.as_ptr())) }.ok()?;
    let proc = unsafe { GetProcAddress(handle, windows::core::PCSTR(name_bytes.as_ptr())) }?;
    Some(proc as usize)
}

// ── `_apply_mica` ────────────────────────────────────────────────────────

/// One DWM/USER32 operation planned by [`mica_ops`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MicaOp {
    /// Try each attribute in order, stopping on the first success.
    SetDarkModeAttrs {
        attrs: Vec<u32>,
        value: bool,
    },
    /// Win10 `SetWindowCompositionAttribute(WCA_USEDARKMODECOLORS)`.
    SetWindowCompositionAttribute(bool),
    /// uxtheme `SetWindowTheme`.
    SetWindowTheme(&'static str),
    DwmExtendFrameZeroMargins,
    DwmBorderColorNone,
    DwmCaptionColor(u32),
    DwmTextColor(u32),
    DwmSystemBackdrop(u32),
    DwmMicaEffect(bool),
    RedrawWindow,
}

/// Normalize `backdrop_name` exactly like `_apply_mica`:
/// strip/lowercase, acrylic->mica, unknown->mica.
pub fn normalize_backdrop(name: &str) -> &'static str {
    let name = name.trim().to_lowercase();
    let name = if name == "acrylic" {
        "mica"
    } else {
        name.as_str()
    };
    match name {
        "mica" | "solid" => match name {
            "mica" => "mica",
            _ => "solid",
        },
        _ => "mica",
    }
}

/// Pure planning of `_apply_mica`'s attribute sequence so the contract
/// asserted by `test_window_frame_styling.py` stays testable without a real
/// window.
pub fn mica_ops(build: u32, dark: bool, backdrop: &str) -> Vec<MicaOp> {
    let backdrop_name = normalize_backdrop(backdrop);
    let mut ops = Vec::new();

    let dark_mode_attributes = if build >= 22000 {
        vec![DWMWA_USE_IMMERSIVE_DARK_MODE]
    } else {
        vec![
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
        ]
    };
    ops.push(MicaOp::SetDarkModeAttrs {
        attrs: dark_mode_attributes,
        value: dark,
    });

    if build < 22000 {
        // The documented immersive-DWM attribute only became public on
        // Windows 11. The custom frame still needs the legacy Windows 10
        // composition flag; without it the resize frame remains light even
        // when the application is dark.
        ops.push(MicaOp::SetWindowCompositionAttribute(dark));
        ops.push(MicaOp::SetWindowTheme(if dark {
            "DarkMode_Explorer"
        } else {
            "Explorer"
        }));
    }

    if build >= 22000 {
        ops.push(MicaOp::DwmExtendFrameZeroMargins);
        // Prevent Windows from drawing the user's accent colour as a 1px
        // active-window line above the custom title bar.
        ops.push(MicaOp::DwmBorderColorNone);
        ops.push(MicaOp::DwmCaptionColor(if dark {
            CAPTION_DARK
        } else {
            CAPTION_LIGHT
        }));
        ops.push(MicaOp::DwmTextColor(if dark {
            TEXT_DARK
        } else {
            TEXT_LIGHT
        }));

        if build >= 22621 {
            let backdrop = match backdrop_name {
                "mica" => DWMSBT_MAINWINDOW,
                "solid" => DWMSBT_NONE,
                _ => DWMSBT_MAINWINDOW,
            };
            ops.push(MicaOp::DwmSystemBackdrop(backdrop));
        } else {
            // Older Win11 only exposes Mica on/off.
            ops.push(MicaOp::DwmMicaEffect(backdrop_name != "solid"));
        }
    }

    ops.push(MicaOp::RedrawWindow);
    ops
}

/// Apply a planned op sequence to a window handle (the real `_apply_mica`).
/// DWM failures are harmless: a solid backdrop is already active.
pub fn apply_mica_ops(hwnd: windows::Win32::Foundation::HWND, ops: &[MicaOp]) {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};

        #[repr(C)]
        struct Margins {
            cx_left_width: i32,
            cx_right_width: i32,
            cy_top_height: i32,
            cy_bottom_height: i32,
        }
        #[repr(C)]
        struct WindowCompositionAttributeData {
            attribute: i32,
            data: *mut std::ffi::c_void,
            size: usize,
        }

        let dwm_set = |attr: u32, value: *const std::ffi::c_void, size: u32| unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(attr as i32),
                value,
                size,
            );
        };

        for op in ops {
            match op {
                MicaOp::SetDarkModeAttrs { attrs, value } => {
                    let flag: i32 = *value as i32;
                    for attr in attrs {
                        let result = unsafe {
                            DwmSetWindowAttribute(
                                hwnd,
                                windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(*attr as i32),
                                &flag as *const i32 as *const std::ffi::c_void,
                                std::mem::size_of::<i32>() as u32,
                            )
                        };
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                MicaOp::SetWindowCompositionAttribute(value) => {
                    if let Some(func) = unsafe {
                        get_module_function("user32.dll", "SetWindowCompositionAttribute")
                    } {
                        type Fn = unsafe extern "system" fn(
                            windows::Win32::Foundation::HWND,
                            *const WindowCompositionAttributeData,
                        ) -> i32;
                        let func: Fn = unsafe { std::mem::transmute(func) };
                        let flag: i32 = *value as i32;
                        let data = WindowCompositionAttributeData {
                            attribute: 26, // WCA_USEDARKMODECOLORS (Windows 10 compatibility API)
                            data: &flag as *const i32 as *mut std::ffi::c_void,
                            size: std::mem::size_of::<i32>(),
                        };
                        unsafe { func(hwnd, &data) };
                    }
                }
                MicaOp::SetWindowTheme(theme) => {
                    if let Some(func) =
                        unsafe { get_module_function("uxtheme.dll", "SetWindowTheme") }
                    {
                        type Fn = unsafe extern "system" fn(
                            windows::Win32::Foundation::HWND,
                            windows::core::PCWSTR,
                            windows::core::PCWSTR,
                        ) -> i32;
                        let func: Fn = unsafe { std::mem::transmute(func) };
                        let theme_wide = to_wide(theme);
                        unsafe {
                            func(
                                hwnd,
                                windows::core::PCWSTR(theme_wide.as_ptr()),
                                windows::core::PCWSTR::null(),
                            )
                        };
                    }
                }
                MicaOp::DwmExtendFrameZeroMargins => {
                    let margins = Margins {
                        cx_left_width: 0,
                        cx_right_width: 0,
                        cy_top_height: 0,
                        cy_bottom_height: 0,
                    };
                    unsafe {
                        let _ = DwmExtendFrameIntoClientArea(
                            hwnd,
                            &margins as *const Margins
                                as *const windows::Win32::UI::Controls::MARGINS,
                        );
                    }
                }
                MicaOp::DwmBorderColorNone => {
                    let color = DWMWA_COLOR_NONE;
                    dwm_set(DWMWA_BORDER_COLOR, &color as *const u32 as _, 4);
                }
                MicaOp::DwmCaptionColor(color) => {
                    dwm_set(DWMWA_CAPTION_COLOR, color as *const u32 as _, 4);
                }
                MicaOp::DwmTextColor(color) => {
                    dwm_set(DWMWA_TEXT_COLOR, color as *const u32 as _, 4);
                }
                MicaOp::DwmSystemBackdrop(backdrop) => {
                    dwm_set(DWMWA_SYSTEMBACKDROP_TYPE, backdrop as *const u32 as _, 4);
                }
                MicaOp::DwmMicaEffect(enable) => {
                    let value = *enable as i32;
                    dwm_set(DWMWA_MICA_EFFECT, &value as *const i32 as _, 4);
                }
                MicaOp::RedrawWindow => {
                    use windows::Win32::Graphics::Gdi::{
                        RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow,
                    };
                    unsafe {
                        let _ = RedrawWindow(
                            Some(hwnd),
                            None,
                            None,
                            RDW_INVALIDATE | RDW_UPDATENOW | RDW_FRAME,
                        );
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        let _ = ops;
    }
}

/// `_apply_mica(window, dark, backdrop)`: apply the native frame colours on
/// Windows 10 and the backdrop on Windows 11.
pub fn apply_mica(hwnd: windows::Win32::Foundation::HWND, dark: bool, backdrop: &str) {
    let Some((_, _, build)) = lumen_platform::os_info::windows_version() else {
        return;
    };
    let ops = mica_ops(build, dark, backdrop);
    apply_mica_ops(hwnd, &ops);
}

/// Locate the shell's own top-level window by process id + title (eframe owns
/// the HWND; winit names the window after the viewport title).
pub fn find_main_window(title: &str) -> Option<windows::Win32::Foundation::HWND> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
        };

        struct Query<'a> {
            title: &'a str,
            found: Option<HWND>,
        }
        let mut query = Query { title, found: None };
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut query as *mut _ as isize));
        }
        return query.found;

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
            let query = unsafe { &mut *(lparam.0 as *mut Query) };
            let mut pid = 0u32;
            let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid != std::process::id() {
                return true.into();
            }
            let mut buf = [0u16; 256];
            let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
            let window_title = String::from_utf16_lossy(&buf[..len.max(0) as usize]);
            if window_title == query.title {
                query.found = Some(hwnd);
                return false.into();
            }
            true.into()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = title;
        None
    }
}

// ── Hidden power-event window ────────────────────────────────────────────

/// `_activate_window` Win32 tail: restore the tray-hidden shell window and
/// take the foreground (ShowWindow SW_RESTORE + BringWindowToTop +
/// SetForegroundWindow via `lumen_platform::single_instance::activate_window`).
pub fn activate_shell_window() {
    #[cfg(windows)]
    {
        if let Some(hwnd) = find_main_window(APP_NAME) {
            lumen_platform::single_instance::activate_window(hwnd);
        }
    }
}

/// A raw (message id, wParam) pair observed by the hidden window thread. The
/// shell replays it through `WindowsPowerEventFilter::native_event_filter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerMessage {
    pub message: u32,
    pub wparam: usize,
}

/// Handle for stopping the hidden window thread (WM_CLOSE).
pub struct PowerEventWindow {
    hwnd: usize,
}

impl PowerEventWindow {
    /// Post WM_CLOSE; the thread destroys the window and exits.
    pub fn close(&self) {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
            let _ = PostMessageW(Some(HWND(self.hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        #[cfg(not(windows))]
        let _ = self.hwnd;
    }
}

/// Spawn the hidden top-level window that receives `WM_POWERBROADCAST`,
/// `WM_QUERYENDSESSION` and `WM_ENDSESSION`. Qt's native event filter saw the
/// process-wide messages; egui has no filter hook, so a dedicated message
/// window replays them into the shell loop.
pub fn spawn_power_event_window() -> (Receiver<PowerMessage>, Option<PowerEventWindow>) {
    let (tx, rx) = std::sync::mpsc::channel::<PowerMessage>();
    #[cfg(windows)]
    {
        let _ = std::thread::Builder::new()
            .name("lumen-power-events".into())
            .spawn(move || run_power_window(tx));
        // The thread registers its HWND in a static once it created the
        // window; wait briefly so early suspend events are not lost.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while POWER_WINDOW_HWND.get().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(not(windows))]
    {
        drop(tx);
    }
    (
        rx,
        POWER_WINDOW_HWND
            .get()
            .map(|hwnd| PowerEventWindow { hwnd: *hwnd }),
    )
}

static POWER_WINDOW_HWND: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

#[cfg(windows)]
fn run_power_window(tx: Sender<PowerMessage>) {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
        PostQuitMessage, RegisterClassExW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
        WM_DESTROY, WM_ENDSESSION, WM_POWERBROADCAST, WM_QUERYENDSESSION, WNDCLASSEXW,
    };
    use windows::core::PCWSTR;

    const CLASS_NAME: &str = "LumenPowerEventWindow";

    // The sender lives in a thread-local so the static wnd_proc reaches it.
    thread_local! {
        static TX: std::cell::RefCell<Option<Sender<PowerMessage>>> =
            const { std::cell::RefCell::new(None) };
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_POWERBROADCAST | WM_QUERYENDSESSION | WM_ENDSESSION => {
                TX.with(|slot| {
                    if let Some(sender) = slot.borrow().as_ref() {
                        let _ = sender.send(PowerMessage {
                            message: msg,
                            wparam: wparam.0,
                        });
                    }
                });
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    TX.with(|slot| *slot.borrow_mut() = Some(tx));

    unsafe {
        let hmodule = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);
        let class_name_wide = to_wide(CLASS_NAME);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(class_name_wide.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            return;
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name_wide.as_ptr()),
            PCWSTR(class_name_wide.as_ptr()),
            // Hidden top-level window: power broadcasts and session-end
            // notifications reach every top-level window, shown or not.
            WINDOW_STYLE(0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(hinstance),
            None,
        );
        let Ok(hwnd) = hwnd else {
            return;
        };
        let _ = POWER_WINDOW_HWND.set(hwnd.0 as usize);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(windows))]
fn run_power_window(_tx: Sender<PowerMessage>) {}

/// `_taskbar_present()` from tray.py: whether Explorer's taskbar exists and
/// can host notification icons.
pub fn taskbar_present() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        use windows::core::PCWSTR;
        let class = to_wide("Shell_TrayWnd");
        let hwnd = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null());
        hwnd.is_ok_and(|h| !h.0.is_null())
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// `_allow_taskbar_created_message()`: let Explorer's TaskbarCreated
/// broadcast reach an elevated Lumen.
pub fn allow_taskbar_created_message() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            ChangeWindowMessageFilter, MSGFLT_ADD, RegisterWindowMessageW,
        };
        use windows::core::PCWSTR;
        let name = to_wide("TaskbarCreated");
        let message = RegisterWindowMessageW(PCWSTR(name.as_ptr()));
        if message != 0 {
            let _ = ChangeWindowMessageFilter(message, MSGFLT_ADD);
        }
    }
}

/// Delay helper used by the tray reinstall timers (1.5 s / 6 s in tray.py).
pub const TRAY_REINSTALL_DELAYS: [Duration; 2] =
    [Duration::from_millis(1500), Duration::from_millis(6000)];

#[cfg(test)]
mod tests {
    use super::*;

    /// test_window_frame_styling.test_windows_10_frame_uses_legacy_dark_mode_fallback
    #[test]
    fn windows_10_frame_uses_legacy_dark_mode_fallback() {
        let ops = mica_ops(19045, true, "mica");
        assert_eq!(
            ops,
            vec![
                MicaOp::SetDarkModeAttrs {
                    attrs: vec![20, 19],
                    value: true
                },
                MicaOp::SetWindowCompositionAttribute(true),
                MicaOp::SetWindowTheme("DarkMode_Explorer"),
                MicaOp::RedrawWindow,
            ]
        );
    }

    /// test_window_frame_styling.test_windows_11_keeps_native_border_and_backdrop_contract
    #[test]
    fn windows_11_keeps_native_border_and_backdrop_contract() {
        let ops = mica_ops(22621, true, "mica");
        assert_eq!(
            ops,
            vec![
                MicaOp::SetDarkModeAttrs {
                    attrs: vec![20],
                    value: true
                },
                MicaOp::DwmExtendFrameZeroMargins,
                MicaOp::DwmBorderColorNone,
                MicaOp::DwmCaptionColor(CAPTION_DARK),
                MicaOp::DwmTextColor(TEXT_DARK),
                MicaOp::DwmSystemBackdrop(DWMSBT_MAINWINDOW),
                MicaOp::RedrawWindow,
            ]
        );
    }

    /// test_window_frame_styling.test_windows10_light_theme_reverses_dark_composition
    #[test]
    fn windows10_light_theme_reverses_dark_composition() {
        let dark = mica_ops(19045, true, "mica");
        let light = mica_ops(19045, false, "mica");
        assert!(matches!(
            dark[1],
            MicaOp::SetWindowCompositionAttribute(true)
        ));
        assert!(matches!(
            light[1],
            MicaOp::SetWindowCompositionAttribute(false)
        ));
        assert_eq!(light[2], MicaOp::SetWindowTheme("Explorer"));
    }

    /// The dark-mode attribute fallback list only applies below Windows 11.
    #[test]
    fn win11_21h2_tries_only_the_public_attribute() {
        let ops = mica_ops(22000, true, "mica");
        assert_eq!(
            ops[0],
            MicaOp::SetDarkModeAttrs {
                attrs: vec![20],
                value: true
            }
        );
        // 21H2 (< 22621) uses the MICA_EFFECT toggle.
        assert_eq!(ops[ops.len() - 2], MicaOp::DwmMicaEffect(true));
    }

    #[test]
    fn backdrop_normalization_matches_main_qml() {
        assert_eq!(normalize_backdrop("mica"), "mica");
        assert_eq!(normalize_backdrop("acrylic"), "mica");
        assert_eq!(normalize_backdrop("solid"), "solid");
        assert_eq!(normalize_backdrop("unknown"), "mica");
        assert_eq!(normalize_backdrop(" MICA "), "mica");
    }

    #[test]
    fn solid_backdrop_maps_to_dwmsbt_none_on_22h2() {
        let ops = mica_ops(22621, true, "solid");
        assert!(ops.contains(&MicaOp::DwmSystemBackdrop(DWMSBT_NONE)));
    }

    #[test]
    fn caption_and_text_colors_match_the_python_bytes() {
        // 0x002C2631 dark caption / 0x00F3F3F3 light, text inverted.
        assert_eq!(CAPTION_DARK, 0x002C_2631);
        assert_eq!(CAPTION_LIGHT, 0x00F3_F3F3);
        assert_eq!(TEXT_DARK, 0x00F3_F3F3);
        assert_eq!(TEXT_LIGHT, 0x0020_2020);
    }

    #[test]
    fn protocol_command_targets_the_lumen_executable() {
        // When running under cargo test the exe is lumen-ui-tests..., so the
        // canonical sibling Lumen.exe is absent; the command must still quote
        // an existing path and keep the "%1" placeholder.
        if let Some((command, _)) = protocol_launch_command() {
            assert!(command.contains("\" \"%1\""), "{command}");
            assert!(command.starts_with('"'));
        }
    }
}
