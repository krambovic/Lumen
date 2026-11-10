//! Local SOCKS5 proxy for Discord voice via the external `droute` component
//! (port of `windows/xray_fluent/discord_proxy_manager.py`).
//!
//! droute installs a Discord-local `version.dll` loader, `droute.dll` payload
//! and a Squirrel updater hook so Discord's TCP/UDP traffic goes through a
//! local SOCKS5 proxy. The droute binary itself stays external (GPL-3.0).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use lumen_core::constants::{base_dir, data_dir};
use lumen_platform::subprocess::{hidden_command, is_same_path, run_text_owned};

// ── Module constants ────────────────────────────────────────────────────────

pub const DROUTE_LEGACY_VERSION: &str = "1.1.2";
pub const DROUTE_SOURCE_URL: &str = "https://github.com/snowluwu/droute";

/// `INSTALL_DATA_DIR / "external" / "droute"` (payload shipped with Lumen).
pub fn droute_bundled_dir() -> PathBuf {
    base_dir().join("data").join("external").join("droute")
}

/// `DATA_DIR / "external" / "droute"` (installed copy; online updates land here).
pub fn droute_dir() -> PathBuf {
    data_dir().join("external").join("droute")
}

pub fn droute_exe() -> PathBuf {
    droute_dir().join("droute.exe")
}

pub fn droute_bundled_exe() -> PathBuf {
    let name = droute_exe()
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    droute_bundled_dir().join(name)
}

/// Copied alongside the checksummed payload; none of them is loaded as code.
pub const DROUTE_EXTRA_FILES: [&str; 5] = [
    "version.txt",
    "SHA256SUMS.txt",
    "droute.exe.config",
    "README.md",
    "LICENSE.txt",
];

pub fn droute_notice() -> PathBuf {
    droute_dir().join("README.droute.txt")
}

pub fn droute_version_file() -> PathBuf {
    droute_dir().join("version.txt")
}

pub const DROUTE_INSTALL_VERSION_FILE: &str = "Lumen.droute.version";
pub const LEGACY_DROUTE_INSTALL_VERSION_FILE: &str = "LumenKVN.droute.version";

const DISCORD_BRANCHES: [(&str, &str, &str); 3] = [
    ("stable", "Discord", "Discord.exe"),
    ("canary", "DiscordCanary", "DiscordCanary.exe"),
    ("ptb", "DiscordPTB", "DiscordPTB.exe"),
];

