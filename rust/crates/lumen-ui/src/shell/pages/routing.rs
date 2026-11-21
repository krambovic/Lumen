//! Routing presets, process rules, service overrides and domain rules.
//!
//! Mirrors `RoutingPage.qml`: the behaviour card (presets grid, bypass switch,
//! default TUN outbound, Discord Voice), applications table, services card,
//! domain/IP rules and the Fluent dialogs behind them.

#[allow(unused_imports)]
use crate::shell::wheel::GlideRoom;
use crate::shell::widgets::FluentTip;
use std::path::Path;

use egui::{
    Align, Color32, Frame, Layout, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Ui, vec2,
};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::{PageContext, fluent_button, text_edit_menu};
use crate::shell::{theme::Theme, win_native};
use crate::view_models::AppBridge;

const ACTIONS: &[(&str, &str)] = &[
    ("direct", "Прямой"),
    ("proxy", "Прокси"),
    ("block", "Блокировка"),
];
const DEFAULT_ROUTES: &[(&str, &str)] = &[("proxy", "Прокси"), ("direct", "Напрямую")];

const HELP: &str = "Быстрые пресеты зависят от регионального профиля в настройках сети. Пресет «Всё кроме региона» пускает ресурсы выбранной страны напрямую, а остальной трафик — через прокси.\n\nСервисы:\n«Прокси» — всегда вести сервис через VPN/прокси.\n«Прямой» — всегда вести сервис напрямую.\n\nПравила по приложениям работают только в режиме TUN.";

const PROXY_RULES_WARNING: &str = "В режиме системного прокси правила по приложениям не работают — включите TUN, чтобы перехватывать трафик приложений.";
const TUN_DEFAULT_HINT: &str = "Маршрут для трафика, который не совпал ни с одним правилом.";
const DISCORD_HINT: &str = "Голос и стримы Discord через SOCKS5 без TUN";
const DISCORD_TUN_HINT: &str = "Недоступно при включенном TUN — трафик Discord уже идет через VPN";
const SERVICES_HINT: &str = "Быстрые переключатели для популярных сервисов (YouTube, Discord и др.). Выберите прямое подключение или прокси.";

/// QML `GridLayout columns: page.width < 850 ? 1 : 3`; the page viewport sits
/// inside the rail and 24pt margins, so the same breakpoint lands here.
const PRESET_GRID_MIN_WIDTH: f32 = 780.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    Help,
    Save,
    Process,
    Folder,
    Running,
    Domain,
    Import,
}

impl Dialog {
    fn title(self) -> &'static str {
        match self {
            Self::Help => "Как работает маршрутизация",
            Self::Save => "Сохранить пресет маршрутизации",
            Self::Process => "Добавить приложение",
            Self::Folder => "Добавить папку",
            Self::Running => "Запущенные приложения",
            Self::Domain => "Добавить домен или IP",
            Self::Import => "Импорт правил",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Help => HELP,
            Self::Save => "Текущие правила маршрутизации будут сохранены как именованный пресет.",
            Self::Process => "Укажите имя процесса (например, chrome.exe) или полный путь к .exe.",
            Self::Folder => {
                "Укажите путь к папке. Все процессы внутри будут маршрутизироваться по выбранному действию."
            }
            Self::Running => "Выберите запущенное приложение и действие маршрутизации.",
            Self::Domain => "Домен (example.com), маска (*.example.com), IP или CIDR (10.0.0.0/8).",
            Self::Import => {
                "Одно правило в строке, формат: адрес|действие (direct/proxy/block). Без «|действие» применяется proxy."
            }
        }
    }

    /// QML dialog `width` values (460/480/520).
    fn width(self) -> f32 {
        match self {
            Self::Help | Self::Save => 460.0,
            Self::Import => 520.0,
            _ => 480.0,
        }
    }

    fn action_index(self) -> Option<usize> {
        match self {
            Self::Process | Self::Folder => Some(0),
            Self::Running => Some(1),
            Self::Domain => Some(2),
            _ => None,
        }
    }
}

