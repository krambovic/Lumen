//! Hidden-window subprocess helpers.
//!
//! Port of `windows/xray_fluent/subprocess_utils.py`. The Qt event-pumping
//! variants (`run_text_pumped`, `wait_for_qprocess_*`) collapse into
//! [`run_text`]: the Rust UI never blocks its render thread on a child
//! process, every caller runs on a worker thread or a tokio blocking task.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// `CREATE_NO_WINDOW` creation flag applied to every spawned helper.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Result of [`run_text`]; mirrors `subprocess.CompletedProcess[bytes]`.
#[derive(Debug, Clone, Default)]
pub struct CompletedProcess {
    pub returncode: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CompletedProcess {
    /// `result_output_text`: stdout when present, otherwise stderr.
    pub fn output_text(&self) -> String {
        if !self.stdout.is_empty() {
            decode_output(&self.stdout)
        } else {
            decode_output(&self.stderr)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("Command '{command}' timed out after {timeout} seconds")]
    Timeout { command: String, timeout: f64 },
    #[error("Command '{command}' returned non-zero exit status {code}.")]
    NonZero { command: String, code: i32, result: CompletedProcess },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// True once Windows has started ending the interactive session
/// (`GetSystemMetrics(SM_SHUTTINGDOWN)`).
pub fn is_windows_shutting_down() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SYSTEM_METRICS_INDEX};
        GetSystemMetrics(SYSTEM_METRICS_INDEX(0x2000)) != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn decode_codepage(data: &[u8], codepage: u32) -> Option<String> {
    use windows::Win32::Globalization::{MultiByteToWideChar, MB_ERR_INVALID_CHARS};
    if data.is_empty() {
        return Some(String::new());
    }
    unsafe {
        let needed = MultiByteToWideChar(codepage, MB_ERR_INVALID_CHARS, data, None);
        if needed <= 0 {
            return None;
        }
        let mut wide = vec![0u16; needed as usize];
        let written = MultiByteToWideChar(codepage, MB_ERR_INVALID_CHARS, data, Some(&mut wide));
        if written <= 0 {
            return None;
        }
        wide.truncate(written as usize);
        String::from_utf16(&wide).ok()
    }
}

/// Decode console output: UTF-8, then OEM cp866, the ANSI code page and
/// cp1251, finally lossy UTF-8 (same order as the Python helper).
pub fn decode_output(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    if let Ok(text) = std::str::from_utf8(data) {
        return text.to_string();
    }
    #[cfg(windows)]
    {
        let ansi = unsafe { windows::Win32::Globalization::GetACP() };
        let mut pages = vec![866u32, ansi, 1251];
        pages.dedup();
        for page in pages {
            if let Some(text) = decode_codepage(data, page) {
                return text;
            }
        }
    }
    String::from_utf8_lossy(data).into_owned()
}

fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = reader.read_to_end(&mut out);
        out
    })
}

/// Build a hidden-window command (`creationflags=CREATE_NO_WINDOW`).
pub fn hidden_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// `run_text`: run `command`, capture raw stdout/stderr, enforce `timeout`.
/// The child is always started without a console window.
pub fn run_text(command: &[&str], timeout: f64, check: bool) -> Result<CompletedProcess, RunError> {
    let owned: Vec<String> = command.iter().map(|s| s.to_string()).collect();
    run_text_owned(&owned, timeout, check)
}

pub fn run_text_owned(command: &[String], timeout: f64, check: bool) -> Result<CompletedProcess, RunError> {
    let display = command.join(" ");
    let Some((program, args)) = command.split_first() else {
        return Err(RunError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty command")));
    };
    let mut child = hidden_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().map(spawn_reader);
    let stderr = child.stderr.take().map(spawn_reader);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout { command: display, timeout });
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let result = CompletedProcess {
        returncode: status.code().unwrap_or(-1),
        stdout: stdout.map(|h| h.join().unwrap_or_default()).unwrap_or_default(),
        stderr: stderr.map(|h| h.join().unwrap_or_default()).unwrap_or_default(),
    };
    if check && result.returncode != 0 {
        return Err(RunError::NonZero { command: display, code: result.returncode, result });
    }
    Ok(result)
}

/// `is_same_path`: both paths resolve to the same canonical location.
pub fn is_same_path(left: Option<&Path>, right: Option<&Path>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    if left.as_os_str().is_empty() || right.as_os_str().is_empty() {
        return false;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf8_and_cp866() {
        assert_eq!(decode_output(b""), "");
        assert_eq!(decode_output("Привет".as_bytes()), "Привет");
        // "Ок" in cp866: 0x8E 0xAA
        #[cfg(windows)]
        assert_eq!(decode_output(&[0x8E, 0xAA]), "Ок");
    }

    #[cfg(windows)]
    #[test]
    fn run_text_captures_output() {
        let result = run_text(&["cmd", "/c", "echo lumen"], 10.0, true).unwrap();
        assert_eq!(result.returncode, 0);
        assert!(result.output_text().contains("lumen"));
        let failed = run_text(&["cmd", "/c", "exit 3"], 10.0, true);
        assert!(matches!(failed, Err(RunError::NonZero { code: 3, .. })));
    }
}