static SHA256_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9a-f]{64}$").expect("valid digest regex"));
static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").expect("valid digits regex"));

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordInstall {
    pub branch_id: String,
    pub root: PathBuf,
    pub app_dir: PathBuf,
    pub exe_path: PathBuf,
    pub process_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordProxyResult {
    pub ok: bool,
    pub message: String,
    pub affected: i64,
}

impl DiscordProxyResult {
    fn new(ok: bool, message: impl Into<String>) -> Self {
        Self {
            ok,
            message: message.into(),
            affected: 0,
        }
    }

    fn with_affected(ok: bool, message: impl Into<String>, affected: i64) -> Self {
        Self {
            ok,
            message: message.into(),
            affected,
        }
    }
}

// ── Discord install discovery ───────────────────────────────────────────────

fn version_key(path: &Path) -> Vec<u64> {
    let raw = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let raw = raw.strip_prefix("app-").unwrap_or(&raw);
    raw.split('.')
        .map(|chunk| chunk.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

fn local_appdata() -> PathBuf {
    if let Ok(value) = std::env::var("LOCALAPPDATA")
        && !value.trim().is_empty()
    {
        return PathBuf::from(value);
    }
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("AppData").join("Local")
}

fn read_dir_dirs(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

fn find_install(branch_id: &str) -> Option<DiscordInstall> {
    let Some((_, folder, exe_name)) = DISCORD_BRANCHES.iter().find(|(id, _, _)| *id == branch_id)
    else {
        return None;
    };
    let root = local_appdata().join(folder);
    if !root.is_dir() {
        return None;
    }
    let mut app_dirs: Vec<PathBuf> = read_dir_dirs(&root)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with("app-"))
                .unwrap_or(false)
        })
        .collect();
    if app_dirs.is_empty() {
        return None;
    }
    app_dirs.sort_by_key(|path| std::cmp::Reverse(version_key(path)));
    let app_dir = app_dirs.into_iter().next()?;
    let exe_path = app_dir.join(exe_name);
    if !exe_path.is_file() {
        return None;
    }
    Some(DiscordInstall {
        branch_id: branch_id.to_string(),
        root,
        app_dir,
        exe_path,
        process_name: (*exe_name).to_string(),
    })
}

/// All installed Discord branches in the stable/canary/ptb order.
pub fn find_installed_discords() -> Vec<DiscordInstall> {
    let mut installs = Vec::new();
    for (branch_id, _, _) in DISCORD_BRANCHES {
        if let Some(install) = find_install(branch_id) {
            installs.push(install);
        }
    }
    installs
}

/// Rename the old product-branded Discord marker without reinstalling droute.
pub fn migrate_legacy_droute_markers() {
    let installs = find_installed_discords();
    migrate_legacy_droute_markers_for(&installs);
}

fn migrate_legacy_droute_markers_for(installs: &[DiscordInstall]) {
    for install in installs {
        let legacy = install.root.join(LEGACY_DROUTE_INSTALL_VERSION_FILE);
        let current = install.root.join(DROUTE_INSTALL_VERSION_FILE);
        if !legacy.is_file() {
            continue;
        }
        let result = if current.is_file() {
            fs::remove_file(&legacy)
        } else {
            fs::rename(&legacy, &current)
        };
        if result.is_err() {
            continue;
        }
    }
}

// ── Bundled droute package ──────────────────────────────────────────────────

/// Kept for parity with the Python module; the notice is written on demand.
pub fn write_droute_notice() -> io::Result<()> {
    fs::write(
        droute_notice(),
        format!(
            "droute is an external GPL-3.0 component published at:\n{DROUTE_SOURCE_URL}\n\n\
             Lumen bundles the latest verified release and supports GitHub updates.\n\
             The external binary is used\n\
             to install a Discord-local version.dll loader, droute.dll payload and Squirrel\n\
             updater hook for Discord TCP/UDP SOCKS5 proxying.\n"
        ),
    )
}

fn droute_version_at(directory: &Path) -> String {
    let exe = directory.join("droute.exe");
    let size = fs::metadata(&exe).map(|meta| meta.len()).unwrap_or(0);
    if !exe.is_file() || size < 1024 {
        return String::new();
    }
    let version = fs::read_to_string(directory.join("version.txt"))
        .map(|text| text.trim().trim_start_matches('v').to_string())
        .unwrap_or_default();
    if version.is_empty() {
        DROUTE_LEGACY_VERSION.to_string()
    } else {
        version
    }
}

/// `(sha256, relative name)` rows of a droute SHA256SUMS.txt manifest.
fn droute_manifest_entries(directory: &Path) -> Result<Vec<(String, String)>> {
    let manifest = directory.join("SHA256SUMS.txt");
    let text = fs::read_to_string(&manifest).map_err(|exc| {
        anyhow!("У встроенного droute отсутствует список контрольных сумм. ({exc})")
    })?;
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let line = line.trim();
        let Some((expected, relative_name)) = line.split_once("  ") else {
            bail!("Список контрольных сумм droute поврежден.");
        };
        let relative_name = relative_name.trim();
        if !SHA256_LINE.is_match(expected) || relative_name.is_empty() {
            bail!("Список контрольных сумм droute поврежден.");
        }
        entries.push((expected.to_string(), relative_name.to_string()));
    }
    // An empty or truncated manifest would otherwise verify vacuously.
    if !entries.iter().any(|(_, name)| name == "droute.exe") {
        bail!("Список контрольных сумм droute поврежден.");
    }
    Ok(entries)
}

