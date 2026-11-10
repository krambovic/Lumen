//! Autostart registration (port of `windows/xray_fluent/startup.py`).
//!
//! HKCU Run + StartupApproved registry entries, the elevated scheduled-task
//! fallback, the AppCompatFlags "always run as admin" layer and the one-time
//! legacy rebrand cleanup. The Rust client always ships as a compiled
//! executable, so the Python `sys.frozen` branches are the live ones.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use std::collections::HashSet;

#[cfg(windows)]
use anyhow::{anyhow, Result};
#[cfg(not(windows))]
use anyhow::Result;

#[cfg(windows)]
use crate::subprocess::run_text_owned;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const STARTUP_APPROVED_RUN_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const APP_COMPAT_LAYERS_KEY: &str =
    r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";
pub const RUNASADMIN_FLAG: &str = "RUNASADMIN";
pub const TASK_NAME: &str = "Lumen";
pub const SCHTASKS_TIMEOUT: f64 = 15.0;
pub const STARTUP_STATE_ABSENT: &str = "absent";
pub const STARTUP_STATE_ENABLED: &str = "enabled";
pub const STARTUP_STATE_DISABLED: &str = "disabled";

pub const LEGACY_APP_NAMES: [&str; 4] = ["Lumen KVN", "LumenKVN", "lumen-kvn", "Lumen_KVN"];
pub const LEGACY_EXECUTABLE_NAMES: [&str; 2] = ["lumenkvn.exe", "lumenkvn-qml.exe"];
pub const LEGACY_PROTOCOL_KEYS: [&str; 5] = [
    r"Software\Classes\lumen-kvn",
    r"Software\Classes\AppUserModelId\Lumen.LumenKVN",
    r"Software\Classes\Applications\LumenKVN.exe",
    r"Software\Microsoft\Windows\CurrentVersion\App Paths\LumenKVN.exe",
    r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\Lumen.LumenKVN",
];

static LEGACY_STARTUP_WAS_DISABLED: AtomicBool = AtomicBool::new(false);
static LEGACY_CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

/// The Python client distinguishes the interpreter run from the frozen
/// launcher; the Rust client is always the compiled executable.
fn is_frozen() -> bool {
    true
}