pub(super) struct RoutingPage {
    dialog: Option<Dialog>,
    reset_dialog_editor: bool,
    input: String,
    running: Vec<String>,
    actions: [String; 3],
}

impl Default for RoutingPage {
    fn default() -> Self {
        Self {
            dialog: None,
            reset_dialog_editor: false,
            input: String::new(),
            running: Vec::new(),
            actions: std::array::from_fn(|_| "proxy".into()),
        }
    }
}

impl RoutingPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        ScrollArea::vertical()
            .id_salt("routing-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                // Title + ghost help button pinned to the right edge.
                right_row(ui, theme.control_height as f32, |ui| {
                    fluent_button::style(ui);
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e946}",
                            &tr("Справка", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Help);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(
                            RichText::new(tr("Маршрутизация", &[]))
                                .size(theme.font_title as f32)
                                .family(crate::shell::theme::semibold_family()),
                        );
                    });
                });
                self.behavior(ui, theme, bridge);

                section(ui, theme, "Приложения");
                ui.horizontal_wrapped(|ui| {
                    fluent_button::style(ui);
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e8e5}",
                            &tr("Добавить .exe", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Process);
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e8b7}",
                            &tr("Добавить папку", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Folder);
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e7c4}",
                            &tr("Из запущенных", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Running);
                        self.running = bridge.running_processes();
                        self.input = self.running.first().cloned().unwrap_or_default();
                    }
                });
                rules(ui, theme, bridge, true);
                if !bridge.tun_mode() {
                    warning_band(ui, theme);
                }

                services(ui, theme, bridge);
                section(ui, theme, "Домены и IP");
                ui.horizontal_wrapped(|ui| {
                    fluent_button::style(ui);
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e710}",
                            &tr("Добавить", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Domain);
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e896}",
                            &tr("Импорт", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open(Dialog::Import);
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e8e5}",
                            &tr("Из файла", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        import_file(bridge);
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e72d}",
                            &tr("Экспорт", &[]),
                            false,
                        ))
                        .clicked()
                        && let Some(payload) = bridge.export_domain_rules()
                    {
                        ui.ctx().copy_text(payload);
                    }
                });
                rules(ui, theme, bridge, false);
            }).glide_room(ui);
        if let Some(dialog) = self.dialog {
            self.render_dialog(page.ctx, theme, bridge, dialog);
        }
    }

    fn open(&mut self, dialog: Dialog) {
        self.dialog = Some(dialog);
        self.input.clear();
        self.reset_dialog_editor = true;
    }

    fn behavior(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let active = bridge.active_routing_preset_id();
        let custom = bridge.custom_routing_presets();
        card(ui, theme, 16.0, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let built_in = bridge.regional_routing_presets();
            if ui.available_width() >= PRESET_GRID_MIN_WIDTH {
                // GridLayout: two equal preset columns and the icon buttons,
                // 16 px apart; each column is caption + 4 + combo.
                let buttons = 2.0 * theme.control_height as f32 + 8.0;
                let column = ((ui.available_width() - buttons - 32.0) / 2.0).max(0.0);
                let height = 20.0 + theme.control_height as f32;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.allocate_ui_with_layout(
                        vec2(column, height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(column);
                            ui.spacing_mut().item_spacing.y = 4.0;
                            caption(ui, theme, "Быстрые пресеты маршрутизации");
                            if let Some(id) = preset_combo(ui, "built-in", &built_in, &active) {
                                bridge.apply_routing_preset(&id);
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(column, height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(column);
                            ui.spacing_mut().item_spacing.y = 4.0;
                            caption(ui, theme, "Свои пресеты маршрутизации");
                            ui.add_enabled_ui(!custom.is_empty(), |ui| {
                                if let Some(id) = preset_combo(ui, "custom", &custom, &active) {
                                    bridge.apply_custom_routing_preset(&id);
                                }
                            });
                        },
                    );
                    // Icon buttons line up with the combos (QML AlignBottom).
                    ui.allocate_ui_with_layout(
                        vec2(buttons, height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add_space(height - theme.control_height as f32);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if crate::shell::widgets::FluentButton::icon(
                                    "\u{e74e}",
                                    &tr("Сохранить текущие правила как пресет", &[]),
                                )
                                .show(ui, theme)
                                .clicked()
                                {
                                    self.open(Dialog::Save);
                                }
                                let custom_active =
                                    custom.iter().any(|preset| field(preset, "id") == active);
                                if crate::shell::widgets::FluentButton::icon(
                                    "\u{e74d}",
                                    &tr("Удалить выбранный пользовательский пресет", &[]),
                                )
                                .enabled(custom_active)
                                .show(ui, theme)
                                .clicked()
                                {
                                    bridge.delete_routing_preset(&active);
                                }
                            });
                        },
                    );
                });
            } else {
                self.presets_stacked(ui, theme, bridge, &built_in, &custom, &active);
            }
            let mut bypass = bridge.bypass_lan();
            // "Label left, control right": a height-constrained right-to-left
            // row with the label block filling the remainder (QML RowLayout).
            right_row(ui, theme.control_height as f32, |ui| {
                if let Some(value) = switch_widget(ui, theme, bypass, true) {
                    bypass = value;
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(text("Обход локальной сети", theme));
                });
            });
            if bypass != bridge.bypass_lan() {
                bridge.set_bypass_lan(bypass);
            }
            hline(ui, theme, 0.0);
            right_row(ui, 37.0 * (theme.font_scale as f32).max(1.0), |ui| {
                if let Some(action) = action_combo(
                    ui,
                    "default-outbound",
                    &bridge.tun_default_outbound(),
                    DEFAULT_ROUTES,
                    160.0,
                ) {
                    bridge.set_tun_default_outbound(&action);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(text("По умолчанию (TUN)", theme));
                        caption(ui, theme, TUN_DEFAULT_HINT);
                    });
                });
            });
            hline(ui, theme, 0.0);
            let tun = bridge.tun_mode();
            let discord = bridge.discord_proxy();
            ui.add_enabled_ui(!tun, |ui| {
                right_row(ui, 37.0 * (theme.font_scale as f32).max(1.0), |ui| {
                    if let Some(value) = switch_widget(ui, theme, discord, !tun) {
                        bridge.set_discord_proxy(value);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("\u{e767}")
                                    .size(theme.font_normal as f32)
                                    .color(if discord {
                                        theme.accent
                                    } else {
                                        theme.text_muted
                                    }),
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(text("Discord Voice", theme));
                                caption(
                                    ui,
                                    theme,
                                    if tun { DISCORD_TUN_HINT } else { DISCORD_HINT },
                                );
                            });
                        });
                    });
                });
            });
        });
    }

    /// Single-column preset stack for narrow windows; the toolbar row keeps
    /// labelled ghost buttons (QML stacks the same grid into one column).
    fn presets_stacked(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        built_in: &[Value],
        custom: &[Value],
        active: &str,
    ) {
        caption(ui, theme, "Быстрые пресеты маршрутизации");
        // Caption sits 4 px above its combo; the card spacing is 12.
        ui.add_space(4.0 - ui.spacing().item_spacing.y);
        if let Some(id) = preset_combo(ui, "built-in", built_in, active) {
            bridge.apply_routing_preset(&id);
        }
        caption(ui, theme, "Свои пресеты маршрутизации");
        ui.add_space(4.0 - ui.spacing().item_spacing.y);
        ui.add_enabled_ui(!custom.is_empty(), |ui| {
            if let Some(id) = preset_combo(ui, "custom", custom, active) {
                bridge.apply_custom_routing_preset(&id);
            }
        });
        // The single-column grid keeps the icon-only toolbar row (the QML
        // third grid cell simply stacks below the combos).
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if fluent_button::icon(
                ui,
                theme,
                "\u{e74e}",
                &tr("Сохранить текущие правила как пресет", &[]),
                true,
            )
            .clicked()
            {
                self.open(Dialog::Save);
            }
            let custom_active = custom.iter().any(|preset| field(preset, "id") == active);
            if fluent_button::icon(
                ui,
                theme,
                "\u{e74d}",
                &tr("Удалить выбранный пользовательский пресет", &[]),
                custom_active,
            )
            .clicked()
            {
                bridge.delete_routing_preset(active);
            }
        });
    }

    fn render_dialog(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        dialog: Dialog,
    ) {
        use crate::shell::fluent_dialog::{DialogAction, FluentDialog, body_text, body_text_spaced};
        let reset_editor = std::mem::take(&mut self.reset_dialog_editor);
        let small = egui::FontId::proportional(theme.font_small as f32);
        let action = if dialog == Dialog::Help {
            // helpDialog: 460 px, header bottom 6, `padding: 0` (the style's
            // verticalPadding 18 stays), muted text at 1.25 line height with
            // 6 px below it, a single accent "Понятно".
            FluentDialog::new("routing-help", tr(dialog.title(), &[]), dialog.width())
                .ok(tr("Понятно", &[]))
                .no_cancel()
                .padding(egui::Margin { left: 20, right: 20, top: 22, bottom: 18 })
                .show(ctx, theme, |ui| {
                    body_text_spaced(
                        ui,
                        &tr(HELP, &[]),
                        egui::FontId::proportional(theme.font_normal as f32),
                        theme.text_muted,
                        1.25,
                    );
                    ui.add_space(6.0);
                })
        } else {
            let ok_label = match dialog {
                Dialog::Save => "Сохранить",
                Dialog::Import => "Импорт",
                _ => "Добавить",
            };
            let ok_enabled = match dialog {
                Dialog::Save => !self.input.trim().is_empty(),
                Dialog::Running => !self.running.is_empty(),
                _ => true,
            };
            FluentDialog::new(("routing", dialog.title()), tr(dialog.title(), &[]), dialog.width())
                .ok(tr(ok_label, &[]))
                .ok_enabled(ok_enabled)
                .show(ctx, theme, |ui| {
                    // ColumnLayout spacing 10: description, editor, preview, action.
                    body_text(ui, &tr(dialog.description(), &[]), small.clone(), theme.text_muted);
                    ui.add_space(10.0);
                    match dialog {
                        Dialog::Import => {
                            ScrollArea::vertical().max_height(180.0).min_scrolled_height(180.0).show(ui, |ui| {
                                text_edit_menu::show(
                                    ui,
                                    TextEdit::multiline(&mut self.input)
                                        .font(egui::FontId::monospace(theme.font_small as f32))
                                        .background_color(theme.card)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(9)
                                        .hint_text("example.com|proxy\nads.example.net|block\n10.0.0.0/8|direct"),
                                    "import-domain-rules",
                                    true,
                                    reset_editor,
                                )
                            }).glide_room(ui);
                        }
                        Dialog::Running => {
                            if let Some(process) = combo(
                                ui,
                                "running-process",
                                &self.input,
                                self.running.iter().map(|name| (name.as_str(), name.clone())),
                                ui.available_width(),
                            ) {
                                self.input = process;
                            }
                        }
                        _ => {
                            let hint = match dialog {
                                Dialog::Save => tr("Название пресета", &[]),
                                Dialog::Process => "chrome.exe".into(),
                                Dialog::Folder => "C:\\Games\\".into(),
                                _ => "example.com".into(),
                            };
                            let width = ui.available_width();
                            // DnsField: a TextField whose background has no
                            // implicit height, so Universal sizes it to the
                            // text plus topPadding 5 + bottomPadding 7.
                            let font = egui::FontId::proportional(theme.font_normal as f32);
                            let height = (ui.fonts_mut(|fonts| fonts.row_height(&font)) + 12.0).round();
                            text_edit_menu::show(
                                ui,
                                TextEdit::singleline(&mut self.input)
                                    .font(font)
                                    .background_color(theme.card)
                                    .desired_width(width - 20.0)
                                    .min_size(vec2(width, height))
                                    .margin(egui::Margin::symmetric(10, 0))
                                    .vertical_align(Align::Center)
                                    .hint_text(hint),
                                dialog.title(),
                                true,
                                reset_editor,
                            );
                        }
                    }
                    if dialog == Dialog::Domain && !self.input.trim().is_empty() {
                        let preview = bridge.normalize_domain_rule_input(&self.input);
                        if let Some((text, valid)) = domain_preview(&preview) {
                            ui.add_space(10.0);
                            body_text(ui, &text, small.clone(), if valid { theme.text_muted } else { theme.danger });
                        }
                    }
                    if let Some(index) = dialog.action_index() {
                        ui.add_space(10.0);
                        // RowLayout centres the label on the combo's height.
                        let row = vec2(ui.available_width(), theme.control_height as f32);
                        ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            caption(ui, theme, "Действие");
                            if let Some(action) =
                                action_combo(ui, "dialog-action", &self.actions[index], ACTIONS, 160.0)
                            {
                                self.actions[index] = action;
                            }
                        });
                    }
                })
        };
        if action == DialogAction::Accept {
            let input = self.input.trim();
            let rule_action = dialog
                .action_index()
                .map(|index| self.actions[index].as_str())
                .unwrap_or("proxy");
            match dialog {
                Dialog::Help => {}
                Dialog::Save => bridge.save_routing_preset(input),
                Dialog::Process | Dialog::Folder | Dialog::Running => {
                    if !input.is_empty() {
                        bridge.add_process_rule(input, rule_action)
                    }
                }
                Dialog::Domain => {
                    if !input.is_empty() {
                        bridge.add_domain_rule(input, rule_action)
                    }
                }
                Dialog::Import => {
                    if !input.is_empty() {
                        bridge.import_domain_rules(&self.input)
                    }
                }
            }
        }
        if action != DialogAction::None {
            self.dialog = None;
        }
    }
}