fn sha256_file_bytes(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

/// Verify every manifest-listed payload file of a droute directory.
pub(crate) fn verify_bundled_droute(directory: &Path) -> Result<()> {
    for (expected, relative_name) in droute_manifest_entries(directory)? {
        let source = directory.join(&relative_name);
        let resolved = fs::canonicalize(&source)
            .with_context(|| format!("Контрольная сумма droute не совпадает: {relative_name}"))?;
        let root = fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
        if !resolved.starts_with(&root) {
            bail!("Некорректный путь в списке контрольных сумм droute.");
        }
        if !source.is_file() || sha256_file_bytes(&source)? != expected {
            bail!("Контрольная сумма droute не совпадает: {relative_name}");
        }
    }
    Ok(())
}

fn current_droute_version() -> String {
    let exe = droute_exe();
    let size = fs::metadata(&exe).map(|meta| meta.len()).unwrap_or(0);
    if !exe.is_file() || size == 0 {
        return String::new();
    }
    let version = fs::read_to_string(droute_version_file())
        .map(|text| text.trim().trim_start_matches('v').to_string())
        .unwrap_or_default();
    if version.is_empty() {
        DROUTE_LEGACY_VERSION.to_string()
    } else {
        version
    }
}

fn droute_version_key(value: &str) -> Vec<u64> {
    DIGITS
        .find_iter(value)
        .filter_map(|found| found.as_str().parse().ok())
        .collect()
}

/// Version of the bundled droute package when its payload verifies.
pub fn get_bundled_droute_version(bundle_dir: Option<&Path>) -> String {
    let directory = bundle_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(droute_bundled_dir);
    let version = droute_version_at(&directory);
    if version.is_empty() {
        return String::new();
    }
    if verify_bundled_droute(&directory).is_err() {
        return String::new();
    }
    version
}

/// Unpack the bundled droute payload into the target directory.
pub fn install_bundled_droute(
    target_dir: Option<&Path>,
    bundle_dir: Option<&Path>,
) -> Result<String> {
    let source = bundle_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(droute_bundled_dir);
    let target = target_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(droute_dir);
    let version = droute_version_at(&source);
    if version.is_empty() {
        bail!("Встроенный droute не найден. Переустановите Lumen.");
    }
    verify_bundled_droute(&source)?;
    if is_same_path(Some(&source), Some(&target)) {
        return Ok(version);
    }

    fs::create_dir_all(&target)?;
    let manifest_names: Vec<String> = droute_manifest_entries(&source)?
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    let mut names: Vec<&str> = manifest_names.iter().map(String::as_str).collect();
    names.extend(DROUTE_EXTRA_FILES);
    for relative_name in names {
        let source_file = source.join(relative_name);
        if !source_file.is_file() {
            continue;
        }
        let destination = target.join(relative_name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let file_name = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let staged = destination.with_file_name(format!("{file_name}.lumen-new"));
        // copy to a staged name, then os.replace for an atomic swap
        let result =
            fs::copy(&source_file, &staged).and_then(|_| fs::rename(&staged, &destination));
        let _ = fs::remove_file(&staged);
        result?;
    }
    if droute_version_at(&target) != version {
        bail!("Не удалось развернуть встроенный droute.");
    }
    Ok(version)
}

/// Deploy the bundled droute when it is newer than the installed copy.
pub fn get_droute_bundle_version() -> String {
    let bundled_version = get_bundled_droute_version(None);
    let mut current_version = current_droute_version();
    if !bundled_version.is_empty()
        && (current_version.is_empty()
            || droute_version_key(&bundled_version) > droute_version_key(&current_version))
    {
        match install_bundled_droute(None, None) {
            Ok(_) => {}
            // Python catches OSError only; RuntimeError failures propagate.
            Err(error) if error.downcast_ref::<io::Error>().is_some() => {}
            Err(error) => {
                // Non-IO failures cannot be reported through this String API;
                // drop the staged install like a failed OSError would.
                let _ = error;
                current_version = current_droute_version();
                return current_version;
            }
        }
        current_version = current_droute_version();
    }
    current_version
}

/// Fetch a missing/legacy package, then verify every installed payload file.
pub async fn ensure_droute_bundle() -> Result<PathBuf> {
    let bundled_version = get_bundled_droute_version(None);
    let current = current_droute_version();
    if !bundled_version.is_empty()
        && (current.is_empty()
            || droute_version_key(&bundled_version) >= droute_version_key(&current))
    {
        if current.is_empty() || droute_version_key(&bundled_version) > droute_version_key(&current)
        {
            install_bundled_droute(None, None)?;
        }
        return Ok(droute_bundled_exe());
    }
    let verified =
        verify_bundled_droute(&droute_dir()).is_ok() && !current_droute_version().is_empty();
    if !verified {
        let cancellation = crate::core_resource_updater::UpdateCancellation::default();
        let result = crate::core_resource_updater::check_or_update_droute(
            true,
            None,
            None,
            None,
            &cancellation,
        )
        .await
        .map_err(|_| anyhow!("update cancelled"))?;
        if !matches!(
            result.status,
            crate::core_resource_updater::ResourceUpdateStatus::Updated
                | crate::core_resource_updater::ResourceUpdateStatus::UpToDate
        ) {
            bail!("{}", result.message);
        }
        verify_bundled_droute(&droute_dir())?;
    }
    Ok(droute_exe())
}

// ── Windows helpers ─────────────────────────────────────────────────────────

fn powershell_quote(value: impl AsRef<Path>) -> String {
    let text = value.as_ref().to_string_lossy().replace('\'', "''");
    format!("'{text}'")
}

fn run_powershell(
    script: &str,
    timeout: f64,
) -> Result<lumen_platform::subprocess::CompletedProcess> {
    let command: Vec<String> = vec![
        "powershell".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script.to_string(),
    ];
    run_text_owned(&command, timeout, false).map_err(|error| anyhow!("{error}"))
}

/// Python `_discord_processes()`: native snapshot first, PowerShell fallback.
fn discord_processes() -> Vec<Value> {
    if let Some(native) = discord_processes_win32() {
        return native;
    }
    discord_processes_powershell()
}

#[cfg(windows)]
fn discord_processes_powershell() -> Vec<Value> {
    let names = DISCORD_BRANCHES
        .iter()
        .map(|(_, _, exe)| format!("'{}'", exe))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$names = @({names}); \
         $items = @(Get-CimInstance Win32_Process | Where-Object {{ $names -contains $_.Name }} | \
         Select-Object ProcessId,Name,ExecutablePath); \
         if ($items.Count -eq 0) {{ '[]' }} else {{ $items | ConvertTo-Json -Compress }}"
    );
    let Ok(result) = run_powershell(&script, 6.0) else {
        return Vec::new();
    };
    if result.returncode != 0 {
        return Vec::new();
    }
    let text = result.output_text().trim().to_string();
    if text.is_empty() {
        return Vec::new();
    }
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    match data {
        Value::Object(_) => vec![data],
        Value::Array(items) => items.into_iter().filter(|item| item.is_object()).collect(),
        _ => Vec::new(),
    }
}

#[cfg(not(windows))]
fn discord_processes_powershell() -> Vec<Value> {
    Vec::new()
}

/// Native Toolhelp snapshot of running Discord processes.
#[cfg(windows)]
fn discord_processes_win32() -> Option<Vec<Value>> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let wanted: BTreeSet<String> = DISCORD_BRANCHES
        .iter()
        .map(|(_, _, exe)| exe.to_lowercase())
        .collect();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    // SAFETY: the snapshot handle is owned by this function only.
    let owned = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_err() {
        return Some(Vec::new());
    }
    let mut result: Vec<Value> = Vec::new();
    loop {
        let len = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if wanted.contains(&name.to_lowercase()) {
            let pid = entry.th32ProcessID;
            let path = query_process_path_win32(pid).unwrap_or_default();
            result.push(json!({
                "ProcessId": pid,
                "Name": name,
                "ExecutablePath": path,
            }));
        }
        if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
            break;
        }
    }
    drop(owned);
    Some(result)
}

