//! Supervise the upstream Xray binary (`engines/xray/manager.py`).

mod ports;

use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, BufReader, PipeReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use lumen_core::constants::{xray_config_file, xray_path_default};
use lumen_core::core_log_limiter::CoreLogLimiter;
use lumen_core::path_utils::resolve_configured_path;
use lumen_core::pyjson;
#[cfg(windows)]
use lumen_platform::runtime_priority::CORE_RUN_PRIORITY_FLAG;
#[cfg(windows)]
use lumen_platform::subprocess::CREATE_NO_WINDOW;
use lumen_platform::subprocess::{
    RunError, decode_output, hidden_command, is_windows_shutting_down, run_merged, run_text_owned,
    spawn_merged,
};
use parking_lot::{Condvar, Mutex};
use ports::{PortRoles, ensure_available, missing_ports, port_ready, required_ports};
use serde_json::Value;
use tokio::sync::broadcast;

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const EXIT_REPORT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RECENT_LINES: usize = 20;
const UNCONFIGURED_PATH: &str = "Путь к Xray не настроен (укажите его в Настройки -> Пути к ядрам)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrayEvent {
    Started,
    Stopped(i32),
    Log(String),
    Error(String),
    StateChanged(bool),
}

type Process = Arc<Mutex<Child>>;

#[derive(Default)]
struct State {
    process: Option<Process>,
    running: bool,
    stop_expected: bool,
    last_exit_expected: bool,
    last_exit_code: Option<i32>,
    recent_output: VecDeque<String>,
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
    settled: Condvar,
    events: broadcast::Sender<XrayEvent>,
}

impl Shared {
    fn emit(&self, event: XrayEvent) {
        let _ = self.events.send(event);
    }

    fn wait_for_exit_report(&self, process: &Process) -> bool {
        let mut state = self.state.lock();
        let deadline = Instant::now() + EXIT_REPORT_TIMEOUT;
        while state.owns(process) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            self.settled.wait_for(&mut state, remaining);
        }
        true
    }

    fn report_exit(&self, process: &Process, code: i32, shutting_down: bool) {
        let mut state = self.state.lock();
        if !state.owns(process) {
            return;
        }
        let expected = state.stop_expected || shutting_down;
        if !expected {
            self.emit(XrayEvent::Error(unexpected_exit(
                &state.recent_output,
                code,
                !state.running,
            )));
        }
        state.last_exit_expected = expected;
        state.last_exit_code = Some(code);
        state.process = None;
        state.running = false;
        state.stop_expected = false;
        self.emit(XrayEvent::Stopped(code));
        self.emit(XrayEvent::StateChanged(false));
        self.settled.notify_all();
    }
}

/// Starts and stops Xray synchronously. Call lifecycle methods on a worker
/// thread and subscribe before starting to receive lifecycle and log events.
pub struct XrayManager {
    config_path: PathBuf,
    shared: Arc<Shared>,
}

impl Default for XrayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl XrayManager {
    pub fn new() -> Self {
        Self::with_config_path(xray_config_file())
    }

