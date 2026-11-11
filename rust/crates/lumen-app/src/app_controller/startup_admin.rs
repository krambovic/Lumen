//! Deferred startup services, startup/admin reconciliation and the Windows
//! preferences writer queue (`AppController.start_deferred_services` and the
//! `startup*`/`windows preferences` method group).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};


use lumen_core::constants::APP_NAME;
use lumen_engines::discord_proxy_manager::migrate_legacy_droute_markers;
use lumen_engines::singbox::manager::get_singbox_version;
use lumen_engines::xray::manager::get_xray_version;
use lumen_platform::network_monitor::NetworkMonitor;
use lumen_platform::startup::{
    STARTUP_STATE_DISABLED, build_startup_command, get_startup_state, set_always_run_as_admin,
    set_startup_enabled,
};

use crate::app_controller::{AppController, AppEvent, DelayedAction};

/// Python `_background_threads` set: named one-shot jobs joined at shutdown.
#[derive(Default)]
pub(crate) struct BackgroundTasks {
    threads: Vec<(&'static str, JoinHandle<()>)>,
}

impl BackgroundTasks {
    pub(crate) fn push(&mut self, name: &'static str, thread: JoinHandle<()>) {
        self.threads.push((name, thread));
    }

    pub fn reap_finished(&mut self) {
        self.threads.retain(|(_, thread)| !thread.is_finished());
    }

    pub fn has_pending(&mut self) -> bool {
        self.reap_finished();
        !self.threads.is_empty()
    }

    /// Python `_join_background_tasks(timeout)`: bounded waits with one
    /// shared deadline; threads that miss it keep running detached.
    pub fn join_until(&mut self, deadline: Instant) {
        self.reap_finished();
        for (_, thread) in self.threads.drain(..) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || !wait_thread(thread, remaining) {
                // Abandoned after the deadline, like the Python join timeout.
                continue;
            }
        }
    }
}

fn wait_thread(thread: JoinHandle<()>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !thread.is_finished() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = thread.join();
    true
}

/// Pending Windows preference writes plus the single-writer flag
/// (`_windows_pref_pending` / `_windows_pref_running`).
#[derive(Clone, Default)]
pub(crate) struct WindowsPrefsQueue {
    inner: Arc<parking_lot::Mutex<PrefsInner>>,
}

#[derive(Default)]
struct PrefsInner {
    pending: Option<PrefsJob>,
    running: bool,
}

#[derive(Clone, Copy)]
struct PrefsJob {
    launch: bool,
    tray: bool,
    admin_enabled: bool,
    write_startup: bool,
    write_admin: bool,
}

impl WindowsPrefsQueue {
    /// Returns whether a new writer thread must start.
    pub fn submit(
        &self,
        launch: bool,
        tray: bool,
        admin_enabled: bool,
        write_startup: bool,
        write_admin: bool,
    ) -> bool {
        let mut inner = self.inner.lock();
        let mut job = PrefsJob {
            launch,
            tray,
            admin_enabled,
            write_startup,
            write_admin,
        };
        if let Some(pending) = inner.pending.as_mut() {
            // Python merges: `startup or pending[3], admin or pending[4]`.
            job.write_startup |= pending.write_startup;
            job.write_admin |= pending.write_admin;
        }
        inner.pending = Some(job);
        if inner.running {
            return false;
        }
        inner.running = true;
        true
    }

    fn take(&self) -> Option<PrefsJob> {
        let mut inner = self.inner.lock();
        let job = inner.pending.take();
        if job.is_none() {
            inner.running = false;
        }
        job
    }
}