/// `Path.resolve()` without the `\\?\` verbatim prefix, so registry command
/// strings stay human-readable exactly like the Python client wrote them.
fn resolve_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(resolved) => {
            let text = resolved.as_os_str().to_string_lossy();
            if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{rest}"))
            } else if let Some(rest) = text.strip_prefix(r"\\?\") {
                PathBuf::from(rest)
            } else {
                resolved
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

/// `subprocess.list2cmdline`: MSVC command-line quoting rules.
pub fn list2cmdline(args: &[String]) -> String {
    let mut result = String::new();
    for arg in args {
        if !result.is_empty() {
            result.push(' ');
        }
        let need_quote = arg.contains(' ') || arg.contains('\t') || arg.is_empty();
        let mut bs_buf = 0usize;
        if need_quote {
            result.push('"');
        }
        for c in arg.chars() {
            if c == '\\' {
                bs_buf += 1;
            } else if c == '"' {
                // Double the pending backslashes, then escape the quote.
                for _ in 0..bs_buf * 2 {
                    result.push('\\');
                }
                bs_buf = 0;
                result.push_str("\\\"");
            } else {
                for _ in 0..bs_buf {
                    result.push('\\');
                }
                bs_buf = 0;
                result.push(c);
            }
        }
        if need_quote {
            // CPython flushes the pending backslashes once, then doubles them
            // again before the closing quote ("Add remaining backslashes",
            // then `result.extend(bs_buf)` inside the needquote branch).
            for _ in 0..bs_buf {
                result.push('\\');
            }
            for _ in 0..bs_buf {
                result.push('\\');
            }
            result.push('"');
        } else {
            for _ in 0..bs_buf {
                result.push('\\');
            }
        }
    }
    result
}

#[cfg(windows)]
fn legacy_shell_paths() -> Vec<PathBuf> {
    let env_dir = |name: &str| std::env::var(name).ok().map(|v| v.trim().to_string());
    let appdata = env_dir("APPDATA");
    let program_data = env_dir("ProgramData");
    let user_profile = env_dir("USERPROFILE");
    let public_profile = env_dir("PUBLIC");
    let mut paths: Vec<PathBuf> = Vec::new();
    for name in LEGACY_APP_NAMES {
        if let Some(appdata) = &appdata {
            let programs = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            paths.push(programs.join(name));
            paths.push(programs.join("Startup").join(format!("{name}.lnk")));
            paths.push(
                PathBuf::from(appdata)
                    .join("Microsoft")
                    .join("Internet Explorer")
                    .join("Quick Launch")
                    .join("User Pinned")
                    .join("TaskBar")
                    .join(format!("{name}.lnk")),
            );
        }
        if let Some(program_data) = &program_data {
            let programs = PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            paths.push(programs.join(name));
            paths.push(programs.join("Startup").join(format!("{name}.lnk")));
        }
        if let Some(user_profile) = &user_profile {
            paths.push(
                PathBuf::from(user_profile)
                    .join("Desktop")
                    .join(format!("{name}.lnk")),
            );
        }
        if let Some(public_profile) = &public_profile {
            paths.push(
                PathBuf::from(public_profile)
                    .join("Desktop")
                    .join(format!("{name}.lnk")),
            );
        }
    }
    // dict.fromkeys preserves the first occurrence of every path.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    paths
}

#[cfg(windows)]
fn cleanup_legacy_shell_entries() {
    for path in legacy_shell_paths() {
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// One-time rebrand migration: registry, scheduled tasks and shell entries
/// left by previous product names. Re-running it on every state poll would
/// spawn schtasks and walk the filesystem forever.
#[cfg(windows)]
pub fn cleanup_legacy_system_entries() {
    if LEGACY_CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    for name in LEGACY_APP_NAMES {
        if let Ok(key) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, winreg::enums::KEY_READ)
        {
            if let Ok(value) = key.get_raw_value(name) {
                if value.vtype == winreg::enums::REG_BINARY
                    && !value.bytes.is_empty()
                    && value.bytes[0] == 0x03
                {
                    LEGACY_STARTUP_WAS_DISABLED.store(true, Ordering::SeqCst);
                }
            }
        }
        delete_registry_startup(name);
        delete_startup_approved(name);
        let _ = run_text_owned(
            &[
                "schtasks".to_string(),
                "/Delete".to_string(),
                "/TN".to_string(),
                name.to_string(),
                "/F".to_string(),
            ],
            SCHTASKS_TIMEOUT,
            false,
        );
    }

    if let Ok(key) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            APP_COMPAT_LAYERS_KEY,
            winreg::enums::KEY_READ | winreg::enums::KEY_SET_VALUE,
        )
    {
        let mut to_delete: Vec<String> = Vec::new();
        for (val_name, _) in key.enum_values().filter_map(|item| item.ok()) {
            let executable_name = Path::new(val_name.trim().trim_matches('"'))
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if LEGACY_EXECUTABLE_NAMES.contains(&executable_name.as_str()) {
                to_delete.push(val_name);
            }
        }
        for val_name in to_delete {
            let _ = key.delete_value(&val_name);
        }
    }

    for key_path in LEGACY_PROTOCOL_KEYS {
        delete_registry_tree(&winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER), key_path);
    }
    cleanup_legacy_shell_entries();
}

#[cfg(not(windows))]
pub fn cleanup_legacy_system_entries() {}

