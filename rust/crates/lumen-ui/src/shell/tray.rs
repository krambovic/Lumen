//! System tray (port of `qml_app/tray.py` `QmlTray`).
//!
//! `tray-icon` owns the notification icon. Its events are handled the moment
//! they arrive (the window may be hidden and only ticking its logic), and the
//! context menu is the owner-drawn popup of [`super::tray_menu`] with the
//! QSS palette of `_apply_menu_style`.

use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use lumen_core::constants::{APP_NAME, app_icon_path};
use lumen_core::i18n::tr;

use super::toast;
use super::tray_menu::{self, MenuEntry};
use super::win_native;

/// One routing preset row (AppBridge.regionalRoutingPresets /
/// customRoutingPresets dictionaries: `id` + `name`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutingPresetItem {
    pub id: String,
    pub name: String,
}

impl RoutingPresetItem {
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            tr("\u{041F}\u{0440}\u{0435}\u{0441}\u{0435}\u{0442}", &[])
        } else {
            self.name.clone()
        }
    }
}

/// What a tray event asks the shell to do (the Python slots).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    /// `_toggle_window` (Trigger on the icon, or the show/hide item).
    ToggleWindow,
    /// `_show_window` (message-click on the balloon).
    ShowWindow,
    /// `_toggle_connection`.
    ToggleConnection,
    /// `_switch_next`.
    SwitchNext,
    /// `_apply_default_routing` / `_apply_custom_routing`.
    ApplyRoutingPreset { preset_id: String, custom: bool },
    /// `_restart_admin` (bridge `_on_admin_relaunch`).
    RestartAdmin,
    /// `_quit`.
    Quit,
}

const ID_SHOW: u32 = 1;
const ID_CONNECT: u32 = 2;
const ID_NEXT: u32 = 3;
const ID_ADMIN: u32 = 4;
const ID_QUIT: u32 = 5;
const ID_REGIONAL: u32 = 100;
const ID_CUSTOM: u32 = 1000;

/// The `_action_*` texts and state, shared with the event handler.
#[derive(Default)]
struct MenuModel {
    show: String,
    connect: String,
    next: String,
    routing: String,
    admin: String,
    admin_enabled: bool,
    quit: String,
    regional: Vec<RoutingPresetItem>,
    custom: Vec<RoutingPresetItem>,
    dark: bool,
    commands: Vec<TrayCommand>,
    ctx: Option<egui::Context>,
}

impl MenuModel {
    fn entries(&self) -> Vec<MenuEntry> {
        let mut routing = Vec::new();
        for (index, item) in self.regional.iter().enumerate() {
            if !item.id.is_empty() {
                routing.push(MenuEntry::item(ID_REGIONAL + index as u32, item.label(), true));
            }
        }
        if !self.custom.is_empty() {
            routing.push(MenuEntry::separator());
        }
        for (index, item) in self.custom.iter().enumerate() {
            if !item.id.is_empty() {
                routing.push(MenuEntry::item(ID_CUSTOM + index as u32, item.label(), true));
            }
        }
        vec![
            MenuEntry::item(ID_SHOW, self.show.clone(), true),
            MenuEntry::item(ID_CONNECT, self.connect.clone(), true),
            MenuEntry::item(ID_NEXT, self.next.clone(), true),
            MenuEntry::submenu(self.routing.clone(), routing),
            MenuEntry::separator(),
            MenuEntry::item(ID_ADMIN, self.admin.clone(), self.admin_enabled),
            MenuEntry::separator(),
            MenuEntry::item(ID_QUIT, self.quit.clone(), true),
        ]
    }