#[cfg(windows)]
fn query_process_path_win32(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    if pid == 0 {
        return None;
    }
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.unwrap_or_default();
    if handle.is_invalid() {
        return None;
    }
    let result = (|| {
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        }
        .ok()?;
        let len = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(size as usize);
        Some(String::from_utf16_lossy(&buffer[..len]))
    })();
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

#[cfg(not(windows))]
fn discord_processes_win32() -> Option<Vec<Value>> {
    None
}

fn terminate_discord(install: &DiscordInstall) {
    let command: Vec<String> = vec![
        "taskkill".into(),
        "/F".into(),
        "/T".into(),
        "/IM".into(),
        install.process_name.clone(),
    ];
    let _ = run_text_owned(&command, 8.0, false);
}

fn launch_discord(install: &DiscordInstall) -> Result<()> {
    // Hand the launch to the shell. A direct CreateProcess would give Discord
    // (and its Squirrel updater) the Administrator token of the Lumen process.
    #[allow(unused_mut)]
    let mut command = hidden_command("explorer.exe");
    command.arg(&install.exe_path).current_dir(&install.app_dir);
    command
        .spawn()
        .context("не удалось запустить Discord через explorer.exe")?;
    Ok(())
}

fn process_pids_for_install(install: &DiscordInstall) -> Vec<u32> {
    let install_root = install.root.to_string_lossy().to_lowercase();
    let mut pids = BTreeSet::new();
    for item in discord_processes() {
        let name = item
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        if name != install.process_name.to_lowercase() {
            continue;
        }
        let exe_path = item
            .get("ExecutablePath")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        if !exe_path.contains(&install_root) {
            continue;
        }
        let pid = item
            .get("ProcessId")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        if pid > 0 {
            pids.insert(pid as u32);
        }
    }
    pids.into_iter().collect()
}

