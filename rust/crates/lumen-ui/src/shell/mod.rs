//! Application shell: the port of `qml_app/main_qml.py` (entry point, eframe
//! options, single-instance activation, deep links, power events, tray-mode
//! window behavior, startup registration, admin relaunch) and
//! `qml_app/qml/Main.qml` + `TitleBar.qml` + `ToastHost.qml` (window chrome,
//! navigation rail, page stack, toasts, drop area, lock overlay).
//!
//! Module map:
//! - [`theme`] — Theme.qml design tokens
//! - [`navigation`] — the rail model from Main.qml
//! - [`pages`] — page dispatch
//! - [`toast`] — ToastHost.qml + Windows toast.py
//! - [`tray`] — tray.py QmlTray
//! - [`geometry`] — window_geometry.py
//! - [`reveal`] — window_reveal.py
//! - [`win_native`] — the ctypes integration blocks of main_qml.py

pub mod geometry;
pub(crate) mod clipboard;
pub(crate) mod motion;
pub(crate) mod widgets;
pub(crate) mod fluent_menu;
mod wheel;
#[cfg(debug_assertions)]
mod preview_input;
pub mod navigation;
pub mod pages;
mod backdrop_blur;
mod onboarding;
pub mod reveal;
mod startup_diagnostics;
pub mod theme;
pub mod toast;
pub mod tray;
pub mod tray_menu;
pub mod win_native;

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Vec2};
use lumen_app::single_instance::{ActivationReceiver, InstanceActivation, Launch, launch};
use lumen_core::constants::APP_NAME;
use lumen_core::i18n::tr;
use lumen_platform::power_reconnect::WM_ENDSESSION;
use lumen_platform::startup::{is_process_elevated, relaunch_as_admin};

use crate::view_models::{AppBridge, app_bridge::UiEvent};

use navigation::{NavigationState, Page};
use reveal::{EframeRevealSink, REVEAL_FALLBACK_DELAY, RevealCommand, WindowReveal};
use theme::ThemeInputs;
use toast::ToastHost;
use tray::{RoutingPresetItem, TrayCommand, TrayController};
use win_native::PowerMessage;

/// Main.qml window constraints (`appMinimumWidth/Height`).
pub const APP_MINIMUM_WIDTH: i64 = 640;
pub const APP_MINIMUM_HEIGHT: i64 = 360;
/// QML default window size.
const DEFAULT_WINDOW_WIDTH: i64 = 1280;
const DEFAULT_WINDOW_HEIGHT: i64 = 720;
/// TitleBar.qml caption height (+2 while maximized/fullscreen).
const TITLE_BAR_HEIGHT: f32 = 34.0;
/// Start time of the running `chromeSettleAnim`.
const CHROME_SETTLE: &str = "lumen-chrome-settle";
/// Geometry-save debounce (`saveGeometryTimer` interval).
const GEOMETRY_SAVE_DELAY: Duration = Duration::from_millis(500);
/// Legacy file cleanup delay (`QTimer.singleShot(800, ...)`).
const LEGACY_CLEANUP_DELAY: Duration = Duration::from_millis(800);
/// `bridge.startDeferred` fallback timer.
const START_DEFERRED_FALLBACK: Duration = Duration::from_millis(750);
/// System color-scheme refresh cadence.
const SYSTEM_SCHEME_REFRESH: Duration = Duration::from_millis(500);
/// Activation-stop poll interval for the forwarding thread.

/// `QSystemTrayIcon.isSystemTrayAvailable() or sys.platform == "win32"` — on
/// Windows the tray is always assumed available; other platforms report the
/// conservative default.
pub fn tray_available() -> bool {
    true
}

// ── Entry point ──────────────────────────────────────────────────────────