/// Process or domain rules table (RoutingPage.qml `Card` with padding 0,
/// inset header divider, 90pt empty state and 150pt action column).
fn rules(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, processes: bool) {
    let (rows, key, title, empty, hint) = if processes {
        (
            bridge.process_rules(),
            "process",
            "Приложение / папка",
            "Правила для приложений не заданы",
            "Работают только в режиме TUN.",
        )
    } else {
        (
            bridge.domain_rules(),
            "addr",
            "Адрес",
            "Правила по доменам и IP не заданы",
            "Формат импорта: адрес|действие",
        )
    };
    card(ui, theme, 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(14.0);
        right_row(ui, 16.0, |ui| {
            ui.add_space(14.0);
            ui.allocate_exact_size(vec2(40.0, 16.0), Sense::hover());
            ui.allocate_ui_with_layout(
                vec2(150.0, 16.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(
                        RichText::new(tr("Действие", &[]))
                            .size(theme.font_small as f32)
                            .family(crate::shell::theme::semibold_family())
                            .color(theme.text_muted),
                    );
                },
            );
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(14.0);
                ui.label(
                    RichText::new(tr(title, &[]))
                        .size(theme.font_small as f32)
                        .family(crate::shell::theme::semibold_family())
                        .color(theme.text_muted),
                );
            });
        });
        ui.add_space(8.0);
        hline(ui, theme, 14.0);
        if rows.is_empty() {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 90.0),
                Layout::top_down(Align::Center),
                |ui| {
                    // Centred column: muted title, 6 px, faint hint.
                    ui.add_space(24.0);
                    ui.label(text(empty, theme).color(theme.text_muted));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr(hint, &[]))
                            .size(theme.font_small as f32)
                            .color(theme.text_faint),
                    );
                },
            );
        }
        for (index, row) in rows.iter().enumerate() {
            let value = field(row, key).to_string();
            let action = field(row, "action").to_string();
            ui.add_space(8.0);
            right_row(ui, theme.control_height as f32, |ui| {
                ui.add_space(14.0);
                if ui
                    .add(
                        fluent_button::button(theme, "\u{e74d}", "", false)
                            .min_width(40.0),
                    )
                    .clicked()
                {
                    if processes {
                        bridge.remove_process_rule(&value);
                    } else {
                        bridge.remove_domain_rule(&value);
                    }
                }
                if let Some(next) = action_combo(ui, (key, &value), &action, ACTIONS, 150.0) {
                    if processes {
                        bridge.add_process_rule(&value, &next);
                    } else {
                        bridge.add_domain_rule(&value, &next);
                    }
                }
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width().max(0.0), theme.control_height as f32),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(egui::Label::new(text(&value, theme)).truncate())
                            .fluent_tip(&value);
                    },
                );
            });
            ui.add_space(8.0);
            if index + 1 < rows.len() {
                hline(ui, theme, 14.0);
            }
        }
        ui.add_space(14.0);
    });
}