    pub fn with_config_path(config_path: PathBuf) -> Self {
        let (events, _) = broadcast::channel(512);
        let config_path = std::path::absolute(&config_path).unwrap_or(config_path);
        Self {
            config_path,
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                settled: Condvar::new(),
                events,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<XrayEvent> {
        self.shared.events.subscribe()
    }

    pub fn is_running(&self) -> bool {
        self.shared.state.lock().running
    }

    pub fn last_exit_expected(&self) -> bool {
        self.shared.state.lock().last_exit_expected
    }

    pub fn last_exit_code(&self) -> Option<i32> {
        self.shared.state.lock().last_exit_code
    }

    pub fn validate_config(&self, xray_path: &str, config: &Value) -> (bool, String) {
        let exe = resolve_xray_path(xray_path);
        match exe {
            Some(exe) if exe.is_file() => self.validate(&exe, config),
            Some(exe) => (false, format!("xray.exe не найден: {}", exe.display())),
            None => (false, format!("xray.exe не найден: {xray_path}")),
        }
    }

    fn validate(&self, exe: &Path, config: &Value) -> (bool, String) {
        let result = (|| {
            let dir = self.config_dir();
            fs::create_dir_all(dir)?;
            let mut file = tempfile::Builder::new()
                .prefix("xray-check-")
                .suffix(".json")
                .tempfile_in(dir)?;
            write_config(&mut file, config)?;
            let path = file.into_temp_path();
            let mut command = xray_command(exe);
            command.args(["run", "-test", "-c"]).arg(&path);
            run_merged(command, 20.0)
        })();
        match result {
            Ok(result) => (
                result.returncode == 0,
                result.output_text().trim().to_owned(),
            ),
            Err(RunError::Timeout { .. }) => (
                false,
                "Проверка конфигурации Xray превысила 20 секунд".into(),
            ),
            Err(error) => (
                false,
                format!("Не удалось проверить конфигурацию Xray: {error}"),
            ),
        }
    }

    pub fn start(&mut self, xray_path: &str, config: &Value, prevalidated: bool) -> bool {
        self.start_with_timeout(xray_path, config, prevalidated, READY_TIMEOUT)
    }

    fn start_with_timeout(
        &mut self,
        xray_path: &str,
        config: &Value,
        prevalidated: bool,
        ready_timeout: Duration,
    ) -> bool {
        let Some(exe) = (!xray_path.trim().is_empty())
            .then(|| resolve_xray_path(xray_path))
            .flatten()
        else {
            return self.fail(UNCONFIGURED_PATH);
        };
        if !exe.is_file() {
            return self.fail(format!("xray.exe не найден: {}", exe.display()));
        }
        if !prevalidated {
            let (valid, output) = self.validate(&exe, config);
            if !valid {
                let detail = if output.is_empty() {
                    "неизвестная ошибка проверки"
                } else {
                    &output
                };
                return self.fail(format!("Xray не принимает новый конфиг: {detail}"));
            }
        }
        if !self.stop(true, false) {
            return self.fail("Не удалось остановить предыдущий процесс Xray");
        }

        let ports = required_ports(config);
        if let Err(error) = self.check_ports(&ports, &exe) {
            return self.fail(error);
        }
        if let Err(error) = self.save_config(config) {
            return self.fail(format!("Не удалось записать конфигурацию Xray: {error}"));
        }

        for attempt in 0..2 {
            if attempt != 0 {
                thread::sleep(Duration::from_millis(500));
                self.shared.emit(XrayEvent::Log(
                    "[xray] ports were not ready; restarting core once".into(),
                ));
                if let Err(error) = self.check_ports(&ports, &exe) {
                    return self.fail(error);
                }
            }
            let process = match self.launch(&exe) {
                Ok(process) => process,
                Err(error) => return self.fail(format!("Не удалось запустить Xray: {error}")),
            };
            match self.wait_until_ready(&process, &ports, ready_timeout) {
                Readiness::Ready => return true,
                Readiness::Exited => {
                    self.shared.wait_for_exit_report(&process);
                    return false;
                }
                Readiness::TimedOut => {
                    if !self.stop(true, attempt == 0) || !self.last_exit_expected() {
                        return false;
                    }
                }
            }
        }
        self.fail(format!(
            "Xray запустился, но не открыл нужные порты: {}",
            missing_ports(&ports)
        ))
    }

    pub fn stop(&mut self, expected: bool, fast: bool) -> bool {
        let process = {
            let mut state = self.shared.state.lock();
            let Some(process) = state.process.clone() else {
                return true;
            };
            let mut child = process.lock();
            if !matches!(child.try_wait(), Ok(Some(_))) {
                state.stop_expected = expected;
                let _ = child.kill();
            }
            drop(child);
            process
        };
        let (terminate_timeout, kill_timeout) = if fast {
            (Duration::from_millis(800), Duration::from_millis(500))
        } else {
            (Duration::from_secs(3), Duration::from_secs(2))
        };
        let stopped = wait_for_process(&process, terminate_timeout) || {
            let _ = process.lock().kill();
            wait_for_process(&process, kill_timeout)
        };
        if stopped && self.shared.wait_for_exit_report(&process) {
            return true;
        }
        let mut state = self.shared.state.lock();
        if !state.owns(&process) {
            return true;
        }
        if !stopped {
            state.stop_expected = false;
        }
        if !is_windows_shutting_down() {
            self.fail("Не удалось вовремя остановить процесс Xray");
        }
        false
    }

    fn fail(&self, message: impl Into<String>) -> bool {
        self.shared.emit(XrayEvent::Error(message.into()));
        false
    }

    fn check_ports(&self, ports: &PortRoles, exe: &Path) -> Result<(), String> {
        ensure_available(ports, exe, |line| self.shared.emit(XrayEvent::Log(line)))
    }

    fn config_dir(&self) -> &Path {
        self.config_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }

    fn save_config(&self, config: &Value) -> io::Result<()> {
        let dir = self.config_dir();
        fs::create_dir_all(dir)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        write_config(&mut file, config)?;
        file.persist(&self.config_path)?;
        Ok(())
    }

    fn launch(&self, exe: &Path) -> io::Result<Process> {
        *self.shared.state.lock() = State::default();
        let mut command = xray_command(exe);
        command.args(["run", "-c"]).arg(&self.config_path);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW | CORE_RUN_PRIORITY_FLAG);
        let (child, output) = spawn_merged(command)?;
        let process = Arc::new(Mutex::new(child));
        self.shared.state.lock().process = Some(process.clone());

        let (drained, output_done) = mpsc::channel();
        let spawn = thread::Builder::new()
            .name("xray-output-reader".into())
            .spawn({
                let shared = self.shared.clone();
                let process = process.clone();
                move || {
                    read_output(output, &shared, &process);
                    let _ = drained.send(());
                }
            });
        let spawn = spawn.and_then(|_| {
            thread::Builder::new()
                .name("xray-process-monitor".into())
                .spawn({
                    let shared = self.shared.clone();
                    let process = process.clone();
                    move || {
                        let code = loop {
                            let status = process.lock().try_wait();
                            match status {
                                Ok(Some(status)) => break status.code().unwrap_or(-1),
                                Ok(None) => thread::sleep(Duration::from_millis(25)),
                                Err(_) => {
                                    let mut child = process.lock();
                                    let _ = child.kill();
                                    break child
                                        .wait()
                                        .ok()
                                        .and_then(|status| status.code())
                                        .unwrap_or(-1);
                                }
                            }
                        };
                        let _ = output_done.recv_timeout(OUTPUT_DRAIN_TIMEOUT);
                        shared.report_exit(&process, code, is_windows_shutting_down());
                    }
                })
        });
        if let Err(error) = spawn {
            self.shared.state.lock().process = None;
            let mut child = process.lock();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(process)
    }

    fn wait_until_ready(
        &self,
        process: &Process,
        ports: &PortRoles,
        timeout: Duration,
    ) -> Readiness {
        let deadline = Instant::now() + timeout;
        loop {
            if !process_alive(process) {
                return Readiness::Exited;
            }
            if ports.iter().all(|(port, _)| {
                let remaining = deadline.saturating_duration_since(Instant::now());
                !remaining.is_zero() && port_ready(*port, remaining.min(Duration::from_millis(500)))
            }) {
                let mut state = self.shared.state.lock();
                if !state.owns(process) || !process_alive(process) {
                    return Readiness::Exited;
                }
                state.running = true;
                self.shared.emit(XrayEvent::Started);
                self.shared.emit(XrayEvent::StateChanged(true));
                return Readiness::Ready;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return if process_alive(process) {
                    Readiness::TimedOut
                } else {
                    Readiness::Exited
                };
            }
            thread::sleep(remaining.min(Duration::from_millis(100)));
        }
    }
}

impl Drop for XrayManager {
    fn drop(&mut self) {
        self.stop(true, true);
    }
}

enum Readiness {
    Ready,
    Exited,
    TimedOut,
}

fn process_alive(process: &Process) -> bool {
    matches!(process.lock().try_wait(), Ok(None))
}

fn wait_for_process(process: &Process, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if matches!(process.lock().try_wait(), Ok(Some(_))) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

fn read_output(output: PipeReader, shared: &Shared, process: &Process) {
    let mut reader = BufReader::new(output);
    let mut limiter = CoreLogLimiter::with_limits("xray", 30, 10);
    let mut bytes = Vec::new();
    while let Ok(1..) = reader.read_until(b'\n', &mut bytes) {
        for line in decode_output(&bytes)
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
        {
            let mut state = shared.state.lock();
            if !state.owns(process) {
                return;
            }
            if state.recent_output.len() == MAX_RECENT_LINES {
                state.recent_output.pop_front();
            }
            state.recent_output.push_back(line.to_owned());
            let (summary, visible) = limiter.admit(line);
            if let Some(summary) = summary {
                shared.emit(XrayEvent::Log(summary));
            }
            if visible {
                shared.emit(XrayEvent::Log(line.to_owned()));
            }
        }
        bytes.clear();
    }
}

fn unexpected_exit(recent: &VecDeque<String>, code: i32, startup: bool) -> String {
    let stage = if startup {
        "во время запуска"
    } else {
        "неожиданно"
    };
    let joined = recent
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    if joined.contains("fakedns")
        && (joined.contains("panic:") || joined.contains("nil pointer dereference"))
    {
        return format!(
            "Xray завершился {stage}: текущий Xray core упал на секции FakeDNS в конфиге. \
             Отключите FakeDNS в Xray JSON, сбросьте конфиг на шаблон по умолчанию или обновите Xray core."
        );
    }
    let lines = || recent.iter().rev().map(|line| line.trim());
    let detail = lines()
        .find(|line| {
            let lower = line.to_lowercase();
            [
                "panic:",
                "[xray-error]",
                "error",
                "failed",
                "invalid",
                "not found",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
        })
        .or_else(|| {
            lines().find(|line| {
                let lower = line.to_lowercase();
                !line.is_empty()
                    && !line.starts_with("github.com/")
                    && !lower.starts_with("goroutine ")
                    && !lower.starts_with("[signal")
            })
        })
        .or_else(|| lines().next());
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        return format!("Xray завершился {stage}: {detail}");
    }
    let status = if code < 0 { "CrashExit" } else { "NormalExit" };
    format!("Xray завершился {stage} с кодом {code} ({status}).")
}

fn write_config(file: &mut impl Write, config: &Value) -> io::Result<()> {
    let text = pyjson::dumps(config, true, Some(2));
    if cfg!(windows) {
        file.write_all(text.replace('\n', "\r\n").as_bytes())
    } else {
        file.write_all(text.as_bytes())
    }
}

fn xray_command(exe: &Path) -> Command {
    let mut command = hidden_command(exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command
}

fn resolve_xray_path(path: &str) -> Option<PathBuf> {
    resolve_configured_path(path, Some(&xray_path_default()), true, true)
}

pub fn get_xray_version(xray_path: &str) -> Option<String> {
    let exe = resolve_xray_path(xray_path)?;
    if !exe.exists() {
        return None;
    }
    let result = run_text_owned(
        &[exe.to_string_lossy().into_owned(), "version".into()],
        3.0,
        false,
    )
    .ok()?;
    result
        .output_text()
        .lines()
        .next()
        .map(|line| line.trim().to_owned())
}

#[cfg(test)]
mod tests;