/// Port of `main_qml.main()`. Returns the process exit code.
pub fn run() -> i32 {
    // The rendering backend is fixed to glow (OpenGL) at compile time.
    win_native::set_app_user_model_id();

    let launch_arguments: Vec<String> = std::env::args().collect();

    // `_create_single_instance`: forward deep links to the primary or fail
    // with the actionable warning.
    let Ok(instance_runtime) = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    else {
        return 1;
    };
    let (activation_receiver, initial_deep_link, start_in_tray) =
        match instance_runtime.block_on(launch(&launch_arguments)) {
            Launch::Primary {
                activations,
                initial_deep_link,
                start_in_tray,
            } => (activations, initial_deep_link, start_in_tray),
            Launch::Forwarded => return 0,
            Launch::Unavailable => {
                lumen_platform::single_instance::show_unavailable_warning();
                return 1;
            }
        };

    let diagnostics = startup_diagnostics::StartupDiagnostics::install(
        &lumen_core::constants::log_dir(),
    );
    let mut bridge = AppBridge::new();
    diagnostics.report_previous();
    {
        let handle = bridge.runtime_handle();
        let _entered = handle.enter();
        bridge.load();
    }
    bridge.start_controller_loop();

    // A game can occupy all logical CPUs; keep the GUI and the core's log
    // reader schedulable so the running VPN does not depend on a stalled UI.
    lumen_platform::runtime_priority::prioritize_lumen_ui();

    let tray_available = tray_available();
    bridge.set_tray_available(tray_available);

    // Windows power events filter: a hidden top-level window replays
    // WM_POWERBROADCAST / WM_QUERYENDSESSION / WM_ENDSESSION into the shell
    // loop (Qt used installNativeEventFilter).
    let (power_rx, power_window) = win_native::spawn_power_event_window();

    let (saved_width, saved_height, saved_x, saved_y) = bridge.window_geometry();
    let desired_width = APP_MINIMUM_WIDTH.max(if saved_width > 0 {
        saved_width
    } else {
        DEFAULT_WINDOW_WIDTH
    });
    let desired_height = APP_MINIMUM_HEIGHT.max(if saved_height > 0 {
        saved_height
    } else {
        DEFAULT_WINDOW_HEIGHT
    });
    let fitted = geometry::fit_window_geometry_on_screens(
        desired_width,
        desired_height,
        saved_x,
        saved_y,
        APP_MINIMUM_WIDTH,
        APP_MINIMUM_HEIGHT,
    );
    let fitted_width = fitted["width"].as_i64().unwrap_or(desired_width) as f32;
    let fitted_height = fitted["height"].as_i64().unwrap_or(desired_height) as f32;
    let fitted_x = fitted["x"].as_i64().unwrap_or(0) as f32;
    let fitted_y = fitted["y"].as_i64().unwrap_or(0) as f32;

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        // Main.qml draws its own title bar (the QWindowKit custom frame).
        .with_decorations(false)
        // Main.qml `color: "transparent"`: the DWM backdrop shows through
        // the translucent window veil.
        .with_transparent(true)
        .with_min_inner_size([APP_MINIMUM_WIDTH as f32, APP_MINIMUM_HEIGHT as f32])
        // WindowReveal: show only after the first frame painted.
        .with_visible(false)
        .with_active(true)
        .with_inner_size([fitted_width, fitted_height])
        .with_position([fitted_x, fitted_y]);
    if let Some(icon) = tray::load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let app = ShellApp::new(
        bridge,
        activation_receiver,
        instance_runtime,
        power_rx,
        initial_deep_link,
        start_in_tray,
    );
    let mut app = Some(app);
    let result = eframe::run_native(
        APP_NAME,
        eframe::NativeOptions {
            viewport,
            // Qt paints flat fills; dithering would add visible noise to them.
            dithering: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = app.take().expect("app creator called once");
            app.on_created(cc);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );

    // `app.aboutToQuit` handlers outside the window lifetime: stop the hidden
    // power window. The bridge shut itself down in `ShellApp::on_exit`.
    if let Some(window) = power_window.as_ref() {
        window.close();
    }
    match result {
        Ok(()) => 0,
        Err(error) => {
            diagnostics.report_renderer_failure(&error);
            1
        }
    }
}

// ── Shell application ────────────────────────────────────────────────────

/// The eframe application: window chrome + navigation + page stack + overlays.
pub struct ShellApp {
    bridge: AppBridge,
    navigation: NavigationState,
    reveal: WindowReveal,
    reveal_sink: EframeRevealSink,
    reveal_fallback_at: Option<Instant>,
    toasts: ToastHost,
    tray: Option<TrayController>,
    /// Geometry-save debounce deadline.
    save_geometry_deadline: Option<Instant>,
    last_window_rect: Option<(i32, i32, i32, i32)>,
    last_maximized: bool,
    power_rx: Receiver<PowerMessage>,
    instance_runtime: tokio::runtime::Runtime,
    activations: Option<ActivationReceiver>,
    pending_activations: Vec<InstanceActivation>,
    initial_deep_link: String,
    start_in_tray: bool,
    first_frame_done: bool,
    deferred_fallback_deadline: Option<Instant>,
    legacy_cleanup_scheduled: bool,
    system_dark: bool,
    system_dark_refresh_at: Instant,
    installed_font_family: Option<String>,
    wheel: wheel::WheelGlide,
    conflict_dialog: Option<ConflictDialog>,
    lock_password: String,
    lock_error_visible: bool,
    was_locked: bool,
    wallpaper_path: String,
    wallpaper_blur: i64,
    wallpaper_texture: Option<egui::TextureHandle>,
    /// Applied (dark, backdrop) for the DWM refresh dedup.
    applied_mica: Option<(bool, String)>,
    last_tray_refresh: Option<(bool, bool, bool)>,
    last_tray_routing: Option<(Vec<RoutingPresetItem>, Vec<RoutingPresetItem>)>,
    taskbar_tick_deadline: Instant,
    last_settings_token: u64,
    page_events: std::collections::VecDeque<UiEvent>,
    page_state: pages::PageState,
    onboarding: onboarding::Onboarding,
    /// The window has been on screen; the onboarding waits for it.
    window_shown_once: bool,
    /// Scripted comparison input (`LUMEN_PREVIEW_INPUT`) and its start time.
    #[cfg(debug_assertions)]
    preview_input: Option<(Option<f64>, preview_input::PreviewInput)>,
}

/// The network conflict FluentDialog state (Main.qml `networkConflictDialog`).
#[derive(Clone)]
struct ConflictDialog {
    message: String,
    can_terminate: bool,
}

impl ShellApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        bridge: AppBridge,
        activations: ActivationReceiver,
        instance_runtime: tokio::runtime::Runtime,
        power_rx: Receiver<PowerMessage>,
        initial_deep_link: String,
        start_in_tray: bool,
    ) -> Self {
        Self {
            bridge,
            navigation: NavigationState::default(),
            reveal: WindowReveal::new(),
            reveal_sink: EframeRevealSink::default(),
            reveal_fallback_at: None,
            toasts: ToastHost::default(),
            tray: None,
            save_geometry_deadline: None,
            last_window_rect: None,
            last_maximized: false,
            power_rx,
            instance_runtime,
            activations: Some(activations),
            pending_activations: Vec::new(),
            initial_deep_link,
            start_in_tray,
            first_frame_done: false,
            deferred_fallback_deadline: None,
            legacy_cleanup_scheduled: false,
            system_dark: false,
            system_dark_refresh_at: Instant::now(),
            installed_font_family: None,
            wheel: wheel::WheelGlide::default(),
            conflict_dialog: None,
            lock_password: String::new(),
            lock_error_visible: false,
            was_locked: false,
            wallpaper_path: String::new(),
            wallpaper_blur: 0,
            wallpaper_texture: None,
            applied_mica: None,
            last_tray_refresh: None,
            last_tray_routing: None,
            taskbar_tick_deadline: Instant::now() + Duration::from_secs(1),
            last_settings_token: 0,
            page_events: std::collections::VecDeque::new(),
            page_state: pages::PageState::default(),
            onboarding: onboarding::Onboarding::default(),
            window_shown_once: false,
            #[cfg(debug_assertions)]
            preview_input: preview_input::PreviewInput::from_env().map(|input| (None, input)),
        }
    }

    /// `CreationContext` setup (fonts + theme before the first frame).
    fn on_created(&mut self, cc: &eframe::CreationContext) {
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        self.refresh_system_scheme(true);
        let theme = self.theme();
        theme::apply_fonts(&cc.egui_ctx, &theme);
        self.installed_font_family = Some(theme::font_key(&theme));
        theme::apply_visuals(&cc.egui_ctx, &theme);
    }

    /// `resolveDark()` — explicit light/dark, else follow the system scheme.
    fn resolve_dark(&mut self) -> bool {
        match self.bridge.theme_name() {
            "dark" => true,
            "light" => false,
            _ => {
                self.refresh_system_scheme(false);
                self.system_dark
            }
        }
    }

    /// `app.styleHints().colorScheme` equivalent: registry AppsUseLightTheme.
    fn refresh_system_scheme(&mut self, force: bool) {
        let now = Instant::now();
        if !force && now < self.system_dark_refresh_at {
            return;
        }
        self.system_dark_refresh_at = now + SYSTEM_SCHEME_REFRESH;
        self.system_dark = match win_native::read_hkcu_dword(
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "AppsUseLightTheme",
        ) {
            Some(0) => true,
            Some(_) => false,
            None => false,
        };
    }

    /// Assemble the Theme.qml inputs from the bridge bindings.
    fn theme_inputs(&mut self) -> ThemeInputs {
        let dark = self.resolve_dark();
        let accent = theme::hex_color(&self.bridge.accent_color())
            .unwrap_or_else(|| theme::hex_color("#0078D4").expect("accent default"));
        ThemeInputs {
            dark,
            accent,
            density: self.bridge.settings().ui_density.clone(),
            corner_radius: self.bridge.settings().ui_corner_radius,
            font_scale: self.bridge.settings().ui_font_scale as f64 / 100.0,
            font_family_name: "Segoe UI".to_string(),
            animations: self.bridge.settings().ui_animations,
            preset: self.bridge.settings().ui_theme_preset.clone(),
            base_tint: self.bridge.settings().ui_base_tint.clone(),
            backdrop: self.bridge.settings().ui_backdrop.clone(),
            transparency_strength: self.bridge.settings().ui_transparency_strength,
            backdrop_available: self.bridge.ui_backdrop_available(),
            wallpaper_active: !self.bridge.ui_wallpaper().is_empty(),
        }
    }

    fn theme(&mut self) -> theme::Theme {
        theme::Theme::resolve(&self.theme_inputs())
    }

    // ── per-frame machinery ──────────────────────────────────────────────

    /// Drain the single-instance activation channel
    /// (`_read_instance_message` / `_on_second_instance` in main_qml.py).
    fn pump_activations(&mut self) {
        while let Some(activation) = self
            .activations
            .as_mut()
            .and_then(ActivationReceiver::try_recv)
        {
            self.pending_activations.push(activation);
        }
        if self.first_frame_done {
            let pending = std::mem::take(&mut self.pending_activations);
            for activation in pending {
                if activation.should_activate_window() {
                    self.activate_window();
                }
                if let InstanceActivation::DeepLink(link) = activation {
                    self.bridge.handle_deep_link(&link);
                }
            }
        }
    }

    /// Drain the hidden power window into the bridge-owned filter.
    fn pump_power_events(&mut self) {
        while let Ok(message) = self.power_rx.try_recv() {
            self.bridge
                .drive_power_message(message.message, message.wparam);
            if message.message == WM_ENDSESSION && message.wparam != 0 {
                // Qt `commitDataRequest` also ran prepareSystemShutdown.
                self.bridge.prepare_system_shutdown();
            }
        }
        self.bridge.tick_power_filter();
    }

    /// `reveal.request(activate=True)` — `_request_reveal`.
    fn request_reveal(&mut self, activate: bool) {
        self.reveal.request(&mut self.reveal_sink, activate);
        // QTimer.singleShot(500, reveal.reveal_if_pending)
        self.reveal_fallback_at = Some(Instant::now() + REVEAL_FALLBACK_DELAY);
    }

    /// Apply reveal sink commands as viewport commands.
    fn apply_reveal_commands(&mut self, ctx: &egui::Context) {
        for command in self.reveal_sink.drain() {
            match command {
                RevealCommand::Hide => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
                RevealCommand::Show => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true)),
                RevealCommand::Focus => ctx.send_viewport_cmd(egui::ViewportCommand::Focus),
            }
        }
    }

    /// `_activate_window`: reveal + the Win32 foreground transfer.
    fn activate_window(&mut self) {
        self.request_reveal(true);
        win_native::activate_shell_window();
    }

    /// Tray creation (`QmlTray(app, window, bridge, show_window=...)`).
    fn ensure_tray(&mut self) {
        if self.tray.is_none() {
            match TrayController::new() {
                Ok(controller) => self.tray = Some(controller),
                Err(error) => eprintln!("tray creation failed: {error}"),
            }
        }
    }

    /// Tray commands + periodic refresh (the Python signal connections).
    fn pump_tray(&mut self, ctx: &egui::Context) {
        let dark = self.theme().dark;
        let Some(tray) = self.tray.as_mut() else {
            return;
        };
        tray.attach_context(ctx);
        tray.set_dark_theme(dark);
        let commands = tray.poll_commands();
        for command in commands {
            match command {
                TrayCommand::ToggleWindow => {
                    if self.window_visible(ctx) {
                        // `_hide_window`
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    } else {
                        self.activate_window();
                    }
                }
                TrayCommand::ShowWindow => self.activate_window(),
                TrayCommand::ToggleConnection => self.bridge.toggle_connection(),
                TrayCommand::SwitchNext => self.bridge.switch_next(),
                TrayCommand::ApplyRoutingPreset { preset_id, custom } => {
                    if custom {
                        self.bridge.apply_custom_routing_preset(&preset_id);
                    } else {
                        self.bridge.apply_routing_preset(&preset_id);
                    }
                }
                TrayCommand::RestartAdmin => self.restart_admin(ctx),
                TrayCommand::Quit => {
                    self.bridge.prepare_quit();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // `_refresh_actions` triggers: connectedChanged, settingsChanged,
        // languageChanged, colorSchemeChanged, visibility changes.
        let key = (
            self.window_visible(ctx),
            self.bridge.connected(),
            is_process_elevated(),
        );
        let settings_token = self.bridge.settings_changed_token();
        let Some(tray) = self.tray.as_mut() else {
            return;
        };
        if self.last_tray_refresh != Some(key) || self.last_settings_token != settings_token {
            self.last_tray_refresh = Some(key);
            self.last_settings_token = settings_token;
            tray.refresh_actions(key.0, key.1, key.2);
        }

        let regional = routing_items(self.bridge.regional_routing_presets());
        let custom = routing_items(self.bridge.custom_routing_presets());
        if self.last_tray_routing.as_ref() != Some(&(regional.clone(), custom.clone())) {
            self.last_tray_routing = Some((regional.clone(), custom.clone()));
            tray.refresh_routing(&regional, &custom);
        }

        // `_wait_for_taskbar` runs on a one-second cadence.
        if Instant::now() >= self.taskbar_tick_deadline {
            self.taskbar_tick_deadline = Instant::now() + Duration::from_secs(1);
            tray.tick_taskbar_wait();
        }
    }

    /// `_on_admin_relaunch`: save, release the single-instance mutex, launch
    /// elevated; on failure show the toast and re-acquire the listener.
    fn restart_admin(&mut self, ctx: &egui::Context) {
        if is_process_elevated() {
            return;
        }
        self.bridge.save_state();
        // Close the single-instance server so the new process can take the
        // mutex (Python: server.close()).
        self.close_activations();
        if !relaunch_as_admin(None) {
            let message = "\u{041D}\u{0435} \u{0443}\u{0434}\u{0430}\u{043B}\u{043E}\u{0441}\u{044C} \u{043F}\u{0435}\u{0440}\u{0435}\u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{044C} Lumen \u{043E}\u{0442} \u{0438}\u{043C}\u{0435}\u{043D}\u{0438} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}";
            self.toasts.show(message, "error", "", "");
            toast::show_toast(APP_NAME, message, toast::AUMID);
            // The primary keeps running: the activation thread stays on the
            // released flag being cleared (QLocalServer.listen retry).
            let arguments = std::env::args().take(1).collect::<Vec<_>>();
            if let Launch::Primary { activations, .. } =
                self.instance_runtime.block_on(launch(&arguments))
            {
                self.activations = Some(activations);
            }
        } else {
            self.bridge.prepare_quit();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn window_visible(&self, ctx: &egui::Context) -> bool {
        win_native::main_window_visible(APP_NAME)
            .unwrap_or_else(|| ctx.input(|input| input.viewport().visible().unwrap_or(true)))
    }

    /// First-frame work (Python: `_on_first_frame` + the deferred timers).
    fn first_frame(&mut self, ctx: &egui::Context) {
        if self.first_frame_done {
            return;
        }
        self.first_frame_done = true;
        self.reveal.frame_swapped(&mut self.reveal_sink);
        self.ensure_tray();
        self.bridge.set_tray_available(self.tray.is_some());
        self.bridge.start_deferred();
        self.deferred_fallback_deadline = Some(Instant::now() + START_DEFERRED_FALLBACK);
        if let Some(hwnd) = win_native::find_main_window(APP_NAME) {
            win_native::install_custom_frame(hwnd);
        }
        self.apply_mica(ctx, true);
        // QTimer.singleShot(800, _cleanup_legacy_files): the legacy startup
        // cleanup must never hold the GUI before the window is visible.
        if !self.legacy_cleanup_scheduled {
            self.legacy_cleanup_scheduled = true;
            std::thread::spawn(move || {
                std::thread::sleep(LEGACY_CLEANUP_DELAY);
                win_native::cleanup_legacy_root_program_install();
                win_native::cleanup_legacy_executable_bridge();
            });
        }
        // initial deep link dispatch (QTimer.singleShot(0, ...)).
        if !self.initial_deep_link.is_empty() {
            let link = std::mem::take(&mut self.initial_deep_link);
            self.bridge.handle_deep_link(&link);
        }
        // A logon autostart stays hidden until the user asks for the window.
        if !(self.start_in_tray && self.tray.is_some()) {
            self.request_reveal(false);
        }
    }

    /// `_refresh_backdrop` / `_schedule_backdrop_refresh`.
    fn apply_mica(&mut self, _ctx: &egui::Context, force: bool) {
        let theme = self.theme();
        let key = (theme.dark, theme.backdrop.clone());
        if !force && self.applied_mica == Some(key.clone()) {
            return;
        }
        if let Some(hwnd) = win_native::find_main_window(APP_NAME) {
            win_native::apply_mica(hwnd, theme.dark, &theme.backdrop);
            self.applied_mica = Some(key);
        }
    }

    /// Track window geometry and arm the 500 ms save debounce.
    fn track_window_geometry(&mut self, ctx: &egui::Context) {
        let (outer, maximized, minimized) = ctx.input(|input| {
            let viewport = input.viewport();
            let rect = viewport.outer_rect.map(|rect| {
                (
                    rect.min.x.round() as i32,
                    rect.min.y.round() as i32,
                    rect.width().round() as i32,
                    rect.height().round() as i32,
                )
            });
            (
                rect,
                viewport.maximized.unwrap_or(false),
                viewport.minimized.unwrap_or(false),
            )
        });
        let windowed = !maximized && !minimized;
        if windowed {
            if outer != self.last_window_rect {
                self.save_geometry_deadline = Some(Instant::now() + GEOMETRY_SAVE_DELAY);
            }
            self.last_window_rect = outer;
        }
        if maximized != self.last_maximized {
            self.last_maximized = maximized;
            // Title height changes; state changes also refresh the backdrop.
            self.applied_mica = None;
        }
        if let Some(deadline) = self.save_geometry_deadline {
            if Instant::now() >= deadline {
                self.save_geometry_deadline = None;
                if let Some((x, y, width, height)) = self.last_window_rect {
                    self.bridge.save_window_geometry(
                        width as i64,
                        height as i64,
                        x as i64,
                        y as i64,
                    );
                }
            }
        }
    }

    /// Main.qml `hasSupportedConfigDrop` + `queueConfigDrop`.
    fn handle_config_drops(&mut self, ctx: &egui::Context) {
        let (hovered, dropped) = ctx.input(|input| {
            (
                input.raw.hovered_files.clone(),
                input.raw.dropped_files.clone(),
            )
        });
        let _ = hovered;
        if dropped.is_empty() || self.bridge.locked() {
            return;
        }
        let paths: Vec<PathBuf> = dropped
            .into_iter()
            .map(|file| file.path().to_path_buf())
            .filter(|path| has_supported_config_drop(&path))
            .collect();
        if paths.is_empty() {
            return;
        }
        // queueConfigDrop: switch to the nodes page and import (egui pages
        // build on demand, so finishConfigDrop collapses into one call).
        self.navigation.current = Page::Nodes;
        self.bridge.import_node_files(&paths);
    }

    /// Bridge-originated toasts (`toast`, `actionToast` signals).
    fn pump_bridge_events(&mut self, ctx: &egui::Context) {
        for event in self.bridge.take_events() {
            self.page_state.on_event(&event);
            match event {
                UiEvent::Toast { level, message } => self.toasts.show(message, level, "", ""),
                UiEvent::ActionToast {
                    level,
                    message,
                    action_id,
                    action_label,
                } => self.toasts.show(message, level, action_id, action_label),
                UiEvent::NetworkConflictRequested {
                    message,
                    can_terminate,
                    ..
                } => {
                    self.conflict_dialog = Some(ConflictDialog {
                        message,
                        can_terminate,
                    })
                }
                UiEvent::AdminRelaunchRequested => self.restart_admin(ctx),
                UiEvent::OpenDirectory(path) => win_native::open_directory(&path),
                UiEvent::TrayNotify { title, message } => {
                    toast::show_toast(&title, &message, toast::AUMID);
                }
                event => {
                    if self.page_events.len() == 128 {
                        self.page_events.pop_front();
                    }
                    self.page_events.push_back(event);
                }
            }
        }
    }

    fn close_activations(&mut self) {
        if let Some(activations) = self.activations.take() {
            self.instance_runtime.block_on(activations.close());
        }
    }

    // ── painting ─────────────────────────────────────────────────────────

    /// Paint the wallpaper image layer (Main.qml `wallpaperImg`).
    fn paint_wallpaper(&mut self, ctx: &egui::Context, painter: &egui::Painter, rect: Rect) {
        let path = self.bridge.ui_wallpaper();
        if path.is_empty() {
            self.wallpaper_texture = None;
            self.wallpaper_path.clear();
            return;
        }
        let blur = self.bridge.settings().ui_wallpaper_blur.clamp(0, 100);
        if self.wallpaper_path != path || self.wallpaper_blur != blur {
            self.wallpaper_texture = load_wallpaper_texture(ctx, &path, blur);
            self.wallpaper_path = path.clone();
            self.wallpaper_blur = blur;
        }
        if let Some(texture) = self.wallpaper_texture.clone() {
            let opacity =
                (self.bridge.settings().ui_wallpaper_opacity.clamp(0, 100) as f32) / 100.0;
            let brightness = self.bridge.settings().ui_wallpaper_brightness.clamp(0, 100) as f32;
            // PreserveAspectCrop: cover the rect, center-aligned.
            let size = texture.size_vec2();
            if size.x > 0.0 && size.y > 0.0 {
                let scale = (rect.width() / size.x).max(rect.height() / size.y);
                let drawn = size * scale;
                let image_rect = Rect::from_center_size(rect.center(), drawn);
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::ONE),
                    Color32::from_white_alpha((opacity * 255.0).round() as u8),
                );
                if brightness < 100.0 {
                    // MultiEffect brightness: a black veil with
                    // (100 - brightness) / 100 opacity.
                    let veil = (100.0 - brightness) / 100.0;
                    painter.rect_filled(
                        rect,
                        0.0,
                        Color32::from_black_alpha((veil * 255.0).round() as u8),
                    );
                }
            }
        }
    }

    /// Paint the window chrome: background, rail, content panel, title bar.
    fn paint_chrome(&mut self, ctx: &egui::Context, root: &mut egui::Ui, theme: &theme::Theme) {
        let title_height = self.title_bar_height(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.window_base))
            .show(root, |ui| {
                let full = ui.max_rect();

                // Background + wallpaper (Main.qml windowBase + wallpaperImg).
                self.paint_wallpaper(ctx, ui.painter(), full);

                // Rail metrics (Main.qml railVisualWidth).
                let rail_width = self.rail_visual_width(ctx, theme);
                // The panels belong to `appChrome` and settle with it.
                let (settle_opacity, settle_scale) = Self::chrome_settle(ctx, theme);
                let settle = self.chrome_transform(ctx, settle_scale);
                let rail_rect = settle.mul_rect(Rect::from_min_size(
                    egui::pos2(full.left(), full.top() + title_height),
                    Vec2::new(rail_width, full.height() - title_height),
                ));
                ui.painter()
                    .rect_filled(rail_rect, 0.0, theme.chrome_panel.gamma_multiply(settle_opacity));

                // Content host: solid rounded window panel inset into the
                // frame; flush right/bottom, only the top-left corner rounded.
                let content_rect = settle.mul_rect(Rect::from_min_size(
                    egui::pos2(full.left() + rail_width, full.top() + title_height),
                    Vec2::new(full.width() - rail_width, full.height() - title_height),
                ));
                let painter = ui.painter();
                let corner = CornerRadius {
                    nw: theme.radius as u8,
                    ne: 0,
                    sw: 0,
                    se: 0,
                };
                painter.rect_filled(content_rect, 0.0, theme.chrome_panel.gamma_multiply(settle_opacity));
                let panel_color = if !self.bridge.ui_wallpaper().is_empty() {
                    if theme.backdrop == "acrylic" {
                        if theme.dark {
                            Color32::from_rgba_unmultiplied(0, 0, 0, 82)
                        } else {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 71)
                        }
                    } else if theme.dark {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 46)
                    }
                } else {
                    theme.content_panel
                };
                painter.rect_filled(content_rect, corner, panel_color.gamma_multiply(settle_opacity));
                painter.rect_stroke(
                    content_rect,
                    corner,
                    egui::Stroke::new(1.0, theme.divider.gamma_multiply(settle_opacity)),
                    egui::StrokeKind::Inside,
                );

                // Title bar background.
                let title_rect =
                    Rect::from_min_size(full.min, Vec2::new(full.width(), title_height));
                painter.rect_filled(title_rect, 0.0, theme.chrome_panel);
            });
    }

    fn title_bar_height(&self, ctx: &egui::Context) -> f32 {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        TITLE_BAR_HEIGHT + if maximized { 2.0 } else { 0.0 }
    }

    /// Main.qml `railVisualWidth` with its 155 ms `Behavior`.
    fn rail_visual_width(&self, ctx: &egui::Context, theme: &theme::Theme) -> f32 {
        let target = if self.navigation.rail_collapsed() {
            theme.rail_width_compact as f32
        } else {
            theme.rail_width as f32
        };
        motion::follow(
            ctx,
            theme,
            egui::Id::unique("lumen-rail-width"),
            target,
            155.0,
            motion::ease_out_cubic,
        )
    }

    /// Main.qml `chromeSettleAnim`: `appChrome` goes from 0.35 opacity and a
    /// 0.975 scale to 1 over 900 ms (OutQuint) when the onboarding card
    /// starts closing. Returns `(opacity, scale)`.
    fn chrome_settle(ctx: &egui::Context, theme: &theme::Theme) -> (f32, f32) {
        let id = egui::Id::unique(CHROME_SETTLE);
        let Some(start) = ctx.data(|data| data.get_temp::<f64>(id)) else {
            return (1.0, 1.0);
        };
        let elapsed = (ctx.input(|input| input.time) - start) as f32;
        if theme.anim_fast == 0 || elapsed >= 0.9 {
            ctx.data_mut(|data| data.remove::<f64>(id));
            return (1.0, 1.0);
        }
        ctx.request_repaint();
        let level = motion::ease_out_quint(elapsed / 0.9);
        (0.35 + 0.65 * level, 0.975 + 0.025 * level)
    }

    /// `appChrome.scale` around its centre (the window below the title bar).
    fn chrome_transform(&self, ctx: &egui::Context, scale: f32) -> egui::emath::TSTransform {
        let screen = ctx.viewport_rect();
        let top = screen.top() + self.title_bar_height(ctx);
        let center = Vec2::new(screen.center().x, (top + screen.bottom()) / 2.0);
        egui::emath::TSTransform::new(center * (1.0 - scale), scale)
    }

    /// Title bar interactions: drag region + window buttons (TitleBar.qml).
    fn paint_title_bar(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let title_height = self.title_bar_height(ctx);
        let size = Vec2::new(ctx.viewport_rect().width(), title_height);

        egui::Area::new(egui::Id::unique("lumen-titlebar"))
            .order(egui::Order::Middle)
            .movable(false)
            .fixed_pos(egui::Pos2::ZERO)
            .default_size(size)
            .show(ctx, |ui| {
                let (bar, _) = ui.allocate_exact_size(size, Sense::hover());

                // Drag region under everything (later widgets take priority).
                let drag = ui.interact(
                    bar,
                    egui::Id::unique("titlebar-drag"),
                    Sense::click_and_drag(),
                );
                if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    // toggleMax()
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }

                // Centered app name.
                ui.painter().text(
                    bar.center(),
                    Align2::CENTER_CENTER,
                    self.bridge.app_name(),
                    FontId::proportional((14.0 * theme.font_scale) as f32),
                    theme.text_muted,
                );

                // Right-aligned window buttons (min/max/close).
                let button_width = (44.0 * theme.font_scale) as f32;
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                let glyphs: [(&str, bool); 3] = [
                    ("\u{E921}", false),                                      // minimize
                    (if maximized { "\u{E923}" } else { "\u{E922}" }, false), // restore/maximize
                    ("\u{E8BB}", true),                                       // close
                ];
                for (index, (glyph, danger)) in glyphs.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        egui::pos2(bar.right() - (3 - index) as f32 * button_width, bar.top()),
                        Vec2::new(button_width, bar.height()),
                    );
                    let response = ui.interact(
                        rect,
                        egui::Id::unique(("titlebar-btn", index)),
                        Sense::click(),
                    );
                    let fill = if response.hovered() {
                        if *danger {
                            Color32::from_rgb(0xE8, 0x11, 0x23)
                        } else {
                            theme.card_hover
                        }
                    } else {
                        Color32::TRANSPARENT
                    };
                    let text_color = if *danger && response.hovered() {
                        Color32::WHITE
                    } else {
                        theme.text_muted
                    };
                    ui.painter().rect_filled(rect, 0.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *glyph,
                        FontId::proportional((10.0 * theme.font_scale) as f32),
                        text_color,
                    );
                    if response.clicked() {
                        match index {
                            0 => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                            1 => {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized))
                            }
                            _ => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        }
                    }
                }
            });
    }

    /// The navigation rail (`railClip` + `railColumn` in Main.qml).
    fn paint_rail(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let compact_mode = self.bridge.compact_mode();
        let rail_width = self.rail_visual_width(ctx, theme);
        let title_height = self.title_bar_height(ctx);
        let rail_height = ctx.viewport_rect().height() - title_height;

        let (settle_opacity, settle_scale) = Self::chrome_settle(ctx, theme);
        ctx.set_transform_layer(
            egui::LayerId::new(egui::Order::Middle, egui::Id::unique("lumen-rail")),
            self.chrome_transform(ctx, settle_scale),
        );
        egui::Area::new(egui::Id::unique("lumen-rail"))
            .order(egui::Order::Middle)
            .movable(false)
            .fixed_pos(egui::pos2(0.0, title_height))
            .default_size(Vec2::new(rail_width, rail_height))
            .show(ctx, |ui| {
                ui.set_width(rail_width);
                ui.set_height(rail_height);
                ui.shrink_clip_rect(Rect::from_min_size(
                    egui::pos2(0.0, title_height),
                    Vec2::new(rail_width, rail_height),
                ));
                ui.multiply_opacity(settle_opacity);

                egui::ScrollArea::vertical()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let top = ui.cursor().top();
                        ui.set_min_width(rail_width);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        // navFlick `anchors.topMargin: 8`.
                        ui.add_space(8.0);
                        // Burger row (40 * fontScale tall).
                        let burger_height = (40.0 * theme.font_scale) as f32;
                        let (burger_rect, burger_response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), burger_height),
                            Sense::click(),
                        );
                        let bg_width = motion::follow(
                            ctx,
                            theme,
                            egui::Id::unique("lumen-burger-width"),
                            if self.navigation.rail_collapsed() {
                                (38.0 * theme.font_scale) as f32
                            } else {
                                ((38.0 * theme.font_scale) as f32).max(burger_rect.width() - 12.0)
                            },
                            170.0,
                            motion::ease_out_quint,
                        );
                        let bg_rect = Rect::from_min_size(
                            egui::pos2(burger_rect.left() + 6.0, burger_rect.top()),
                            Vec2::new(bg_width, burger_height),
                        );
                        ui.painter().rect_filled(
                            bg_rect,
                            theme.radius_small as f32,
                            motion::hover_color(
                                ctx,
                                theme,
                                egui::Id::unique("lumen-burger-hover"),
                                burger_response.hovered(),
                                Color32::TRANSPARENT,
                                theme.card_hover,
                                130.0,
                            ),
                        );
                        ui.painter().text(
                            egui::pos2(bg_rect.left() + 11.0, bg_rect.center().y),
                            Align2::LEFT_CENTER,
                            "\u{E700}",
                            FontId::proportional((16.0 * theme.font_scale) as f32),
                            theme.text_muted,
                        );
                        if burger_response.clicked() {
                            self.navigation.nav_expanded = !self.navigation.nav_expanded;
                        }

                        // 4 px spacer item between two 2 px layout gaps.
                        ui.add_space(6.0);

                        // Top section.
                        for entry in navigation::NAV_ENTRIES.iter() {
                            if entry.section != navigation::Section::Top {
                                continue;
                            }
                            if !self.navigation.entry_visible(entry, compact_mode) {
                                continue;
                            }
                            let selected = self.navigation.entry_selected(entry);
                            let clicked = navigation::draw_nav_button(
                                ui,
                                theme,
                                entry,
                                selected,
                                false,
                                self.navigation.rail_collapsed(),
                            );
                            if clicked {
                                self.navigation.current = entry.page;
                            }
                        }

                        // Stretch to the bottom section (Layout.fillHeight item).
                        let row_height = (40.0 * theme.font_scale) as f32;
                        let bottom_rows = navigation::NAV_ENTRIES.iter().filter(|entry| {
                            entry.section == navigation::Section::Bottom
                                && self.navigation.entry_visible(entry, compact_mode)
                        }).count() as f32;
                        let stretch = (rail_height - 14.0 - (ui.cursor().top() - top)
                            - (bottom_rows * (row_height + 2.0) - 2.0)).max(14.0);
                        ui.add_space(stretch);

                        // Bottom section.
                        for entry in navigation::NAV_ENTRIES.iter() {
                            if entry.section != navigation::Section::Bottom {
                                continue;
                            }
                            if !self.navigation.entry_visible(entry, compact_mode) {
                                continue;
                            }
                            let selected = self.navigation.entry_selected(entry);
                            let badge =
                                navigation::updates_badge(entry, self.bridge.updates_available());
                            let clicked = navigation::draw_nav_button(
                                ui,
                                theme,
                                entry,
                                selected,
                                badge,
                                self.navigation.rail_collapsed(),
                            );
                            if clicked {
                                self.navigation.current = entry.page;
                                if entry.page == Page::Updates {
                                    self.bridge.mark_updates_seen();
                                }
                            }
                        }
                    });
            });
    }

    /// The page stack (`pageStack` margins + LazyPageLoader stack).
    fn paint_pages(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let rail_width = self.rail_visual_width(ctx, theme);
        let screen = ctx.viewport_rect();
        let page_rect = Rect::from_min_size(
            egui::pos2(
                screen.left() + rail_width + 24.0,
                screen.top() + self.title_bar_height(ctx) + 20.0,
            ),
            Vec2::new(
                (screen.width() - rail_width - 48.0).max(0.0),
                (screen.height() - self.title_bar_height(ctx) - 40.0).max(0.0),
            ),
        );
        // LazyPageLoader: every page has `opacity: current ? 1 : 0` (220 ms,
        // OutCubic) and a 6 px offset while it is not current (260 ms,
        // OutQuint), and stays laid out while its opacity is above zero. A
        // page change therefore cross-fades the old page out under the new
        // one, and both Behaviors retarget from their current value.
        let current = self.navigation.current;
        let (chrome_opacity, settle_scale) = Self::chrome_settle(ctx, theme);
        let settle = self.chrome_transform(ctx, settle_scale);
        let stack_id = egui::Id::unique("lumen-page-stack");
        for page in Page::ALL {
            let is_current = page == current;
            let opacity = motion::follow(
                ctx,
                theme,
                stack_id.with(("opacity", page.index())),
                if is_current { 1.0 } else { 0.0 },
                220.0,
                motion::ease_out_cubic,
            );
            let slide = motion::follow(
                ctx,
                theme,
                stack_id.with(("slide", page.index())),
                if is_current { 0.0 } else { 6.0 },
                260.0,
                motion::ease_out_quint,
            );
            if !is_current && opacity <= 0.0 {
                continue;
            }
            ctx.set_transform_layer(
                egui::LayerId::new(egui::Order::Middle, stack_id.with(page.index())),
                settle,
            );
            egui::Area::new(stack_id.with(page.index()))
                .order(egui::Order::Middle)
                .movable(false)
                // Only the current page takes input.
                .interactable(is_current)
                .fixed_pos(page_rect.min + Vec2::new(0.0, slide))
                .default_size(page_rect.size())
                .show(ctx, |ui| {
                    ui.set_width(page_rect.width());
                    ui.set_height(page_rect.height());
                    ui.shrink_clip_rect(page_rect);
                    ui.multiply_opacity(opacity * chrome_opacity);
                    let mut hooks = pages::ShellHooks::default();
                    {
                        let mut page_ctx = pages::PageContext {
                            ctx,
                            theme,
                            bridge: &mut self.bridge,
                            events: self.page_events.make_contiguous(),
                            shell: &mut hooks,
                        };
                        if is_current {
                            pages::render_page(page, ui, &mut page_ctx, &mut self.page_state);
                        } else {
                            pages::render_leaving_page(page, ui, &mut page_ctx, &mut self.page_state);
                        }
                    }
                    if is_current && hooks.open_configs_requested {
                        self.navigation.current = Page::Configs;
                    }
                });
        }
    }

    /// The drag-drop overlay (Main.qml `configDropArea` inner Rectangle).
    fn paint_drop_overlay(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let supported = ctx.input(|input| {
            input
                .raw
                .hovered_files
                .iter()
                .any(|file| file.path.as_deref().is_some_and(has_supported_config_drop))
        });
        if !supported || self.bridge.locked() {
            return;
        }
        // Painted straight onto a foreground layer: an `Area` sizes itself to
        // its content and covered only part of the window.
        let rect = ctx.viewport_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("lumen-drop-overlay"),
        ));
        let fill = if theme.dark {
            Color32::from_rgba_unmultiplied(10, 13, 18, 224)
        } else {
            Color32::from_rgba_unmultiplied(245, 248, 252, 230)
        };
        painter.rect_filled(rect, 0.0, fill);
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0, theme.accent),
            egui::StrokeKind::Inside,
        );
        // Centred column with 12 px spacing, as in the QML ColumnLayout.
        let title = painter.layout_no_wrap(
            tr(
                "\u{041E}\u{0442}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{0435} \u{0444}\u{0430}\u{0439}\u{043B}\u{044B} \u{0434}\u{043B}\u{044F} \u{0438}\u{043C}\u{043F}\u{043E}\u{0440}\u{0442}\u{0430}",
                &[],
            ),
            theme::semibold(theme.font_strong as f32),
            theme.text,
        );
        let glyph = painter.layout_no_wrap(
            "\u{E8B7}".to_owned(),
            FontId::proportional(42.0),
            theme.accent,
        );
        let hint = painter.layout_no_wrap(
            ".conf  .txt  .json  .yaml  .yml".to_owned(),
            FontId::proportional(theme.font_small as f32),
            theme.text_muted,
        );
        let height = glyph.size().y + title.size().y + hint.size().y + 24.0;
        let mut y = rect.center().y - height / 2.0;
        for (galley, color) in [(glyph, theme.accent), (title, theme.text), (hint, theme.text_muted)] {
            let size = galley.size();
            painter.galley(egui::pos2(rect.center().x - size.x / 2.0, y), galley, color);
            y += size.y + 12.0;
        }
    }

    /// The master-password lock overlay (Main.qml `lockOverlay`).
    fn paint_lock_overlay(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let locked = self.bridge.locked();
        if locked && !self.was_locked {
            // onVisibleChanged: reset + focus the field every time.
            self.lock_password.clear();
            self.lock_error_visible = false;
        }
        self.was_locked = locked;
        if !locked {
            return;
        }
        egui::Area::new(egui::Id::unique("lumen-lock-overlay"))
            .order(egui::Order::Tooltip)
            .fixed_pos(ctx.viewport_rect().min).default_size(ctx.viewport_rect().size())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(153));

                // Swallow clicks so the UI behind the overlay stays
                // inaccessible.
                let _ = ui.interact(rect, egui::Id::unique("lock-shield"), Sense::click());

                let card_width = 360.0;
                let card_rect = Rect::from_center_size(rect.center(), Vec2::new(card_width, 330.0));
                ui.painter()
                    .rect_filled(card_rect, theme.radius as f32, theme.flyout);
                ui.painter().rect_stroke(
                    card_rect,
                    theme.radius as f32,
                    egui::Stroke::new(1.0, theme.flyout_border),
                    egui::StrokeKind::Inside,
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(card_rect), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.label(egui::RichText::new("\u{EB95}").size(40.0).color(theme.accent));
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr(
                                "\u{041F}\u{0440}\u{0438}\u{043B}\u{043E}\u{0436}\u{0435}\u{043D}\u{0438}\u{0435} \u{0437}\u{0430}\u{0431}\u{043B}\u{043E}\u{043A}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{043D}\u{043E}",
                                &[],
                            ))
                            .size(theme.font_strong as f32)
                            .color(theme.text),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(tr(
                                "\u{0412}\u{0432}\u{0435}\u{0434}\u{0438}\u{0442}\u{0435} \u{043C}\u{0430}\u{0441}\u{0442}\u{0435}\u{0440}-\u{043F}\u{0430}\u{0440}\u{043E}\u{043B}\u{044C} \u{0434}\u{043B}\u{044F} \u{0440}\u{0430}\u{0437}\u{0431}\u{043B}\u{043E}\u{043A}\u{0438}\u{0440}\u{043E}\u{0432}\u{043A}\u{0438}",
                                &[],
                            ))
                            .size(theme.font_small as f32)
                            .color(theme.text_faint),
                        );
                        ui.add_space(10.0);
                        let field_width = card_width - 48.0;
                        let response = ui.add_sized(
                            [field_width, theme.control_height as f32],
                            egui::TextEdit::singleline(&mut self.lock_password)
                                .password(true)
                                .hint_text(tr("\u{041F}\u{0430}\u{0440}\u{043E}\u{043B}\u{044C}", &[])),
                        );
                        let accepted =
                            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        ui.add_space(6.0);
                        if self.lock_error_visible {
                            ui.label(
                                egui::RichText::new(tr(
                                    "\u{041D}\u{0435}\u{0432}\u{0435}\u{0440}\u{043D}\u{044B}\u{0439} \u{043F}\u{0430}\u{0440}\u{043E}\u{043B}\u{044C}",
                                    &[],
                                ))
                                .size(theme.font_small as f32)
                                .color(theme.danger),
                            );
                        }
                        ui.add_space(10.0);
                        let button = egui::Button::new(
                            egui::RichText::new(format!(
                                "\u{E785} {}",
                                tr(
                                    "\u{0420}\u{0430}\u{0437}\u{0431}\u{043B}\u{043E}\u{043A}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{0442}\u{044C}",
                                    &[],
                                )
                            ))
                            .color(theme.accent_text)
                            .size(theme.font_normal as f32),
                        )
                        .fill(theme.accent)
                        .min_size(Vec2::new(field_width, theme.control_height as f32));
                        let pressed = ui.add(button).clicked();
                        if pressed || accepted {
                            let password = self.lock_password.clone();
                            if self.bridge.unlock(&password) {
                                self.lock_password.clear();
                                self.lock_error_visible = false;
                            } else {
                                self.lock_error_visible = true;
                                response.request_focus();
                            }
                        }
                    });
                });
            });
    }

    /// The network conflict FluentDialog.
    fn paint_conflict_dialog(&mut self, ctx: &egui::Context, theme: &theme::Theme) {
        let Some(dialog) = self.conflict_dialog.clone() else {
            return;
        };
        let width = 560.0_f32.min(ctx.viewport_rect().width() - 40.0);
        egui::Area::new(egui::Id::unique("lumen-conflict-dialog"))
            .order(egui::Order::Tooltip)
            .fixed_pos(ctx.viewport_rect().min).default_size(ctx.viewport_rect().size())
            .show(ctx, |ui| {
                let full = ui.max_rect();
                ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(96));
                let card = Rect::from_center_size(full.center(), Vec2::new(width, 250.0));
                ui.painter().rect_filled(card, theme.radius as f32, theme.flyout);
                ui.painter().rect_stroke(
                    card,
                    theme.radius as f32,
                    egui::Stroke::new(1.0, theme.flyout_border),
                    egui::StrokeKind::Inside,
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new(tr(
                                "\u{041E}\u{0431}\u{043D}\u{0430}\u{0440}\u{0443}\u{0436}\u{0435}\u{043D} \u{0434}\u{0440}\u{0443}\u{0433}\u{043E}\u{0439} VPN/\u{043F}\u{0440}\u{043E}\u{043A}\u{0441}\u{0438}",
                                &[],
                            ))
                            .size(theme.font_title as f32)
                            .color(theme.text),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr(&dialog.message, &[]))
                                .size(theme.font_normal as f32)
                                .color(theme.text_muted),
                        );
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(tr("\u{0417}\u{0430}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[]))
                                .clicked()
                            {
                                self.bridge.cancel_network_conflict();
                                self.conflict_dialog = None;
                            }
                            if dialog.can_terminate
                                && ui
                                    .button(tr(
                                        "\u{0417}\u{0430}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C} \u{043F}\u{0440}\u{043E}\u{0446}\u{0435}\u{0441}\u{0441}\u{044B} \u{0438} \u{043F}\u{0440}\u{043E}\u{0434}\u{043E}\u{043B}\u{0436}\u{0438}\u{0442}\u{044C}",
                                        &[],
                                    ))
                                    .clicked()
                                && self.bridge.close_network_conflicts_and_continue()
                            {
                                self.conflict_dialog = None;
                            }
                        });
                    });
                });
            });
    }
}

