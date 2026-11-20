//! Coordinate endpoint, proxy and connectivity measurement workers.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use lumen_core::constants::{
    DEFAULT_HTTP_PORT, SPEED_TEST_TIMEOUT, singbox_path_default, xray_path_default,
};
use lumen_core::http_utils::connectivity_test;
use lumen_core::i18n::tr;
use lumen_core::models::{AppState, Node, utc_now_iso};
use lumen_core::path_utils::resolve_configured_path;
use lumen_core::ping::{PING_METHODS, PingEvent, PingWorker};
use lumen_core::probe_capabilities::{probe_backend, probe_capability};
use lumen_engines::speed_test::{SpeedTestEvent, SpeedTestWorker};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::probe_targets::prepare_target;
use crate::session_state::ActiveSessionSnapshot;

const DEFAULT_CONNECTIVITY_URL: &str = "https://www.gstatic.com/generate_204";
const HISTORY_LIMIT: usize = 50;

#[derive(Clone, Copy, Default)]
pub struct TestConnection<'a> {
    pub connected: bool,
    pub active_session: Option<&'a ActiveSessionSnapshot>,
    pub effective_http_port: Option<u16>,
    pub effective_proxy_url: Option<&'a str>,
}

impl TestConnection<'_> {
    fn bypass_tun(self) -> bool {
        self.connected && self.active_session.is_some_and(|session| session.tun_mode)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestKind {
    Ping,
    Speed,
}

#[derive(Debug, PartialEq)]
pub enum TestEvent {
    Status {
        level: &'static str,
        message: String,
    },
    Log(String),
    PingUpdated {
        node_id: String,
        ms: Option<i64>,
    },
    SpeedUpdated {
        node_id: String,
        mbps: Option<f64>,
        alive: bool,
    },
    SpeedNodeProgress {
        node_id: String,
        percent: u32,
    },
    Progress {
        kind: TestKind,
        completed: usize,
        total: usize,
        finished: bool,
    },
    SpeedCancelled {
        completed: usize,
        total: usize,
    },
    ConnectivityDone {
        ok: bool,
        message: String,
        elapsed_ms: Option<u64>,
    },
    SaveRequested,
    ScheduleSave,
}

#[derive(Clone)]
enum ProbeWorker {
    Endpoint(Arc<PingWorker>),
    Proxy(Arc<SpeedTestWorker>),
}

impl ProbeWorker {
    fn cancel(&self) {
        match self {
            Self::Endpoint(worker) => worker.cancel(),
            Self::Proxy(worker) => worker.cancel(),
        }
    }

    fn unavailable(&self, id: &str) -> bool {
        matches!(self, Self::Proxy(worker) if worker.is_unavailable(id))
    }
}

struct RunningProbe {
    id: u64,
    worker: ProbeWorker,
    thread: JoinHandle<()>,
    targets: HashMap<String, String>,
    completed: usize,
    total: usize,
}

struct RunningConnectivity {
    id: u64,
    task: tokio::task::JoinHandle<()>,
}

struct RetiredProbe {
    worker: ProbeWorker,
    thread: JoinHandle<()>,
}

pub struct WorkerMessage {
    id: u64,
    event: Message,
}

enum Message {
    Endpoint(PingEvent),
    Proxy(SpeedTestEvent),
    Finished {
        completed: Option<usize>,
        cancelled: bool,
        failed: bool,
    },
    Connectivity(bool, String, Option<u64>),
}

pub struct WorkerService {
    ping: Option<RunningProbe>,
    speed: Option<RunningProbe>,
    connectivity: Option<RunningConnectivity>,
    retired: Vec<RetiredProbe>,
    retired_connectivity: Vec<tokio::task::JoinHandle<()>>,
    sequence: u64,
    send: UnboundedSender<WorkerMessage>,
    receive: UnboundedReceiver<WorkerMessage>,
}

impl Default for WorkerService {
    fn default() -> Self {
        let (send, receive) = mpsc::unbounded_channel();
        Self {
            ping: None,
            speed: None,
            connectivity: None,
            retired: Vec::new(),
            retired_connectivity: Vec::new(),
            sequence: 0,
            send,
            receive,
        }
    }
}

impl WorkerService {
    pub fn is_running(&self) -> bool {
        self.ping.is_some() || self.speed.is_some() || self.connectivity.is_some()
    }

    pub(crate) fn extend_core_process_pids(&self, pids: &mut HashSet<u32>) {
        let active = [&self.ping, &self.speed]
            .into_iter()
            .flatten()
            .map(|probe| &probe.worker);
        for worker in active.chain(self.retired.iter().map(|probe| &probe.worker)) {
            if let ProbeWorker::Proxy(worker) = worker {
                pids.extend(worker.process_ids());
            }
        }
    }

    pub fn ping_nodes(
        &mut self,
        state: &mut AppState,
        ids: Option<&HashSet<String>>,
        method: Option<&str>,
        connection: TestConnection<'_>,
        mut emit: impl FnMut(TestEvent),
    ) -> bool {
        let method = method
            .filter(|method| !method.is_empty())
            .unwrap_or(&state.settings.ping_method)
            .trim()
            .to_lowercase();
        let method = if PING_METHODS.contains(&method.as_str()) {
            method.as_str()
        } else {
            "http"
        };
        let nodes = select_targets(state, ids, method, &mut emit);
        if nodes.is_empty() {
            return false;
        }
        let targets = target_kinds(&nodes, method);
        let bypass = connection.bypass_tun();
        let worker = if matches!(method, "http" | "real") {
            let Some(worker) = proxy_worker(state, nodes, method, bypass, connection, &mut emit) else {
                return false;
            };
            emit(TestEvent::Log(
                "[ping] Измеряю реальную задержку через временные прокси".into(),
            ));
            worker
        } else {
            if bypass {
                emit(TestEvent::Log(
                    "[ping] TUN включен: проверяю серверы через временные прямые маршруты".into(),
                ));
            }
            ProbeWorker::Endpoint(Arc::new(PingWorker::new(nodes, 2.0, bypass, method)))
        };
        self.reap_finished();
        let Some(running) = self.start_probe(worker, targets, &mut emit) else {
            return false;
        };
        if let Some(previous) = self.ping.take() {
            previous.worker.cancel();
            self.retire_probe(previous);
        }
        clear_measurements(state, &running.targets, TestKind::Ping, &mut emit);
        progress(&mut emit, TestKind::Ping, 0, running.total, false);
        self.ping = Some(running);
        true
    }

    pub fn speed_test_nodes(
        &mut self,
        state: &mut AppState,
        ids: Option<&HashSet<String>>,
        connection: TestConnection<'_>,
        mut emit: impl FnMut(TestEvent),
    ) -> bool {
        if !state.nodes.iter().any(|node| selected(node, ids)) {
            return false;
        }
        if self.speed.is_some() {
            status(
                &mut emit,
                "info",
                "Тест скорости уже выполняется. Остановите его перед новым запуском.",
            );
            return false;
        }
        let nodes = select_targets(state, ids, "speed", &mut emit);
        if nodes.is_empty() {
            return false;
        }
        let targets = target_kinds(&nodes, "speed");
        let bypass = connection.bypass_tun();
        let Some(worker) = proxy_worker(state, nodes, "speed", bypass, connection, &mut emit) else {
            return false;
        };
        if bypass {
            emit(TestEvent::Log(
                "[speed] TUN включен: тестирую через временные прямые маршруты".into(),
            ));
        }
        self.reap_finished();
        let Some(running) = self.start_probe(worker, targets, &mut emit) else {
            return false;
        };
        clear_measurements(state, &running.targets, TestKind::Speed, &mut emit);
        progress(&mut emit, TestKind::Speed, 0, running.total, false);
        self.speed = Some(running);
        true
    }

    pub fn cancel_speed_test(&self, mut emit: impl FnMut(TestEvent)) -> bool {
        let cancelled = self.speed.as_ref().is_some_and(cancel_probe);
        status(
            &mut emit,
            "info",
            if cancelled {
                "Останавливаю тест скорости..."
            } else {
                "Тест скорости сейчас не выполняется"
            },
        );
        cancelled
    }

    pub fn cancel_tests(&self, mut emit: impl FnMut(TestEvent)) -> bool {
        let ping = self.ping.as_ref().is_some_and(cancel_probe);
        let speed = self.speed.as_ref().is_some_and(cancel_probe);
        let cancelled = ping || speed;
        status(
            &mut emit,
            "info",
            if cancelled {
                "Останавливаю тесты..."
            } else {
                "Тесты сейчас не выполняются"
            },
        );
        cancelled
    }

    pub fn test_connectivity(
        &mut self,
        state: &AppState,
        url: Option<&str>,
        connection: TestConnection<'_>,
        mut emit: impl FnMut(TestEvent),
    ) -> bool {
        if self.connectivity.is_some() {
            status(&mut emit, "info", "Тест подключения уже выполняется");
            return false;
        }
        let url = url
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .unwrap_or(DEFAULT_CONNECTIVITY_URL)
            .to_owned();
        let port = connection
            .effective_http_port
            .filter(|port| *port != 0)
            .unwrap_or_else(|| {
                u16::try_from(state.settings.local_http_port).unwrap_or(DEFAULT_HTTP_PORT)
            });
        let proxy = connection
            .effective_proxy_url
            .unwrap_or_default()
            .to_owned();
        let tun = state.settings.tun_mode;
        let id = self.next_id();
        let send = self.send.clone();
        let task = tokio::spawn(async move {
            let (ok, message, elapsed) = connectivity_test(port, &url, 8.0, tun, &proxy).await;
            let _ = send.send(WorkerMessage {
                id,
                event: Message::Connectivity(ok, message, elapsed),
            });
        });
        self.connectivity = Some(RunningConnectivity { id, task });
        true
    }

    /// Receive while a test is running, then pass the message to `handle_worker_event`.
    pub async fn next_worker_event(&mut self) -> Option<WorkerMessage> {
        if self.is_running() {
            self.receive.recv().await
        } else {
            None
        }
    }

    pub fn handle_worker_event(
        &mut self,
        state: &mut AppState,
        message: WorkerMessage,
        mut emit: impl FnMut(TestEvent),
    ) {
        self.reap_finished();
        let WorkerMessage { id, event } = message;
        if self
            .connectivity
            .as_ref()
            .is_some_and(|worker| worker.id == id)
        {
            if let Message::Connectivity(ok, message, elapsed_ms) = event {
                self.connectivity = None;
                report_connectivity(ok, message, elapsed_ms, &mut emit);
            }
            return;
        }
        let (kind, slot) = if self.ping.as_ref().is_some_and(|probe| probe.id == id) {
            (TestKind::Ping, &mut self.ping)
        } else if self.speed.as_ref().is_some_and(|probe| probe.id == id) {
            (TestKind::Speed, &mut self.speed)
        } else {
            return;
        };
        if let Message::Finished {
            completed,
            cancelled,
            failed,
        } = event
        {
            let probe = slot.take().expect("matched active worker");
            let completed = completed.unwrap_or(probe.completed);
            if cancelled && kind == TestKind::Speed {
                emit(TestEvent::SpeedCancelled {
                    completed,
                    total: probe.total,
                });
            }
            progress(&mut emit, kind, completed, probe.total, true);
            emit(TestEvent::SaveRequested);
            if failed {
                status(&mut emit, "error", "Не удалось завершить тест");
            } else if kind == TestKind::Speed {
                if cancelled {
                    emit(TestEvent::Status {
                        level: "info",
                        message: tr(
                            "Тест скорости остановлен ({completed}/{total})",
                            &[
                                ("completed", &completed.to_string()),
                                ("total", &probe.total.to_string()),
                            ],
                        ),
                    });
                } else {
                    status(&mut emit, "success", "Тест скорости завершён");
                }
            }
            self.retire_probe(probe);
            return;
        }
        let probe = slot.as_mut().expect("matched active worker");
        apply_measurement(state, probe, kind, event, &mut emit);
    }

    /// Stop current work before changing profiles or shutting down.
    pub fn stop_workers(&mut self) {
        for probe in [self.ping.take(), self.speed.take()].into_iter().flatten() {
            probe.worker.cancel();
            self.retire_probe(probe);
        }
        if let Some(worker) = self.connectivity.take() {
            worker.task.abort();
            self.retired_connectivity.push(worker.task);
        }
        self.reap_finished();
    }

    /// Includes cancelled probes that are still releasing temporary cores and routes.
    pub fn has_pending_workers(&mut self) -> bool {
        self.reap_finished();
        self.is_running() || !self.retired.is_empty() || !self.retired_connectivity.is_empty()
    }

    fn next_id(&mut self) -> u64 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    fn start_probe(
        &mut self,
        worker: ProbeWorker,
        targets: HashMap<String, String>,
        emit: &mut impl FnMut(TestEvent),
    ) -> Option<RunningProbe> {
        let id = self.next_id();
        let send = self.send.clone();
        let run = worker.clone();
        let thread = match thread::Builder::new()
            .name("lumen-measurement".into())
            .spawn(move || run_probe(id, run, send))
        {
            Ok(thread) => thread,
            Err(error) => {
                status(emit, "error", "Не удалось запустить тест");
                emit(TestEvent::Log(format!("[test] {error}")));
                return None;
            }
        };
        Some(RunningProbe {
            id,
            worker,
            thread,
            total: targets.len(),
            targets,
            completed: 0,
        })
    }

    fn reap_finished(&mut self) {
        self.retired_connectivity.retain(|task| !task.is_finished());
        let mut index = 0;
        while index < self.retired.len() {
            if self.retired[index].thread.is_finished() {
                let _ = self.retired.swap_remove(index).thread.join();
            } else {
                index += 1;
            }
        }
    }

    fn retire_probe(&mut self, probe: RunningProbe) {
        self.retired.push(RetiredProbe {
            worker: probe.worker,
            thread: probe.thread,
        });
    }
}

impl Drop for WorkerService {
    fn drop(&mut self) {
        self.stop_workers();
    }
}

fn run_probe(id: u64, worker: ProbeWorker, send: UnboundedSender<WorkerMessage>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &worker {
        ProbeWorker::Endpoint(worker) => worker.run(&mut |event| {
            if !matches!(event, PingEvent::Completed) {
                let _ = send.send(WorkerMessage {
                    id,
                    event: Message::Endpoint(event),
                });
            }
        }),
        ProbeWorker::Proxy(worker) => {
            let send = send.clone();
            worker.run(Arc::new(move |event| {
                if !matches!(event, SpeedTestEvent::Completed) {
                    let _ = send.send(WorkerMessage {
                        id,
                        event: Message::Proxy(event),
                    });
                }
            }));
        }
    }));
    let (completed, cancelled) = match worker {
        ProbeWorker::Endpoint(worker) => (None, worker.is_cancelled()),
        ProbeWorker::Proxy(worker) => (Some(worker.completed_nodes()), worker.was_cancelled()),
    };
    let _ = send.send(WorkerMessage {
        id,
        event: Message::Finished {
            completed,
            cancelled,
            failed: result.is_err(),
        },
    });
}

