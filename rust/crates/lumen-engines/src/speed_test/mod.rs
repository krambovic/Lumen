//! Worker for v2rayN-style speed testing through temporary xray cores.
//!
//! Port of `windows/xray_fluent/speed_test_worker.py`. The Python module is a
//! `QThread` emitting `pyqtSignal`s; here the worker is a plain thread that
//! reports the same events through an `EmitFn` callback, with the same
//! ping-then-download semantics, concurrency limits and Russian
//! user-facing strings.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, ScopedJoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use serde_json::{Value, json};
use url::Url;

use lumen_core::config::xray_fragments::apply_xray_final_fragment;
use lumen_core::constants::{
    PROXY_HOST, SPEED_TEST_DEFAULT_URL, SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT, SPEED_TEST_MAX_PING_MS,
    SPEED_TEST_MIN_BYTES_AFTER_GRACE, SPEED_TEST_MIN_MBPS_AFTER_GRACE,
    SPEED_TEST_MIXED_CONCURRENCY, SPEED_TEST_PING_TIMEOUT, SPEED_TEST_PING_URL,
    SPEED_TEST_SLOW_GRACE_SECONDS, SPEED_TEST_STARTUP_TIMEOUT, SPEED_TEST_TIMEOUT,
    singbox_path_default,
};
use lumen_core::models::{Node, RoutingSettings};
use lumen_core::http_utils::WarmHttpProbe;
use lumen_core::ping::{CancelCheck, WindowsPingBypass};
use lumen_core::probe_capabilities::{node_protocol, probe_backend};
use lumen_core::pyjson;
use lumen_core::secret_scrubber::scrub_text_depth;

#[cfg(windows)]
use std::os::windows::process::CommandExt as _;

#[cfg(windows)]
use lumen_platform::subprocess::CREATE_NO_WINDOW;

use crate::native_test_config::{build_native_test_config, py_str_or};
use crate::singbox::runtime_planner::is_falsy_json;

mod strict_proxy;
mod tunnel;

use strict_proxy::{StrictResponse, strict_proxy_get};
use tunnel::{TlsTunnel, WarmTunnel};

#[cfg(test)]
mod tests;

/// Python class attribute `tests_profile = True`.
pub const TESTS_PROFILE: bool = true;

/// Signals of the Qt worker (`result`, `ping_result`, `progress`,
/// `node_progress`, `completed`, `failure`).
#[derive(Debug, Clone, PartialEq)]
pub enum SpeedTestEvent {
    /// `result`: node_id, speed_mbps (`None` | f64), is_alive.
    NodeResult {
        node_id: String,
        speed_mbps: Option<f64>,
        is_alive: bool,
    },
    /// `ping_result`: node_id, delay_ms (`None` | i64) — ping mode.
    PingResult {
        node_id: String,
        delay_ms: Option<i64>,
    },
    /// `progress`: completed, total.
    Progress {
        completed: usize,
        total: usize,
    },
    /// `node_progress`: node_id, percent 0..100.
    NodeProgress {
        node_id: String,
        percent: u32,
    },
    Completed,
    /// `failure`: node_id, scrubbed reason.
    Failure {
        node_id: String,
        reason: String,
    },
}

/// Thread-safe replacement for the Qt signal slot; emitted from the worker
/// thread and from every per-node test thread.
pub type EmitFn = Arc<dyn Fn(SpeedTestEvent) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SpeedTestTarget {
    pub node: Node,
    pub http_port: u16,
}

struct WorkerState {
    processes: HashMap<u32, Arc<Mutex<Child>>>,
    unavailable: HashSet<String>,
}

type ResponseSlot = Arc<Mutex<Option<StrictResponse>>>;

/// An unavailable connectivity site does not mean the proxy is down: HTTP
/// GET pings fall back to this independent target.
const SPEED_TEST_FALLBACK_PING_URL: &str = "https://cp.cloudflare.com/generate_204";
type TunnelSlot = Arc<Mutex<Option<Box<dyn WarmTunnel + Send>>>>;

/// Tests nodes like v2rayN Mixedtest: ping first, then one speed download.
pub struct SpeedTestWorker {
    nodes: Vec<Node>,
    xray_path: String,
    singbox_path: String,
    timeout: f64,
    mode: String,
    warm_ping: bool,
    test_url: String,
    concurrency: i64,
    bypass_tun: bool,
    /// Python `active_node_id` / `active_proxy_url`: the connected node and
    /// its pinned health listener.
    active_node_id: String,
    active_proxy_url: String,
    cancelled: Arc<AtomicBool>,
    completed_nodes: AtomicUsize,
    bypass: Mutex<Option<Arc<WindowsPingBypass>>>,
    state: Mutex<WorkerState>,
    responses: Mutex<Vec<ResponseSlot>>,
    tunnels: Mutex<Vec<TunnelSlot>>,
}

/// Python `_resolve_speed_test_concurrency`.
pub fn resolve_speed_test_concurrency(node_count: usize, configured: i64) -> usize {
    let total = node_count.max(1);
    if configured > 0 {
        return (configured as usize).min(total);
    }
    SPEED_TEST_MIXED_CONCURRENCY.max(1).min(total)
}

impl SpeedTestWorker {
    /// Python defaults: `routing=None`, `timeout=SPEED_TEST_TIMEOUT`,
    /// `mode="speed"`, `test_url=""`, `concurrency=0`, `bypass_tun=False`,
    /// `singbox_path=""`, `warm_ping=False`.
    pub fn new(nodes: Vec<Node>, xray_path: &str) -> Self {
        Self::with_options(
            nodes,
            xray_path,
            None,
            SPEED_TEST_TIMEOUT,
            "speed",
            "",
            0,
            false,
            "",
            false,
        )
    }