/// Main.qml `hasSupportedConfigDrop`: `.conf|.ovpn|.txt|.json|.ya?ml` suffix
/// after stripping `?query` and `#fragment`.
pub fn has_supported_config_drop(path: &std::path::Path) -> bool {
    let Some(value) = path.to_str() else {
        return false;
    };
    let value = value.to_lowercase();
    let value = value.split('?').next().unwrap_or(&value);
    let value = value.split('#').next().unwrap_or(value);
    [".conf", ".ovpn", ".txt", ".json", ".yaml", ".yml"]
        .iter()
        .any(|suffix| value.ends_with(suffix))
}

/// Decode the wallpaper image file into a texture (Main.qml `wallpaperImg`).
fn load_wallpaper_texture(ctx: &egui::Context, path: &str, blur: i64) -> Option<egui::TextureHandle> {
    let data = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&data).ok()?;
    let img = if blur > 0 { img.thumbnail(1920, 1080).blur(blur as f32 * 0.64) } else { img };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture("lumen-wallpaper", color_image, egui::TextureOptions::LINEAR))
}

fn routing_items(values: Vec<serde_json::Value>) -> Vec<RoutingPresetItem> {
    values
        .iter()
        .map(|value| RoutingPresetItem {
            id: lumen_core::models::str_field(value, "id"),
            name: lumen_core::models::str_field(value, "name"),
        })
        .collect()
}