impl AppController {
    /// Python `start_deferred_services`: non-visual work after the first
    /// frame.
    pub fn start_deferred_services(&mut self) {
        if !self.profile_loaded() || self.deferred_services_started {
            return;
        }
        self.deferred_services_started = true;

        let tasks = self.tasks.0.clone();
        let monitor = NetworkMonitor::new(5000, Box::new(move |old, new| {
            let old = old.to_owned();
            let new = new.to_owned();
            let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                controller.on_network_changed(&old, &new);
            }));
        }));
        self.network_monitor = Some(monitor);
        self.network_monitor
            .as_mut()
            .expect("monitor just set")
            .start();
        self.timers.lock_check = Some(Instant::now() + Duration::from_millis(15_000));

        // A large subscription can contain thousands of unnamed countries.
        // Keep each pass short so the first window stays responsive even when
        // another process has saturated the CPU during boot.
        self.timers.country_detect.node_ids = self
            .state
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect();
        self.timers.country_detect.cursor = 0;
        self.timers.country_detect.changed = false;
        self.schedule_delayed(Duration::from_millis(10), DelayedAction::CountryDetectBatch);

        self.spawn_background_task("core-version-probe", probe_core_versions_job);
        self.spawn_background_task("connection-context-prewarm", prewarm_connection_context_job);
        self.spawn_background_task("droute-marker-migration", |controller| {
            controller.spawn_io("droute-marker-migration", |_| {
                migrate_legacy_droute_markers()
            });
        });

        if self.state.settings.always_run_as_admin {
            self.spawn_background_task("admin-startup", configure_admin_startup_job);
        }
        self.spawn_background_task("startup-reconcile", reconcile_startup_registration_job);
        self.timers.startup_sync = Some(Instant::now() + Duration::from_millis(120_000));
        // Restrictions are explained by the relevant controls when clicked.
        // A delayed global warning here could surface during an unrelated
        // server switch and incorrectly make that switch look rejected.
    }

    /// Python `_start_background_task`: run `job` on the owner task; the job
    /// usually forwards its blocking work to `spawn_io`.
    pub(crate) fn spawn_background_task(
        &mut self,
        name: &'static str,
        job: impl FnOnce(&mut AppController) + Send + 'static,
    ) {
        let _ = name;
        let _ = self.tasks.0.send(Box::new(job));
    }
}

// ── background job bodies ───────────────────────────────────────────────────

/// Python `_probe_core_versions` (OS probing on a thread, reporting on the
/// owner task).
fn probe_core_versions_job(controller: &mut AppController) {
    let xray_path = controller.state.settings.xray_path.clone();
    let singbox_path = controller.state.settings.singbox_path.clone();
    controller.spawn_io("core-version-probe", move |tasks| {
        let xray_version = get_xray_version(&xray_path);
        let singbox_version = get_singbox_version(&singbox_path);
        let _ = tasks.send(Box::new(move |controller: &mut AppController| {
            if let Some(version) = xray_version {
                controller.log(&format!("[core] {version}"));
            } else {
                controller.emit_status("warning", "Не удалось прочитать версию Xray");
            }
            if let Some(sb_version) = singbox_version {
                controller.log(&format!("[core] sing-box: {sb_version}"));
            }
        }));
    });
}

/// Python `_prewarm_connection_context` (global caches; no controller state).
fn prewarm_connection_context_job(controller: &mut AppController) {
    let server = controller.selected_node().map(|node| node.server);
    controller.spawn_io("connection-context-prewarm", move |_| {
        let _ = lumen_platform::network_route_context::get_windows_default_route_context(false);
        if let Some(server) = server {
            lumen_engines::singbox::runtime_planner::prime_endpoint_resolution(&server);
        }
    });
}

/// Python `_configure_admin_startup`.
fn configure_admin_startup_job(controller: &mut AppController) {
    let generation = controller.startup_settings_generation();
    let generation_arc = Arc::clone(&controller.startup_generation);
    let shutting_down = Arc::clone(&controller.shutting_down);
    let lock = Arc::clone(&controller.startup_io_lock);
    controller.spawn_io("admin-startup", move |tasks| {
        let _guard = lock.lock();
        if shutting_down.load(Ordering::Acquire) || generation != generation_arc.load(Ordering::SeqCst)
        {
            return;
        }
        if let Err(error) = set_always_run_as_admin(true, None) {
            let message = lumen_core::secret_scrubber::scrub_text_depth(&error.to_string(), 0);
            let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                controller.emit_status(
                    "error",
                    &format!("Ошибка настройки запуска от администратора: {message}"),
                );
            }));
        }
    });
}

