//! Supervise the upstream sing-box binary (`engines/singbox/manager.py`).
//!
//! The Qt signals (`started`, `stopped`, `log_received`, `error`,
//! `state_changed`) become a broadcast [`SingboxEvent`] channel; lifecycle
//! calls stay synchronous and are meant to run on a worker thread, like the
//! Xray manager port.

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, PipeReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle, ThreadId};

type ReaderHandle = JoinHandle<()>;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use lumen_core::constants::{
    base_dir, data_dir, runtime_dir, singbox_config_file, singbox_path_default, PROXY_HOST,
    SINGBOX_CLASH_API_PORT, SINGBOX_LEGACY_TUN_INTERFACE_NAMES, SINGBOX_TUN_INTERFACE_NAME,
};
use lumen_core::core_log_limiter::CoreLogLimiter;
use lumen_core::log_utils::is_routine_core_log;
use lumen_core::path_utils::resolve_configured_path;
use lumen_core::pyjson;
#[cfg(windows)]
use lumen_platform::runtime_priority::CORE_RUN_PRIORITY_FLAG;
#[cfg(windows)]
use lumen_platform::subprocess::CREATE_NO_WINDOW;
use lumen_platform::subprocess::{
    decode_output, hidden_command, is_windows_shutting_down, run_merged, run_text_owned,
    spawn_merged, CompletedProcess, RunError,
};
use parking_lot::Mutex;
use rand::RngCore;
use regex::Regex;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::singbox::config_builder::normalize_shadowsocks_methods;
use crate::singbox::runtime_planner::{is_falsy_json, py_int, py_str_list_item};

// ── Module-level constants (`manager.py` header) ────────────────────────────

/// Optional user hook.  `custom/pre_launch_singbox.ps1` runs before sing-box
/// starts and receives a copy of the runtime config as `$args[0]`; it may
/// rewrite that file.  The edited config is checked by sing-box and then used
/// for both the core and Lumen's readiness probes (ports, TUN).  Any failure
/// falls back to the config Lumen generated.  The script runs with Lumen's own
/// rights, including administrator rights when Lumen is elevated.
pub const PRE_LAUNCH_SCRIPT_NAME: &str = "pre_launch_singbox.ps1";
/// The data folder copy wins over the install folder one.
pub fn custom_pre_launch_script() -> PathBuf {
    data_dir().join("custom").join(PRE_LAUNCH_SCRIPT_NAME)
}
/// Also looked up next to Lumen.exe; the data folder copy wins.
pub fn install_pre_launch_script() -> PathBuf {
    base_dir().join("custom").join(PRE_LAUNCH_SCRIPT_NAME)
}
pub fn pre_launch_config_file() -> PathBuf {
    runtime_dir().join("singbox_config.pre_launch.json")
}
pub const PRE_LAUNCH_TIMEOUT_SEC: f64 = 30.0;
const PRE_LAUNCH_LOG_LINES: usize = 50;

const MAX_LAST_OUTPUT_LINES: usize = 100;
const NOISY_RUNTIME_WINDOW_SECS: f64 = 1.0;
const NOISY_RUNTIME_VISIBLE_LIMIT: usize = 20;

static MANAGED_TUN_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.\- ]{1,128}$").expect("valid regex"));
static LUMEN_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)-lumen(?:\.|\b)").expect("valid regex"));
static INFO_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|\s)INFO\s").expect("valid regex"));
static TRAFFIC_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "(?i)(?:inbound/[^\\s:]+: inbound (?:packet )?connection (?:from|to)\\b|outbound/[^\\s:]+: outbound (?:packet )?connection to\\b|router: match\\[)",
    )
    .expect("valid regex")
});
static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?").expect("valid regex"));

// ── Module-level helpers ────────────────────────────────────────────────────

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
fn windows_system_dir() -> Option<PathBuf> {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buffer = [0u16; 260];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
}

#[cfg(not(windows))]
fn windows_system_dir() -> Option<PathBuf> {
    None
}

#[cfg_attr(not(windows), allow(dead_code))]
#[cfg(windows)]
fn program_files_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_ProgramFiles, KNOWN_FOLDER_FLAG};
    // FOLDERID_ProgramFiles
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KNOWN_FOLDER_FLAG(0), None) };
    match result {
        Ok(path) => {
            let owned = unsafe { path.to_string().ok() };
            unsafe { CoTaskMemFree(Some(path.as_ptr().cast())) };
            owned.filter(|value| !value.is_empty()).map(PathBuf::from)
        }
        Err(_) => None,
    }
}

#[cfg(not(windows))]
fn program_files_dir() -> Option<PathBuf> {
    None
}

/// Absolute PowerShell path, never looked up through the user's PATH.
fn powershell_executable() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(program_files) = program_files_dir() {
            candidates.push(program_files.join("PowerShell").join("7").join("pwsh.exe"));
        }
        if let Some(system_dir) = windows_system_dir() {
            candidates.push(system_dir.join("WindowsPowerShell").join("v1.0").join("powershell.exe"));
        }
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Windows PowerShell's Out-File/Set-Content may write UTF-16 or a BOM.
fn read_hook_json(path: &Path) -> anyhow::Result<Value> {
    let data = fs::read(path)?;
    let text = if data.starts_with(&[0xFF, 0xFE]) {
        decode_utf16(&data[2..], false)?
    } else if data.starts_with(&[0xFE, 0xFF]) {
        decode_utf16(&data[2..], true)?
    } else {
        let payload = data.strip_prefix([0xEF, 0xBB, 0xBF].as_slice()).unwrap_or(&data);
        String::from_utf8(payload.to_vec())?
    };
    Ok(serde_json::from_str(&text)?)
}