fn services(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    section(ui, theme, "Сервисы");
    card(ui, theme, 16.0, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(tr(SERVICES_HINT, &[]))
                .size(theme.font_small as f32)
                .color(theme.text_muted),
        );
        ui.add_space(4.0);
        let items = bridge.service_list();
        if items.is_empty() {
            caption(ui, theme, "Список сервисов пуст.");
        }
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                hline(ui, theme, 0.0);
            }
            let id = field(item, "id").to_string();
            let name = field(item, "name").to_string();
            let description = field(item, "description").to_string();
            let action = field(item, "action").to_string();
            right_row(ui, 40.0, |ui| {
                if let Some(next) =
                    action_combo(ui, ("service", &id), &action, &ACTIONS[..2], 150.0)
                {
                    bridge.set_service_route(&id, &next);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.vertical(|ui| {
                        ui.label(text(&name, theme));
                        if !description.is_empty() {
                            caption(ui, theme, &tr(&description, &[]));
                        }
                    });
                });
            });
        }
    });
}

fn import_file(bridge: &mut AppBridge) {
    let title = tr("Импорт правил маршрутизации", &[]);
    let filter = format!(
        "{}\0*.txt;*.json;*.yaml;*.yml\0{}\0*.*\0",
        tr("Правила (*.txt *.json *.yaml *.yml)", &[]),
        tr("Все файлы (*)", &[])
    );
    match win_native::pick_file(Path::new(""), &title, &filter) {
        Ok(Some(path)) => bridge.import_domain_rules_file(&path),
        Ok(None) => {}
        Err(error) => bridge.notify_rule_import_error(&error.to_string()),
    }
}