/// Python `_reconcile_startup_registration`.
fn reconcile_startup_registration_job(controller: &mut AppController) {
    let generation = controller.startup_settings_generation();
    let generation_arc = Arc::clone(&controller.startup_generation);
    let shutting_down = Arc::clone(&controller.shutting_down);
    let lock = Arc::clone(&controller.startup_io_lock);
    let settings = controller.state.settings.clone();
    controller.spawn_io("startup-reconcile", move |tasks| {
        let launch = settings.launch_on_startup;
        let tray = settings.launch_in_tray_on_startup;
        let outcome = (|| -> Result<Option<u64>, anyhow::Error> {
            let _guard = lock.lock();
            if shutting_down.load(Ordering::Acquire)
                || generation != generation_arc.load(Ordering::SeqCst)
            {
                return Ok(None);
            }
            let external = get_startup_state(APP_NAME);
            if generation != generation_arc.load(Ordering::SeqCst) {
                return Ok(None);
            }
            if launch && matches!(external, Ok(STARTUP_STATE_DISABLED)) {
                return Ok(Some(generation));
            }
            set_startup_enabled(APP_NAME, launch, &build_startup_command(tray))?;
            Ok(None)
        })();
        match outcome {
            Ok(Some(generation)) => {
                let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                    controller.accept_external_startup_disable(generation);
                }));
            }
            Ok(None) => {}
            Err(error) => {
                let message =
                    lumen_core::secret_scrubber::scrub_text_depth(&error.to_string(), 0);
                let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                    controller.log_raw(&format!(
                        "Failed to reconcile startup registration: {message}"
                    ));
                }));
            }
        }
    });
}

/// Python `_drain_windows_preferences`: one writer, latest pending snapshot;
/// the GUI never waits on OS IO.
fn drain_windows_preferences_job(controller: &mut AppController) {
    let queue = controller.windows_prefs.clone();
    let lock = Arc::clone(&controller.startup_io_lock);
    controller.spawn_io("windows-preferences", move |tasks| loop {
        let Some(job) = queue.take() else {
            return;
        };
        {
            let _guard = lock.lock();
            if job.write_startup {
                if let Err(error) =
                    set_startup_enabled(APP_NAME, job.launch, &build_startup_command(job.tray))
                {
                    let message =
                        lumen_core::secret_scrubber::scrub_text_depth(&error.to_string(), 0);
                    let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                        controller.emit_status(
                            "error",
                            &format!("Ошибка настройки автозапуска: {message}"),
                        );
                    }));
                }
            }
            if job.write_admin {
                let result = set_always_run_as_admin(job.admin_enabled, None);
                let elevated = lumen_platform::startup::is_process_elevated();
                let error = result
                    .err()
                    .map(|error| lumen_core::secret_scrubber::scrub_text_depth(&error.to_string(), 0));
                let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                    controller.on_admin_setting_applied(
                        job.admin_enabled,
                        elevated,
                        error.as_deref().unwrap_or(""),
                    );
                }));
            }
        }
    });
}