fn wait_for_discord_start(install: &DiscordInstall, timeout_sec: f64) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_sec.max(0.0));
    while Instant::now() < deadline {
        if !process_pids_for_install(install).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn wait_for_discord_exit(install: &DiscordInstall, timeout_sec: f64) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_sec.max(0.0));
    while Instant::now() < deadline {
        if process_pids_for_install(install).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn write_droute_registry(socks_port: i64) -> Result<()> {
    let script = format!(
        "$path = 'HKCU:\\Software\\droute'; \
         New-Item -Path $path -Force | Out-Null; \
         Set-ItemProperty -Path $path -Name Host -Type String -Value '127.0.0.1'; \
         Set-ItemProperty -Path $path -Name Port -Type DWord -Value {socks_port}; \
         Set-ItemProperty -Path $path -Name User -Type String -Value ''; \
         Set-ItemProperty -Path $path -Name Password -Type String -Value ''; \
         Set-ItemProperty -Path $path -Name ConnectTimeout -Type DWord -Value 5000; \
         Set-ItemProperty -Path $path -Name ReconnectInterval -Type DWord -Value 3000; \
         Set-ItemProperty -Path $path -Name RetryTimeout -Type DWord -Value 10000; \
         Set-ItemProperty -Path $path -Name LogLevel -Type DWord -Value 2"
    );
    let result = run_powershell(&script, 8.0)?;
    if result.returncode != 0 {
        let text = result.output_text();
        bail!(
            "{}",
            if text.is_empty() {
                "failed to write droute registry settings".to_string()
            } else {
                text
            }
        );
    }
    Ok(())
}

fn read_droute_registry_port() -> i64 {
    #[cfg(windows)]
    {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Software\\droute", KEY_READ)
            .and_then(|key| key.get_value::<u32, _>("Port"))
            .map(|value| value as i64)
            .unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Whether the droute payload of `install` matches the active bundle version.
pub(crate) fn droute_payload_installed(install: &DiscordInstall) -> bool {
    droute_payload_installed_with(
        install,
        &droute_version_file(),
        &get_droute_bundle_version(),
    )
}

/// Testable core of `_droute_payload_installed` with explicit bundle paths.
pub(crate) fn droute_payload_installed_with(
    install: &DiscordInstall,
    version_file: &Path,
    current_version: &str,
) -> bool {
    let files_installed = install.app_dir.join("version.dll").is_file()
        && install.app_dir.join("droute.dll").is_file()
        && install.root.join("Droute.UpdaterHook.dll").is_file()
        && install.root.join("Update.exe.config").is_file();
    if !files_installed {
        return false;
    }
    if current_version.is_empty() {
        return false;
    }
    let marker = install.root.join(DROUTE_INSTALL_VERSION_FILE);
    let installed_version = if !marker.is_file() {
        let legacy_marker = install.root.join(LEGACY_DROUTE_INSTALL_VERSION_FILE);
        match fs::read_to_string(&legacy_marker) {
            Ok(text) => {
                let installed = text.trim().to_string();
                let _ = fs::write(&marker, format!("{installed}\n"));
                let _ = fs::remove_file(&legacy_marker);
                installed
            }
            // Existing droute installations predate version markers. Keep them
            // as-is until the user explicitly downloads an update, which
            // creates version.txt.
            Err(_) => return !version_file.is_file(),
        }
    } else {
        match fs::read_to_string(&marker) {
            Ok(text) => {
                let _ = fs::remove_file(install.root.join(LEGACY_DROUTE_INSTALL_VERSION_FILE));
                text.trim().to_string()
            }
            Err(_) => return !version_file.is_file(),
        }
    };
    installed_version == current_version
}

/// Any leftover droute file counts; partial installs must be cleaned too.
pub(crate) fn droute_payload_present(install: &DiscordInstall) -> bool {
    if install.root.join("Droute.UpdaterHook.dll").is_file()
        || install.root.join("Update.exe.config").is_file()
        || install.root.join(DROUTE_INSTALL_VERSION_FILE).is_file()
        || install
            .root
            .join(LEGACY_DROUTE_INSTALL_VERSION_FILE)
            .is_file()
    {
        return true;
    }
    for app_dir in read_dir_dirs(&install.root) {
        if app_dir
            .file_name()
            .map(|name| name.to_string_lossy().starts_with("app-"))
            .unwrap_or(false)
            && (app_dir.join("version.dll").is_file() || app_dir.join("droute.dll").is_file())
        {
            return true;
        }
    }
    false
}

fn install_droute_payload(exe: &Path, install: &DiscordInstall) -> Result<()> {
    let app_dir = &install.app_dir;
    let branch_root = &install.root;

    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $asm = [System.Reflection.Assembly]::LoadFile({}); \
         $patchType = $asm.GetType('Droute.Core.PatchInstaller', $true); \
         $stream = $asm.GetManifestResourceStream('Droute.Installer.Properties.Resources.resources'); \
         $reader = New-Object System.Resources.ResourceReader($stream); \
         $items = @{{}}; foreach ($entry in $reader) {{ $items[$entry.Key] = $entry.Value }}; \
         $patch = @($patchType.GetMethods() | Where-Object {{ $_.Name -eq 'InstallPatch' -and $_.GetParameters()[1].ParameterType -eq [byte[]] }})[0]; \
         $hook = @($patchType.GetMethods() | Where-Object {{ $_.Name -eq 'InstallUpdaterHook' -and $_.GetParameters()[1].ParameterType -eq [byte[]] }})[0]; \
         $patch.Invoke($null, @({}, [byte[]]$items['Droute64'], $null)); \
         $hook.Invoke($null, @({}, [byte[]]$items['UpdaterHook'], [string]$items['UpdaterConfig'], $null)); \
         $reader.Close(); $stream.Close()",
        powershell_quote(exe),
        powershell_quote(app_dir),
        powershell_quote(branch_root),
    );
    let result = run_powershell(&script, 30.0)?;
    if result.returncode != 0 {
        let text = result.output_text();
        bail!(
            "{}",
            if text.is_empty() {
                format!("failed to install droute for {}", install.process_name)
            } else {
                text
            }
        );
    }
    let version = get_droute_bundle_version();
    if version.is_empty() {
        bail!("droute bundle version is unknown");
    }
    fs::write(
        branch_root.join(DROUTE_INSTALL_VERSION_FILE),
        format!("{version}\n"),
    )?;
    let _ = fs::remove_file(branch_root.join(LEGACY_DROUTE_INSTALL_VERSION_FILE));
    Ok(())
}

fn remove_droute_payload(install: &DiscordInstall) -> Result<Vec<String>> {
    let mut targets = vec![
        install.root.join("Droute.UpdaterHook.dll"),
        install.root.join("Update.exe.config"),
        install.root.join(DROUTE_INSTALL_VERSION_FILE),
        install.root.join(LEGACY_DROUTE_INSTALL_VERSION_FILE),
    ];
    for app_dir in read_dir_dirs(&install.root) {
        if app_dir
            .file_name()
            .map(|name| name.to_string_lossy().starts_with("app-"))
            .unwrap_or(false)
        {
            targets.push(app_dir.join("version.dll"));
            targets.push(app_dir.join("droute.dll"));
        }
    }
    let mut leftovers = Vec::new();
    for path in targets {
        let mut removed = false;
        // loader dll handle can outlive taskkill for a moment
        for _attempt in 0..12 {
            match fs::remove_file(&path) {
                Ok(()) => {
                    removed = true;
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    removed = true;
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    std::thread::sleep(Duration::from_millis(250));
                }
                Err(error) => return Err(error.into()),
            }
        }
        if !removed && path.is_file() {
            if let Some(name) = path.file_name() {
                leftovers.push(name.to_string_lossy().into_owned());
            }
        }
    }
    leftovers.sort();
    leftovers.dedup();
    Ok(leftovers)
}

// ── DiscordProxyManager ─────────────────────────────────────────────────────

/// Installs/removes the droute payload for every installed Discord branch.
#[derive(Debug, Default)]
pub struct DiscordProxyManager;

impl DiscordProxyManager {
    pub fn new() -> Self {
        Self
    }

    /// `enable(socks_port)`: prepare droute and (re)install the payload.
    pub async fn enable(&self, socks_port: i64) -> DiscordProxyResult {
        #[cfg(not(windows))]
        {
            let _ = socks_port;
            return DiscordProxyResult::new(false, "Discord proxy is only available on Windows");
        }
        #[cfg(windows)]
        {
            let installs = find_installed_discords();
            if installs.is_empty() {
                return DiscordProxyResult::new(false, "Discord not found");
            }
            if socks_port <= 0 {
                return DiscordProxyResult::new(false, "Lumen SOCKS5 port not found");
            }

            let target_port = socks_port;
            let prepared = async {
                let needs_install = installs
                    .iter()
                    .any(|install| !droute_payload_installed(install));
                let mut exe = if needs_install {
                    ensure_droute_bundle().await?
                } else {
                    droute_exe()
                };
                if !exe.is_file() {
                    exe = ensure_droute_bundle().await?;
                }
                let current_port = read_droute_registry_port();
                if current_port != target_port {
                    write_droute_registry(target_port)?;
                }
                Ok::<PathBuf, anyhow::Error>(exe)
            };
            let exe = match prepared.await {
                Ok(exe) => exe,
                Err(exc) => {
                    return DiscordProxyResult::new(
                        false,
                        format!("Failed to prepare droute: {exc:#}"),
                    );
                }
            };

            let mut affected: i64 = 0;
            let mut already_ready: i64 = 0;
            let mut errors: Vec<String> = Vec::new();
            for install in &installs {
                let outcome = (|| -> Result<bool> {
                    if droute_payload_installed(install) {
                        return Ok(true);
                    }
                    let was_running = !process_pids_for_install(install).is_empty();
                    terminate_discord(install);
                    install_droute_payload(&exe, install)?;
                    if was_running {
                        launch_discord(install)?;
                        wait_for_discord_start(install, 10.0);
                    }
                    Ok(false)
                })();
                match outcome {
                    Ok(true) => {
                        already_ready += 1;
                        affected += 1;
                    }
                    Ok(false) => {
                        affected += 1;
                    }
                    Err(exc) => {
                        errors.push(format!("{}: {exc:#}", install.process_name));
                    }
                }
            }

            if affected > 0 {
                let mut message =
                    format!("droute ready for Discord via SOCKS5 127.0.0.1:{target_port}");
                if already_ready > 0 && already_ready == affected {
                    message = format!(
                        "droute already active for Discord via SOCKS5 127.0.0.1:{target_port}"
                    );
                }
                if !errors.is_empty() {
                    let preview = errors
                        .iter()
                        .take(2)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ");
                    message.push_str(&format!("; partial errors: {preview}"));
                }
                return DiscordProxyResult::with_affected(true, message, affected);
            }
            DiscordProxyResult::new(
                false,
                if errors.is_empty() {
                    "Failed to enable Discord proxy".to_string()
                } else {
                    errors.join("; ")
                },
            )
        }
    }

    /// `disable()`: remove the droute payload from every Discord branch.
    pub fn disable(&self) -> DiscordProxyResult {
        #[cfg(not(windows))]
        {
            return DiscordProxyResult::new(false, "Discord proxy is only available on Windows");
        }
        #[cfg(windows)]
        {
            let installs = find_installed_discords();
            if installs.is_empty() {
                return DiscordProxyResult::new(false, "Discord not found");
            }
            let mut affected: i64 = 0;
            let mut errors: Vec<String> = Vec::new();
            for install in &installs {
                let outcome = (|| -> Result<Vec<String>> {
                    if !droute_payload_present(install) {
                        // nothing to remove, skip the needless Discord restart
                        return Ok(Vec::new());
                    }
                    let was_running = !process_pids_for_install(install).is_empty();
                    terminate_discord(install);
                    if was_running {
                        // version.dll stays locked until the process really exits
                        wait_for_discord_exit(install, 8.0);
                    }
                    let leftovers = remove_droute_payload(install)?;
                    if was_running {
                        launch_discord(install)?;
                        wait_for_discord_start(install, 10.0);
                    }
                    Ok(leftovers)
                })();
                match outcome {
                    Ok(leftovers) if leftovers.is_empty() => {
                        affected += 1;
                    }
                    Ok(leftovers) => {
                        errors.push(format!(
                            "{}: droute files still locked: {}",
                            install.process_name,
                            leftovers.join(", ")
                        ));
                    }
                    Err(exc) => {
                        errors.push(format!("{}: {exc:#}", install.process_name));
                    }
                }
            }
            if !errors.is_empty() {
                return DiscordProxyResult::new(false, errors.join("; "));
            }
            if affected > 0 {
                return DiscordProxyResult::with_affected(
                    true,
                    "droute removed from Discord",
                    affected,
                );
            }
            DiscordProxyResult::new(true, "droute is not installed in Discord")
        }
    }
}