fn domain_preview(info: &Value) -> Option<(String, bool)> {
    if info["changed"].as_bool() != Some(true)
        && info["invalid"].as_array().is_none_or(Vec::is_empty)
    {
        return None;
    }
    let values = info["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    Some(if values.is_empty() {
        (tr("Не похоже на домен, IP или CIDR", &[]), false)
    } else {
        (
            tr("Будет добавлено: {value}", &[("value", &values.join(", "))]),
            true,
        )
    })
}

/// Preset combo. QML's `FluentCombo` with a `boundIndex` of -1 keeps
/// `currentIndex` at the first model item, so an active id outside the list
/// still displays the first entry.
fn preset_combo(ui: &mut Ui, id: &str, items: &[Value], active: &str) -> Option<String> {
    let label = items
        .iter()
        .find(|item| field(item, "id") == active)
        .or_else(|| items.first())
        .map(|item| field(item, "name").to_string())
        .unwrap_or_default();
    combo(
        ui,
        id,
        &label,
        items
            .iter()
            .map(|item| (field(item, "id"), field(item, "name").into())),
        ui.available_width(),
    )
}

fn action_combo(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    active: &str,
    options: &[(&str, &str)],
    width: f32,
) -> Option<String> {
    let label = options
        .iter()
        .find(|(key, _)| *key == active)
        .unwrap_or(&options[0])
        .1;
    combo(
        ui,
        id,
        &tr(label, &[]),
        options.iter().map(|(key, label)| (*key, tr(label, &[]))),
        width,
    )
}