fn selected(node: &Node, ids: Option<&HashSet<String>>) -> bool {
    ids.is_none_or(|ids| ids.is_empty() || ids.contains(&node.id))
}

fn select_targets(
    state: &AppState,
    ids: Option<&HashSet<String>>,
    method: &str,
    emit: &mut impl FnMut(TestEvent),
) -> Vec<Node> {
    let mut supported = Vec::new();
    let mut skipped = Vec::new();
    for node in state.nodes.iter().filter(|node| selected(node, ids)) {
        if let Some(target) = prepare_target(node, method) {
            supported.push(target);
        } else {
            let label = [&*node.name, &*node.server, &*node.scheme]
                .into_iter()
                .find(|label| !label.is_empty())
                .unwrap_or("—");
            skipped.push(label);
        }
    }
    if !skipped.is_empty() {
        let test = tr(
            match method {
                "speed" => "Тест скорости",
                "http" => "HTTP GET ping",
                "real" => "Реальный пинг",
                _ => "Пинг",
            },
            &[],
        );
        let mut labels = skipped
            .iter()
            .take(3)
            .map(|label| (*label).to_owned())
            .collect::<Vec<_>>();
        if skipped.len() > 3 {
            labels.push(tr(
                "и ещё {count}",
                &[("count", &(skipped.len() - 3).to_string())],
            ));
        }
        emit(TestEvent::Status {
            level: "warning",
            message: tr(
                "{test} не поддерживается для выбранных серверов: {servers}",
                &[("test", &test), ("servers", &labels.join(", "))],
            ),
        });
    }
    supported
}