/// Python `_run_startup_state_sync`: read-only periodic observation.
fn startup_state_sync_job(controller: &mut AppController) {
    let generation = controller.startup_settings_generation();
    let generation_arc = Arc::clone(&controller.startup_generation);
    let lock = Arc::clone(&controller.startup_io_lock);
    let launch = controller.state.settings.launch_on_startup;
    controller.spawn_io("startup-state-sync", move |tasks| {
        let outcome = (|| -> Result<bool, anyhow::Error> {
            let external = {
                let _guard = lock.lock();
                get_startup_state(APP_NAME)
            };
            if generation != generation_arc.load(Ordering::SeqCst) {
                return Ok(false);
            }
            if launch && matches!(external, Ok(STARTUP_STATE_DISABLED)) {
                return Ok(true);
            }
            Ok(false)
        })();
        let _ = match outcome {
            Ok(disabled) => tasks.send(Box::new(move |controller: &mut AppController| {
                if disabled {
                    controller.accept_external_startup_disable(generation);
                }
                controller.startup_sync_active = false;
            })),
            Err(error) => {
                let message =
                    lumen_core::secret_scrubber::scrub_text_depth(&error.to_string(), 0);
                tasks.send(Box::new(move |controller: &mut AppController| {
                    controller.log_raw(&format!(
                        "Failed to sync startup state from Windows: {message}"
                    ));
                    controller.startup_sync_active = false;
                }))
            }
        };
    });
}

impl AppController {
    /// Python `_queue_windows_preferences`.
    pub(crate) fn queue_windows_preferences(&mut self, startup: bool, admin: bool) {
        if self.shutting_down() {
            return;
        }
        let settings = self.state.settings.clone();
        let should_start = self.windows_prefs.submit(
            settings.launch_on_startup,
            settings.launch_in_tray_on_startup,
            settings.always_run_as_admin,
            startup,
            admin,
        );
        if should_start {
            self.spawn_background_task("windows-preferences", drain_windows_preferences_job);
        }
    }

    /// Python `_on_admin_setting_applied`.
    pub(crate) fn on_admin_setting_applied(&mut self, enabled: bool, elevated: bool, error: &str) {
        self.emit(AppEvent::AdminSettingApplied {
            enabled,
            elevated,
            error: error.to_owned(),
        });
        if self.shutting_down() || enabled != self.state.settings.always_run_as_admin {
            return;
        }
        if !error.is_empty() {
            self.emit_status(
                "error",
                &format!("Ошибка настройки запуска от администратора: {error}"),
            );
        } else if enabled && !elevated {
            self.emit_status(
                "warning",
                "Запуск от имени администратора включён. Перезапускаю Lumen с повышенными правами.",
            );
            self.emit(AppEvent::AdminRelaunchRequested);
        } else {
            self.emit_status(
                if enabled { "success" } else { "info" },
                if enabled {
                    "Запуск от имени администратора включён"
                } else {
                    "Запуск от имени администратора отключён"
                },
            );
        }
    }

    /// Python `_accept_external_startup_disable`.
    pub(crate) fn accept_external_startup_disable(&mut self, generation: u64) {
        if generation != self.startup_settings_generation()
            || self.shutting_down()
            || !self.profile_loaded()
        {
            return;
        }
        if self.state.settings.launch_on_startup {
            self.state.settings.launch_on_startup = false;
            self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
            self.schedule_save();
        }
    }

    /// Python `_schedule_startup_state_sync`.
    pub(crate) fn schedule_startup_state_sync(&mut self) {
        // Registry + schtasks probing must never run on the GUI thread.
        if self.startup_sync_active || self.shutting_down() {
            return;
        }
        self.startup_sync_active = true;
        self.spawn_background_task("startup-state-sync", startup_state_sync_job);
    }

    /// Python `_on_network_changed`.
    pub(crate) fn on_network_changed(&mut self, old: &str, new: &str) {
        self.log(&format!("[network] changed: {old} -> {new}"));
        lumen_platform::network_route_context::invalidate_windows_default_route_context();
        if self.state.settings.tun_mode {
            self.log("[network] ignoring change in TUN mode");
            return;
        }
        if self.connected && self.state.settings.reconnect_on_network_change {
            self.runtime.desired_connected = true;
            self.request_transition("network changed");
        }
    }
}