fn combo<'a>(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    label: &str,
    items: impl Iterator<Item = (&'a str, String)>,
    width: f32,
) -> Option<String> {
    let mut picked = None;
    crate::shell::widgets::fading_combo(ui, |ui| egui::ComboBox::from_id_salt(id).icon(crate::shell::widgets::combo_chevron)
        .width(width)
        .truncate()
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (key, item) in items {
                if ui.selectable_label(item == label, item).clicked() {
                    picked = Some(key.into());
                    ui.close();
                }
            }
        }));
    picked
}

/// QML `Switch` as a 44×20 Fluent pill (same drawing as the logs autoscroll).
fn switch_widget(ui: &mut Ui, theme: &Theme, checked: bool, enabled: bool) -> Option<bool> {
    crate::shell::pages::settings::switch_widget(ui, theme, checked, enabled)
}

/// Full-width warning rectangle with the 12% warning tint (RoutingPage.qml).
fn warning_band(ui: &mut Ui, theme: &Theme) {
    let fill = Color32::from_rgba_unmultiplied(
        theme.warning.r(),
        theme.warning.g(),
        theme.warning.b(),
        31,
    );
    Frame::new()
        .fill(fill)
        .corner_radius(theme.radius_small as u8)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(tr(PROXY_RULES_WARNING, &[]))
                    .size(theme.font_small as f32)
                    .color(theme.warning),
            );
        });
}