fn decode_utf16(data: &[u8], big_endian: bool) -> anyhow::Result<String> {
    if data.len() % 2 != 0 {
        anyhow::bail!("truncated UTF-16 data");
    }
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|pair| {
            if big_endian {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from_le_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    Ok(String::from_utf16(&units)?)
}

/// Return the exact config that validation and startup must both use.
pub fn normalized_runtime_config(config: &Value) -> anyhow::Result<Value> {
    let mut normalized = config.clone();
    normalize_shadowsocks_methods(&mut normalized)?;
    Ok(normalized)
}

/// Python `str(value or "")` for JSON scalars.
fn json_or_empty_str(value: Option<&Value>) -> String {
    match value {
        Some(value) if !is_falsy_json(value) => py_str_list_item(value),
        _ => String::new(),
    }
}

/// Python `str(inbound.get("type", "")).lower()` (no strip, unlike the
/// `_extract_tun_interface_name` variant).
fn json_type_lower(value: Option<&Value>) -> String {
    match value {
        Some(value) => py_str_list_item(value).to_lowercase(),
        None => String::new(),
    }
}

/// Return only safe adapter aliases that Lumen is allowed to touch.
///
/// sing-box accepts a user supplied `interface_name`.  The old cleanup code
/// only knew the historical aliases, so a failed startup with a custom name
/// left the adapter behind and the next attempt raced with Wintun.  The value
/// is deliberately restricted before it is embedded into PowerShell;
/// arbitrary input must never become a command fragment.
pub fn managed_tun_interface_names(interface_name: &str) -> Vec<String> {
    let mut names: Vec<String> = SINGBOX_LEGACY_TUN_INTERFACE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    names.push(SINGBOX_TUN_INTERFACE_NAME.to_string());
    let candidate = interface_name.trim();
    if !candidate.is_empty() && MANAGED_TUN_NAME_RE.is_match(candidate) {
        names.push(candidate.to_string());
    }
    let mut seen = HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names
}

pub fn managed_tun_names_ps(interface_name: &str) -> String {
    managed_tun_interface_names(interface_name)
        .iter()
        .map(|name| "'".to_owned() + &name.replace('\'', "''") + "'")
        .collect::<Vec<_>>()
        .join(",")
}

/// Normalize a config alias before it reaches Windows/PowerShell APIs.
pub fn normalize_tun_interface_name_str(value: &str) -> String {
    let candidate = value.trim();
    if candidate.is_empty() {
        return SINGBOX_TUN_INTERFACE_NAME.to_string();
    }
    if MANAGED_TUN_NAME_RE.is_match(candidate) {
        return candidate.to_string();
    }
    SINGBOX_TUN_INTERFACE_NAME.to_string()
}

pub fn normalize_tun_interface_name(value: Option<&Value>) -> String {
    normalize_tun_interface_name_str(&json_or_empty_str(value))
}

/// Require a Wintun / sing-box description for legacy and custom aliases,
/// while allowing the dedicated default alias even when Windows reports a
/// generic description.  This prevents cleanup from disabling an unrelated
/// adapter that happens to share a generic name such as tun0.
fn managed_tun_adapter_filter_ps() -> String {
    let mut filter = String::from("(($_.Name -eq '");
    filter.push_str(SINGBOX_TUN_INTERFACE_NAME);
    filter.push_str("') -or ");
    filter.push_str("($_.InterfaceDescription -match '(?i)wintun|sing.?box'))");
    filter
}

// Python also exposes `_managed_tun_adapter_filter_ps(interface_name)`, a
// classmethod wrapper that ignores the argument (the explicit name is already
// validated by `managed_tun_interface_names` and is part of the
// Get-NetAdapter name filter; requiring a Wintun / sing-box description for
// custom and legacy aliases avoids touching a different VPN that happens to
// use a generic alias such as `tun0`). The Rust port folds it into the
// builder above.

/// Python `ip_network(str(value), strict=False).version == 4`.
pub fn is_ipv4_prefix(value: &Value) -> bool {
    parse_network_version(&py_str_list_item(value)) == Some(4)
}

fn parse_network_version(text: &str) -> Option<u8> {
    let (addr_part, mask) = match text.rsplit_once('/') {
        Some((addr, mask)) => (addr, Some(mask)),
        None => (text, None),
    };
    let addr: IpAddr = addr_part.parse().ok()?;
    if let Some(mask) = mask {
        let max_bits = match addr {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let mask: u32 = mask.trim().parse().ok()?;
        if mask > max_bits {
            return None;
        }
    }
    Some(match addr {
        IpAddr::V4(_) => 4,
        IpAddr::V6(_) => 6,
    })
}

/// Python `json.dumps(config, ensure_ascii=..., indent=2)` written through a
/// text-mode file handle (Windows newline translation included).
fn config_text(config: &Value, ensure_ascii: bool) -> String {
    let text = pyjson::dumps(config, ensure_ascii, Some(2));
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

fn monotonic() -> f64 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_secs_f64()
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

/// Render a Python tuple of ints (`str((10808, 10809))`).
fn python_tuple_repr(ports: &[i64]) -> String {
    match ports {
        [] => "()".to_string(),
        [one] => format!("({one},)"),
        many => {
            let rendered = many
                .iter()
                .map(|port| port.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("({rendered})")
        }
    }
}

fn random_token_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ── Lifecycle state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingboxEvent {
    Started,
    Stopped(i32),
    Log(String),
    Error(String),
    StateChanged(bool),
}

type Process = Arc<Mutex<Child>>;

struct State {
    process: Option<Process>,
    reader: Option<ReaderHandle>,
    reader_thread_id: Option<ThreadId>,
    running: bool,
    starting: bool,
    stop_requested: bool,
    startup_failure_reported: bool,
    runtime_error_reported: bool,
    // Keep enough startup context to retain the original WARP/MASQUE
    // initialization error even when subsequent DNS requests repeat the
    // shorter "endpoint/tunnel not initialized" message.
    last_output_lines: VecDeque<String>,
    last_exit_code: Option<i32>,
    exe_path: Option<PathBuf>,
    tun_mode: bool,
    tun_interface_name: String,
    runtime_log_window: f64,
    runtime_log_visible: usize,
    runtime_log_hidden: usize,
    other_log_limiter: CoreLogLimiter,
}

impl Default for State {
    fn default() -> Self {
        Self {
            process: None,
            reader: None,
            reader_thread_id: None,
            running: false,
            starting: false,
            stop_requested: false,
            startup_failure_reported: false,
            runtime_error_reported: false,
            last_output_lines: VecDeque::new(),
            last_exit_code: None,
            exe_path: None,
            tun_mode: true,
            tun_interface_name: String::new(),
            runtime_log_window: 0.0,
            runtime_log_visible: 0,
            runtime_log_hidden: 0,
            other_log_limiter: CoreLogLimiter::with_limits("sing-box", 20, 10),
        }
    }
}

impl State {
    fn owns(&self, process: &Process) -> bool {
        self.process
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, process))
    }
}

struct Shared {
    state: Mutex<State>,
    events: broadcast::Sender<SingboxEvent>,
    // sing-box's own ready marker is a better startup boundary than a
    // separate PowerShell route/DNS probe.  The latter used to keep the UI
    // in "connecting" for several seconds after real traffic was already
    // flowing through the TUN adapter.
    core_ready: AtomicBool,
    profile_ready: AtomicBool,
    profile_error: AtomicBool,
    // sing-box emits this warning several seconds before the final Wintun
    // "already exists | Element not found" failure.  Treat it as an
    // internal retry boundary instead of making the UI wait for the core's
    // full adapter-open timeout.
    tun_startup_stalled: AtomicBool,
}

impl Shared {
    fn emit(&self, event: SingboxEvent) {
        let _ = self.events.send(event);
    }

    fn output_snapshot(&self) -> Vec<String> {
        self.state.lock().last_output_lines.iter().cloned().collect()
    }
}

/// Starts and stops the sing-box core synchronously. Call lifecycle methods
/// on a worker thread and subscribe before starting to receive lifecycle and
/// log events.
pub struct SingBoxManager {
    config_path: PathBuf,
    shared: Arc<Shared>,
}

impl Default for SingBoxManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SingBoxManager {
    pub fn new() -> Self {
        Self::with_config_path(singbox_config_file())
    }

    pub fn with_config_path(config_path: PathBuf) -> Self {
        let (events, _) = broadcast::channel(512);
        let config_path = std::path::absolute(&config_path).unwrap_or(config_path);
        Self {
            config_path,
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                events,
                core_ready: AtomicBool::new(false),
                profile_ready: AtomicBool::new(false),
                profile_error: AtomicBool::new(false),
                tun_startup_stalled: AtomicBool::new(false),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SingboxEvent> {
        self.shared.events.subscribe()
    }

    pub fn is_running(&self) -> bool {
        self.shared.state.lock().running
    }

    pub fn last_exit_code(&self) -> Option<i32> {
        self.shared.state.lock().last_exit_code
    }

    fn proc_alive(&self) -> bool {
        let state = self.shared.state.lock();
        state
            .process
            .as_ref()
            .is_some_and(|process| process_alive(process))
    }

    fn set_starting(&self, value: bool) {
        self.shared.state.lock().starting = value;
    }

    // ── validate_config ─────────────────────────────────────────────────────

    pub fn validate_config(&self, singbox_path: &str, config: &Value) -> (bool, String) {
        let exe = resolve_configured_path(singbox_path, Some(&singbox_path_default()), true, true);
        let exe = match exe {
            Some(exe) if exe.is_file() => exe,
            exe => {
                let shown = exe
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| singbox_path.to_string());
                return (false, format!("sing-box.exe not found: {shown}"));
            }
        };
        let config = match normalized_runtime_config(config) {
            Ok(config) => config,
            Err(error) => return (false, error.to_string()),
        };
        let compatibility_error = direct_masque_compatibility_error(&exe, &config);
        if !compatibility_error.is_empty() {
            return (false, compatibility_error);
        }
        let result = (|| -> Result<CompletedProcess, RunError> {
            let dir = runtime_dir();
            fs::create_dir_all(&dir)?;
            let mut file = tempfile::Builder::new()
                .prefix("singbox-check-")
                .suffix(".json")
                .tempfile_in(&dir)?;
            file.write_all(config_text(&config, true).as_bytes())?;
            file.flush()?;
            let path = file.into_temp_path();
            let core_dir = exe.parent().unwrap_or_else(|| Path::new("."));
            let mut command = hidden_command(&exe);
            command
                .args(["check", "-c"])
                .arg(&path)
                .arg("-D")
                .arg(core_dir)
                .current_dir(core_dir);
            run_merged(command, 20.0)
        })();
        match result {
            Ok(result) => (
                result.returncode == 0,
                result.output_text().trim().to_owned(),
            ),
            Err(RunError::Timeout { .. }) => (
                false,
                "sing-box config check timed out after 20 seconds".into(),
            ),
            Err(error) => (false, format!("failed to check sing-box config: {error}")),
        }
    }

    // ── pre-launch hook ─────────────────────────────────────────────────────

    fn pre_launch_log(&self, text: &str, prefix: &str) {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .collect();
        for line in lines.iter().take(PRE_LAUNCH_LOG_LINES) {
            let truncated: String = line.chars().take(500).collect();
            self.shared
                .emit(SingboxEvent::Log(format!("{prefix} {truncated}")));
        }
        if lines.len() > PRE_LAUNCH_LOG_LINES {
            self.shared.emit(SingboxEvent::Log(format!(
                "{prefix} ... {} more lines",
                lines.len() - PRE_LAUNCH_LOG_LINES
            )));
        }
    }

    /// Run the optional pre-launch script on a copy of `config`.
    ///
    /// Returns the edited and normalized config, or `None` when there is no
    /// usable script or it failed; the caller then keeps its own config.
    fn apply_pre_launch_script(&self, config: &Value) -> Option<Value> {
        if !cfg!(windows) {
            return None;
        }
        let mut present: Vec<PathBuf> = Vec::new();
        for path in [custom_pre_launch_script(), install_pre_launch_script()] {
            if !present.contains(&path) && path.is_file() {
                present.push(path);
            }
        }
        if present.is_empty() {
            return None;
        }
        let Some(shell) = powershell_executable() else {
            self.shared
                .emit(SingboxEvent::Log("[pre-launch] PowerShell not found; skipping script".into()));
            return None;
        };
        let script = present.remove(0);

        let pre_launch_config = pre_launch_config_file();
        let prepared = (|| -> std::io::Result<()> {
            fs::create_dir_all(runtime_dir())?;
            fs::write(&pre_launch_config, config_text(config, true))
        })();
        if let Err(error) = prepared {
            self.shared.emit(SingboxEvent::Log(format!(
                "[pre-launch] cannot prepare the config copy: {error}"
            )));
            return None;
        }
        let keep = "starting sing-box with the original config";
        let shell_name = shell
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.shared.emit(SingboxEvent::Log(format!(
            "[pre-launch] running {} via {shell_name}",
            script.display()
        )));
        let command: Vec<String> = vec![
            shell.to_string_lossy().into_owned(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            script.to_string_lossy().into_owned(),
            pre_launch_config.to_string_lossy().into_owned(),
        ];
        let outcome = self.run_pre_launch_hook(&command, &pre_launch_config, keep);
        // Python `finally`: the hook copy never survives the call.
        let _ = fs::remove_file(&pre_launch_config);
        outcome
    }

    fn run_pre_launch_hook(
        &self,
        command: &[String],
        pre_launch_config: &Path,
        keep: &str,
    ) -> Option<Value> {
        let result = match run_text_owned(command, PRE_LAUNCH_TIMEOUT_SEC, false) {
            Ok(result) => result,
            Err(RunError::Timeout { .. }) => {
                self.shared.emit(SingboxEvent::Log(format!(
                    "[pre-launch] script did not finish in {} s; {keep}",
                    PRE_LAUNCH_TIMEOUT_SEC as i64
                )));
                return None;
            }
            Err(error) => {
                self.shared.emit(SingboxEvent::Log(format!(
                    "[pre-launch] failed to run script: {error}; {keep}"
                )));
                return None;
            }
        };
        self.pre_launch_log(&decode_output(&result.stdout), "[pre-launch]");
        if result.returncode != 0 {
            self.pre_launch_log(&decode_output(&result.stderr), "[pre-launch][err]");
            self.shared.emit(SingboxEvent::Log(format!(
                "[pre-launch] script exited with code {}; {keep}",
                result.returncode
            )));
            return None;
        }
        let edited = match read_hook_json(pre_launch_config) {
            Ok(edited) => edited,
            Err(error) => {
                self.shared.emit(SingboxEvent::Log(format!(
                    "[pre-launch] script left invalid JSON ({error}); {keep}"
                )));
                return None;
            }
        };
        if !edited.is_object() {
            self.shared.emit(SingboxEvent::Log(format!(
                "[pre-launch] script left a non-object config; {keep}"
            )));
            return None;
        }
        match normalized_runtime_config(&edited) {
            Ok(edited) => Some(edited),
            Err(error) => {
                self.shared.emit(SingboxEvent::Log(format!(
                    "[pre-launch] edited config is not usable ({error}); {keep}"
                )));
                None
            }
        }
    }

    // ── start ───────────────────────────────────────────────────────────────

    pub fn start(&mut self, singbox_path: &str, config: &Value, prevalidated: bool) -> bool {
        let Some(exe) =
            resolve_configured_path(singbox_path, Some(&singbox_path_default()), true, true)
        else {
            self.shared.emit(SingboxEvent::Error(
                "sing-box path is not configured (set it in Settings → Core paths)".into(),
            ));
            return false;
        };
        if !exe.is_file() {
            self.shared
                .emit(SingboxEvent::Error(format!("sing-box.exe not found: {}", exe.display())));
            return false;
        }
        self.shared.state.lock().exe_path = Some(exe.clone());

        // Do this even for a prevalidated plan: validation intentionally works
        // on a copy, while this is the object written to the runtime file.
        let mut config = match normalized_runtime_config(config) {
            Ok(config) => config,
            Err(error) => {
                self.shared.emit(SingboxEvent::Error(error.to_string()));
                return false;
            }
        };

        // The user's pre-launch script edits a copy before validation, so the
        // edited config is checked and drives the port/TUN readiness probes.
        let mut prevalidated = prevalidated;
        if let Some(edited) = self.apply_pre_launch_script(&config) {
            let (edited_valid, edited_output) = self.validate_config(&exe.to_string_lossy(), &edited);
            if edited_valid {
                config = edited;
                prevalidated = true;
                self.shared
                    .emit(SingboxEvent::Log("[pre-launch] using the config edited by the script".into()));
            } else {
                let detail = if edited_output.is_empty() {
                    "unknown config validation error"
                } else {
                    &edited_output
                };
                self.shared.emit(SingboxEvent::Log(format!(
                    "[pre-launch] sing-box rejected the edited config: {detail}; starting with the original config"
                )));
            }
        }

        if !prevalidated {
            let (valid, validation_output) = self.validate_config(&exe.to_string_lossy(), &config);
            if !valid {
                let detail = if validation_output.is_empty() {
                    "unknown config validation error"
                } else {
                    &validation_output
                };
                self.shared.emit(SingboxEvent::Error(format!(
                    "sing-box does not accept the new config: {detail}"
                )));
                return false;
            }
        }

        // Readiness and the core must use the same normalized alias. Never mutate
        // a caller-owned profile while recovering a Windows adapter collision.
        let mut tun_interface_name = Self::extract_tun_interface_name(&config);
        if !tun_interface_name.is_empty() {
            if let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) {
                for inbound in inbounds.iter_mut() {
                    let is_tun = inbound
                        .get("type")
                        .map(|value| json_type_lower(Some(value)) == "tun")
                        .unwrap_or(true);
                    if is_tun {
                        if let Some(map) = inbound.as_object_mut() {
                            let normalized = normalize_tun_interface_name(map.get("interface_name"));
                            map.insert("interface_name".into(), Value::String(normalized));
                        }
                    }
                }
            }
        }
        let proxy_ports = Self::extract_local_proxy_ports(&config);
        let requires_profile_readiness = Self::requires_profile_outbound_readiness(&config);
        let requires_profile_confirmation = Self::requires_lumen_direct_masque(&config);
        let tun_mode = !tun_interface_name.is_empty();
        {
            let mut state = self.shared.state.lock();
            state.tun_mode = tun_mode;
            state.tun_interface_name = tun_interface_name.clone();
        }
        if !tun_mode && proxy_ports.is_empty() {
            self.shared.emit(SingboxEvent::Error(
                "sing-box config does not contain a TUN or local proxy inbound".into(),
            ));
            return false;
        }

        if self.proc_alive() {
            if !self.stop(true, true) {
                self.shared.emit(SingboxEvent::Error(
                    "failed to stop previous sing-box process".into(),
                ));
                return false;
            }
        } else {
            let was_running = {
                let mut state = self.shared.state.lock();
                let was_running = state.running;
                state.running = false;
                was_running
            };
            if was_running {
                self.shared.emit(SingboxEvent::StateChanged(false));
            }
        }

        if let Err(error) = fs::create_dir_all(runtime_dir()) {
            self.shared.emit(SingboxEvent::Error(format!(
                "failed to write sing-box config: {error}"
            )));
            return false;
        }
        if let Err(error) = fs::write(&self.config_path, config_text(&config, true)) {
            self.shared.emit(SingboxEvent::Error(format!(
                "failed to write sing-box config: {error}"
            )));
            return false;
        }
        // Only a retained process handle proves ownership. Do not inspect/kill
        // other instances by executable name, or sweep unrelated Wintun devices.

        // A freshly killed sing-box can still hold the clash-api port; wait for it.
        Self::wait_clash_api_port_released(5.0);

        // Set working directory to core/ so sing-box can find wintun.dll
        let core_dir = exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        {
            let mut state = self.shared.state.lock();
            state.starting = true;
            state.startup_failure_reported = false;
            state.runtime_error_reported = false;
            state.last_output_lines.clear();
        }

        // Retry only actual Wintun/socket races. Profile initialization errors
        // are deterministic for a given config, so repeating the whole startup
        // only turns a useful failure into a long UI stall.
        // Even profiles that need an additional WARP/MASQUE handshake get one
        // adapter-race retry.  The old single attempt made the exact
        // "already exists | open existing adapter: Element not found"
        // transient failure unrecoverable for those configs.
        let max_attempts = if tun_mode {
            if requires_profile_readiness || requires_profile_confirmation {
                2
            } else {
                3
            }
        } else {
            1
        };
        for attempt in 0..max_attempts {
            let attempt_started = Instant::now();
            let runtime_label = if tun_mode {
                format!("TUN interface={tun_interface_name}")
            } else {
                format!("proxy ports={}", python_tuple_repr(&proxy_ports))
            };
            self.shared.emit(SingboxEvent::Log(format!(
                "[sing-box] startup attempt {}/{max_attempts}, {runtime_label}",
                attempt + 1
            )));
            {
                let mut state = self.shared.state.lock();
                state.last_output_lines.clear();
                state.runtime_log_window = 0.0;
                state.runtime_log_visible = 0;
                state.runtime_log_hidden = 0;
                state.other_log_limiter.reset();
            }
            self.shared.core_ready.store(false, Ordering::SeqCst);
            self.shared.profile_ready.store(false, Ordering::SeqCst);
            self.shared.profile_error.store(false, Ordering::SeqCst);
            self.shared.tun_startup_stalled.store(false, Ordering::SeqCst);
            {
                let mut state = self.shared.state.lock();
                state.stop_requested = false;
                state.last_exit_code = None;
            }

            let process = match self.launch(&exe, &core_dir) {
                Ok(process) => process,
                Err(error) => {
                    self.set_starting(false);
                    self.report_startup_failure(format!(
                        "failed to start sing-box process: {error}"
                    ));
                    return false;
                }
            };

            let mut profile_readiness_failed = false;
            let mut ready = if tun_mode {
                self.wait_until_tun_ready(&process, &tun_interface_name, 20.0)
            } else {
                self.wait_until_proxy_ready(&process, &proxy_ports, 10.0)
            };
            if ready {
                let ready_ms = elapsed_ms(attempt_started);
                let ready_label = if tun_mode {
                    "core and TUN"
                } else {
                    "local proxy inbounds"
                };
                self.shared.emit(SingboxEvent::Log(format!(
                    "[sing-box] {ready_label} ready in {ready_ms} ms"
                )));
                if tun_mode {
                    if requires_profile_confirmation {
                        ready = self.wait_for_profile_confirmation(&process, 3.0);
                        profile_readiness_failed = !ready;
                    } else if requires_profile_readiness
                        && !self.wait_for_profile_startup_settle(&process, 0.4)
                    {
                        profile_readiness_failed = true;
                        ready = false;
                    }
                }
                if ready {
                    self.set_starting(false);
                    let total_ms = elapsed_ms(attempt_started);
                    self.shared.emit(SingboxEvent::Log(format!(
                        "[sing-box] runtime ready in {total_ms} ms, {runtime_label}"
                    )));
                    self.mark_running();
                    return true;
                }
                let suffix = if attempt + 1 < max_attempts {
                    "; retrying"
                } else {
                    ""
                };
                if profile_readiness_failed {
                    self.shared.emit(SingboxEvent::Log(format!(
                        "[sing-box] WARP/MASQUE outbound did not become ready{suffix}"
                    )));
                } else {
                    self.shared.emit(SingboxEvent::Log(format!(
                        "[sing-box] TUN did not become ready{suffix}"
                    )));
                }
            }

            let exited = !process_alive(&process);
            if exited {
                // poll() can observe exit before the reader has drained the error.
                self.join_reader(0.5);
            } else if !self.stop(true, true) {
                self.set_starting(false);
                self.report_startup_failure(
                    "Previous sing-box process is still running; retry cancelled".into(),
                );
                return false;
            }
            let retryable = self.startup_error_is_retryable();
            if tun_mode && retryable && attempt + 1 < max_attempts {
                if self.startup_error_is_ipv6_disabled() {
                    self.disable_ipv6_in_singbox_config();
                } else if self.startup_error_is_stale_adapter() {
                    // A ghost adapter may be invisible even to Get-NetAdapter.
                    // A fresh alias avoids its name/GUID collision without deleting
                    // any device or modifying another VPN's routes.
                    match self.rotate_tun_interface_name(&tun_interface_name) {
                        Ok(fresh) => {
                            tun_interface_name = fresh;
                            self.shared.state.lock().tun_interface_name = tun_interface_name.clone();
                        }
                        Err(error) => {
                            self.set_starting(false);
                            self.report_startup_failure(format!(
                                "Cannot prepare TUN collision retry: {}",
                                error_type_name(&error)
                            ));
                            return false;
                        }
                    }
                }
                Self::wait_clash_api_port_released(1.0);
                thread::sleep(Duration::from_secs_f64(0.15 * (attempt as f64 + 1.0)));
                self.set_starting(true);
                continue;
            }

            self.set_starting(false);
            if exited {
                let code = self.shared.state.lock().last_exit_code;
                self.report_startup_failure(self.unexpected_exit_message(code, true));
            } else if profile_readiness_failed {
                self.report_startup_failure(self.profile_outbound_not_ready_message());
            } else if tun_mode {
                self.report_startup_failure(self.tun_not_ready_message(&tun_interface_name));
            } else {
                self.report_startup_failure(format!(
                    "sing-box local proxy ports did not become ready: {}",
                    python_tuple_repr(&proxy_ports)
                ));
            }
            return false;
        }

        self.set_starting(false);
        false
    }

    /// Compatibility hook. An executable name/path does not establish ownership.
    #[allow(dead_code)]
    fn kill_orphaned(_exe: &Path) {}

    pub fn stop(&mut self, expected: bool, fast: bool) -> bool {
        let already_stopped;
        let process = {
            let mut state = self.shared.state.lock();
            match state.process.clone() {
                Some(process) => {
                    let exited = process_exited(&process);
                    if exited {
                        state.stop_requested = false;
                        state.starting = false;
                        if state.running {
                            state.running = false;
                            self.shared.emit(SingboxEvent::StateChanged(false));
                        }
                        already_stopped = true;
                        None
                    } else {
                        state.stop_requested = expected;
                        already_stopped = false;
                        Some(process)
                    }
                }
                None => {
                    state.stop_requested = false;
                    state.starting = false;
                    if state.running {
                        state.running = false;
                        self.shared.emit(SingboxEvent::StateChanged(false));
                    }
                    already_stopped = true;
                    None
                }
            }
        };
        if already_stopped {
            self.join_reader(2.0);
            return true;
        }
        let process = process.expect("live process handle");

        let _ = process.lock().kill();
        let (terminate_timeout, kill_timeout, orphan_timeout, final_timeout) = if fast {
            (
                Duration::from_millis(800),
                Duration::from_millis(500),
                Duration::from_secs(2),
                Duration::from_millis(300),
            )
        } else {
            (
                Duration::from_secs(3),
                Duration::from_secs(2),
                Duration::from_secs(5),
                Duration::from_secs(1),
            )
        };

        if !wait_for_process(&process, terminate_timeout) {
            let _ = process.lock().kill();
            wait_for_process(&process, kill_timeout);
        }

        if !process_exited(&process) {
            let exe = self.shared.state.lock().exe_path.clone();
            if cfg!(windows) {
                if let Some(exe) = exe {
                    let process_name = exe
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if kill_processes_by_path(
                        &process_name,
                        &exe,
                        orphan_timeout.as_secs_f64(),
                    ) {
                        wait_for_process(&process, final_timeout);
                    }
                }
            }
        }

        if !process_exited(&process) {
            self.shared.state.lock().stop_requested = false;
            if !is_windows_shutting_down() {
                self.shared
                    .emit(SingboxEvent::Error("failed to stop sing-box process in time".into()));
            }
            return false;
        }

        // Let sing-box/Wintun perform the normal adapter teardown. Disabling a
        // healthy adapter here makes Windows rebuild its network state on every
        // reconnect and delays the first real connections. Do not start a
        // PowerShell adapter-release probe here either: retry handling already
        // covers the rare Wintun race, while the probe delayed every hot-swap.
        self.set_starting(false);
        self.join_reader(2.0);
        true
    }

    fn join_reader(&self, timeout: f64) {
        let current = thread::current().id();
        let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
        loop {
            let finished = {
                let state = self.shared.state.lock();
                if state.reader_thread_id == Some(current) {
                    return;
                }
                match &state.reader {
                    None => true,
                    Some(handle) => handle.is_finished(),
                }
            };
            if finished {
                let mut state = self.shared.state.lock();
                if let Some(handle) = state.reader.take() {
                    let _ = handle.join();
                }
                return;
            }
            if Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[allow(dead_code)]
    fn wait_tun_released(max_wait: f64, interface_name: &str) {
        if !cfg!(windows) {
            return;
        }
        let adapter_names_ps = managed_tun_names_ps(interface_name);
        let filter = managed_tun_adapter_filter_ps();
        let mut command = String::from("$active = @(Get-NetAdapter -Name @(");
        command.push_str(&adapter_names_ps);
        command.push_str(") -ErrorAction SilentlyContinue | Where-Object { (");
        command.push_str(&filter);
        command.push_str(") -and ");
        command.push_str("$_");
        command.push_str(".Status -notin @('Disabled','Not Present') }); ");
        command.push_str("if ($active.Count -eq 0) { exit 0 } else { exit 1 }");
        let step = 0.3f64;
        let started = Instant::now();
        loop {
            let remaining = max_wait.max(0.0) - started.elapsed().as_secs_f64();
            if remaining <= 0.0 {
                return;
            }
            let timeout = 3.0f64.min(remaining).max(0.1);
            let command_owned: Vec<String> = vec![
                "powershell".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                command.clone(),
            ];
            match run_text_owned(&command_owned, timeout, false) {
                Ok(result) if result.returncode == 0 => return,
                Err(_) => return, // can't check, proceed anyway
                _ => {}
            }
            let remaining = max_wait.max(0.0) - started.elapsed().as_secs_f64();
            if remaining <= 0.0 {
                return;
            }
            thread::sleep(Duration::from_secs_f64(step.min(remaining)));
        }
    }

    /// Wait until the clash-api controller port can be bound again.
    pub fn wait_clash_api_port_released(max_wait: f64) {
        if !cfg!(windows) {
            return;
        }
        let host: IpAddr = PROXY_HOST.parse().expect("PROXY_HOST literal");
        let address = SocketAddr::new(host, SINGBOX_CLASH_API_PORT);
        let mut waited = 0.0f64;
        while waited < max_wait {
            if TcpListener::bind(address).is_ok() {
                return;
            }
            thread::sleep(Duration::from_secs_f64(0.2));
            waited += 0.2;
        }
    }

    /// No durable device ownership receipt exists for legacy aliases. Wintun
    /// tears down our live adapter with its owned process; never sweep by name.
    pub fn cleanup_orphaned_tun_adapters(_max_wait: f64, _interface_name: &str) {}

    /// A SWD/WINTUN prefix or a non-OK status is not evidence of Lumen ownership.
    #[allow(dead_code)]
    fn purge_stale_wintun_devices(&self, _max_wait: f64, _interface_name: &str) {}

    fn rotate_tun_interface_name(&self, previous: &str) -> Result<String, ConfigIoError> {
        let text = fs::read_to_string(&self.config_path).map_err(ConfigIoError::Io)?;
        let mut config: Value = serde_json::from_str(&text).map_err(ConfigIoError::Json)?;
        let fresh = format!(
            "{}-{:x}-{}",
            SINGBOX_TUN_INTERFACE_NAME
                .get(..80)
                .unwrap_or(SINGBOX_TUN_INTERFACE_NAME),
            std::process::id(),
            random_token_hex(6)
        );
        let mut changed = false;
        if let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) {
            for inbound in inbounds.iter_mut() {
                let Some(map) = inbound.as_object_mut() else {
                    continue;
                };
                if json_type_lower(map.get("type")) != "tun" {
                    continue;
                }
                if normalize_tun_interface_name(map.get("interface_name")) == previous {
                    map.insert("interface_name".into(), Value::String(fresh.clone()));
                    changed = true;
                }
            }
        }
        if !changed {
            return Err(ConfigIoError::Runtime(
                "Cannot find the failed TUN inbound in the active config".into(),
            ));
        }
        fs::write(&self.config_path, config_text(&config, true)).map_err(ConfigIoError::Io)?;
        self.shared.emit(SingboxEvent::Log(format!(
            "[tun] adapter name collision; retrying with interface={fresh}"
        )));
        Ok(fresh)
    }

    // ── launch and output reader ────────────────────────────────────────────

    fn launch(&self, exe: &Path, core_dir: &Path) -> std::io::Result<Process> {
        let mut command = hidden_command(exe);
        command
            .args(["run", "-c"])
            .arg(&self.config_path)
            .arg("-D")
            .arg(core_dir);
        command.current_dir(core_dir);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW | CORE_RUN_PRIORITY_FLAG);
        let (child, output) = spawn_merged(command)?;
        let process = Arc::new(Mutex::new(child));
        self.shared.state.lock().process = Some(process.clone());

        let reader = thread::Builder::new()
            .name("singbox-output-reader".into())
            .spawn({
                let shared = self.shared.clone();
                let process = process.clone();
                move || read_output(output, &shared, &process)
            });
        let reader = match reader {
            Ok(handle) => handle,
            Err(error) => {
                self.shared.state.lock().process = None;
                let mut child = process.lock();
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        self.shared.state.lock().reader = Some(reader);
        Ok(process)
    }

    /// Translate native core markers into thread-safe startup events.
    pub fn observe_startup_line(&self, line: &str) {
        set_startup_markers(&self.shared, line);
    }

    pub fn is_noisy_runtime_line(line: &str) -> bool {
        is_routine_core_log(line)
    }

    pub fn is_startup_routine_line(line: &str) -> bool {
        let text = line.to_lowercase();
        if ["error", "failed", "fatal", "panic"]
            .iter()
            .any(|marker| text.contains(marker))
        {
            return false;
        }
        Self::is_noisy_runtime_line(line)
    }

    /// Show connection and routing work without flooding busy TUN logs.
    pub fn emit_bounded_runtime_activity(&self, line: &str) {
        bounded_runtime_activity_at(&self.shared, line, monotonic());
    }

    /// Clock-injected variant of [`Self::emit_bounded_runtime_activity`]
    /// (Python tests monkeypatch `time.monotonic`).
    pub fn emit_bounded_runtime_activity_at(&self, line: &str, now: f64) {
        bounded_runtime_activity_at(&self.shared, line, now);
    }

    // ── readiness probes ────────────────────────────────────────────────────

    fn wait_until_proxy_ready(&self, process: &Process, ports: &[i64], max_wait: f64) -> bool {
        // The native started marker is emitted after all inbounds are ready.
        // Opening bare TCP probes against HTTP/SOCKS inbounds creates false
        // "read http request: EOF" errors in the user's runtime log.
        let now = Instant::now();
        let deadline = now + Duration::from_secs_f64(max_wait.max(0.0));
        let marker_deadline = deadline.min(now + Duration::from_secs_f64(2.0));
        while Instant::now() < marker_deadline && process_alive(process) {
            if self.shared.core_ready.load(Ordering::SeqCst) {
                return process_alive(process);
            }
            thread::sleep(Duration::from_millis(50));
        }
        // Compatibility fallback for older extended cores without the marker.
        let host: IpAddr = PROXY_HOST.parse().expect("PROXY_HOST literal");
        while Instant::now() < deadline && process_alive(process) {
            let mut all_ready = true;
            for port in ports {
                let Ok(port) = u16::try_from(*port) else {
                    all_ready = false;
                    break;
                };
                let address = SocketAddr::new(host, port);
                match TcpStream::connect_timeout(&address, Duration::from_millis(150)) {
                    Ok(stream) => drop(stream),
                    Err(_) => {
                        all_ready = false;
                        break;
                    }
                }
            }
            if all_ready {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn wait_until_tun_ready(&self, process: &Process, tun_interface_name: &str, max_wait: f64) -> bool {
        // `NOTICE sing-box started` is emitted after the TUN service has been
        // initialized.  In real logs traffic already flows before the slower
        // Get-NetIPAddress/Get-NetRoute probe returns, so use the native marker
        // as the primary readiness contract and keep the Windows probe only as
        // a compatibility fallback for cores that do not print it.
        // Wintun's slow-open warning is normally emitted at the five-second
        // boundary.  Keep a short grace window around that boundary so the
        // reader can signal an immediate alias retry before the compatibility
        // PowerShell probe starts waiting on the doomed adapter.
        let marker_wait = max_wait.min(6.5);
        let deadline = Instant::now() + Duration::from_secs_f64(marker_wait.max(0.0));
        while Instant::now() < deadline && process_alive(process) {
            if self.shared.tun_startup_stalled.load(Ordering::SeqCst) {
                return false;
            }
            if self.shared.core_ready.load(Ordering::SeqCst) {
                return process_alive(process);
            }
            thread::sleep(Duration::from_millis(50));
        }

        if !process_alive(process) || self.shared.tun_startup_stalled.load(Ordering::SeqCst) {
            return false;
        }
        if cfg!(windows) {
            let remaining = (max_wait - marker_wait).max(0.2);
            return Self::wait_for_windows_tun_ready(process, tun_interface_name, remaining);
        }
        true
    }

    /// Wait for the patched direct-MASQUE handshake, without DNS probes.
    fn wait_for_profile_confirmation(&self, process: &Process, max_wait: f64) -> bool {
        let deadline = Instant::now() + Duration::from_secs_f64(max_wait.max(0.0));
        while Instant::now() < deadline && process_alive(process) {
            if self.shared.profile_ready.load(Ordering::SeqCst) {
                return process_alive(process);
            }
            thread::sleep(Duration::from_millis(50));
        }
        self.shared.profile_ready.load(Ordering::SeqCst) && process_alive(process)
    }

    /// Give lazy WARP/profile endpoints a brief error-detection window.
    fn wait_for_profile_startup_settle(&self, process: &Process, max_wait: f64) -> bool {
        let deadline = Instant::now() + Duration::from_secs_f64(max_wait.max(0.0));
        while Instant::now() < deadline && process_alive(process) {
            if self.shared.profile_error.load(Ordering::SeqCst) {
                return false;
            }
            thread::sleep(Duration::from_millis(50));
        }
        process_alive(process) && !self.shared.profile_error.load(Ordering::SeqCst)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn wait_for_windows_tun_ready(process: &Process, tun_interface_name: &str, max_wait: f64) -> bool {
        let escaped_name =
            normalize_tun_interface_name_str(tun_interface_name).replace('\'', "''");
        let wait_ms = ((max_wait * 1000.0) as i64).max(200);
        let pid = process.lock().id();
        let script = windows_tun_ready_script(pid, &escaped_name, wait_ms);
        let command: Vec<String> = vec![
            "powershell".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script,
        ];
        let result = run_text_owned(&command, max_wait + 2.0, false);
        match result {
            Ok(result) => result.returncode == 0 && process_alive(process),
            Err(_) => false,
        }
    }

    // ── startup classification ──────────────────────────────────────────────

    fn output_snapshot(&self) -> Vec<String> {
        self.shared.output_snapshot()
    }

    fn startup_error_is_retryable(&self) -> bool {
        const NEEDLES: [&str; 8] = [
            "already exists",
            "cannot create a file when that file already exists",
            "only one usage of each socket address",
            "external controller listen error",
            "address already in use",
            "bind:",
            "element not found",
            "open interface take too much time",
        ];
        self.output_snapshot().iter().any(|line| {
            let text = line.to_lowercase();
            NEEDLES.iter().any(|needle| text.contains(needle))
        })
    }

    fn profile_outbound_not_ready_message(&self) -> String {
        let mut marker_detail = String::new();
        for line in self.output_snapshot().iter().rev() {
            let text = line.to_lowercase();
            if text.contains("tunnel not initialized") || text.contains("endpoint not initialized") {
                marker_detail = line.trim().to_string();
                break;
            }
        }
        let mut message = String::from(
            "sing-box could not initialize the WARP/MASQUE profile outbound; \
             TUN was stopped instead of being reported as connected.",
        );
        if !marker_detail.is_empty() {
            message.push_str(&format!(" Last core error: {marker_detail}"));
        }
        message
    }

    fn startup_error_is_stale_adapter(&self) -> bool {
        self.output_snapshot().iter().any(|line| {
            let text = line.to_lowercase();
            if text.contains("create adapter") || text.contains("open existing adapter") {
                return true;
            }
            if text.contains("open interface take too much time") {
                return true;
            }
            text.contains("element not found")
                && ["adapter", "wintun", "tun interface"]
                    .iter()
                    .any(|marker| text.contains(marker))
        })
    }

    fn startup_error_is_ipv6_disabled(&self) -> bool {
        self.output_snapshot()
            .iter()
            .any(|line| line.to_lowercase().contains("set ipv6 address"))
    }

    fn disable_ipv6_in_singbox_config(&self) {
        match self.disable_ipv6_inner() {
            Ok(()) => {}
            Err(error) => {
                self.shared.emit(SingboxEvent::Log(format!(
                    "[sing-box] Failed to auto-disable TUN IPv6: {}: {error}",
                    error_type_name(&error)
                )));
            }
        }
    }

    fn disable_ipv6_inner(&self) -> Result<(), ConfigIoError> {
        let text = fs::read_to_string(&self.config_path).map_err(ConfigIoError::Io)?;
        let mut config: Value = serde_json::from_str(&text).map_err(ConfigIoError::Json)?;
        let mut modified = false;
        if let Some(inbounds) = config.get_mut("inbounds").and_then(Value::as_array_mut) {
            for inbound in inbounds.iter_mut() {
                let Some(map) = inbound.as_object_mut() else {
                    continue;
                };
                // Python compares the type exactly: inbound.get("type") != "tun".
                if map
                    .get("type")
                    .map(|value| value.as_str() != Some("tun"))
                    .unwrap_or(true)
                {
                    continue;
                }
                map.remove("inet6_address");
                let Some(addresses) = map.get("address").and_then(Value::as_array).cloned() else {
                    continue;
                };
                let ipv4_only: Vec<Value> = addresses
                    .iter()
                    .filter(|prefix| is_ipv4_prefix(prefix))
                    .cloned()
                    .collect();
                if ipv4_only != addresses {
                    map.insert("address".into(), Value::Array(ipv4_only));
                    modified = true;
                }
            }
        }
        if modified {
            fs::write(&self.config_path, config_text(&config, false)).map_err(ConfigIoError::Io)?;
            self.shared.emit(SingboxEvent::Log(
                "[sing-box] Automatically disabled IPv6 for TUN due to broken Windows IPv6 stack."
                    .into(),
            ));
        }
        Ok(())
    }

    fn tun_not_ready_message(&self, tun_interface_name: &str) -> String {
        const MARKERS: [&str; 4] = [
            "open interface take too much time",
            "configure tun interface",
            "wintun",
            "create adapter",
        ];
        for line in self.output_snapshot() {
            let text = line.to_lowercase();
            if MARKERS.iter().any(|marker| text.contains(marker)) {
                return format!(
                    "sing-box did not open the Wintun TUN adapter '{tun_interface_name}': \
                     opening the interface stalls. Common Windows-side causes: TUN address/subnet \
                     172.18.0.1/30 collides with an existing adapter (Docker/WSL/Hyper-V 172.18.0.0/16), \
                     a stale hidden TUN adapter, a stopped Base Filtering Engine service, \
                     or another VPN's network filter driver."
                );
            }
        }
        format!(
            "sing-box started but TUN interface '{tun_interface_name}' \
             did not get an address and route in time."
        )
    }

    fn unexpected_exit_message(&self, exit_code: Option<i32>, startup: bool) -> String {
        unexpected_exit_message(&self.shared, exit_code, startup)
    }

    fn report_startup_failure(&self, message: String) {
        {
            let mut state = self.shared.state.lock();
            if state.startup_failure_reported {
                return;
            }
            state.startup_failure_reported = true;
        }
        self.shared.emit(SingboxEvent::Error(message));
    }

    // ── config shape helpers (Python static methods) ────────────────────────

    pub fn extract_tun_interface_name(config: &Value) -> String {
        let Some(inbounds) = config.get("inbounds").and_then(Value::as_array) else {
            return String::new();
        };
        for inbound in inbounds {
            let Some(map) = inbound.as_object() else {
                continue;
            };
            let inbound_type = json_or_empty_str(map.get("type")).trim().to_lowercase();
            if inbound_type != "tun" {
                continue;
            }
            // Empty interface_name means sing-box's default TUN inbound.  Use
            // the stable Lumen alias for readiness/cleanup as well; otherwise
            // an imported raw TUN profile was incorrectly treated as proxy
            // mode and never got statistics or adapter cleanup.
            return normalize_tun_interface_name(map.get("interface_name"));
        }
        String::new()
    }

    pub fn extract_local_proxy_ports(config: &Value) -> Vec<i64> {
        let mut ports: Vec<i64> = Vec::new();
        let Some(inbounds) = config.get("inbounds").and_then(Value::as_array) else {
            return ports;
        };
        for inbound in inbounds {
            let Some(map) = inbound.as_object() else {
                continue;
            };
            let inbound_type = json_or_empty_str(map.get("type")).trim().to_lowercase();
            if !matches!(inbound_type.as_str(), "mixed" | "socks" | "http") {
                continue;
            }
            // int(inbound.get("listen_port") or inbound.get("port") or 0)
            let raw = if map
                .get("listen_port")
                .is_some_and(|value| !is_falsy_json(value))
            {
                map.get("listen_port")
            } else if map.get("port").is_some_and(|value| !is_falsy_json(value)) {
                map.get("port")
            } else {
                None
            };
            // TypeError/ValueError → the inbound is skipped.
            let Some(port) = py_int(raw) else {
                continue;
            };
            if port > 0 && port <= 65535 && !ports.contains(&port) {
                ports.push(port);
            }
        }
        ports
    }

    /// Return whether adapter readiness alone cannot prove connectivity.
    pub fn requires_profile_outbound_readiness(config: &Value) -> bool {
        for section in ["outbounds", "endpoints"] {
            let Some(items) = config.get(section).and_then(Value::as_array) else {
                continue;
            };
            for item in items {
                let Some(map) = item.as_object() else {
                    continue;
                };
                let item_type = json_or_empty_str(map.get("type")).trim().to_lowercase();
                if item_type == "warp" {
                    return true;
                }
                if item_type == "masque" {
                    let complete = ["server", "private_key", "public_key", "address"]
                        .iter()
                        .all(|key| map.get(*key).is_some_and(|value| !is_falsy_json(value)));
                    if !complete {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Return whether the config uses Lumen's raw Clash/usque MASQUE fields.
    pub fn requires_lumen_direct_masque(config: &Value) -> bool {
        let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
            return false;
        };
        for outbound in outbounds {
            let Some(map) = outbound.as_object() else {
                continue;
            };
            let outbound_type = json_or_empty_str(map.get("type")).trim().to_lowercase();
            if outbound_type != "masque" {
                continue;
            }
            let complete = ["server", "private_key", "public_key", "address"]
                .iter()
                .all(|key| map.get(*key).is_some_and(|value| !is_falsy_json(value)));
            if complete {
                return true;
            }
        }
        false
    }

    fn mark_running(&self) {
        let mut state = self.shared.state.lock();
        if state.running {
            return;
        }
        state.stop_requested = false;
        state.running = true;
        self.shared.emit(SingboxEvent::Started);
        self.shared.emit(SingboxEvent::StateChanged(true));
    }
}

// ── Reader thread and exit handling (free functions over `Shared`) ──────────

enum RoutedLine {
    Bounded,
    Admit {
        summary: Option<String>,
        visible: bool,
    },
}

fn set_startup_markers(shared: &Shared, line: &str) {
    let text = line.to_lowercase();
    if text.contains("sing-box started") {
        shared.core_ready.store(true, Ordering::SeqCst);
    }
    if text.contains("connected to masque server") {
        shared.profile_ready.store(true, Ordering::SeqCst);
    }
    if text.contains("tunnel not initialized") || text.contains("endpoint not initialized") {
        shared.profile_error.store(true, Ordering::SeqCst);
    }
    if text.contains("open interface take too much time")
        || (text.contains("create adapter") && text.contains("open existing adapter"))
    {
        shared.tun_startup_stalled.store(true, Ordering::SeqCst);
    }
}

fn ring_push(lines: &mut VecDeque<String>, line: String) {
    if lines.len() == MAX_LAST_OUTPUT_LINES {
        lines.pop_front();
    }
    lines.push_back(line);
}

fn read_output(output: PipeReader, shared: &Shared, process: &Process) {
    shared.state.lock().reader_thread_id = Some(thread::current().id());
    let mut reader = BufReader::new(output);
    let mut bytes = Vec::new();
    'outer: while let Ok(1..) = reader.read_until(b'\n', &mut bytes) {
        let text = decode_output(&bytes);
        bytes.clear();
        for line in text.lines() {
            let clean = line.trim_end();
            if clean.is_empty() {
                continue;
            }
            let lower = clean.to_lowercase();
            if lower.contains("found process path") || lower.contains("process_path=") {
                continue;
            }
            let routed = {
                let mut state = shared.state.lock();
                if !state.owns(process) {
                    continue 'outer;
                }
                ring_push(&mut state.last_output_lines, clean.to_owned());
                set_startup_markers(shared, clean);
                // Keep a bounded view of real traffic and routing, but suppress
                // high-volume DNS and process-path chatter. Genuine failures
                // remain visible.
                if is_routine_core_log(clean) {
                    RoutedLine::Bounded
                } else {
                    let (summary, visible) = state.other_log_limiter.admit(clean);
                    RoutedLine::Admit { summary, visible }
                }
            };
            match routed {
                RoutedLine::Bounded => bounded_runtime_activity(shared, clean),
                RoutedLine::Admit { summary, visible } => {
                    if let Some(summary) = summary {
                        shared.emit(SingboxEvent::Log(summary));
                    }
                    if visible {
                        shared.emit(SingboxEvent::Log(clean.to_owned()));
                    }
                }
            }
        }
    }
    let code = {
        let mut child = process.lock();
        match child.wait() {
            Ok(status) => status.code().unwrap_or(-1),
            Err(_) => -1,
        }
    };
    handle_process_exit(shared, process, code, is_windows_shutting_down());
}

fn handle_process_exit(shared: &Shared, process: &Process, exit_code: i32, shutting_down: bool) {
    let (expected, was_starting, was_running, tun_mode, tun_interface_name) = {
        let mut state = shared.state.lock();
        if !state.owns(process) {
            // A newer process superseded this handle; ignore the stale exit.
            return;
        }
        let expected = state.stop_requested || shutting_down;
        let was_starting = state.starting;
        let was_running = state.running;
        state.last_exit_code = Some(exit_code);
        state.stop_requested = false;
        state.running = false;
        state.process = None;
        (
            expected,
            was_starting,
            was_running,
            state.tun_mode,
            state.tun_interface_name.clone(),
        )
    };

    // start() owns startup retries and reports only the final failure.  The
    // reader thread used to report the first transient exit immediately,
    // before start() could retry a Wintun race.
    if was_running && !expected {
        let report = {
            let mut state = shared.state.lock();
            if state.runtime_error_reported {
                None
            } else {
                state.runtime_error_reported = true;
                Some(())
            }
        };
        if report.is_some() {
            if tun_mode {
                // cleanup_orphaned_tun_adapters is a deliberate no-op (see the
                // ownership comments above and in the Python module).
                SingBoxManager::cleanup_orphaned_tun_adapters(5.0, &tun_interface_name);
            }
            shared.emit(SingboxEvent::Error(unexpected_exit_message(
                shared,
                Some(exit_code),
                false,
            )));
        }
    }
    // A failed startup attempt is an internal detail.  Emitting `stopped`
    // here makes the controller log it as a final core crash even when the
    // next Wintun attempt succeeds a moment later.
    if !was_starting {
        shared.emit(SingboxEvent::Stopped(exit_code));
    }
    if was_running {
        shared.emit(SingboxEvent::StateChanged(false));
    }
}

fn bounded_runtime_activity(shared: &Shared, line: &str) {
    bounded_runtime_activity_at(shared, line, monotonic());
}

fn bounded_runtime_activity_at(shared: &Shared, line: &str, now: f64) {
    if !INFO_LINE_RE.is_match(line) {
        return;
    }
    let lower = line.to_lowercase();
    if lower.contains("found process path") || lower.contains("process_path=") {
        return;
    }
    if !TRAFFIC_LINE_RE.is_match(line) {
        return;
    }
    let mut summary = 0usize;
    {
        let mut state = shared.state.lock();
        if now - state.runtime_log_window >= NOISY_RUNTIME_WINDOW_SECS {
            summary = state.runtime_log_hidden;
            state.runtime_log_window = now;
            state.runtime_log_visible = 0;
            state.runtime_log_hidden = 0;
        }
        if state.runtime_log_visible >= NOISY_RUNTIME_VISIBLE_LIMIT {
            state.runtime_log_hidden += 1;
            return;
        }
        state.runtime_log_visible += 1;
    }
    if summary > 0 {
        shared.emit(SingboxEvent::Log(format!(
            "INFO [sing-box] {summary} additional traffic events hidden in the previous second"
        )));
    }
    shared.emit(SingboxEvent::Log(line.to_owned()));
}

fn unexpected_exit_message(shared: &Shared, exit_code: Option<i32>, startup: bool) -> String {
    let stage = if startup {
        "during startup"
    } else {
        "unexpectedly"
    };
    let lines = shared.output_snapshot();
    let detail = lines
        .last()
        .map(|line| line.trim().to_string())
        .unwrap_or_default();
    if !detail.is_empty() {
        return format!("sing-box exited {stage}: {detail}");
    }
    let Some(exit_code) = exit_code else {
        return format!("sing-box exited {stage}.");
    };
    let status_name = if exit_code < 0 {
        "CrashExit"
    } else {
        "NormalExit"
    };
    format!("sing-box exited {stage} with code {exit_code} ({status_name}).")
}

fn process_alive(process: &Process) -> bool {
    matches!(process.lock().try_wait(), Ok(None))
}

fn process_exited(process: &Process) -> bool {
    matches!(process.lock().try_wait(), Ok(Some(_)))
}

fn wait_for_process(process: &Process, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if process_exited(process) {
            return true;
        }
        if Instant::now() >= deadline {
            return process_exited(process);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// `subprocess_utils.kill_processes_by_path` compatibility shim: a path/name
/// does not establish process ownership. Runtime managers already
/// terminate/wait on their retained handles. Never kills anything.
fn kill_processes_by_path(_process_name: &str, _executable_path: &Path, _timeout: f64) -> bool {
    false
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_tun_ready_script(pid: u32, escaped_name: &str, wait_ms: i64) -> String {
    let mut script = String::new();
    script.push_str("$deadline = [DateTime]::UtcNow.AddMilliseconds(");
    script.push_str(&wait_ms.to_string());
    script.push_str("); ");
    script.push_str("while ([DateTime]::UtcNow -lt $deadline) { ");
    script.push_str("if (-not (Get-Process -Id ");
    script.push_str(&pid.to_string());
    script.push_str(" -ErrorAction SilentlyContinue)) { exit 2 }; ");
    script.push_str("$ipv4 = Get-NetIPAddress -InterfaceAlias '");
    script.push_str(escaped_name);
    script.push_str("' -AddressFamily IPv4 ");
    script.push_str("-ErrorAction SilentlyContinue | Where-Object { ");
    script.push_str("$_");
    script.push_str(".IPAddress -and ");
    script.push_str("$_");
    script.push_str(".IPAddress -ne '0.0.0.0' } ");
    script.push_str("| Select-Object -First 1 IPAddress; ");
    script.push_str("$route = Get-NetRoute -InterfaceAlias '");
    script.push_str(escaped_name);
    script.push_str("' -AddressFamily IPv4 ");
    script.push_str("-ErrorAction SilentlyContinue | Where-Object { ");
    script.push_str("$_");
    script.push_str(".DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','128.0.0.0/1') } ");
    script.push_str("| Select-Object -First 1 DestinationPrefix; ");
    script.push_str("if ($ipv4 -and $route) { exit 0 }; Start-Sleep -Milliseconds 500 }; exit 1");
    script
}

// ── Error typing for the runtime-file edits ─────────────────────────────────

#[derive(Debug)]
enum ConfigIoError {
    Json(serde_json::Error),
    Io(std::io::Error),
    Runtime(String),
}

impl std::fmt::Display for ConfigIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigIoError::Json(error) => write!(formatter, "{error}"),
            ConfigIoError::Io(error) => write!(formatter, "{error}"),
            ConfigIoError::Runtime(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for ConfigIoError {}

fn error_type_name(error: &ConfigIoError) -> &'static str {
    match error {
        ConfigIoError::Json(_) => "JSONDecodeError",
        ConfigIoError::Runtime(_) => "RuntimeError",
        ConfigIoError::Io(error) => match error.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            _ => "OSError",
        },
    }
}

fn direct_masque_compatibility_error(exe: &Path, config: &Value) -> String {
    if !SingBoxManager::requires_lumen_direct_masque(config) {
        return String::new();
    }
    let version = get_singbox_version(&exe.to_string_lossy()).unwrap_or_default();
    direct_masque_version_error(&version)
}

fn direct_masque_version_error(version: &str) -> String {
    if LUMEN_VERSION_RE.is_match(version) {
        return String::new();
    }
    let rendered = if version.is_empty() {
        "unknown"
    } else {
        version
    };
    format!(
        "This MASQUE profile requires the Lumen-compatible sing-box core (installed: {rendered}). Reinstall or update Lumen; the regular sing-box extended build ignores raw MASQUE keys and leaves the tunnel uninitialized."
    )
}

/// Run `sing-box version` and extract the version token from the first line.
pub fn get_singbox_version(singbox_path: &str) -> Option<String> {
    let exe = resolve_configured_path(singbox_path, Some(&singbox_path_default()), true, true)?;
    if !exe.exists() {
        return None;
    }
    let command: Vec<String> = vec![exe.to_string_lossy().into_owned(), "version".into()];
    let result = run_text_owned(&command, 3.0, false).ok()?;

    let output = result.output_text();
    let lines: Vec<&str> = output.lines().collect();
    let first_line = lines.first()?.trim();
    Some(
        VERSION_RE
            .find(first_line)
            .map(|found| found.as_str().to_owned())
            .unwrap_or_else(|| first_line.to_owned()),
    )
}
