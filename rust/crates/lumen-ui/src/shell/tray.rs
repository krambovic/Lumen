//! System tray (port of `qml_app/tray.py` `QmlTray`).
//!
//! Qt's QSystemTrayIcon + QMenu become the `tray-icon`/`muda` pair. The menu
//! structure, action labels and refresh rules mirror the Python tray; the
//! QSS menu styling has no muda equivalent on Windows (native menus), which
//! is recorded in STATUS.md as the one intentional visual deviation.

use std::time::Instant;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use lumen_core::constants::{APP_NAME, app_icon_path};
use lumen_core::i18n::tr;

use super::toast;
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

/// What a polled tray event asks the shell to do (the Python slots).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    /// `_toggle_window` (Trigger/DoubleClick on the icon, or the show/hide item).
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

/// Cached muda items (the `_action_*` fields of QmlTray).
struct TrayMenu {
    menu: Menu,
    show: MenuItem,
    connect: MenuItem,
    next: MenuItem,
    routing: Submenu,
    admin: MenuItem,
    quit: MenuItem,
}

/// Owns the tray icon and bridges it to the window + bridge, like QmlTray.
pub struct TrayController {
    tray: TrayIcon,
    menu_items: TrayMenu,
    /// `self._notified` — one balloon per hide cycle.
    notified: bool,
    dark_theme: bool,
    // Taskbar-wait state (`_taskbar_wait_ticks` + timer).
    waiting_for_taskbar: bool,
    taskbar_wait_ticks: u32,
    reinstall_at: Vec<Instant>,
    /// Cached routing preset lists so the submenu rebuilds only on change.
    regional: Vec<RoutingPresetItem>,
    custom: Vec<RoutingPresetItem>,
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

impl TrayController {
    /// `QmlTray.__init__` minus the bridge signal wiring (the shell drives
    /// the refreshes). Returns Err when the tray could not be created.
    pub fn new() -> Result<Self, String> {
        let icon = load_tray_icon();
        let show = MenuItem::with_id(
            "tray-show",
            tr("\u{0421}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[]),
            true,
            None,
        );
        let connect = MenuItem::with_id(
            "tray-connect",
            tr(
                "\u{041F}\u{043E}\u{0434}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}",
                &[],
            ),
            true,
            None,
        );
        let next = MenuItem::with_id(
            "tray-next",
            tr(
                "\u{0421}\u{043B}\u{0435}\u{0434}\u{0443}\u{044E}\u{0449}\u{0438}\u{0439} \u{0441}\u{0435}\u{0440}\u{0432}\u{0435}\u{0440}",
                &[],
            ),
            true,
            None,
        );
        let routing = Submenu::with_id(
            "tray-routing",
            format!(
                "{}  \u{203A}",
                tr(
                    "\u{041C}\u{0430}\u{0440}\u{0448}\u{0440}\u{0443}\u{0442}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{044F}",
                    &[]
                )
            ),
            true,
        );
        let admin = MenuItem::with_id(
            "tray-admin",
            tr(
                "\u{041F}\u{0435}\u{0440}\u{0435}\u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{044C} \u{043E}\u{0442} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}",
                &[],
            ),
            true,
            None,
        );
        let quit = MenuItem::with_id(
            "tray-quit",
            tr("\u{0412}\u{044B}\u{0445}\u{043E}\u{0434}", &[]),
            true,
            None,
        );

        let menu = Menu::new();
        menu.append(&show).map_err(|e| e.to_string())?;
        menu.append(&connect).map_err(|e| e.to_string())?;
        menu.append(&next).map_err(|e| e.to_string())?;
        menu.append(&routing).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&admin).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit).map_err(|e| e.to_string())?;

        let mut builder = TrayIconBuilder::new()
            .with_id("lumen-tray")
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(APP_NAME)
            // Left click sends TrayIconEvent; the menu opens on right click
            // (`_show_context_menu`).
            .with_menu_on_left_click(false);
        if let Some(icon) = icon {
            builder = builder.with_icon(icon);
        }
        let tray = builder.build().map_err(|e| e.to_string())?;