#[cfg(windows)]
pub fn set_startup_enabled(app_name: &str, enabled: bool, command: &str) -> Result<()> {
    cleanup_legacy_system_entries();
    if enabled {
        if startup_requires_scheduled_task()? {
            // HKCU Run entries pointing at executables that require elevation
            // are silently skipped by Windows at logon, while Task Manager
            // keeps showing them as "Enabled". Lumen runs elevated (UAC
            // manifest / RUNASADMIN layer), so register a scheduled task with
            // the highest run level instead.
            // Keep a normal Run/StartupApproved entry as the discoverable
            // Startup Apps registration.  Windows 10 Task Manager does not
            // enumerate Task Scheduler entries, which made an enabled Lumen
            // disappear from its Startup tab.  Windows skips this elevated
            // command at logon; the highest-run-level task performs the launch.
            // If a Windows build launches both, the single-instance guard
            // immediately closes the duplicate.
            create_registry_startup(app_name, command)?;
            set_startup_approved(app_name, true)?;
            create_startup_task(command)?;
        } else {
            delete_startup_task();
            create_registry_startup(app_name, command)?;
            set_startup_approved(app_name, true)?;
        }
    } else {
        delete_startup_task();
        delete_registry_startup(app_name);
        delete_startup_approved(app_name);
    }
    LEGACY_STARTUP_WAS_DISABLED.store(false, Ordering::SeqCst);
    Ok(())
}

#[cfg(not(windows))]
pub fn set_startup_enabled(_app_name: &str, _enabled: bool, _command: &str) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn get_startup_state(app_name: &str) -> Result<&'static str> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    cleanup_legacy_system_entries();
    if !registry_startup_exists(app_name) {
        if app_name == TASK_NAME && startup_task_exists() {
            return Ok(STARTUP_STATE_ENABLED);
        }
        if app_name == TASK_NAME && LEGACY_STARTUP_WAS_DISABLED.load(Ordering::SeqCst) {
            return Ok(STARTUP_STATE_DISABLED);
        }
        return Ok(STARTUP_STATE_ABSENT);
    }
    let value = match winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_READ)
    {
        Ok(key) => key
            .get_raw_value(app_name)
            .map_err(|err| anyhow!("startup approved read failed: {err}"))?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(STARTUP_STATE_ENABLED),
        Err(err) => return Err(anyhow!("startup approved open failed: {err}")),
    };
    if value.vtype == winreg::enums::REG_BINARY && !value.bytes.is_empty() && value.bytes[0] == 0x03 {
        // Elevated installations intentionally keep a discoverable Run entry
        // beside the real highest-privilege task.  Windows can mark that
        // helper entry as disabled after replacing the executable, while the
        // task remains the effective startup mechanism.
        if app_name == TASK_NAME && startup_task_exists() {
            return Ok(STARTUP_STATE_ENABLED);
        }
        return Ok(STARTUP_STATE_DISABLED);
    }
    Ok(STARTUP_STATE_ENABLED)
}

#[cfg(not(windows))]
pub fn get_startup_state(_app_name: &str) -> Result<&'static str> {
    Ok(STARTUP_STATE_ABSENT)
}

#[cfg(windows)]
fn startup_requires_scheduled_task() -> Result<bool> {
    if is_process_elevated() {
        return Ok(true);
    }
    Ok(is_always_run_as_admin_enabled(None).unwrap_or(false))
}

#[cfg(not(windows))]
fn startup_requires_scheduled_task() -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
fn startup_task_exists() -> bool {
    run_text_owned(
        &[
            "schtasks".to_string(),
            "/Query".to_string(),
            "/TN".to_string(),
            TASK_NAME.to_string(),
        ],
        SCHTASKS_TIMEOUT,
        false,
    )
    .map(|result| result.returncode == 0)
    .unwrap_or(false)
}

/// `ctypes.windll.shell32.IsUserAnAdmin()`.
#[cfg(windows)]
pub fn is_process_elevated() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin() }.as_bool()
}

#[cfg(not(windows))]
pub fn is_process_elevated() -> bool {
    true
}

#[cfg(windows)]
pub fn relaunch_as_admin(extra_args: Option<&[String]>) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;

    let (executable, arguments, working_dir) = admin_launch_command(extra_args);
    // The Rust client is always the frozen build: show the window normally.
    let file = HSTRING::from(executable.as_os_str());
    let params = HSTRING::from(arguments.as_str());
    let dir = HSTRING::from(working_dir.as_os_str());
    let verb = windows::core::w!("runas");
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SHOW_WINDOW_CMD(1),
        )
    };
    (result.0 as isize) > 32
}

#[cfg(not(windows))]
pub fn relaunch_as_admin(_extra_args: Option<&[String]>) -> bool {
    false
}