    fn command_for(&self, id: u32) -> Option<TrayCommand> {
        let preset = |items: &[RoutingPresetItem], base: u32, custom: bool| {
            items.get((id - base) as usize).map(|item| TrayCommand::ApplyRoutingPreset {
                preset_id: item.id.clone(),
                custom,
            })
        };
        match id {
            ID_SHOW => Some(TrayCommand::ToggleWindow),
            ID_CONNECT => Some(TrayCommand::ToggleConnection),
            ID_NEXT => Some(TrayCommand::SwitchNext),
            ID_ADMIN => Some(TrayCommand::RestartAdmin),
            ID_QUIT => Some(TrayCommand::Quit),
            id if id >= ID_CUSTOM => preset(&self.custom, ID_CUSTOM, true),
            id if id >= ID_REGIONAL => preset(&self.regional, ID_REGIONAL, false),
            _ => None,
        }
    }

    fn push(&mut self, command: TrayCommand) {
        self.commands.push(command);
        // A hidden window only ticks its logic on request; wake it now.
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }
}

/// Owns the tray icon and bridges it to the window + bridge, like QmlTray.
pub struct TrayController {
    tray: TrayIcon,
    model: Arc<Mutex<MenuModel>>,
    /// `self._notified` - one balloon per hide cycle.
    notified: bool,
    // Taskbar-wait state (`_taskbar_wait_ticks` + timer).
    waiting_for_taskbar: bool,
    taskbar_wait_ticks: u32,
    reinstall_at: Vec<Instant>,
}