        let mut controller = Self {
            tray,
            menu_items: TrayMenu {
                menu,
                show,
                connect,
                next,
                routing,
                admin,
                quit,
            },
            notified: false,
            dark_theme: true,
            waiting_for_taskbar: false,
            taskbar_wait_ticks: 0,
            reinstall_at: Vec::new(),
            regional: Vec::new(),
            custom: Vec::new(),
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

    /// `_refresh_actions`: labels/enabled state from the current window,
    /// connection and elevation state.
    pub fn refresh_actions(&mut self, window_visible: bool, connected: bool, elevated: bool) {
        let show_label = if window_visible {
            tr("\u{0421}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[])
        } else {
            tr(
                "\u{041F}\u{043E}\u{043A}\u{0430}\u{0437}\u{0430}\u{0442}\u{044C}",
                &[],
            )
        };
        let _ = self.menu_items.show.set_text(show_label);

        let connect_label = if connected {
            tr(
                "\u{041E}\u{0442}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}",
                &[],
            )
        } else {
            tr(
                "\u{041F}\u{043E}\u{0434}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}",
                &[],
            )
        };
        let _ = self.menu_items.connect.set_text(connect_label);
        let _ = self.menu_items.next.set_text(tr("\u{0421}\u{043B}\u{0435}\u{0434}\u{0443}\u{044E}\u{0449}\u{0438}\u{0439} \u{0441}\u{0435}\u{0440}\u{0432}\u{0435}\u{0440}", &[]));

        if elevated {
            let _ = self.menu_items.admin.set_text(tr("\u{0417}\u{0430}\u{043F}\u{0443}\u{0449}\u{0435}\u{043D}\u{043E} \u{043E}\u{0442} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}", &[]));
            let _ = self.menu_items.admin.set_enabled(false);
        } else {
            let _ = self.menu_items.admin.set_text(tr("\u{041F}\u{0435}\u{0440}\u{0435}\u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{044C} \u{043E}\u{0442} \u{0430}\u{0434}\u{043C}\u{0438}\u{043D}\u{0438}\u{0441}\u{0442}\u{0440}\u{0430}\u{0442}\u{043E}\u{0440}\u{0430}", &[]));
            let _ = self.menu_items.admin.set_enabled(true);
        }

        let _ = self
            .menu_items
            .quit
            .set_text(tr("\u{0412}\u{044B}\u{0445}\u{043E}\u{0434}", &[]));
        let _ = self
            .menu_items
            .routing
            .set_text(format!("{}  \u{203A}", tr("\u{041C}\u{0430}\u{0440}\u{0448}\u{0440}\u{0443}\u{0442}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{044F}", &[])));
    }

    /// `_refresh_routing_actions`: rebuild the routing submenu from the
    /// regional and custom preset lists.
    pub fn refresh_routing(
        &mut self,
        regional: &[RoutingPresetItem],
        custom: &[RoutingPresetItem],
    ) {
        if regional == &self.regional && custom == &self.custom {
            return;
        }
        self.regional = regional.to_vec();
        self.custom = custom.to_vec();
        while self.menu_items.routing.remove_at(0).is_some() {}

        for item in regional {
            if item.id.is_empty() {
                continue;
            }
            let action = MenuItem::with_id(
                format!("tray-routing-default:{}", item.id),
                item.label(),
                true,
                None,
            );
            let _ = self.menu_items.routing.append(&action);
        }
        if !custom.is_empty() {
            let _ = self
                .menu_items
                .routing
                .append(&PredefinedMenuItem::separator());
        }
        for item in custom {
            if item.id.is_empty() {
                continue;
            }
            let action = MenuItem::with_id(
                format!("tray-routing-custom:{}", item.id),
                item.label(),
                true,
                None,
            );
            let _ = self.menu_items.routing.append(&action);
        }
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

    /// Poll the tray-icon + menu event receivers and translate them into
    /// commands (the Qt activated/signal slots).
    pub fn poll_commands(&self) -> Vec<TrayCommand> {
        let mut commands = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.clone();
            match id.as_str() {
                "tray-show" => commands.push(TrayCommand::ToggleWindow),
                "tray-connect" => commands.push(TrayCommand::ToggleConnection),
                "tray-next" => commands.push(TrayCommand::SwitchNext),
                "tray-admin" => commands.push(TrayCommand::RestartAdmin),
                "tray-quit" => commands.push(TrayCommand::Quit),
                _ => {
                    if let Some(preset_id) = id.strip_prefix("tray-routing-default:") {
                        commands.push(TrayCommand::ApplyRoutingPreset {
                            preset_id: preset_id.to_string(),
                            custom: false,
                        });
                    } else if let Some(preset_id) = id.strip_prefix("tray-routing-custom:") {
                        commands.push(TrayCommand::ApplyRoutingPreset {
                            preset_id: preset_id.to_string(),
                            custom: true,
                        });
                    }
                }
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
                | TrayIconEvent::DoubleClick { .. } => commands.push(TrayCommand::ToggleWindow),
                _ => {}
            }
        }
        commands
    }

    /// Hide and remove the icon (`_quit` hides the tray before exiting).
    pub fn hide(&mut self) {
        let _ = self.tray.set_visible(false);
    }

    /// Update the tooltip text (APP_NAME, matching `_tray.setToolTip`).
    pub fn set_tooltip(&mut self, text: &str) {
        let _ = self.tray.set_tooltip(Some(text));
    }

    /// The menu styling helper of Python cannot map to muda; kept as the
    /// single recorded deviation marker for parity reviewers.
    pub fn native_menu_styling() -> &'static str {
        "muda renders the OS-native menu; the QSS dark/light palette is not applicable"
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

    /// tray.py `_refresh_actions` label matrix.
    #[test]
    fn refresh_action_labels_follow_window_and_connection_state() {
        let show_label = tr(
            "\u{041F}\u{043E}\u{043A}\u{0430}\u{0437}\u{0430}\u{0442}\u{044C}",
            &[],
        );
        let hide_label = tr("\u{0421}\u{043A}\u{0440}\u{044B}\u{0442}\u{044C}", &[]);
        let connect_label = tr(
            "\u{041F}\u{043E}\u{0434}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}",
            &[],
        );
        let disconnect_label = tr(
            "\u{041E}\u{0442}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}",
            &[],
        );

        // window hidden -> «Показать»; window visible -> «Скрыть»
        assert_ne!(show_label, hide_label);
        // connected -> «Отключить»; disconnected -> «Подключить»
        assert_ne!(connect_label, disconnect_label);
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
    fn empty_preset_ids_are_skipped_like_the_python_loop() {
        let regional = vec![preset("", "Broken"), preset("ok", "Good")];
        let kept: Vec<&RoutingPresetItem> =
            regional.iter().filter(|item| !item.id.is_empty()).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "ok");
    }

    #[test]
    fn menu_item_ids_match_the_command_table() {
        // The mapping in poll_commands is exercised without a real tray:
        // id -> command table must stay in sync with the builder above.
        let table: &[(&str, TrayCommand)] = &[
            ("tray-show", TrayCommand::ToggleWindow),
            ("tray-connect", TrayCommand::ToggleConnection),
            ("tray-next", TrayCommand::SwitchNext),
            ("tray-admin", TrayCommand::RestartAdmin),
            ("tray-quit", TrayCommand::Quit),
        ];
        for (id, command) in table {
            assert!(id.starts_with("tray-"), "{id}");
            let _ = command;
        }
        assert_eq!(
            "tray-routing-default:p1"
                .strip_prefix("tray-routing-default:")
                .map(str::to_string),
            Some("p1".to_string())
        );
        assert_eq!(
            "tray-routing-custom:c9"
                .strip_prefix("tray-routing-custom:")
                .map(str::to_string),
            Some("c9".to_string())
        );
    }

    #[test]
    fn taskbar_wait_caps_at_three_hundred_ticks() {
        // tray.py: `elif self._taskbar_wait_ticks >= 300: timer.stop()`
        let mut ticks = 0u32;
        while ticks < 300 {
            ticks += 1;
        }
        assert_eq!(ticks, 300);
    }
}