#[cfg(windows)]
pub fn set_always_run_as_admin(enabled: bool, executable: Option<&Path>) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let target = match executable {
        Some(path) => resolve_path(path).to_string_lossy().into_owned(),
        None => current_executable_path().to_string_lossy().into_owned(),
    };
    let key = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(APP_COMPAT_LAYERS_KEY, KEY_READ | KEY_WRITE)?
        .0;
    let current: String = key.get_value(&target).unwrap_or_default();
    let updated = update_layer_value(&current, enabled);
    if !updated.is_empty() {
        key.set_value(&target, &updated)?;
    } else {
        match key.delete_value(&target) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_always_run_as_admin(_enabled: bool, _executable: Option<&Path>) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn is_always_run_as_admin_enabled(executable: Option<&Path>) -> Result<bool> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    let target = match executable {
        Some(path) => resolve_path(path).to_string_lossy().into_owned(),
        None => current_executable_path().to_string_lossy().into_owned(),
    };
    let key = match winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(APP_COMPAT_LAYERS_KEY, KEY_READ)
    {
        Ok(key) => key,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    let value: String = match key.get_value(&target) {
        Ok(value) => value,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    Ok(value
        .split_whitespace()
        .any(|part| part.to_uppercase() == RUNASADMIN_FLAG))
}

#[cfg(not(windows))]
pub fn is_always_run_as_admin_enabled(_executable: Option<&Path>) -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
fn current_executable_path() -> PathBuf {
    std::env::current_exe()
        .map(|path| resolve_path(&path))
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn current_executable_path() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

fn canonical_frozen_executable() -> PathBuf {
    let executable = resolve_path(&std::env::current_exe().unwrap_or_default());
    if let Some(parent) = executable.parent() {
        let canonical = parent.join("Lumen.exe");
        if canonical.is_file() {
            return resolve_path(&canonical);
        }
    }
    executable
}

fn admin_launch_command(extra_args: Option<&[String]>) -> (PathBuf, String, PathBuf) {
    // `--tray` describes an OS autostart, not the user's current visibility.
    // Carrying it into an elevation relaunch made the new window disappear.
    let mut args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| {
            !matches!(
                arg.as_str(),
                "--tray" | "--relaunch-as-admin" | "--relaunched"
            )
        })
        .collect();
    if let Some(extra_args) = extra_args {
        args.extend(extra_args.iter().cloned());
    }
    args.push("--relaunch-as-admin".to_string());
    if is_frozen() {
        let executable = canonical_frozen_executable();
        let working_dir = executable
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        return (executable, list2cmdline(&args), working_dir);
    }
    // Interpreter-only branch of the Python module; unreachable in the Rust
    // build, mirrored so the fallback stays explicit.
    let executable = resolve_path(&std::env::current_exe().unwrap_or_default());
    let working_dir = executable
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    (executable, list2cmdline(&args), working_dir)
}

/// `_update_layer_value`: normalize an AppCompatFlags Layers value around the
/// RUNASADMIN flag. Every other flag keeps its original spelling.
fn update_layer_value(value: &str, enabled: bool) -> String {
    let mut flags: Vec<(String, String)> = Vec::new();
    for part in value.split_whitespace() {
        if part.is_empty() || part == "~" {
            continue;
        }
        let key = part.to_uppercase();
        match flags.iter_mut().find(|(flag, _)| *flag == key) {
            Some((_, stored)) => *stored = part.to_string(),
            None => flags.push((key, part.to_string())),
        }
    }
    let runasadmin = RUNASADMIN_FLAG.to_string();
    if enabled {
        match flags.iter_mut().find(|(flag, _)| *flag == runasadmin) {
            Some((_, stored)) => *stored = runasadmin.clone(),
            None => flags.push((runasadmin.clone(), runasadmin)),
        }
    } else {
        flags.retain(|(flag, _)| *flag != runasadmin);
    }
    if flags.is_empty() {
        return String::new();
    }
    let joined: Vec<&str> = flags.iter().map(|(_, part)| part.as_str()).collect();
    format!("~ {}", joined.join(" "))
}

#[cfg(windows)]
fn delete_registry_startup(app_name: &str) {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let Ok(key) = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
    else {
        return;
    };
    match key.delete_value(app_name) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

#[cfg(not(windows))]
fn delete_registry_startup(_app_name: &str) {}

#[cfg(windows)]
fn delete_registry_tree(root: &winreg::RegKey, key_path: &str) {
    use winreg::enums::KEY_READ;

    let Ok(key) = root.open_subkey_with_flags(key_path, KEY_READ) else {
        return;
    };
    let children: Vec<String> = key.enum_keys().filter_map(|item| item.ok()).collect();
    drop(key);
    for child in children {
        delete_registry_tree(root, &format!(r"{key_path}\{child}"));
    }
    // winreg.DeleteKey(root, key_path): remove the (now empty) deepest key.
    if let Some((parent_path, name)) = key_path.rsplit_once('\\') {
        if let Ok(parent) =
            root.open_subkey_with_flags(parent_path, winreg::enums::KEY_SET_VALUE)
        {
            let _ = parent.delete_subkey(name);
        }
    }
}

#[cfg(not(windows))]
fn delete_registry_tree(_root: &(), _key_path: &str) {}

#[cfg(windows)]
fn create_registry_startup(app_name: &str, command: &str) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let key = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?
        .0;
    key.set_value(app_name, &command)?;
    Ok(())
}

#[cfg(not(windows))]
fn create_registry_startup(_app_name: &str, _command: &str) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn registry_startup_exists(app_name: &str) -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    match winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) {
        Ok(key) => key.get_raw_value(app_name).is_ok(),
        Err(_) => false,
    }
}