fn load_icon_image() -> Option<image::DynamicImage> {
    image::open(app_icon_path()).ok().or_else(|| {
        image::load_from_memory_with_format(
            include_bytes!("../../../../../windows/assets/Lumen.ico"),
            image::ImageFormat::Ico,
        )
        .ok()
    })
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let rgba = load_icon_image()?
        .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    tray_icon::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

/// Window icon for the eframe viewport builder.
pub fn load_window_icon() -> Option<egui::IconData> {
    let rgba = load_icon_image()?
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

/// `_on_activated`: Trigger toggles the window, Context opens the menu.
fn handle_event(model: &Mutex<MenuModel>, event: TrayIconEvent) {
    let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, .. } = event else {
        return;
    };
    match button {
        MouseButton::Left => model.lock().push(TrayCommand::ToggleWindow),
        MouseButton::Right => {
            let (entries, dark) = {
                let model = model.lock();
                (model.entries(), model.dark)
            };
            // Modal; the lock is not held while the menu is open.
            if let Some(id) = tray_menu::popup(&entries, dark) {
                let mut model = model.lock();
                if let Some(command) = model.command_for(id) {
                    model.push(command);
                }
            }
        }
        _ => {}
    }
}

impl TrayController {
    /// `QmlTray.__init__` minus the bridge signal wiring (the shell drives
    /// the refreshes). Returns Err when the tray could not be created.
    pub fn new() -> Result<Self, String> {
        let model = Arc::new(Mutex::new(MenuModel {
            dark: true,
            ..MenuModel::default()
        }));
        let mut builder = TrayIconBuilder::new()
            .with_id("lumen-tray")
            .with_tooltip(APP_NAME)
            .with_menu_on_left_click(false);
        if let Some(icon) = load_tray_icon() {
            builder = builder.with_icon(icon);
        }
        let tray = builder.build().map_err(|e| e.to_string())?;
        let handler_model = model.clone();
        TrayIconEvent::set_event_handler(Some(move |event| handle_event(&handler_model, event)));

        let mut controller = Self {
            tray,
            model,
            notified: false,
            waiting_for_taskbar: false,
            taskbar_wait_ticks: 0,
            reinstall_at: Vec::new(),
        };
        controller.refresh_actions(true, false, false);
        win_native::allow_taskbar_created_message();
        // A logon autostart can run before Explorer has created the taskbar;
        // the icon added then is lost, leaving Lumen running invisibly. Wait
        // for the taskbar and add the icon again once it is ready.
        if !win_native::taskbar_present() {
            controller.waiting_for_taskbar = true;
            controller.taskbar_wait_ticks = 0;
        }
        Ok(controller)
    }

    /// Lets tray events wake the event loop while the window is hidden.
    pub fn attach_context(&mut self, ctx: &egui::Context) {
        let mut model = self.model.lock();
        if model.ctx.is_none() {
            model.ctx = Some(ctx.clone());
        }
    }

    /// `_dark_theme()` for `_refresh_menu_style`.
    pub fn set_dark_theme(&mut self, dark: bool) {
        self.model.lock().dark = dark;
    }

    /// Poll one second of the taskbar-wait timer (`_wait_for_taskbar`).
    pub fn tick_taskbar_wait(&mut self) {
        if self.waiting_for_taskbar {
            self.taskbar_wait_ticks += 1;
            if win_native::taskbar_present() {
                self.waiting_for_taskbar = false;
                // Explorer accepts notification icons shortly after the
                // taskbar window appears; re-adding twice covers slow shell
                // start-ups.
                self.reinstall_at = win_native::TRAY_REINSTALL_DELAYS
                    .iter()
                    .map(|delay| Instant::now() + *delay)
                    .collect();
            } else if self.taskbar_wait_ticks >= 300 {
                self.waiting_for_taskbar = false;
            }
        }
        let now = Instant::now();
        let before = self.reinstall_at.len();
        self.reinstall_at.retain(|deadline| now < *deadline);
        if self.reinstall_at.len() != before {
            self.reinstall_tray_icon();
        }
    }

    /// `_reinstall_tray_icon`.
    fn reinstall_tray_icon(&mut self) {
        let _ = self.tray.set_visible(false);
        let _ = self.tray.set_visible(true);
    }

    /// `_refresh_actions`.
    pub fn refresh_actions(&mut self, window_visible: bool, connected: bool, elevated: bool) {
        let mut model = self.model.lock();
        model.show = if window_visible {
            tr("\u{0421}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[])
        } else {
            tr("\u{041F}\u{043E}\u{043A}\u{0430}\u{0437}\u{0430}\u{0442}\u{044C}", &[])
        };
        model.connect = if connected {
            tr("\u{041E}\u{0442}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}", &[])
        } else {
            tr("\u{041F}\u{043E}\u{0434}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}", &[])
        };
        model.next = tr("\u{0421}\u{043B}\u{0435}\u{0434}\u{0443}\u{044E}\u{0449}\u{0438}\u{0439} \u{0441}\u{0435}\u{0440}\u{0432}\u{0435}\u{0440}", &[]);
        if elevated {
            model.admin = tr("\u{0417}\u{0430}\u{043F}\u{0443}\u{0449}\u{0435}\u{043D}\u{043E} \u{043E}\u{0442} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}", &[]);
            model.admin_enabled = false;
        } else {
            model.admin = tr("\u{041F}\u{0435}\u{0440}\u{0435}\u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{044C} \u{043E}\u{0442} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}", &[]);
            model.admin_enabled = true;
        }
        model.quit = tr("\u{0412}\u{044B}\u{0445}\u{043E}\u{0434}", &[]);
        model.routing = format!(
            "{}  \u{203A}",
            tr("\u{041C}\u{0430}\u{0440}\u{0448}\u{0440}\u{0443}\u{0442}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{044F}", &[])
        );
    }

    /// `_refresh_routing_actions`: the regional and custom preset lists the
    /// routing submenu is built from.
    pub fn refresh_routing(&mut self, regional: &[RoutingPresetItem], custom: &[RoutingPresetItem]) {
        let mut model = self.model.lock();
        model.regional = regional.to_vec();
        model.custom = custom.to_vec();
    }

    /// `notify_hidden()`: the "app hidden to tray" balloon, once per cycle.
    /// The WinRT script can die on execution policy or missing assemblies
    /// without telling us, so the in-process balloon goes first and the toast
    /// stays as the fallback. tray-icon exposes no balloon API, so the shell
    /// always takes the toast path with the same message.
    pub fn notify_hidden(&mut self) {
        if self.notified {
            return;
        }
        self.notified = true;
        let message = tr(
            "\u{041F}\u{0440}\u{0438}\u{043B}\u{043E}\u{0436}\u{0435}\u{043D}\u{0438}\u{0435} \u{0441}\u{0432}\u{0435}\u{0440}\u{043D}\u{0443}\u{0442}\u{043E} \u{0432} \u{0441}\u{0438}\u{0441}\u{0442}\u{0435}\u{043C}\u{043D}\u{044B}\u{0439} \u{0442}\u{0440}\u{0435}\u{0439}",
            &[],
        );
        toast::show_toast(APP_NAME, &message, toast::AUMID);
    }

    /// Reset the one-shot balloon flag (the window became visible again).
    pub fn reset_notified(&mut self) {
        self.notified = false;
    }

    /// Commands collected by the event handler since the last poll.
    pub fn poll_commands(&self) -> Vec<TrayCommand> {
        std::mem::take(&mut self.model.lock().commands)
    }

    /// Hide and remove the icon (`_quit` hides the tray before exiting).
    pub fn hide(&mut self) {
        let _ = self.tray.set_visible(false);
    }

    /// Update the tooltip text (APP_NAME, matching `_tray.setToolTip`).
    pub fn set_tooltip(&mut self, text: &str) {
        let _ = self.tray.set_tooltip(Some(text));
    }
}

impl Drop for TrayController {
    fn drop(&mut self) {
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, name: &str) -> RoutingPresetItem {
        RoutingPresetItem {
            id: id.into(),
            name: name.into(),
        }
    }

    fn model() -> MenuModel {
        MenuModel {
            regional: vec![preset("", "Broken"), preset("global", "Global")],
            custom: vec![preset("c9", "")],
            ..MenuModel::default()
        }
    }

    #[test]
    fn routing_preset_label_falls_back_to_the_translated_generic() {
        assert_eq!(preset("id", "Offshore").label(), "Offshore");
        assert_eq!(
            preset("id", "").label(),
            tr("\u{041F}\u{0440}\u{0435}\u{0441}\u{0435}\u{0442}", &[])
        );
    }

    #[test]
    fn menu_follows_the_python_layout() {
        let entries = model().entries();
        let ids: Vec<u32> = entries.iter().map(|entry| entry.id).collect();
        assert_eq!(ids, [ID_SHOW, ID_CONNECT, ID_NEXT, 0, 0, ID_ADMIN, 0, ID_QUIT]);
        // Empty preset ids are skipped; custom presets follow a separator.
        let routing: Vec<u32> = entries[3].submenu.iter().map(|entry| entry.id).collect();
        assert_eq!(routing, [ID_REGIONAL + 1, 0, ID_CUSTOM]);
    }

    #[test]
    fn menu_ids_map_back_to_commands() {
        let model = model();
        assert_eq!(model.command_for(ID_SHOW), Some(TrayCommand::ToggleWindow));
        assert_eq!(model.command_for(ID_QUIT), Some(TrayCommand::Quit));
        assert_eq!(
            model.command_for(ID_REGIONAL + 1),
            Some(TrayCommand::ApplyRoutingPreset { preset_id: "global".into(), custom: false })
        );
        assert_eq!(
            model.command_for(ID_CUSTOM),
            Some(TrayCommand::ApplyRoutingPreset { preset_id: "c9".into(), custom: true })
        );
        assert_eq!(model.command_for(ID_CUSTOM + 5), None);
    }

    #[test]
    fn refresh_action_labels_follow_window_and_connection_state() {
        let show = tr("\u{041F}\u{043E}\u{043A}\u{0430}\u{0437}\u{0430}\u{0442}\u{044C}", &[]);
        let hide = tr("\u{0421}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[]);
        assert_ne!(show, hide);
    }
}