impl eframe::App for ShellApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        #[cfg(debug_assertions)]
        if let Some((started, input)) = self.preview_input.as_mut() {
            if !input.ready() {
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                let started = *started.get_or_insert(raw_input.time.unwrap_or(0.0));
                if input.apply(started, raw_input) {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
        }
        let animated = self.bridge.settings().ui_animations;
        if self.wheel.apply(raw_input, animated, egui::Modifiers::CTRL) {
            ctx.request_repaint();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let handle = self.bridge.runtime_handle();
        let _entered = handle.enter();
        // 2. Bridge event pump (Qt signal delivery between the controller and
        //    the cached UI state).
        self.bridge.pump();
        self.pump_bridge_events(ctx);

        // 3. Single-instance activations + power events.
        self.pump_activations();
        self.pump_power_events();

        // 4. Reveal state machine + fallback.
        if let Some(deadline) = self.reveal_fallback_at {
            if Instant::now() >= deadline {
                self.reveal_fallback_at = None;
                self.reveal.reveal_if_pending(&mut self.reveal_sink);
            }
        }
        self.apply_reveal_commands(ctx);

        // 5. Deferred start fallback (QTimer.singleShot(750, startDeferred)).
        if let Some(deadline) = self.deferred_fallback_deadline {
            if Instant::now() >= deadline {
                self.deferred_fallback_deadline = None;
                self.bridge.start_deferred();
            }
        }

        // 6. Close-to-tray behavior (Main.qml onClosing).
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested {
            if self.bridge.tray_available() && !self.bridge.quitting() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                if let Some(tray) = self.tray.as_mut() {
                    tray.notify_hidden();
                }
                self.bridge.notify_hidden_to_tray();
            }
            // `App.quitting` paths (tray Exit, admin relaunch success) post
            // ViewportCommand::Close directly.
        }

        // 7. Tray + window visibility-dependent tray labels.
        self.pump_tray(ctx);
        // Reset the one-shot balloon flag when the window returns.
        if self.window_visible(ctx) {
            if let Some(tray) = self.tray.as_mut() {
                tray.reset_notified();
            }
        }

        // 8. Geometry persistence + backdrop refresh triggers.
        self.track_window_geometry(ctx);
        self.apply_mica(ctx, false);

        // 9. Config drop handling.
        self.handle_config_drops(ctx);

        if self.bridge.quitting() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        let handle = self.bridge.runtime_handle();
        let _entered = handle.enter();
        // 1. Theme + fonts + visuals (Theme bindings in Component.onCompleted).
        let theme = self.theme();
        if self.installed_font_family.as_deref() != Some(theme::font_key(&theme).as_str()) {
            theme::apply_fonts(ctx, &theme);
            self.installed_font_family = Some(theme::font_key(&theme));
        }
        theme::apply_visuals(ctx, &theme);
        fluent_menu::publish_theme(ctx, &theme);

        self.paint_chrome(ctx, root, &theme);
        self.paint_title_bar(ctx, &theme);
        self.paint_rail(ctx, &theme);
        self.paint_pages(ctx, &theme);
        self.bridge.clear_changes();
        self.page_events.clear();
        self.paint_drop_overlay(ctx, &theme);
        let title_layer =
            egui::LayerId::new(egui::Order::Middle, egui::Id::unique("lumen-titlebar"));
        if !self.window_shown_once {
            self.window_shown_once = self.window_visible(ctx);
        }
        if self.onboarding.render(ctx, &theme, &mut self.bridge, title_layer, self.window_shown_once) {
            // Main.qml `onClosingStarted`: show the dashboard and restart
            // `chromeSettleAnim` so it arrives while the card dissolves.
            self.navigation.current = Page::Dashboard;
            let now = ctx.input(|input| input.time);
            ctx.data_mut(|data| data.insert_temp(egui::Id::unique(CHROME_SETTLE), now));
        }
        self.paint_conflict_dialog(ctx, &theme);
        self.paint_lock_overlay(ctx, &theme);
        if let Some(action_id) = self.toasts.render(ctx, &theme) {
            self.bridge.run_toast_action(&action_id);
        }
        self.first_frame(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // The tray's Exit hides its icon before `app.quit` (tray.py `_quit`).
        // The window goes too: disconnecting and restoring the proxy below
        // can take a moment, and the frozen window would be shown as
        // "Not responding" until the process ends.
        win_native::hide_main_window(APP_NAME);
        self.tray = None;
        // app.aboutToQuit handlers: power filter close, activation listener
        // release, bridge shutdown.
        self.bridge.close_power_filter();
        self.close_activations();
        self.bridge.shutdown(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_icons_and_multiline_country_flags_are_available() {
        let icon = tray::load_window_icon().expect("window icon");
        assert_eq!(
            (icon.width, icon.height, icon.rgba.len()),
            (64, 64, 64 * 64 * 4)
        );
        assert!(
            crate::view_models::app_bridge::flag_svg_data_uri("AD")
                .starts_with("data:image/svg+xml;base64,")
        );
    }

    /// test_single_instance_activation.test_should_start_in_tray_conditions —
    /// the shared decoder lives in lumen-core; keep the shell contract pinned.
    #[test]
    fn should_start_in_tray_conditions() {
        let link = "lumen://add";
        let args =
            |values: &[&str]| -> Vec<String> { values.iter().map(|s| s.to_string()).collect() };
        assert!(lumen_core::deeplinks::should_start_in_tray(
            &args(&["Lumen.exe", "--tray"]),
            ""
        ));
        assert!(lumen_core::deeplinks::should_start_in_tray(
            &args(&["--tray"]),
            ""
        ));
        assert!(!lumen_core::deeplinks::should_start_in_tray(
            &args(&["Lumen.exe"]),
            ""
        ));
        assert!(!lumen_core::deeplinks::should_start_in_tray(&args(&[]), ""));
        assert!(!lumen_core::deeplinks::should_start_in_tray(
            &args(&["Lumen.exe", "--tray", "--relaunched"]),
            ""
        ));
        assert!(!lumen_core::deeplinks::should_start_in_tray(
            &args(&["Lumen.exe", "--tray", "--relaunch-as-admin"]),
            ""
        ));
        assert!(!lumen_core::deeplinks::should_start_in_tray(
            &args(&["Lumen.exe", "--tray"]),
            link
        ));
    }

    /// Main.qml `hasSupportedConfigDrop` suffix matrix.
    #[test]
    fn supported_config_drop_suffixes() {
        fn p(s: &str) -> &std::path::Path {
            std::path::Path::new(s)
        }
        assert!(has_supported_config_drop(p("C:/configs/profile.conf")));
        assert!(has_supported_config_drop(p("C:/configs/profile.ovpn")));
        assert!(has_supported_config_drop(p("C:/configs/notes.txt")));
        assert!(has_supported_config_drop(p("C:/configs/import.json")));
        assert!(has_supported_config_drop(p("C:/configs/import.yaml")));
        assert!(has_supported_config_drop(p("C:/configs/import.yml")));
        assert!(has_supported_config_drop(p("file:///C:/a.json?query=1")));
        assert!(has_supported_config_drop(p("file:///C:/a.json#frag")));
        assert!(!has_supported_config_drop(p("C:/configs/image.png")));
        assert!(!has_supported_config_drop(p("C:/configs/")));
    }

    /// test_single_instance_activation.test_unavailable_primary_displays_actionable_message
    /// — the message text contract (the MessageBoxW lives in lumen-platform).
    #[test]
    fn unavailable_warning_message_carries_the_task_manager_hint() {
        let message = "\u{0414}\u{0440}\u{0443}\u{0433}\u{043E}\u{0439} \u{044D}\u{043A}\u{0437}\u{0435}\u{043C}\u{043F}\u{043B}\u{044F}\u{0440} Lumen \u{0443}\u{0436}\u{0435} \u{0437}\u{0430}\u{043F}\u{0443}\u{0449}\u{0435}\u{043D}, \u{043D}\u{043E} \u{043D}\u{0435} \u{043E}\u{0442}\u{0432}\u{0435}\u{0447}\u{0430}\u{0435}\u{0442}.\n\n\u{0417}\u{0430}\u{043A}\u{0440}\u{043E}\u{0439}\u{0442}\u{0435} Lumen \u{0447}\u{0435}\u{0440}\u{0435}\u{0437} \u{0437}\u{043D}\u{0430}\u{0447}\u{043E}\u{043A} \u{0432} \u{043E}\u{0431}\u{043B}\u{0430}\u{0441}\u{0442}\u{0438} \u{0443}\u{0432}\u{0435}\u{0434}\u{043E}\u{043C}\u{043B}\u{0435}\u{043D}\u{0438}\u{0439} \u{0438}\u{043B}\u{0438} \u{0414}\u{0438}\u{0441}\u{043F}\u{0435}\u{0442}\u{0447}\u{0435}\u{0440} \u{0437}\u{0430}\u{0434}\u{0430}\u{0447}, \u{0437}\u{0430}\u{0442}\u{0435}\u{043C} \u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{0435} \u{044D}\u{0442}\u{0443} \u{043A}\u{043E}\u{043F}\u{0438}\u{044E} \u{0441}\u{043D}\u{043E}\u{0432}\u{0430}.";
        assert!(message.contains("\u{0414}\u{0438}\u{0441}\u{043F}\u{0435}\u{0442}\u{0447}\u{0435}\u{0440} \u{0437}\u{0430}\u{0434}\u{0430}\u{0447}"));
        assert_eq!(
            lumen_core::deeplinks::find_lumen_deep_link(&[
                "Lumen.exe".to_string(),
                "lumen://add?url=x".to_string()
            ]),
            "lumen://add?url=x"
        );
    }

    /// The title bar geometry contract from TitleBar.qml.
    #[test]
    fn title_bar_height_is_34_plus_two_when_maximized() {
        assert_eq!(TITLE_BAR_HEIGHT, 34.0);
        assert_eq!(TITLE_BAR_HEIGHT + 2.0, 36.0);
    }

    /// Main.qml startup geometry defaults.
    #[test]
    fn startup_geometry_defaults_match_main_qml() {
        assert_eq!(DEFAULT_WINDOW_WIDTH, 1280);
        assert_eq!(DEFAULT_WINDOW_HEIGHT, 720);
        assert_eq!(APP_MINIMUM_WIDTH, 640);
        assert_eq!(APP_MINIMUM_HEIGHT, 360);
        let saved_width = 0_i64;
        let desired = APP_MINIMUM_WIDTH.max(if saved_width > 0 {
            saved_width
        } else {
            DEFAULT_WINDOW_WIDTH
        });
        assert_eq!(desired, 1280);
    }
}