#[cfg(not(windows))]
fn registry_startup_exists(_app_name: &str) -> bool {
    false
}

#[cfg(windows)]
fn set_startup_approved(app_name: &str, enabled: bool) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, REG_BINARY};

    // 0x02 = enabled, 0x03 = disabled; the remaining bytes are FILETIME-ish
    // metadata and can be zero for entries created by the app itself.
    let first = if enabled { 0x02u8 } else { 0x03u8 };
    let payload: Vec<u8> = std::iter::once(first)
        .chain(std::iter::repeat(0u8).take(11))
        .collect();
    let key = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_SET_VALUE)?
        .0;
    key.set_raw_value(
        app_name,
        &winreg::RegValue {
            bytes: payload,
            vtype: REG_BINARY,
        },
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn set_startup_approved(_app_name: &str, _enabled: bool) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn delete_startup_approved(app_name: &str) {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let Ok(key) = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_SET_VALUE)
    else {
        return;
    };
    match key.delete_value(app_name) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

#[cfg(not(windows))]
fn delete_startup_approved(_app_name: &str) {}

/// Split a ``list2cmdline`` command into executable and arguments.
fn split_startup_command(command: &str) -> (String, String) {
    let text = command.trim();
    if let Some(rest) = text.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
        }
    }
    match text.split_once(' ') {
        Some((executable, arguments)) => (executable.to_string(), arguments.trim().to_string()),
        None => (text.to_string(), String::new()),
    }
}