    /// Full constructor mirroring the Python keyword arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        nodes: Vec<Node>,
        xray_path: &str,
        _routing: Option<RoutingSettings>,
        timeout: f64,
        mode: &str,
        test_url: &str,
        concurrency: i64,
        bypass_tun: bool,
        singbox_path: &str,
        warm_ping: bool,
    ) -> Self {
        Self {
            nodes,
            xray_path: xray_path.to_string(),
            singbox_path: if singbox_path.is_empty() {
                singbox_path_default().to_string_lossy().into_owned()
            } else {
                singbox_path.to_string()
            },
            timeout,
            mode: if mode == "ping" {
                "ping".into()
            } else {
                "speed".into()
            },
            warm_ping,
            test_url: {
                let trimmed = test_url.trim();
                if trimmed.is_empty() {
                    SPEED_TEST_DEFAULT_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            },
            concurrency,
            bypass_tun,
            active_node_id: String::new(),
            active_proxy_url: String::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            completed_nodes: AtomicUsize::new(0),
            bypass: Mutex::new(None),
            state: Mutex::new(WorkerState {
                processes: HashMap::new(),
                unavailable: HashSet::new(),
            }),
            responses: Mutex::new(Vec::new()),
            tunnels: Mutex::new(Vec::new()),
        }
    }

    /// Python `active_node_id` / `active_proxy_url` keyword arguments.
    pub fn with_active_profile(mut self, node_id: &str, proxy_url: &str) -> Self {
        self.active_node_id = node_id.to_string();
        self.active_proxy_url = proxy_url.to_string();
        self
    }

    /// Python `setObjectName("lumen-speed-test" | "lumen-real-ping")`.
    pub fn object_name(&self) -> &'static str {
        if self.mode == "speed" {
            "lumen-speed-test"
        } else {
            "lumen-real-ping"
        }
    }

    /// Python class attribute `tests_profile`.
    pub fn tests_profile(&self) -> bool {
        TESTS_PROFILE
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn completed_nodes(&self) -> usize {
        self.completed_nodes.load(Ordering::SeqCst)
    }

    pub fn was_cancelled(&self) -> bool {
        self.is_cancelled()
    }

    /// Python `unavailable_nodes` property (a frozenset).
    pub fn unavailable_nodes(&self) -> HashSet<String> {
        self.state.lock().expect("worker state").unavailable.clone()
    }

    pub fn is_unavailable(&self, node_id: &str) -> bool {
        self.state.lock().expect("worker state").unavailable.contains(node_id)
    }

    pub fn process_ids(&self) -> Vec<u32> {
        self.state
            .lock()
            .expect("worker state")
            .processes
            .keys()
            .copied()
            .collect()
    }

    #[cfg(test)]
    fn registry_is_empty(&self) -> bool {
        self.responses.lock().unwrap().is_empty() && self.tunnels.lock().unwrap().is_empty()
    }

    /// Called from the GUI thread: only signal and release, never wait —
    /// `run`'s tail does the kill escalation on the worker thread.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let responses = self.responses.lock().unwrap().clone();
        for slot in responses {
            // A reader currently holding the slot lock finishes its bounded
            // read and then observes the cancelled flag; killing the temporary
            // cores below also breaks any blocked read immediately.
            if let Ok(mut guard) = slot.try_lock() {
                guard.take();
            }
        }
        let tunnels = self.tunnels.lock().unwrap().clone();
        for slot in tunnels {
            if let Ok(mut guard) = slot.try_lock() {
                if let Some(tunnel) = guard.as_mut() {
                    tunnel.close();
                }
            }
        }
        self.terminate_all_processes(false);
    }

    fn terminate_all_processes(&self, wait: bool) {
        let processes: Vec<Arc<Mutex<Child>>> = self
            .state
            .lock()
            .expect("worker state")
            .processes
            .values()
            .cloned()
            .collect();
        for proc in &processes {
            let mut child = proc.lock().unwrap();
            if child.try_wait().ok().flatten().is_none() {
                // subprocess.Popen.terminate() is TerminateProcess on Windows.
                let _ = child.kill();
            }
        }
        if !wait {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs_f64(1.0);
        while Instant::now() < deadline
            && processes
                .iter()
                .any(|proc| proc.lock().unwrap().try_wait().ok().flatten().is_none())
        {
            thread::sleep(Duration::from_millis(50));
        }
        for proc in &processes {
            let mut child = proc.lock().unwrap();
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
        }
    }

    /// Run the whole speed test on the current thread; `emit` receives every
    /// Qt-signal equivalent event.
    pub fn run(&self, emit: EmitFn) {
        let total = self.nodes.len();
        self.completed_nodes.store(0, Ordering::SeqCst);
        for node in &self.nodes {
            emit(SpeedTestEvent::NodeProgress {
                node_id: node.id.clone(),
                percent: 0,
            });
        }

        let max_workers = resolve_speed_test_concurrency(total, self.concurrency);
        let cancelled_check: CancelCheck = {
            let flag = Arc::clone(&self.cancelled);
            Arc::new(move || flag.load(Ordering::SeqCst))
        };
        let mut bypass = WindowsPingBypass::new(self.bypass_tun, Some(cancelled_check), None);
        bypass.enter();
        let bypass = Arc::new(bypass);
        *self.bypass.lock().unwrap() = Some(Arc::clone(&bypass));

        // ThreadPoolExecutor(max_workers): one test thread per pending slot,
        // results collected on the worker thread (wait FIRST_COMPLETED with a
        // 0.2 s poll maps to recv_timeout(200 ms)).
        thread::scope(|scope| {
            let (tx, rx) = mpsc::channel::<(Node, Option<f64>, bool)>();
            let mut handles: Vec<ScopedJoinHandle<'_, ()>> = Vec::new();
            let mut active = 0usize;
            let mut next = 0usize;

            fill_pending_slots(
                self,
                &emit,
                scope,
                &tx,
                max_workers,
                total,
                &mut active,
                &mut next,
                &mut handles,
            );
            while (active > 0 || next < total) && !self.is_cancelled() {
                if active == 0 {
                    fill_pending_slots(
                        self,
                        &emit,
                        scope,
                        &tx,
                        max_workers,
                        total,
                        &mut active,
                        &mut next,
                        &mut handles,
                    );
                    if active == 0 {
                        break;
                    }
                }
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok((node, speed, alive)) => {
                        active -= 1;
                        if self.is_cancelled() {
                            break;
                        }
                        self.emit_node_result(&emit, &node, speed, alive, total);
                        fill_pending_slots(
                            self,
                            &emit,
                            scope,
                            &tx,
                            max_workers,
                            total,
                            &mut active,
                            &mut next,
                            &mut handles,
                        );
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            // executor.shutdown(wait=True, cancel_futures=True): pending tests
            // are never spawned after cancellation; running ones join here.
            for handle in handles.drain(..) {
                let _ = handle.join();
            }
        });

        bypass.exit();
        self.terminate_all_processes(true);
        emit(SpeedTestEvent::Completed);
    }

    fn spawn_test_thread<'scope, 'env>(
        &'scope self,
        emit: &'scope EmitFn,
        scope: &'scope thread::Scope<'scope, 'env>,
        node: Node,
        tx: mpsc::Sender<(Node, Option<f64>, bool)>,
    ) -> ScopedJoinHandle<'scope, ()> {
        scope.spawn(move || {
            let result = self.test_node(&node, emit);
            let _ = tx.send(result);
        })
    }

    fn emit_node_result(
        &self,
        emit: &EmitFn,
        node: &Node,
        value: Option<f64>,
        alive: bool,
        total: usize,
    ) {
        let completed = self.completed_nodes.fetch_add(1, Ordering::SeqCst) + 1;
        emit(SpeedTestEvent::NodeProgress {
            node_id: node.id.clone(),
            percent: 100,
        });
        if self.mode == "ping" {
            let delay = value
                .filter(|value| *value >= 0.0)
                .map(|value| value as i64);
            emit(SpeedTestEvent::PingResult {
                node_id: node.id.clone(),
                delay_ms: delay,
            });
        } else {
            emit(SpeedTestEvent::NodeResult {
                node_id: node.id.clone(),
                speed_mbps: value,
                is_alive: alive,
            });
        }
        emit(SpeedTestEvent::Progress { completed, total });
    }

    fn unavailable(&self, node: &Node, reason: &str, emit: &EmitFn) -> (Node, Option<f64>, bool) {
        self.state
            .lock()
            .expect("worker state")
            .unavailable
            .insert(node.id.clone());
        emit(SpeedTestEvent::Failure {
            node_id: node.id.clone(),
            reason: scrub_text_depth(reason, 0),
        });
        (node.clone(), None, false)
    }

    fn test_node(&self, node: &Node, emit: &EmitFn) -> (Node, Option<f64>, bool) {
        if self.is_cancelled() {
            return (node.clone(), None, false);
        }
        if self.mode == "ping"
            && !self.active_proxy_url.is_empty()
            && node.id == self.active_node_id
            && matches!(node_protocol(node).as_str(), "awg" | "wireguard" | "warp")
        {
            return self.active_profile_ping(node);
        }
        let backend = probe_backend(node);
        let executable = if backend == "singbox" {
            self.singbox_path.clone()
        } else {
            self.xray_path.clone()
        };
        if backend.is_empty() || !Path::new(&executable).is_file() {
            return self.unavailable(node, "Совместимое ядро для теста не найдено", emit);
        }

        let bypass = self.bypass.lock().unwrap().clone();
        if let Some(bypass) = bypass.as_ref() {
            for host in profile_hosts(node) {
                if !bypass.prepare_host(&host) {
                    return self.unavailable(
                        node,
                        "Не удалось подготовить прямой маршрут для изолированного теста",
                        emit,
                    );
                }
            }
        }

        let mut reservation: Option<TcpListener> = None;
        let mut proc: Option<Arc<Mutex<Child>>> = None;
        let mut tmp_path: Option<PathBuf> = None;
        let outcome = self.test_node_inner(
            node,
            emit,
            &executable,
            &mut reservation,
            &mut proc,
            &mut tmp_path,
        );
        let result = match outcome {
            Ok(result) => result,
            Err(reason) => self.unavailable(node, &reason, emit),
        };
        if let Some(listener) = reservation.take() {
            drop(listener);
        }
        if let Some(proc) = proc.take() {
            // Keep the PID visible as Lumen-owned until the temporary core
            // has really exited. Otherwise conflict scanning can briefly
            // report our own test process as a foreign Xray instance.
            Self::stop_process(&proc);
            self.unregister_process(&proc);
        }
        if let Some(path) = tmp_path.take() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn test_node_inner(
        &self,
        node: &Node,
        emit: &EmitFn,
        executable: &str,
        reservation: &mut Option<TcpListener>,
        proc: &mut Option<Arc<Mutex<Child>>>,
        tmp_path: &mut Option<PathBuf>,
    ) -> Result<(Node, Option<f64>, bool), String> {
        let (port, listener) = Self::reserve_port().map_err(|e| e.to_string())?;
        *reservation = Some(listener);
        let target = SpeedTestTarget {
            node: node.clone(),
            http_port: port,
        };
        let config = self.build_config(&target).map_err(|e| e.to_string())?;

        let tmp = tempfile::Builder::new()
            .prefix("xray_speed_")
            .suffix(".json")
            .rand_bytes(8)
            .tempfile()
            .map_err(|e| e.to_string())?;
        let mut tmp = tmp;
        tmp.write_all(pyjson::dumps(&config, true, None).as_bytes())
            .map_err(|e| e.to_string())?;
        tmp.flush().map_err(|e| e.to_string())?;
        // Python NamedTemporaryFile(delete=False) plus an explicit unlink in
        // the finally block.
        let (file, path) = tmp.keep().map_err(|e| e.to_string())?;
        drop(file);
        *tmp_path = Some(path.clone());

        drop(reservation.take());

        let mut command = Command::new(executable);
        command.arg("run").arg("-c").arg(&path);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let child = command.spawn().map_err(|e| e.to_string())?;
        let handle = Arc::new(Mutex::new(child));
        self.register_process(&handle);
        *proc = Some(handle.clone());

        if !self.wait_for_ready(&handle, &target, emit) {
            return Err("Временное ядро не запустилось: профиль не проверен".into());
        }

        let delay_ms = self.real_ping(&target, emit);
        if self.is_cancelled() {
            return Ok((node.clone(), None, false));
        }
        if delay_ms < 0 {
            return Ok((node.clone(), None, false));
        }
        if self.mode == "ping" {
            // Режим реальной задержки: только ping, без загрузки.
            return Ok((node.clone(), Some(delay_ms as f64), true));
        }
        emit(SpeedTestEvent::NodeProgress {
            node_id: node.id.clone(),
            percent: 35,
        });
        let speed = self.measure_speed(&target, emit);
        Ok((node.clone(), speed, speed.is_some_and(|speed| speed > 0.0)))
    }

    /// While TUN is up, point the temp xray outbound at the server's real IP
    /// (resolved directly, bypassing fake-ip) so its connection follows the
    /// temporary direct host route instead of being tunneled. TLS SNI / host
    /// live in streamSettings and are left untouched.
    fn apply_direct_ip_to_outbound(&self, outbound: &mut Value, host: &str) -> Result<()> {
        let bypass = self.bypass.lock().unwrap().clone();
        let Some(bypass) = bypass else {
            return Ok(());
        };
        let host = host.trim().to_string();
        if host.is_empty() {
            return Ok(());
        }
        let ip = bypass.direct_ip(&host);
        if ip.is_empty() || ip == host {
            return Ok(());
        }
        if let Some(stream) = outbound
            .get_mut("streamSettings")
            .filter(|stream| stream.is_object())
        {
            let security = py_str_or(stream.get("security"));
            if matches!(security.as_str(), "tls" | "reality") {
                let key = format!("{security}Settings");
                let settings = stream
                    .as_object_mut()
                    .expect("stream object")
                    .entry(key)
                    .or_insert_with(|| json!({}));
                match settings.as_object_mut() {
                    Some(settings) => {
                        settings
                            .entry("serverName")
                            .or_insert(Value::String(host.clone()));
                    }
                    None => bail!("'str' object has no attribute 'setdefault'"),
                }
            }
            let network_value = py_str_or(stream.get("network"));
            let network = if network_value.is_empty() {
                "tcp".to_string()
            } else {
                network_value
            };
            if network == "ws" {
                let ws_settings = stream
                    .as_object_mut()
                    .expect("stream object")
                    .entry("wsSettings")
                    .or_insert_with(|| json!({}));
                let headers = match ws_settings.as_object_mut() {
                    Some(ws_settings) => ws_settings.entry("headers").or_insert_with(|| json!({})),
                    None => bail!("'str' object has no attribute 'setdefault'"),
                };
                match headers.as_object_mut() {
                    Some(headers) => {
                        headers.entry("Host").or_insert(Value::String(host.clone()));
                    }
                    None => bail!("'str' object has no attribute 'setdefault'"),
                }
            } else if matches!(network.as_str(), "http" | "h2" | "xhttp" | "httpupgrade") {
                let key = if network == "h2" {
                    "httpSettings".to_string()
                } else {
                    format!("{network}Settings")
                };
                let settings = stream
                    .as_object_mut()
                    .expect("stream object")
                    .entry(key)
                    .or_insert_with(|| json!({}));
                match settings.as_object_mut() {
                    Some(settings) => {
                        settings
                            .entry("host")
                            .or_insert(Value::String(host.clone()));
                    }
                    None => bail!("'str' object has no attribute 'setdefault'"),
                }
            }
        }
        let Some(settings) = outbound
            .get_mut("settings")
            .filter(|settings| settings.is_object())
        else {
            return Ok(());
        };
        for key in ["vnext", "servers"] {
            if let Some(entries) = settings.get_mut(key).and_then(Value::as_array_mut) {
                for entry in entries.iter_mut().filter(|entry| entry.is_object()) {
                    if truthy(entry.get("address")) {
                        entry
                            .as_object_mut()
                            .expect("entry object")
                            .insert("address".into(), Value::String(ip.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    fn build_config(&self, target: &SpeedTestTarget) -> Result<Value> {
        if probe_backend(&target.node) == "singbox" {
            let mut config = build_native_test_config(&target.node, i64::from(target.http_port))?;
            self.pin_native_targets(&mut config)?;
            return Ok(config);
        }
        let inbound_tag = "speed-http";
        let outbound_tag = "speed-proxy";
        let stored_outbound = target.node.outbound.as_object();
        let full_config = stored_outbound
            .and_then(|stored| stored.get("xray_config"))
            .filter(|config| config.is_object());
        if stored_outbound
            .map(|stored| py_str_or(stored.get("protocol")).trim().to_lowercase())
            .as_deref()
            == Some("xray_config")
            && full_config.is_some()
        {
            return self.build_auto_config(
                target,
                full_config.expect("checked above"),
                inbound_tag,
            );
        }
        let mut proxy_outbound = target.node.outbound.clone();
        let Some(proxy_object) = proxy_outbound.as_object_mut() else {
            bail!("node outbound must be a JSON object to accept a tag");
        };
        proxy_object.insert("tag".into(), Value::String(outbound_tag.into()));
        self.apply_direct_ip_to_outbound(&mut proxy_outbound, &target.node.server)?;

        let mut config = json!({
            "log": {"loglevel": "none"},
            "inbounds": [
                {
                    "tag": inbound_tag,
                    "listen": PROXY_HOST,
                    "port": i64::from(target.http_port),
                    "protocol": "http",
                    "settings": {},
                    "sniffing": {
                        "enabled": true,
                        "destOverride": ["http", "tls"],
                        "routeOnly": false,
                    },
                }
            ],
            "outbounds": [
                proxy_outbound,
                {"tag": "direct", "protocol": "freedom", "settings": {}},
                {"tag": "block", "protocol": "blackhole", "settings": {}},
            ],
            "routing": {
                "domainStrategy": "AsIs",
                "rules": [
                    {
                        "type": "field",
                        "inboundTag": [inbound_tag],
                        "outboundTag": outbound_tag,
                    }
                ],
            },
        });
        // Python defaults: packets="tlshello", length="50-100", delay="10-20".
        apply_xray_final_fragment(
            &mut config,
            outbound_tag,
            "tlshello",
            "50-100",
            "10-20",
            false,
        );
        Ok(config)
    }

    /// Build a temporary HTTP inbound while preserving AUTO balancer/observer.
    fn build_auto_config(
        &self,
        target: &SpeedTestTarget,
        full_config: &Value,
        inbound_tag: &str,
    ) -> Result<Value> {
        let mut config = full_config.clone();
        if let Some(object) = config.as_object_mut() {
            object.insert("log".into(), json!({"loglevel": "none"}));
            object.insert(
                "inbounds".into(),
                json!([{
                    "tag": inbound_tag,
                    "listen": PROXY_HOST,
                    "port": i64::from(target.http_port),
                    "protocol": "http",
                    "settings": {},
                    "sniffing": {
                        "enabled": true,
                        "destOverride": ["http", "tls"],
                        "routeOnly": false,
                    },
                }]),
            );
            object.shift_remove("api");
            object.shift_remove("stats");
            object.shift_remove("policy");
        }

        if !config.get("routing").map(Value::is_object).unwrap_or(false) {
            config["routing"] = json!({});
        }
        let rules: Vec<Value> = config
            .get("routing")
            .and_then(|routing| routing.get("rules"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let balanced_rules: Vec<&Value> = rules
            .iter()
            .filter(|rule| {
                rule.is_object()
                    && truthy(rule.get("balancerTag"))
                    && !truthy(rule.get("inboundTag"))
            })
            .collect();
        // Internal loopback rescue rules precede the user-facing rules.
        // Prefer the catch-all network rule, then a regular balanced rule.
        let empty_rule = Value::Null;
        let default_rule = balanced_rules
            .iter()
            .rev()
            .find(|rule| {
                rule.as_object()
                    .map(|object| {
                        object.keys().all(|key| {
                            matches!(key.as_str(), "type" | "network" | "balancerTag" | "ruleTag")
                        })
                    })
                    .unwrap_or(false)
            })
            .copied()
            .unwrap_or_else(|| balanced_rules.first().copied().unwrap_or(&empty_rule));
        let mut balancer_tag = py_str_or(default_rule.get("balancerTag"))
            .trim()
            .to_string();
        if balancer_tag.is_empty() {
            let balancers: Vec<&Value> = config
                .get("routing")
                .and_then(|routing| routing.get("balancers"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter(|balancer| balancer.is_object())
                        .collect()
                })
                .unwrap_or_default();
            for balancer in balancers {
                let tag = py_str_or(balancer.get("tag")).trim().to_string();
                if !tag.is_empty() {
                    balancer_tag = tag;
                    break;
                }
            }
        }
        let outbounds: Vec<Value> = config
            .get("outbounds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // by_tag keeps Python dict insertion order (first tag occurrence).
        let by_tag: Vec<(String, &Value)> = {
            let mut seen: HashSet<String> = HashSet::new();
            let mut entries: Vec<(String, &Value)> = Vec::new();
            for outbound in &outbounds {
                if !outbound.is_object() {
                    continue;
                }
                let tag = py_str_or(outbound.get("tag"));
                if seen.insert(tag.clone()) {
                    entries.push((tag, outbound));
                }
            }
            entries
        };
        let target_rule: Value;
        if !balancer_tag.is_empty() {
            let balancers: HashMap<String, &Value> = config
                .get("routing")
                .and_then(|routing| routing.get("balancers"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter(|balancer| balancer.is_object())
                        .filter_map(|balancer| {
                            let tag = balancer.get("tag")?.as_str()?.to_string();
                            Some((tag, balancer))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut visiting: HashSet<String> = HashSet::new();
            check_balancer(&balancer_tag, &by_tag, &rules, &balancers, &mut visiting)?;
            target_rule = json!({"balancerTag": balancer_tag});
        } else {
            let first = outbounds.iter().find(|outbound| outbound.is_object());
            let protocol = py_str_or(first.and_then(|outbound| outbound.get("protocol")));
            let tagged = first
                .map(|outbound| truthy(outbound.get("tag")))
                .unwrap_or(false);
            if matches!(protocol.as_str(), "" | "freedom" | "blackhole" | "dns") || !tagged {
                bail!("Full profile default is not a testable proxy");
            }
            let first_tag = first
                .and_then(|outbound| outbound.get("tag"))
                .cloned()
                .expect("tag checked above");
            target_rule = json!({"outboundTag": first_tag});
        }
        let internal_tags: HashSet<Value> = by_tag
            .iter()
            .filter(|(_, outbound)| {
                outbound.get("protocol").and_then(Value::as_str) == Some("loopback")
            })
            .map(|(_, outbound)| {
                outbound
                    .get("settings")
                    .filter(|settings| settings.is_object())
                    .and_then(|settings| settings.get("inboundTag"))
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .collect();
        let internal_rules: Vec<Value> = rules
            .iter()
            .filter(|rule| {
                rule.is_object()
                    && rule
                        .get("inboundTag")
                        .and_then(Value::as_array)
                        .map(|members| members.iter().any(|member| internal_tags.contains(member)))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        let mut target_entry = json!({"type": "field", "inboundTag": [inbound_tag]});
        if let Some(entry) = target_entry.as_object_mut() {
            if let Some(rule) = target_rule.as_object() {
                for (key, value) in rule {
                    entry.insert(key.clone(), value.clone());
                }
            }
        }
        let mut new_rules = vec![target_entry];
        new_rules.extend(internal_rules);
        config["routing"]["rules"] = Value::Array(new_rules);

        if let Some(outbounds) = config.get_mut("outbounds").and_then(Value::as_array_mut) {
            for outbound in outbounds.iter_mut().filter(|outbound| outbound.is_object()) {
                let host = xray_outbound_host(outbound);
                self.apply_direct_ip_to_outbound(outbound, &host)?;
            }
        }
        Ok(config)
    }

    /// A second WireGuard client with the same key makes the peer roam away
    /// from the live connection. Its pinned health listener already verifies
    /// this exact active profile without a duplicate handshake.
    fn active_profile_ping(&self, node: &Node) -> (Node, Option<f64>, bool) {
        let measured = self.active_profile_ping_with(&mut |url, timeout| {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
            let mut probe = WarmHttpProbe::new();
            let result = runtime.block_on(probe.measure(&self.active_proxy_url, url, timeout)).ok();
            probe.close();
            result
        });
        match measured {
            Some(delay) => (node.clone(), Some(delay as f64), true),
            None => (node.clone(), None, false),
        }
    }

    fn active_profile_ping_with(&self, measure: &mut dyn FnMut(&str, f64) -> Option<u64>) -> Option<u64> {
        let timeout = self.timeout.min(SPEED_TEST_PING_TIMEOUT);
        for url in [SPEED_TEST_PING_URL, SPEED_TEST_FALLBACK_PING_URL] {
            if self.is_cancelled() {
                break;
            }
            if let Some(delay) = measure(url, timeout)
                && !self.is_cancelled()
            {
                return Some(delay);
            }
        }
        None
    }

    fn real_ping(&self, target: &SpeedTestTarget, emit: &EmitFn) -> i64 {
        // Listening locally does not mean the remote tunnel is ready. AWG can
        // need its first handshake retransmission; AUTO needs observer
        // samples. Keep this setup budget separate from the measured round
        // trip.
        let started = Instant::now();
        let clock = move || started.elapsed().as_secs_f64();
        let mut measure = |target: &SpeedTestTarget| -> i64 {
            if self.warm_ping {
                self.warm_http_ping(target, emit)
            } else {
                self.cold_http_ping(target, emit)
            }
        };
        let mut sleep = |delay: f64| thread::sleep(Duration::from_secs_f64(delay));
        self.real_ping_with(target, &clock, &mut sleep, &mut measure)
    }

    fn real_ping_with(
        &self,
        target: &SpeedTestTarget,
        clock: &dyn Fn() -> f64,
        sleep: &mut dyn FnMut(f64),
        measure: &mut dyn FnMut(&SpeedTestTarget) -> i64,
    ) -> i64 {
        let needs_setup =
            node_protocol(&target.node) == "xray_config" || probe_backend(&target.node) == "singbox";
        let deadline = clock() + if needs_setup { 12.0 } else { 0.0 };
        while !self.is_cancelled() {
            let measured = measure(target);
            if measured >= 0 || clock() >= deadline {
                return measured;
            }
            sleep(0.2);
        }
        -1
    }

    fn cold_http_ping(&self, target: &SpeedTestTarget, emit: &EmitFn) -> i64 {
        if self.is_cancelled() {
            return -1;
        }
        let proxy_url = format!("http://{PROXY_HOST}:{}", target.http_port);
        let timeout = self.timeout.min(SPEED_TEST_PING_TIMEOUT);
        emit(SpeedTestEvent::NodeProgress {
            node_id: target.node.id.clone(),
            percent: 20,
        });
        let started = Instant::now();
        let outcome = (|| -> Result<i64, ()> {
            let response = strict_proxy_get(
                &proxy_url,
                timeout,
                SPEED_TEST_PING_URL,
                &[("User-Agent", "Lumen/SpeedTest")],
            )
            .map_err(|_| ())?;
            let slot = self.register_response(response);
            let outcome = (|| -> Result<i64, ()> {
                let mut buffer = [0u8; 16];
                {
                    let mut guard = slot.lock().unwrap();
                    let response = guard.as_mut().ok_or(())?;
                    let mut read = 0usize;
                    while read < buffer.len() {
                        match response.read(&mut buffer[read..]) {
                            Ok(0) => break,
                            Ok(count) => read += count,
                            Err(_) => return Err(()),
                        }
                    }
                }
                let elapsed_ms = py_round((Instant::now() - started).as_secs_f64() * 1000.0).max(1);
                if elapsed_ms > i64::from(SPEED_TEST_MAX_PING_MS) {
                    return Err(());
                }
                emit(SpeedTestEvent::NodeProgress {
                    node_id: target.node.id.clone(),
                    percent: 30,
                });
                Ok(elapsed_ms)
            })();
            self.unregister_response(&slot);
            outcome
        })();
        outcome.unwrap_or(-1)
    }

    /// Measure GET round trips on one TLS tunnel per target, trying the
    /// independent fallback target when the primary site is unavailable.
    fn warm_http_ping(&self, target: &SpeedTestTarget, emit: &EmitFn) -> i64 {
        let started = Instant::now();
        let clock = move || started.elapsed().as_secs_f64();
        let mut connector = move |proxy_host: &str,
                                  proxy_port: u16,
                                  host: &str,
                                  port: u16,
                                  timeout: f64|
              -> Box<dyn WarmTunnel + Send> {
            Box::new(TlsTunnel::new(proxy_host, proxy_port, host, port, timeout))
        };
        let mut measure = |url: &str| self.http_get_ping_with(target, emit, url, &clock, &mut connector);
        self.warm_http_ping_over(&mut measure)
    }

    fn warm_http_ping_over(&self, measure: &mut dyn FnMut(&str) -> i64) -> i64 {
        for url in [SPEED_TEST_PING_URL, SPEED_TEST_FALLBACK_PING_URL] {
            if self.is_cancelled() {
                break;
            }
            let measured = measure(url);
            if measured > 0 {
                return measured;
            }
        }
        -1
    }

    /// Time GET responses; establish any new TLS tunnel before timing.
    fn http_get_ping_with(
        &self,
        target: &SpeedTestTarget,
        emit: &EmitFn,
        target_url: &str,
        clock: &dyn Fn() -> f64,
        connector: &mut dyn FnMut(&str, u16, &str, u16, f64) -> Box<dyn WarmTunnel + Send>,
    ) -> i64 {
        let url = match Url::parse(target_url) {
            Ok(url) => url,
            Err(_) => return -1,
        };
        if url.scheme() != "https" || url.host_str().is_none() {
            return -1;
        }
        let target_host = url.host_str().unwrap_or("").to_string();
        let target_port = url.port().unwrap_or(443);
        let path = match url.query() {
            Some(query) => {
                format!(
                    "{}?{}",
                    if url.path().is_empty() {
                        "/"
                    } else {
                        url.path()
                    },
                    query
                )
            }
            None if url.path().is_empty() => "/".to_string(),
            None => url.path().to_string(),
        };
        let tunnel_slot: TunnelSlot = Arc::new(Mutex::new(Some(connector(
            PROXY_HOST,
            target.http_port,
            &target_host,
            target_port,
            self.timeout.min(SPEED_TEST_PING_TIMEOUT),
        ))));
        self.register_tunnel(&tunnel_slot);
        let outcome = (|| -> Result<i64, ()> {
            emit(SpeedTestEvent::NodeProgress {
                node_id: target.node.id.clone(),
                percent: 20,
            });
            {
                let mut guard = tunnel_slot.lock().unwrap();
                let tunnel = guard.as_mut().ok_or(())?;
                tunnel.connect().map_err(|_| ())?; // CONNECT and TLS are outside the measurement.
            }
            let mut samples: Vec<f64> = Vec::new();
            for attempt in 0..3usize {
                if self.is_cancelled() {
                    return Err(());
                }
                {
                    let mut guard = tunnel_slot.lock().unwrap();
                    let tunnel = guard.as_mut().ok_or(())?;
                    if tunnel.sock_closed() {
                        // Connection: close is a valid HTTP response.
                        // Reconnect outside the timer instead of reporting
                        // the node dead.
                        tunnel.connect().map_err(|_| ())?;
                    }
                }
                let started = clock();
                let response = {
                    let mut guard = tunnel_slot.lock().unwrap();
                    let tunnel = guard.as_mut().ok_or(())?;
                    tunnel.get(&path).map_err(|_| ())?
                };
                if !(200..300).contains(&response.status) || !response.fully_consumed {
                    return Err(());
                }
                if attempt > 0 {
                    // First GET warms the proxy transport and target.
                    samples.push((clock() - started) * 1000.0);
                }
            }
            if self.is_cancelled() {
                return Err(());
            }
            let mut elapsed = f64::INFINITY;
            for sample in &samples {
                elapsed = elapsed.min(*sample);
            }
            if !elapsed.is_finite() {
                return Err(());
            }
            if elapsed > f64::from(SPEED_TEST_MAX_PING_MS) {
                return Err(());
            }
            emit(SpeedTestEvent::NodeProgress {
                node_id: target.node.id.clone(),
                percent: 30,
            });
            Ok(1i64.max(py_round(elapsed)))
        })();
        self.unregister_tunnel(&tunnel_slot);
        if let Ok(mut guard) = tunnel_slot.try_lock() {
            if let Some(tunnel) = guard.as_mut() {
                tunnel.close();
            }
        }
        outcome.unwrap_or(-1)
    }

    fn measure_speed(&self, target: &SpeedTestTarget, emit: &EmitFn) -> Option<f64> {
        let proxy_url = format!("http://{PROXY_HOST}:{}", target.http_port);
        let timeout = self.timeout.min(SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT);
        let started = Instant::now();
        let outcome = (|| -> Result<MeasureTotals, ()> {
            let response = strict_proxy_get(
                &proxy_url,
                timeout,
                &self.test_url,
                &[("User-Agent", "Lumen/SpeedTest")],
            )
            .map_err(|_| ())?;
            let slot = self.register_response(response);
            let outcome = (|| -> Result<MeasureTotals, ()> {
                let mut totals = MeasureTotals::new(started);
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    if self.is_cancelled() {
                        break;
                    }
                    let count = {
                        let mut guard = slot.lock().unwrap();
                        let response = guard.as_mut().ok_or(())?;
                        let mut read = 0usize;
                        while read < buffer.len() {
                            match response.read(&mut buffer[read..]) {
                                Ok(0) => break,
                                Ok(count) => read += count,
                                Err(_) => return Err(()),
                            }
                        }
                        read
                    };
                    let now = Instant::now();
                    if count == 0 {
                        break;
                    }
                    totals.total_bytes += count as u64;
                    totals.window_bytes += count as u64;
                    let elapsed = (now - started).as_secs_f64();
                    if elapsed >= self.timeout {
                        break;
                    }
                    if elapsed >= SPEED_TEST_SLOW_GRACE_SECONDS
                        && totals.total_bytes < u64::from(SPEED_TEST_MIN_BYTES_AFTER_GRACE)
                    {
                        return Err(());
                    }
                    let window_elapsed = (now - totals.last_update).as_secs_f64();
                    if window_elapsed >= 1.0 {
                        let speed = (totals.window_bytes as f64 / (1000.0 * 1000.0))
                            / window_elapsed.max(0.001);
                        totals.max_speed = totals.max_speed.max(speed);
                        if elapsed >= SPEED_TEST_SLOW_GRACE_SECONDS
                            && totals.max_speed < SPEED_TEST_MIN_MBPS_AFTER_GRACE
                        {
                            return Err(());
                        }
                        totals.window_bytes = 0;
                        totals.last_update = now;
                        let percent =
                            35i64 + (60.0 * 1.0f64.min(elapsed / self.timeout.max(0.1))) as i64;
                        emit(SpeedTestEvent::NodeProgress {
                            node_id: target.node.id.clone(),
                            percent: percent.clamp(35, 95) as u32,
                        });
                    }
                }
                Ok(totals)
            })();
            self.unregister_response(&slot);
            outcome
        })();
        let Ok(totals) = outcome else {
            return None;
        };
        let elapsed_total = (Instant::now() - started).as_secs_f64();
        let mut max_speed = totals.max_speed;
        if totals.window_bytes > 0 {
            let tail = (Instant::now() - totals.last_update).as_secs_f64();
            let speed = (totals.window_bytes as f64 / (1000.0 * 1000.0)) / tail.max(0.001);
            max_speed = max_speed.max(speed);
        }
        if totals.total_bytes == 0 || elapsed_total <= 0.0 {
            return None;
        }
        if max_speed <= 0.0 {
            max_speed = (totals.total_bytes as f64 / (1000.0 * 1000.0)) / elapsed_total;
        }
        emit(SpeedTestEvent::NodeProgress {
            node_id: target.node.id.clone(),
            percent: 95,
        });
        Some(py_round_1(max_speed))
    }

    fn wait_for_ready(
        &self,
        proc: &Arc<Mutex<Child>>,
        target: &SpeedTestTarget,
        emit: &EmitFn,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs_f64(SPEED_TEST_STARTUP_TIMEOUT);
        while Instant::now() < deadline {
            if self.is_cancelled() {
                return false;
            }
            if proc.lock().unwrap().try_wait().ok().flatten().is_some() {
                return false;
            }
            if Self::is_port_ready(target.http_port) {
                emit(SpeedTestEvent::NodeProgress {
                    node_id: target.node.id.clone(),
                    percent: 10,
                });
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn register_process(&self, proc: &Arc<Mutex<Child>>) {
        let id = proc.lock().unwrap().id();
        self.state
            .lock()
            .expect("worker state")
            .processes
            .insert(id, Arc::clone(proc));
    }

    fn unregister_process(&self, proc: &Arc<Mutex<Child>>) {
        let id = proc.lock().unwrap().id();
        self.state
            .lock()
            .expect("worker state")
            .processes
            .remove(&id);
    }

    fn register_response(&self, response: StrictResponse) -> ResponseSlot {
        let slot = Arc::new(Mutex::new(Some(response)));
        self.responses.lock().unwrap().push(Arc::clone(&slot));
        slot
    }

    fn unregister_response(&self, slot: &ResponseSlot) {
        self.responses
            .lock()
            .unwrap()
            .retain(|item| !Arc::ptr_eq(item, slot));
    }

    fn register_tunnel(&self, slot: &TunnelSlot) {
        self.tunnels.lock().unwrap().push(Arc::clone(slot));
    }

    fn unregister_tunnel(&self, slot: &TunnelSlot) {
        self.tunnels
            .lock()
            .unwrap()
            .retain(|item| !Arc::ptr_eq(item, slot));
    }

    fn stop_process(proc: &Arc<Mutex<Child>>) {
        let mut child = proc.lock().unwrap();
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs_f64(3.0);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs_f64(1.0);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn reserve_port() -> Result<(u16, TcpListener)> {
        let listener = TcpListener::bind((PROXY_HOST, 0))?;
        let port = listener.local_addr()?.port();
        Ok((port, listener))
    }

    fn is_port_ready(port: u16) -> bool {
        match format!("{PROXY_HOST}:{port}").parse::<std::net::SocketAddr>() {
            Ok(addr) => TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok(),
            Err(_) => false,
        }
    }

    /// Python `_pin_native_targets`: rewrite every native server address to
    /// its directly resolved IP while keeping SNI/Host headers on the hostname.
    fn pin_native_targets(&self, config: &mut Value) -> Result<()> {
        let bypass = self.bypass.lock().unwrap().clone();
        let Some(bypass) = bypass else {
            return Ok(());
        };
        for collection in ["outbounds", "endpoints"] {
            let Some(items) = config.get_mut(collection).and_then(Value::as_array_mut) else {
                continue;
            };
            for item in items.iter_mut().filter(|item| item.is_object()) {
                let host = py_str_or(item.get("server"));
                if !host.is_empty() {
                    let ip = bypass.direct_ip(&host);
                    if !ip.is_empty() && ip != host {
                        if let Some(tls) = item.get_mut("tls").filter(|tls| tls.is_object()) {
                            if truthy(tls.get("enabled")) {
                                let tls_object = tls.as_object_mut().expect("tls object");
                                tls_object
                                    .entry("server_name")
                                    .or_insert(Value::String(host.clone()));
                            }
                        }
                        if let Some(transport) = item
                            .get_mut("transport")
                            .filter(|transport| transport.is_object())
                        {
                            let transport_type = py_str_or(transport.get("type"));
                            if transport_type == "ws" {
                                let headers = transport
                                    .as_object_mut()
                                    .expect("transport object")
                                    .entry("headers")
                                    .or_insert_with(|| json!({}));
                                match headers.as_object_mut() {
                                    Some(headers) => {
                                        headers
                                            .entry("Host")
                                            .or_insert(Value::String(host.clone()));
                                    }
                                    None => bail!("'str' object has no attribute 'setdefault'"),
                                }
                            } else if matches!(
                                transport_type.as_str(),
                                "http" | "httpupgrade" | "xhttp"
                            ) {
                                match transport.as_object_mut() {
                                    Some(transport) => {
                                        transport
                                            .entry("host")
                                            .or_insert(Value::String(host.clone()));
                                    }
                                    None => bail!("'str' object has no attribute 'setdefault'"),
                                }
                            }
                        }
                        item.as_object_mut()
                            .expect("item object")
                            .insert("server".into(), Value::String(ip));
                    }
                }
                let Some(peers) = item.get_mut("peers").and_then(Value::as_array_mut) else {
                    continue;
                };
                for peer in peers.iter_mut().filter(|peer| peer.is_object()) {
                    let mut peer_host = py_str_or(peer.get("address"));
                    if peer_host.is_empty() {
                        peer_host = py_str_or(peer.get("server"));
                    }
                    if peer_host.is_empty() {
                        continue;
                    }
                    let ip = bypass.direct_ip(&peer_host);
                    if !ip.is_empty() {
                        let key = if peer.get("address").is_some() {
                            "address"
                        } else {
                            "server"
                        };
                        peer.as_object_mut()
                            .expect("peer object")
                            .insert(key.into(), Value::String(ip));
                    }
                }
            }
        }
        Ok(())
    }
}

struct MeasureTotals {
    total_bytes: u64,
    window_bytes: u64,
    max_speed: f64,
    last_update: Instant,
}

impl MeasureTotals {
    fn new(started: Instant) -> Self {
        Self {
            total_bytes: 0,
            window_bytes: 0,
            max_speed: 0.0,
            last_update: started,
        }
    }
}

impl Drop for SpeedTestWorker {
    fn drop(&mut self) {
        if let Some(bypass) = self.bypass.lock().unwrap().as_ref() {
            bypass.exit();
        }
    }
}

/// Python `SpeedTestWorker.start()` on a QThread: run on a dedicated thread,
/// returning a handle for cancellation and progress queries.
pub fn start_worker(worker: SpeedTestWorker, emit: EmitFn) -> SpeedTestWorkerHandle {
    let worker = Arc::new(worker);
    let run_worker = Arc::clone(&worker);
    let join = thread::spawn(move || run_worker.run(emit));
    SpeedTestWorkerHandle {
        worker,
        join: Some(join),
    }
}

pub struct SpeedTestWorkerHandle {
    worker: Arc<SpeedTestWorker>,
    join: Option<thread::JoinHandle<()>>,
}

impl SpeedTestWorkerHandle {
    pub fn cancel(&self) {
        self.worker.cancel();
    }

    pub fn completed_nodes(&self) -> usize {
        self.worker.completed_nodes()
    }

    pub fn was_cancelled(&self) -> bool {
        self.worker.was_cancelled()
    }

    pub fn unavailable_nodes(&self) -> HashSet<String> {
        self.worker.unavailable_nodes()
    }

    pub fn is_running(&self) -> bool {
        self.join
            .as_ref()
            .map(|join| !join.is_finished())
            .unwrap_or(false)
    }

    pub fn join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Python `_profile_hosts`: every server/address host found in the outbound
/// payload, plus `node.server` for non-full-profile nodes; sorted.
pub(crate) fn profile_hosts(node: &Node) -> Vec<String> {
    let mut hosts: BTreeSet<String> = BTreeSet::new();
    fn walk(value: &Value, hosts: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for key in ["server", "address"] {
                    if let Some(Value::String(host)) = map.get(key) {
                        if !host.is_empty() && !host.contains('/') {
                            hosts.insert(host.clone());
                        }
                    }
                }
                for child in map.values() {
                    walk(child, hosts);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, hosts);
                }
            }
            _ => {}
        }
    }
    walk(&node.outbound, &mut hosts);
    let protocol = node_protocol(node);
    if !node.server.is_empty() && protocol != "xray_config" && protocol != "singbox_config" {
        hosts.insert(node.server.clone());
    }
    hosts.into_iter().collect()
}

fn truthy(value: Option<&Value>) -> bool {
    value
        .map(is_falsy_json)
        .map(|falsy| !falsy)
        .unwrap_or(false)
}

/// Python `round(value)`: half-to-even on the integer scale.
fn py_round(value: f64) -> i64 {
    py_round_scaled(value) as i64
}

/// Python `round(value, 1)`.
fn py_round_1(value: f64) -> f64 {
    py_round_scaled(value * 10.0) / 10.0
}

fn py_round_scaled(scaled: f64) -> f64 {
    if !scaled.is_finite() {
        return scaled;
    }
    let floor = scaled.floor();
    let frac = scaled - floor;
    if frac > 0.5 {
        floor + 1.0
    } else if frac < 0.5 {
        floor
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    }
}

/// Python `_xray_outbound_host`.
fn xray_outbound_host(outbound: &Value) -> String {
    let settings = outbound
        .get("settings")
        .filter(|settings| settings.is_object());
    for key in ["vnext", "servers"] {
        if let Some(entries) = settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_array)
        {
            if let Some(first) = entries.first().filter(|entry| entry.is_object()) {
                let mut host = py_str_or(first.get("address"));
                if host.is_empty() {
                    host = py_str_or(first.get("server"));
                }
                return host.trim().to_string();
            }
        }
    }
    let mut host = py_str_or(settings.and_then(|settings| settings.get("address")));
    if host.is_empty() {
        host = py_str_or(settings.and_then(|settings| settings.get("server")));
    }
    if host.is_empty() {
        host = py_str_or(outbound.get("address"));
    }
    host.trim().to_string()
}

/// Python `fill_pending_slots` of `run`.
#[allow(clippy::too_many_arguments)]
fn fill_pending_slots<'scope, 'env>(
    worker: &'scope SpeedTestWorker,
    emit: &'scope EmitFn,
    scope: &'scope thread::Scope<'scope, 'env>,
    tx: &mpsc::Sender<(Node, Option<f64>, bool)>,
    max_workers: usize,
    total: usize,
    active: &mut usize,
    next: &mut usize,
    handles: &mut Vec<ScopedJoinHandle<'scope, ()>>,
) {
    while *active < max_workers && *next < total && !worker.is_cancelled() {
        let node = worker.nodes[*next].clone();
        *next += 1;
        *active += 1;
        handles.push(worker.spawn_test_thread(emit, scope, node, tx.clone()));
    }
}

/// Python `check_outbound` of `_build_auto_config`.
fn check_outbound(
    tag: &str,
    by_tag: &[(String, &Value)],
    rules: &[Value],
    balancers: &HashMap<String, &Value>,
    visiting: &mut HashSet<String>,
    fallback: bool,
) -> Result<()> {
    let outbound = by_tag
        .iter()
        .find(|(candidate, _)| candidate == tag)
        .map(|(_, item)| *item);
    let protocol = py_str_or(outbound.and_then(|item| item.get("protocol")));
    if protocol == "blackhole" && fallback {
        return Ok(()); // A blocked fallback fails the probe, never bypasses it.
    }
    if protocol == "loopback" {
        let inbound = outbound
            .and_then(|item| item.get("settings"))
            .filter(|settings| settings.is_object())
            .and_then(|settings| settings.get("inboundTag"));
        let route = rules.iter().find(|rule| {
            rule.is_object()
                && inbound
                    .map(|inbound| {
                        rule.get("inboundTag")
                            .and_then(Value::as_array)
                            .map(|members| members.contains(inbound))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
        });
        if let Some(route) = route {
            if truthy(route.get("balancerTag")) {
                check_balancer(
                    &py_str_or(route.get("balancerTag")),
                    by_tag,
                    rules,
                    balancers,
                    visiting,
                )?;
                return Ok(());
            }
        }
    } else if !matches!(protocol.as_str(), "" | "freedom" | "blackhole" | "dns") {
        return Ok(());
    }
    bail!("AUTO test cannot bypass the profile through direct/unknown members")
}

/// Python `check_balancer` of `_build_auto_config`.
fn check_balancer(
    tag: &str,
    by_tag: &[(String, &Value)],
    rules: &[Value],
    balancers: &HashMap<String, &Value>,
    visiting: &mut HashSet<String>,
) -> Result<()> {
    if visiting.contains(tag) || !balancers.contains_key(tag) {
        bail!("Missing or cyclic AUTO balancer");
    }
    visiting.insert(tag.to_string());
    let balancer = balancers[tag];
    let selectors: Vec<String> = balancer
        .get("selector")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|item| py_str_or(Some(item))).collect())
        .unwrap_or_default();
    let candidates: Vec<String> = by_tag
        .iter()
        .filter(|(candidate, _)| {
            selectors
                .iter()
                .any(|prefix| candidate.starts_with(prefix.as_str()))
        })
        .map(|(candidate, _)| candidate.clone())
        .collect();
    if candidates.is_empty() {
        bail!("AUTO balancer has no proxy members");
    }
    for candidate in &candidates {
        check_outbound(candidate, by_tag, rules, balancers, visiting, false)?;
    }
    let fallback = py_str_or(balancer.get("fallbackTag"));
    if !fallback.is_empty() {
        check_outbound(&fallback, by_tag, rules, balancers, visiting, true)?;
    }
    visiting.remove(tag);
    Ok(())
}