/// 1px `Theme.divider` line, optionally inset by 14pt like the table cards.
fn hline(ui: &mut Ui, theme: &Theme, inset: f32) {
    // Allocated directly: a `horizontal` row would pad the line to a full
    // interact height. The inset applies to both ends, as in QML.
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().max(0.0), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect.shrink2(vec2(inset, 0.0)), 0.0, theme.divider);
}

fn section(ui: &mut Ui, theme: &Theme, title: &str) {
    ui.label(
        RichText::new(tr(title, &[]))
            .size(theme.font_strong as f32)
            .family(crate::shell::theme::semibold_family()),
    );
}

fn caption(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(tr(text, &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
}

fn text(text: &str, theme: &Theme) -> RichText {
    RichText::new(tr(text, &[])).size(theme.font_normal as f32)
}

/// Height-constrained right-aligned row (`QML RowLayout`); egui rows must pin
/// their height or they inherit the scroll viewport's tall rect.
fn right_row<R>(ui: &mut Ui, height: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::right_to_left(Align::Center),
        content,
    )
    .inner
}

fn card(ui: &mut Ui, theme: &Theme, padding: f32, content: impl FnOnce(&mut Ui)) {
    crate::shell::widgets::card_frame(theme, padding.max(1.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

fn field<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preview_distinguishes_normalized_invalid_and_unchanged_inputs() {
        assert!(
            domain_preview(&json!({"changed":false,"invalid":[],"values":["example.com"]}))
                .is_none()
        );
        let (text, valid) =
            domain_preview(&json!({"changed":true,"invalid":[],"values":["example.com"]})).unwrap();
        assert!(valid && text.contains("example.com"));
        assert!(
            !domain_preview(&json!({"changed":false,"invalid":["?"],"values":[]}))
                .unwrap()
                .1
        );
    }
}