fn xml_escape(text: &str) -> String {
    // xml.sax.saxutils.escape: only & < > are replaced.
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Task Scheduler definition for the elevated logon autostart.
///
/// ``schtasks /SC ONLOGON`` keeps Task Scheduler defaults that do not suit a
/// tray app: it refuses to start on battery, is stopped when the laptop
/// unplugs, is killed after 72 hours and runs at below-normal priority (which
/// the network cores inherit).  The XML definition removes those limits, binds
/// the trigger to the current user and starts in the program folder.
pub fn build_startup_task_xml(command: &str, user: &str) -> String {
    let (executable, arguments) = split_startup_command(command);
    let working_dir = if executable.is_empty() {
        String::new()
    } else {
        Path::new(&executable)
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let user_xml = xml_escape(user);
    let arguments_xml = if arguments.is_empty() {
        String::new()
    } else {
        format!("<Arguments>{}</Arguments>", xml_escape(&arguments))
    };
    let working_xml = if working_dir.is_empty() {
        String::new()
    } else {
        format!(
            "<WorkingDirectory>{}</WorkingDirectory>",
            xml_escape(&working_dir)
        )
    };
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n");
    xml.push_str(
        "<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n",
    );
    xml.push_str(
        "  <RegistrationInfo><Description>Lumen autostart</Description></RegistrationInfo>\n",
    );
    xml.push_str("  <Triggers>\n");
    xml.push_str(&format!(
        "    <LogonTrigger><Enabled>true</Enabled><UserId>{user_xml}</UserId></LogonTrigger>\n"
    ));
    xml.push_str("  </Triggers>\n");
    xml.push_str("  <Principals>\n");
    xml.push_str(&format!(
        "    <Principal id=\"Author\"><UserId>{user_xml}</UserId><LogonType>InteractiveToken</LogonType>"
    ));
    xml.push_str("<RunLevel>HighestAvailable</RunLevel></Principal>\n");
    xml.push_str("  </Principals>\n");
    xml.push_str("  <Settings>\n");
    xml.push_str("    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n");
    xml.push_str("    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n");
    xml.push_str("    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n");
    xml.push_str("    <AllowHardTerminate>true</AllowHardTerminate>\n");
    xml.push_str("    <StartWhenAvailable>false</StartWhenAvailable>\n");
    xml.push_str("    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n");
    xml.push_str("    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>\n");
    xml.push_str("    <AllowStartOnDemand>true</AllowStartOnDemand>\n");
    xml.push_str("    <Enabled>true</Enabled>\n");
    xml.push_str("    <Hidden>false</Hidden>\n");
    xml.push_str("    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n");
    xml.push_str("    <WakeToRun>false</WakeToRun>\n");
    xml.push_str("    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n");
    xml.push_str("    <Priority>5</Priority>\n");
    xml.push_str("  </Settings>\n");
    xml.push_str("  <Actions Context=\"Author\">\n");
    xml.push_str(&format!(
        "    <Exec><Command>{}</Command>{}{}</Exec>\n",
        xml_escape(&executable),
        arguments_xml,
        working_xml
    ));
    xml.push_str("  </Actions>\n");
    xml.push_str("</Task>\n");
    xml
}

/// Python `write_text(encoding="utf-16")`: little-endian UTF-16 with a BOM.
fn utf16_bom_bytes(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = Vec::with_capacity((text.chars().count() + 1) * 2);
    for unit in std::iter::once(0xFEFFu16).chain(text.encode_utf16()) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[cfg(windows)]
fn create_startup_task(command: &str) -> Result<()> {
    delete_startup_task();
    let file = tempfile::Builder::new()
        .prefix("lumen-task-")
        .suffix(".xml")
        .tempfile()?;
    let xml_path = file.path().to_path_buf();
    // Python: any failure inside the try (write or schtasks) falls back to the
    // ONLOGON command line; the temp file is removed either way.
    let run_result: Result<bool, anyhow::Error> =
        match std::fs::write(
            &xml_path,
            utf16_bom_bytes(&build_startup_task_xml(command, &current_task_user())),
        ) {
            Err(err) => Err(err.into()),
            Ok(()) => run_text_owned(
                &[
                    "schtasks".to_string(),
                    "/Create".to_string(),
                    "/TN".to_string(),
                    TASK_NAME.to_string(),
                    "/XML".to_string(),
                    xml_path.to_string_lossy().into_owned(),
                    "/F".to_string(),
                ],
                SCHTASKS_TIMEOUT,
                false,
            )
            .map(|result| result.returncode == 0)
            .map_err(anyhow::Error::from),
        };
    drop(file); // the `finally: os.unlink` equivalent
    match run_result {
        Ok(true) => Ok(()),
        _ => create_startup_task_legacy(command),
    }
}

#[cfg(not(windows))]
fn create_startup_task(_command: &str) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn create_startup_task_legacy(command: &str) -> Result<()> {
    run_text_owned(
        &[
            "schtasks".to_string(),
            "/Create".to_string(),
            "/TN".to_string(),
            TASK_NAME.to_string(),
            "/SC".to_string(),
            "ONLOGON".to_string(),
            "/RL".to_string(),
            "HIGHEST".to_string(),
            "/RU".to_string(),
            current_task_user(),
            "/TR".to_string(),
            command.to_string(),
            "/F".to_string(),
        ],
        SCHTASKS_TIMEOUT,
        true,
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn create_startup_task_legacy(_command: &str) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn delete_startup_task() {
    let _ = run_text_owned(
        &[
            "schtasks".to_string(),
            "/Delete".to_string(),
            "/TN".to_string(),
            TASK_NAME.to_string(),
            "/F".to_string(),
        ],
        SCHTASKS_TIMEOUT,
        false,
    );
}

#[cfg(not(windows))]
fn delete_startup_task() {}

#[cfg(windows)]
fn current_task_user() -> String {
    let domain = std::env::var("USERDOMAIN")
        .unwrap_or_default()
        .trim()
        .to_string();
    let user = std::env::var("USERNAME")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !domain.is_empty() && !user.is_empty() {
        return format!("{domain}\\{user}");
    }
    if !user.is_empty() {
        return user;
    }
    interactive_user_name()
}

/// `os.getlogin()` fallback: the account name attached to the process token.
#[cfg(windows)]
fn interactive_user_name() -> String {
    use windows::core::PWSTR;
    use windows::Win32::System::WindowsProgramming::GetUserNameW;

    unsafe {
        let mut size = 0u32;
        let _ = GetUserNameW(None, &mut size);
        if size == 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; size as usize];
        if GetUserNameW(Some(PWSTR(buffer.as_mut_ptr())), &mut size).is_ok() {
            let len = buffer
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(size as usize);
            return String::from_utf16_lossy(&buffer[..len]);
        }
        String::new()
    }
}

#[cfg(not(windows))]
fn interactive_user_name() -> String {
    String::new()
}

#[cfg(windows)]
fn installed_executable_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for env_name in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(root) = std::env::var(env_name) {
            let root = root.trim();
            if !root.is_empty() {
                candidates.push(PathBuf::from(root).join("Lumen").join("Lumen.exe"));
            }
        }
    }
    candidates.push(PathBuf::from(r"C:\Program Files\Lumen\Lumen.exe"));
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\Lumen\Lumen.exe"));
    for candidate in candidates {
        if candidate.is_file() {
            return Some(resolve_path(&candidate));
        }
    }
    None
}

#[cfg(not(windows))]
fn installed_executable_path() -> Option<PathBuf> {
    None
}

pub fn build_startup_command(in_tray: bool) -> String {
    let tray_args: Vec<String> = if in_tray {
        vec!["--tray".to_string()]
    } else {
        Vec::new()
    };
    if is_frozen() {
        let exe = canonical_frozen_executable();
        let mut args = vec![exe.to_string_lossy().into_owned()];
        args.extend(tray_args);
        return list2cmdline(&args);
    }

    if let Some(installed_exe) = installed_executable_path() {
        let mut args = vec![installed_exe.to_string_lossy().into_owned()];
        args.extend(tray_args);
        return list2cmdline(&args);
    }

    // Interpreter-only tail from the Python module; the Rust build always
    // takes the frozen branch above.
    let base_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(Path::parent).map(Path::to_path_buf))
        .unwrap_or_default();
    let script = base_dir.join("run_qml.py");
    let venv_pythonw = base_dir.join(".venv").join("Scripts").join("pythonw.exe");
    let python_exe = if venv_pythonw.exists() {
        venv_pythonw
    } else {
        std::env::current_exe().unwrap_or_default()
    };
    let mut args = vec![
        python_exe.to_string_lossy().into_owned(),
        script.to_string_lossy().into_owned(),
    ];
    args.extend(tray_args);
    list2cmdline(&args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list2cmdline_quotes_like_python() {
        assert_eq!(list2cmdline(&[]), "");
        assert_eq!(
            list2cmdline(&[
                r"C:\Program Files\Lumen\Lumen.exe".to_string(),
                "--tray".to_string()
            ]),
            "\"C:\\Program Files\\Lumen\\Lumen.exe\" --tray"
        );
        assert_eq!(list2cmdline(&["".to_string()]), "\"\"");
        assert_eq!(list2cmdline(&["a b\"c\\".to_string()]), "\"a b\\\"c\\\\\"");
        // Unquoted argument keeps raw content; trailing backslashes stay.
        assert_eq!(list2cmdline(&[r"C:\a\".to_string()]), r"C:\a\");
    }

    #[test]
    fn split_startup_command_handles_quoted_paths() {
        let (exe, args) = split_startup_command(r#""C:\Program Files\Lumen\Lumen.exe" --tray"#);
        assert_eq!(exe, r"C:\Program Files\Lumen\Lumen.exe");
        assert_eq!(args, "--tray");
        let (exe, args) = split_startup_command("Lumen.exe");
        assert_eq!(exe, "Lumen.exe");
        assert_eq!(args, "");
    }

    #[test]
    fn task_xml_has_no_battery_time_or_priority_limits() {
        let xml = build_startup_task_xml(
            "\"C:\\Program Files\\Lumen\\Lumen.exe\" --tray",
            "PC\\user",
        );
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<Priority>5</Priority>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n"));
    }

    #[test]
    fn task_xml_binds_user_and_tray_launch() {
        let xml = build_startup_task_xml(
            "\"C:\\Program Files\\Lumen\\Lumen.exe\" --tray",
            "PC\\user",
        );
        assert!(xml.contains(
            "<LogonTrigger><Enabled>true</Enabled><UserId>PC\\user</UserId></LogonTrigger>"
        ));
        assert!(xml.contains(
            "<Principal id=\"Author\"><UserId>PC\\user</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal>"
        ));
        assert!(xml.contains("<Command>C:\\Program Files\\Lumen\\Lumen.exe</Command>"));
        assert!(xml.contains("<Arguments>--tray</Arguments>"));
        // Python's Path(executable).parent, verified against the module.
        assert!(xml.contains("<WorkingDirectory>C:\\Program Files\\Lumen</WorkingDirectory>"));
    }

    #[test]
    fn task_xml_escapes_paths() {
        let xml = build_startup_task_xml("\"D:\\R&D <x>\\Lumen.exe\"", "PC\\user");
        assert!(xml.contains("<Command>D:\\R&amp;D &lt;x&gt;\\Lumen.exe</Command>"));
        assert!(!xml.contains("<Arguments>"));
        assert!(xml.contains("<WorkingDirectory>D:\\R&amp;D &lt;x&gt;</WorkingDirectory>"));
    }

    #[test]
    fn layer_value_toggles_runasadmin() {
        assert_eq!(update_layer_value("", true), "~ RUNASADMIN");
        assert_eq!(update_layer_value("~ RUNASADMIN", false), "");
        assert_eq!(
            update_layer_value("~ DXHTTP00 WIN8RTM", true),
            "~ DXHTTP00 WIN8RTM RUNASADMIN"
        );
        assert_eq!(update_layer_value("~ RUNASADMIN DXHTTP00", false), "~ DXHTTP00");
        // An existing flag keeps its position, only the payload is normalized.
        assert_eq!(update_layer_value("~ runasadmin", true), "~ RUNASADMIN");
    }

    #[test]
    fn utf16_payload_carries_bom() {
        let bytes = utf16_bom_bytes("<Task/>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        assert_eq!(bytes.len(), 2 + "<Task/>".encode_utf16().count() * 2);
    }

    #[cfg(windows)]
    #[test]
    fn registry_roundtrip_and_state_transitions() {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;

        // Unique sandbox name so parallel test runs cannot collide.
        let app_name = format!("LumenStartupTest{}", std::process::id());
        let command = r#""C:\Program Files\Lumen\Lumen.exe" --tray"#;
        set_startup_enabled(&app_name, true, command).expect("enable startup");
        assert_eq!(get_startup_state(&app_name).unwrap(), STARTUP_STATE_ENABLED);
        assert!(registry_startup_exists(&app_name));

        set_startup_enabled(&app_name, false, command).expect("disable startup");
        assert_eq!(get_startup_state(&app_name).unwrap(), STARTUP_STATE_ABSENT);
        assert!(!registry_startup_exists(&app_name));

        // Leave no test residue even if an assertion above failed midway.
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_SET_VALUE)
        {
            let _ = key.delete_value(&app_name);
        }
    }
}