fn target_kinds(nodes: &[Node], method: &str) -> HashMap<String, String> {
    nodes
        .iter()
        .map(|node| (node.id.clone(), probe_capability(node, method).method))
        .collect()
}

fn configured_xray(state: &AppState, emit: &mut impl FnMut(TestEvent)) -> Option<PathBuf> {
    let default = xray_path_default();
    let path = resolve_configured_path(&state.settings.xray_path, Some(&default), true, true)
        .unwrap_or(default);
    if path.is_file() {
        return Some(path);
    }
    emit(TestEvent::Status {
        level: "error",
        message: format!("{}{}", tr("xray.exe не найден: ", &[]), path.display()),
    });
    None
}

fn proxy_worker(
    state: &AppState,
    nodes: Vec<Node>,
    method: &str,
    bypass: bool,
    connection: TestConnection<'_>,
    emit: &mut impl FnMut(TestEvent),
) -> Option<ProbeWorker> {
    let xray = if nodes.iter().any(|node| probe_backend(node) == "xray") {
        configured_xray(state, emit)?.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let singbox = resolve_configured_path(
        &state.settings.singbox_path,
        Some(&singbox_path_default()),
        true,
        false,
    )
    .unwrap_or_default();
    let speed = method == "speed";
    Some(ProbeWorker::Proxy(Arc::new(SpeedTestWorker::with_options(
        nodes,
        &xray,
        Some(state.routing.clone()),
        SPEED_TEST_TIMEOUT,
        if speed { "speed" } else { "ping" },
        if speed {
            &state.settings.speed_test_url
        } else {
            ""
        },
        state.settings.speed_test_concurrency,
        bypass,
        &singbox.to_string_lossy(),
        method == "http",
    )
    // The connected node and its pinned health listener: an active
    // AWG / WireGuard profile is measured through it, never with a second
    // handshake on the same key.
    .with_active_profile(
        &connection
            .active_session
            .filter(|_| connection.connected)
            .and_then(|session| session.node_id.clone())
            .unwrap_or_default(),
        &connection
            .active_session
            .filter(|_| connection.connected)
            .map(|session| session.health_proxy_url.clone())
            .unwrap_or_default(),
    ))))
}

fn clear_measurements(
    state: &mut AppState,
    targets: &HashMap<String, String>,
    kind: TestKind,
    emit: &mut impl FnMut(TestEvent),
) {
    let mut changed = false;
    for node in state
        .nodes
        .iter_mut()
        .filter(|node| targets.contains_key(&node.id))
    {
        let cleared = match kind {
            TestKind::Ping if node.ping_ms.is_some() || node.is_alive.is_some() => {
                node.ping_ms = None;
                emit(TestEvent::PingUpdated {
                    node_id: node.id.clone(),
                    ms: None,
                });
                true
            }
            TestKind::Speed if node.speed_mbps.is_some() || node.is_alive.is_some() => {
                node.speed_mbps = None;
                emit(TestEvent::SpeedUpdated {
                    node_id: node.id.clone(),
                    mbps: None,
                    alive: false,
                });
                emit(TestEvent::SpeedNodeProgress {
                    node_id: node.id.clone(),
                    percent: 0,
                });
                true
            }
            _ => false,
        };
        if cleared {
            node.is_alive = None;
            changed = true;
        }
    }
    if changed {
        emit(TestEvent::ScheduleSave);
    }
}

fn cancel_probe(probe: &RunningProbe) -> bool {
    if probe.thread.is_finished() {
        false
    } else {
        probe.worker.cancel();
        true
    }
}

fn apply_measurement(
    state: &mut AppState,
    probe: &mut RunningProbe,
    kind: TestKind,
    event: Message,
    emit: &mut impl FnMut(TestEvent),
) {
    match event {
        Message::Endpoint(PingEvent::Result { node_id, ms })
        | Message::Proxy(SpeedTestEvent::PingResult {
            node_id,
            delay_ms: ms,
        }) => {
            if let Some(method) = probe.targets.get(&node_id)
                && let Some(node) = state.nodes.iter_mut().find(|node| node.id == node_id)
            {
                node.ping_ms = ms;
                node.ping_kind = if probe.worker.unavailable(&node_id) {
                    "unavailable".into()
                } else {
                    method.clone()
                };
                node.is_alive = (node.ping_kind == "proxy").then_some(ms.is_some());
                append_history(&mut node.ping_history, ms);
            }
            emit(TestEvent::PingUpdated { node_id, ms });
        }
        Message::Proxy(SpeedTestEvent::NodeResult {
            node_id,
            speed_mbps,
            is_alive,
        }) => {
            if probe.targets.contains_key(&node_id)
                && let Some(node) = state.nodes.iter_mut().find(|node| node.id == node_id)
            {
                let unavailable = probe.worker.unavailable(&node_id);
                node.speed_mbps = speed_mbps;
                node.ping_kind = if unavailable { "unavailable" } else { "proxy" }.into();
                node.is_alive = (!unavailable).then_some(is_alive);
                append_history(&mut node.speed_history, speed_mbps);
                emit(TestEvent::ScheduleSave);
            }
            emit(TestEvent::SpeedUpdated {
                node_id,
                mbps: speed_mbps,
                alive: is_alive,
            });
        }
        Message::Endpoint(PingEvent::Progress { current, total })
        | Message::Proxy(SpeedTestEvent::Progress {
            completed: current,
            total,
        }) => {
            probe.completed = current;
            progress(emit, kind, current, total, false);
        }
        Message::Proxy(SpeedTestEvent::NodeProgress { node_id, percent })
            if kind == TestKind::Speed =>
        {
            emit(TestEvent::SpeedNodeProgress {
                node_id,
                percent: percent.min(100),
            });
        }
        _ => {}
    }
}

fn append_history<T>(history: &mut Vec<(String, Option<T>)>, value: Option<T>) {
    history.push((utc_now_iso(), value));
    let excess = history.len().saturating_sub(HISTORY_LIMIT);
    history.drain(..excess);
}

fn report_connectivity(
    ok: bool,
    message: String,
    elapsed_ms: Option<u64>,
    emit: &mut impl FnMut(TestEvent),
) {
    if let Some(elapsed) = elapsed_ms.filter(|_| ok) {
        emit(TestEvent::Status {
            level: "success",
            message: format!(
                "{}{elapsed} {}",
                tr("Подключение в порядке: ", &[]),
                tr("мс", &[])
            ),
        });
        emit(TestEvent::Log(format!("[test] {message} ({elapsed} ms)")));
    } else {
        status(emit, "warning", "Тест подключения не пройден");
        emit(TestEvent::Log(format!("[test] {message}")));
    }
    emit(TestEvent::ConnectivityDone {
        ok,
        message,
        elapsed_ms,
    });
}

fn progress(
    emit: &mut impl FnMut(TestEvent),
    kind: TestKind,
    completed: usize,
    total: usize,
    finished: bool,
) {
    emit(TestEvent::Progress {
        kind,
        completed,
        total,
        finished,
    });
}

fn status(emit: &mut impl FnMut(TestEvent), level: &'static str, message: &str) {
    emit(TestEvent::Status {
        level,
        message: tr(message, &[]),
    });
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::timeout;

    use super::*;

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn speed_result_and_cancellation_update_history_and_save() {
        let mut state = AppState {
            nodes: vec![Node {
                id: "node".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut service = WorkerService::default();
        service.speed = Some(RunningProbe {
            id: 1,
            worker: ProbeWorker::Proxy(Arc::new(SpeedTestWorker::new(Vec::new(), ""))),
            thread: thread::spawn(|| {}),
            targets: [("node".into(), "proxy".into())].into(),
            total: 1,
            completed: 0,
        });
        let mut events = Vec::new();
        service.handle_worker_event(
            &mut state,
            WorkerMessage {
                id: 1,
                event: Message::Proxy(SpeedTestEvent::NodeResult {
                    node_id: "node".into(),
                    speed_mbps: Some(42.0),
                    is_alive: true,
                }),
            },
            |event| events.push(event),
        );
        service.handle_worker_event(
            &mut state,
            WorkerMessage {
                id: 1,
                event: Message::Finished {
                    completed: Some(1),
                    cancelled: true,
                    failed: false,
                },
            },
            |event| events.push(event),
        );
        let node = &state.nodes[0];
        assert_eq!(node.speed_mbps, Some(42.0));
        assert_eq!(node.speed_history[0].1, Some(42.0));
        assert_eq!(node.is_alive, Some(true));
        assert_eq!(node.ping_kind, "proxy");
        assert!(events.contains(&TestEvent::ScheduleSave));
        assert!(events.contains(&TestEvent::SpeedCancelled {
            completed: 1,
            total: 1
        }));
        assert!(events.contains(&TestEvent::SaveRequested));
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn endpoint_ping_replaces_the_old_run_and_preserves_speed_and_bounded_history() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port() as i64;
        let mut state = AppState {
            nodes: vec![Node {
                id: "node".into(),
                server: "127.0.0.1".into(),
                port,
                scheme: "vless".into(),
                ping_ms: Some(123),
                speed_mbps: Some(42.0),
                is_alive: Some(true),
                ping_history: vec![("old".into(), Some(20)); HISTORY_LIMIT],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut service = WorkerService::default();
        let mut events = Vec::new();
        let connection = TestConnection::default();
        assert!(
            service.ping_nodes(&mut state, None, Some("tcping"), connection, |event| events
                .push(event))
        );
        assert_eq!(state.nodes[0].ping_ms, None);
        assert_eq!(state.nodes[0].speed_mbps, Some(42.0));
        assert!(events.contains(&TestEvent::ScheduleSave));
        let previous_id = service.ping.as_ref().unwrap().id;
        assert!(
            service.ping_nodes(&mut state, None, Some("tcping"), connection, |event| events
                .push(event))
        );
        service.handle_worker_event(
            &mut state,
            WorkerMessage {
                id: previous_id,
                event: Message::Endpoint(PingEvent::Result {
                    node_id: "node".into(),
                    ms: Some(9_999),
                }),
            },
            |event| events.push(event),
        );
        assert!(state.nodes[0].ping_ms.is_none());
        while service.is_running() {
            let message = timeout(WAIT, service.next_worker_event())
                .await
                .unwrap()
                .unwrap();
            service.handle_worker_event(&mut state, message, |event| events.push(event));
        }
        assert!(state.nodes[0].ping_ms.is_some());
        assert_eq!(state.nodes[0].ping_kind, "tcping_endpoint");
        assert_eq!(state.nodes[0].is_alive, None);
        assert_eq!(state.nodes[0].ping_history.len(), HISTORY_LIMIT);
        assert_eq!(
            state.nodes[0].ping_history.last().unwrap().1,
            state.nodes[0].ping_ms
        );
        assert_eq!(state.nodes[0].speed_mbps, Some(42.0));
        assert!(events.contains(&TestEvent::Progress {
            kind: TestKind::Ping,
            completed: 1,
            total: 1,
            finished: true
        }));
        assert!(events.contains(&TestEvent::SaveRequested));
    }

    #[test]
    fn missing_xray_keeps_existing_measurements() {
        let root = tempfile::tempdir().unwrap();
        let mut state = AppState {
            nodes: vec![Node {
                id: "node".into(),
                scheme: "vless".into(),
                outbound: json!({"protocol":"vless"}),
                speed_mbps: Some(25.0),
                is_alive: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };
        state.settings.xray_path = root
            .path()
            .join("missing.exe")
            .to_string_lossy()
            .into_owned();
        let mut service = WorkerService::default();
        let mut events = Vec::new();
        assert!(
            !service.speed_test_nodes(&mut state, None, TestConnection::default(), |event| events
                .push(event))
        );
        assert_eq!(state.nodes[0].speed_mbps, Some(25.0));
        assert_eq!(state.nodes[0].is_alive, Some(true));
        assert!(matches!(
            &events[..],
            [TestEvent::Status { level: "error", .. }]
        ));
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn connectivity_uses_the_effective_proxy_and_rejects_a_duplicate_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let mut state = AppState::default();
        let mut service = WorkerService::default();
        let connection = TestConnection {
            effective_proxy_url: Some(&proxy),
            ..Default::default()
        };
        let mut events = Vec::new();
        assert!(service.test_connectivity(
            &state,
            Some("http://probe.example/generate_204"),
            connection,
            |event| events.push(event)
        ));
        assert!(!service.test_connectivity(
            &state,
            Some("http://probe.example/duplicate"),
            connection,
            |event| events.push(event)
        ));
        let (stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
        let mut stream = BufReader::new(stream);
        let mut headers = String::new();
        timeout(WAIT, async {
            while !headers.ends_with("\r\n\r\n") {
                assert_ne!(stream.read_line(&mut headers).await.unwrap(), 0);
            }
        })
        .await
        .unwrap();
        assert!(headers.starts_with("GET http://probe.example/generate_204 "));
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let message = timeout(WAIT, service.next_worker_event())
            .await
            .unwrap()
            .unwrap();
        service.handle_worker_event(&mut state, message, |event| events.push(event));
        assert!(events.iter().any(|event| matches!(
            event,
            TestEvent::ConnectivityDone {
                ok: true,
                elapsed_ms: Some(_),
                ..
            }
        )));
        assert!(!service.is_running());
    }
}
