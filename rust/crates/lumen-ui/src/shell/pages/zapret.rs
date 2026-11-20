//! Preset management and server-scoped Zapret bindings.

#[allow(unused_imports)]
use crate::shell::widgets::FluentTip;
use std::path::Path;

use egui::{
    Align, ComboBox, Frame, Key, Layout, Modifiers, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Ui, vec2,
};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::{PageContext, code_editor, fluent_button, search_field};
use crate::shell::{
    theme::{Theme, semibold},
    win_native,
};
use crate::view_models::{AppBridge, app_bridge::ZapretBindingRow};

#[derive(Default, Clone, PartialEq, Eq)]
struct PresetText {
    name: String,
    description: String,
    args: String,
}

struct Draft {
    saved: PresetText,
    current: PresetText,
    metadata: String,
    focus: bool,
}

impl Draft {
    fn dirty(&self) -> bool {
        self.current.name.trim() != self.saved.name
            || self.current.description.trim() != self.saved.description
            || self.current.args != self.saved.args
    }
}

#[derive(Clone, Copy)]
enum Confirm {
    Delete,
    Discard,
}

#[derive(Default)]
pub(super) struct ZapretPage {
    presets: Vec<Value>,
    loaded: bool,
    query: String,
    selected: String,
    preview: String,
    draft: Option<Draft>,
    document: u64,
    confirm: Option<Confirm>,
    server: String,
    choice: String,
    import_error: Option<String>,
    bind_options: Option<BindOptions>,
}

/// Server and strategy lists of the binding row. Rebuilding them walks every
/// node and strategy, so they are kept until the settings, the picked server
/// or a second passes.
struct BindOptions {
    built: std::time::Instant,
    settings: u64,
    server: String,
    servers: Vec<(String, String)>,
    choices: Vec<(String, String)>,
}

impl ZapretPage {
    pub(super) fn on_hidden(&mut self) {
        self.loaded = false;
        self.bind_options = None;
    }

    fn refresh(&mut self, bridge: &AppBridge) {
        self.presets = bridge.zapret_presets();
        if !self
            .presets
            .iter()
            .any(|preset| field(preset, "name") == self.selected)
        {
            self.selected = self
                .presets
                .first()
                .map(|preset| field(preset, "name").to_string())
                .unwrap_or_default();
        }
        self.preview = if self.selected.is_empty() {
            String::new()
        } else {
            bridge.read_preset(&self.selected)
        };
        self.loaded = true;
    }

    fn select(&mut self, bridge: &AppBridge, name: &str) {
        if self.selected != name {
            self.selected = name.into();
            self.preview = bridge.read_preset(name);
        }
    }

    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        if !self.loaded {
            self.refresh(bridge);
        }
        if self.draft.is_some() {
            self.render_editor(ui, theme, bridge);
        } else {
            let status = bridge.zapret_status();
            let running = status["running"].as_bool() == Some(true);
            let active = field(&status, "preset");
            let bindings = bridge.zapret_server_bindings();
            let viewport_height = ui.available_height();
            ScrollArea::vertical().id_salt("zapret-page").auto_shrink([false, false]).show(ui, |ui| {
                let top = ui.next_widget_position().y;
                ui.spacing_mut().item_spacing.y = theme.spacing as f32;
                title(ui, theme, "Zapret");
                hint(ui, theme, "Zapret запускает winws2.exe с выбранным пресетом аргументов, чтобы обходить DPI-замедления YouTube, Discord и других сервисов. Работает независимо от VPN/прокси и требует прав администратора.");
                if !bridge.is_admin() {
                    Frame::new().fill(theme.warning.gamma_multiply(0.12))
                        .stroke(Stroke::new(1.0, theme.warning.gamma_multiply(0.35)))
                        .corner_radius(theme.radius_small as u8)
                        .inner_margin(egui::Margin::symmetric(11, 5)).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("\u{e7ba}").size(14.0).color(theme.warning));
                                ui.label(RichText::new(tr("Lumen запущен без прав администратора — Zapret не сможет запуститься.", &[])).size(theme.font_small as f32));
                            });
                        });
                }
                let width = ui.available_width();
                let gap = theme.spacing as f32;
                if let Some(sidebar) = sidebar_width(width) {
                    let remaining_height = viewport_height - (ui.next_widget_position().y - top);
                    let preview_height = (remaining_height - 202.0 - bindings.len().min(4) as f32 * 40.0).max(180.0);
                    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::right_to_left(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        let details = ui.allocate_ui_with_layout(vec2(width - sidebar - gap, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(width - sidebar - gap);
                            self.render_details(ui, theme, bridge, running, preview_height);
                            self.render_bindings(ui, theme, bridge, &bindings);
                        });
                        ui.allocate_ui_with_layout(vec2(sidebar, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(sidebar);
                            self.render_list(ui, theme, bridge, running, active, (details.response.rect.height() - 86.0).max(160.0));
                        });
                    });
                } else {
                    self.render_list(ui, theme, bridge, running, active, 200.0);
                    self.render_details(ui, theme, bridge, running, 210.0);
                    self.render_bindings(ui, theme, bridge, &bindings);
                }
            });
        }
        if let Some(confirm) = self.confirm {
            self.render_confirmation(page.ctx, theme, bridge, confirm);
        }
    }

    fn render_list(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        running: bool,
        active: &str,
        list_height: f32,
    ) {
        card_with_margin(ui, theme, 9.0, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), theme.control_height as f32),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if fluent_button::icon(ui, theme, "\u{e8b5}", &tr("Импорт из файла", &[]), true)
                        .clicked()
                    {
                        self.import(bridge);
                    }
                    if fluent_button::icon(ui, theme, "\u{e710}", &tr("Новый пресет", &[]), true)
                        .clicked()
                    {
                        self.open_editor(bridge, "");
                    }
                    search_field::show(ui, theme, &mut self.query, &tr("Поиск пресета", &[]), true);
                },
            );
            if let Some(error) = &self.import_error {
                ui.label(RichText::new(error).color(theme.danger));
            }
            let query = self.query.trim().to_lowercase();
            let mut selected = None;
            let mut visible = false;
            ui.add(egui::Separator::default().spacing(1.0));
            ScrollArea::vertical()
                .id_salt("zapret-presets")
                .min_scrolled_height(list_height)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for info in &self.presets {
                        let name = field(info, "name");
                        let description = field(info, "description");
                        if !name.to_lowercase().contains(&query)
                            && !description.to_lowercase().contains(&query)
                        {
                            continue;
                        }
                        visible = true;
                        let row = preset_row(
                            ui,
                            theme,
                            info,
                            self.selected == name,
                            running && active == name,
                        );
                        if row.clicked() || row.double_clicked() {
                            selected = Some((name.to_string(), row.double_clicked()));
                        }
                    }
                    if !visible {
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), list_height),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.add_space((list_height * 0.5 - 42.0).max(0.0));
                                ui.label(
                                    RichText::new("\u{e945}").size(28.0).color(theme.text_faint),
                                );
                                if self.presets.is_empty() {
                                    hint(ui, theme, "Нет пресетов Zapret");
                                    hint(
                                        ui,
                                        theme,
                                        "Создайте пресет или импортируйте файл аргументов winws2.",
                                    );
                                } else {
                                    hint(ui, theme, "Ничего не найдено");
                                }
                            },
                        );
                    }
                });
            if let Some((name, activate)) = selected {
                self.select(bridge, &name);
                ui.request_repaint();
                if activate && !(running && active == name) {
                    bridge.start_zapret(&name);
                }
            }
            ui.add(egui::Separator::default().spacing(1.0));
            let count = self.presets.len().to_string();
            ui.label(
                RichText::new(tr("Пресетов: {count}", &[("count", &count)]))
                    .color(theme.text_faint)
                    .size(theme.font_small as f32),
            );
        });
    }

    fn render_details(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        running: bool,
        preview_height: f32,
    ) {
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            let Some(preset) = self
                .presets
                .iter()
                .find(|preset| field(preset, "name") == self.selected)
            else {
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), preview_height + 60.0),
                    Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        hint(ui, theme, "Выберите пресет в списке");
                    },
                );
                return;
            };
            let name = self.selected.clone();
            let description = field(preset, "description").to_owned();
            let count = preset["argCount"].as_u64().unwrap_or_default().to_string();
            let mut info = tr("Аргументов: {count}", &[("count", &count)]);
            if !field(preset, "modified").is_empty() {
                info.push_str(&format!(
                    "  ·  {}{}",
                    tr("Изменён: ", &[]),
                    field(preset, "modified")
                ));
            }
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 58.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    fluent_button::style(ui);
                    let (label, glyph) = if running {
                        ("Остановить", "\u{e71a}")
                    } else {
                        ("Запустить", "\u{e768}")
                    };
                    let mut action = fluent_button::button(theme, glyph, &tr(label, &[]), true);
                    if running {
                        action = action.fill(theme.danger_fill);
                    }
                    if ui.add(action).clicked() {
                        if running {
                            bridge.stop_zapret();
                        } else {
                            bridge.start_zapret(&name);
                        }
                    }
                    if ui
                        .add(fluent_button::button(
                            theme,
                            "\u{e70f}",
                            &tr("Изменить", &[]),
                            false,
                        ))
                        .clicked()
                    {
                        self.open_editor(bridge, &name);
                    }
                    if fluent_button::icon(ui, theme, "\u{e74d}", &tr("Удалить", &[]), true)
                        .clicked()
                    {
                        self.confirm = Some(Confirm::Delete);
                    }
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 52.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add(
                                egui::Label::new(
                                    RichText::new(name).font(semibold(theme.font_strong as f32)),
                                )
                                .truncate(),
                            );
                            if !description.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(description)
                                            .size(theme.font_small as f32)
                                            .color(theme.text_muted),
                                    )
                                    .truncate(),
                                );
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(info)
                                        .size(theme.font_small as f32)
                                        .color(theme.text_faint),
                                )
                                .truncate(),
                            );
                        },
                    );
                },
            );
            ui.add(egui::Separator::default().spacing(1.0));
            let mut preview = self.preview.as_str();
            code_editor::show(
                ui,
                theme,
                ("zapret-preview", &self.selected),
                "",
                &mut preview,
                preview_height,
                None,
            );
        });
    }

    fn render_bindings(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        bindings: &[ZapretBindingRow],
    ) {
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("\u{e71b}").size(16.0).color(theme.text_muted));
                ui.label(RichText::new(tr("Запуск вместе с сервером", &[])).font(semibold(theme.font_strong as f32)));
                ui.label(RichText::new("\u{e946}").color(theme.text_faint)).fluent_tip(tr("Стратегия или пресет работают только для IP и порта сервера, пока вы к нему подключены. Смена во время подключения переподключает сервер.", &[]));
            });
            if !bindings.is_empty() {
                ScrollArea::vertical()
                    .id_salt("zapret-bindings")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for row in bindings {
                            ui.horizontal_wrapped(|ui| {
                                let color = if row.missing_node {
                                    theme.warning
                                } else if row.active {
                                    theme.success
                                } else {
                                    theme.text
                                };
                                let glyph = if row.missing_node {
                                    "\u{e7ba}"
                                } else if row.active {
                                    "\u{e73e}"
                                } else {
                                    "\u{e968}"
                                };
                                ui.label(RichText::new(glyph).color(color));
                                ui.label(RichText::new(&row.name).color(if row.missing_node {
                                    theme.text_muted
                                } else {
                                    theme.text
                                }));
                                if row.missing_preset {
                                    ui.label(
                                        RichText::new(tr("Пресет не найден", &[]))
                                            .color(theme.warning),
                                    );
                                }
                                if row.strategy_mode && !row.kind_title.is_empty() {
                                    ui.label(&row.kind_title);
                                }
                                if !row.missing_node {
                                    let current = row
                                        .choices
                                        .get(row.choice_index as usize)
                                        .map_or("", |choice| choice.1.as_str());
                                    let label = if current.is_empty() {
                                        &row.placeholder
                                    } else {
                                        current
                                    };
                                    if let Some(value) = searchable(
                                        ui,
                                        ("zapret-binding", row.node_id.as_str()),
                                        label,
                                        &row.choices,
                                        "Поиск стратегий…",
                                        260.0,
                                    ) {
                                        bridge.set_zapret_server_binding(&row.node_id, &value);
                                    }
                                }
                                if fluent_button::icon(
                                    ui,
                                    theme,
                                    "\u{e711}",
                                    &tr("Убрать привязку", &[]),
                                    true,
                                )
                                .clicked()
                                {
                                    bridge.remove_zapret_server_binding(&row.node_id);
                                }
                            });
                        }
                    });
            }
            let servers = self.bind_options(bridge).servers.clone();
            if !servers.iter().any(|item| item.0 == self.server) {
                self.server = servers
                    .first()
                    .map(|item| item.0.clone())
                    .unwrap_or_default();
                self.choice.clear();
            }
            let choices = self.bind_options(bridge).choices.clone();
            ui.horizontal_wrapped(|ui| {
                fluent_button::style(ui);
                ui.spacing_mut().interact_size.y = theme.control_height as f32;
                ui.spacing_mut().item_spacing.x = 8.0;
                let width = ui.available_width();
                let choice_width = (width * 0.43).min(260.0);
                let bind_label = tr("Привязать", &[]);
                let bind_width = ui
                    .painter()
                    .layout_no_wrap(
                        bind_label.clone(),
                        egui::FontId::proportional(theme.font_normal as f32),
                        theme.text,
                    )
                    .size()
                    .x
                    + theme.font_normal as f32
                    + 36.0;
                let server_width = (width - choice_width - bind_width - 16.0).max(100.0);
                let label = selected_label(&servers, &self.server)
                    .map(str::to_string)
                    .unwrap_or_else(|| tr("Все серверы уже привязаны", &[]));
                if let Some(server) =
                    searchable(ui, "zapret-server", &label, &servers, "Поиск", server_width)
                {
                    self.server = server;
                    self.choice.clear();
                }
                if selected_label(&choices, &self.choice).is_none() {
                    self.choice = choices
                        .first()
                        .map(|item| item.0.clone())
                        .unwrap_or_default();
                }
                if let Some(choice) = searchable(
                    ui,
                    "zapret-strategy",
                    selected_label(&choices, &self.choice).unwrap_or(""),
                    &choices,
                    "Поиск стратегий…",
                    choice_width,
                ) {
                    self.choice = choice;
                }
                if ui
                    .add_enabled(
                        !self.server.is_empty() && !choices.is_empty(),
                        fluent_button::button(theme, "\u{e710}", &bind_label, false),
                    )
                    .clicked()
                {
                    bridge.set_zapret_server_binding(&self.server, &self.choice);
                    self.bind_options = None;
                }
            });
        });
    }

    fn bind_options(&mut self, bridge: &mut AppBridge) -> &BindOptions {
        let settings = bridge.settings_changed_token();
        let stale = self.bind_options.as_ref().is_none_or(|options| {
            options.settings != settings
                || options.server != self.server
                || options.built.elapsed() >= std::time::Duration::from_secs(1)
        });
        if stale {
            let servers: Vec<_> = bridge
                .zapret_server_options()
                .into_iter()
                .map(|server| (server.value, server.label))
                .collect();
            let choices = if self.server.is_empty() {
                Vec::new()
            } else {
                bridge.zapret_binding_choices_for(&self.server)
            };
            self.bind_options = Some(BindOptions {
                built: std::time::Instant::now(),
                settings,
                server: self.server.clone(),
                servers,
                choices,
            });
        }
        self.bind_options.as_ref().expect("bind options built above")
    }

    fn import(&mut self, bridge: &mut AppBridge) {
        self.import_error = None;
        let filter = format!(
            "{}\0*.txt\0{}\0*.*\0",
            tr("Текстовые файлы (*.txt)", &[]),
            tr("Все файлы (*)", &[])
        );
        match win_native::pick_file(Path::new(""), &tr("Импорт пресета", &[]), &filter)
        {
            Ok(Some(path)) => {
                let name = bridge.import_zapret_preset(&path);
                if !name.is_empty() {
                    self.selected = name;
                    self.refresh(bridge);
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.import_error = Some(tr(
                    "Не удалось импортировать: {error}",
                    &[("error", &error.to_string())],
                ))
            }
        }
    }

    fn open_editor(&mut self, bridge: &AppBridge, name: &str) {
        let mut saved = PresetText {
            name: name.into(),
            ..Default::default()
        };
        let mut metadata = Vec::new();
        if let Some(info) = self
            .presets
            .iter()
            .find(|preset| field(preset, "name") == name)
        {
            saved.description = field(info, "description").into();
            saved.args = bridge.read_preset(name);
            for (key, caption) in [("created", "Создан: "), ("modified", "Изменён: ")]
            {
                if !field(info, key).is_empty() {
                    metadata.push(format!("{}{}", tr(caption, &[]), field(info, key)));
                }
            }
        }
        self.document = self.document.wrapping_add(1);
        self.draft = Some(Draft {
            current: saved.clone(),
            saved,
            metadata: metadata.join("  ·  "),
            focus: true,
        });
    }

    fn render_editor(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let draft = self.draft.as_mut().expect("preset editor is open");
        let fresh = draft.saved.name.is_empty();
        let error = name_error(&draft.current.name);
        ui.spacing_mut().item_spacing.y = theme.spacing as f32;
        let heading = if fresh {
            tr("Новый пресет", &[])
        } else {
            format!("{}{}", tr("Редактирование: ", &[]), draft.saved.name)
        };
        let (mut save, close) = ui
            .allocate_ui_with_layout(
                vec2(ui.available_width(), theme.control_height as f32),
                Layout::right_to_left(Align::Center),
                |ui| {
                    fluent_button::style(ui);
                    let save = ui
                        .add_enabled(
                            error.is_none(),
                            fluent_button::button(theme, "\u{e74e}", &tr("Сохранить", &[]), true),
                        )
                        .fluent_tip("Ctrl+S")
                        .clicked();
                    let cancel = ui
                        .add(fluent_button::button(theme, "", &tr("Отмена", &[]), false))
                        .clicked();
                    let back = ui
                        .with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let back = fluent_button::icon(
                                ui,
                                theme,
                                "\u{e72b}",
                                &tr("Назад к списку", &[]),
                                true,
                            )
                            .clicked();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(heading).font(semibold(theme.font_title as f32)),
                                )
                                .truncate(),
                            );
                            back
                        })
                        .inner;
                    (save, cancel || back)
                },
            )
            .inner;
        card(ui, theme, |ui| {
            let name = editor_field(
                ui,
                theme,
                "Название:",
                "Имя пресета",
                &mut draft.current.name,
            );
            if draft.focus && fresh {
                name.request_focus();
            }
            if let Some(error) =
                name_error(&draft.current.name).filter(|_| !draft.current.name.is_empty())
            {
                ui.horizontal(|ui| {
                    ui.add_space(90.0);
                    ui.label(
                        RichText::new(tr(error, &[]))
                            .size(theme.font_small as f32)
                            .color(theme.danger),
                    );
                });
            }
            editor_field(
                ui,
                theme,
                "Описание:",
                "Краткое описание (необязательно)",
                &mut draft.current.description,
            );
        });
        card(ui, theme, |ui| {
            let height = (ui.available_height() - theme.font_small as f32 - 12.0).max(30.0);
            let jump = (draft.focus && !fresh).then_some(1);
            draft.focus = false;
            code_editor::show(
                ui,
                theme,
                ("zapret-edit", self.document),
                &tr(
                    "Аргументы winws2, по одному на строку.\nСтроки с # — комментарии.",
                    &[],
                ),
                &mut draft.current.args,
                height,
                jump,
            );
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), theme.font_small as f32 + 4.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    let count = argument_count(&draft.current.args).to_string();
                    ui.label(
                        RichText::new(tr("Аргументов: {count}", &[("count", &count)]))
                            .size(theme.font_small as f32)
                            .color(theme.text_muted),
                    );
                    if draft.dirty() {
                        ui.label(
                            RichText::new(tr("Изменено", &[]))
                                .size(theme.font_small as f32)
                                .color(theme.warning),
                        );
                    }
                    if !draft.metadata.is_empty() {
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&draft.metadata)
                                        .size(theme.font_small as f32)
                                        .color(theme.text_faint),
                                )
                                .truncate(),
                            );
                        });
                    }
                },
            );
        });
        save |= self.confirm.is_none()
            && ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::S));
        if close {
            if draft.dirty() {
                self.confirm = Some(Confirm::Discard);
            } else {
                self.draft = None;
            }
        } else if save && name_error(&draft.current.name).is_none() {
            let name = draft.current.name.trim();
            let description = draft.current.description.trim();
            let saved = if fresh || name == draft.saved.name {
                bridge.save_preset(name, description, &draft.current.args)
            } else {
                bridge.save_renamed_preset(
                    &draft.saved.name,
                    name,
                    description,
                    &draft.current.args,
                )
            };
            if saved {
                self.selected = name.into();
                self.draft = None;
                self.refresh(bridge);
            }
        }
    }

    fn render_confirmation(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        confirm: Confirm,
    ) {
        let (heading, message, accept) = match confirm {
            Confirm::Delete => (
                "Удалить пресет?",
                tr(
                    "Пресет «{name}» будет удалён без возможности восстановления.",
                    &[("name", &self.selected)],
                ),
                "Удалить",
            ),
            Confirm::Discard => (
                "Несохранённые изменения",
                tr(
                    "Изменения в пресете не сохранены. Закрыть редактор без сохранения?",
                    &[],
                ),
                "Отменить изменения",
            ),
        };
        use crate::shell::fluent_dialog::{DialogAction, FluentDialog, body_text};
        // deleteDialog / discardDialog: FluentDialog, 440 px.
        let action = FluentDialog::new(("zapret-confirm", heading), tr(heading, &[]), 440.0)
            .ok(tr(accept, &[]))
            .show(ctx, theme, |ui| {
                body_text(ui, &message, egui::FontId::proportional(theme.font_normal as f32), theme.text_muted);
            });
        if action == DialogAction::Accept {
            match confirm {
                Confirm::Delete => {
                    bridge.delete_preset(&self.selected);
                    self.refresh(bridge);
                }
                Confirm::Discard => self.draft = None,
            }
        }
        if action != DialogAction::None {
            self.confirm = None;
        }
    }
}

fn searchable(
    ui: &mut Ui,
    key: impl egui::AsIdSalt,
    label: &str,
    items: &[(String, String)],
    hint: &str,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    ui.add_enabled_ui(!items.is_empty(), |ui| {
        ComboBox::from_id_salt(id).icon(crate::shell::widgets::combo_chevron)
            .width(width)
            .truncate()
            .selected_text(label)
            .show_ui(ui, |ui| {
                let mut query = ui.data_mut(|data| data.get_temp::<String>(id).unwrap_or_default());
                ui.add(TextEdit::singleline(&mut query).hint_text(tr(hint, &[])));
                ui.data_mut(|data| data.insert_temp(id, query.clone()));
                let query = query.trim().to_lowercase();
                for (key, item) in items {
                    if item.to_lowercase().contains(&query)
                        && ui.selectable_label(item == label, item).clicked()
                    {
                        ui.data_mut(|data| data.remove::<String>(id));
                        ui.close();
                        return Some(key.clone());
                    }
                }
                None
            })
            .inner
            .flatten()
    })
    .inner
}

fn selected_label<'a>(items: &'a [(String, String)], key: &str) -> Option<&'a str> {
    items
        .iter()
        .find(|item| item.0 == key)
        .map(|item| item.1.as_str())
}

fn sidebar_width(width: f32) -> Option<f32> {
    (width >= 720.0).then(|| (width * 0.32).clamp(250.0, 340.0))
}

fn editor_field(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    placeholder: &str,
    text: &mut String,
) -> egui::Response {
    ui.horizontal(|ui| {
        let height = theme.control_height as f32;
        ui.allocate_ui_with_layout(
            vec2(90.0, height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(90.0);
                ui.add(
                    egui::Label::new(RichText::new(tr(label, &[])).color(theme.text_muted))
                        .truncate(),
                );
            },
        );
        ui.add_sized(
            vec2(ui.available_width(), height),
            TextEdit::singleline(text).hint_text(tr(placeholder, &[])),
        )
    })
    .inner
}

fn preset_row(
    ui: &mut Ui,
    theme: &Theme,
    preset: &Value,
    selected: bool,
    active: bool,
) -> egui::Response {
    let name = field(preset, "name");
    let description = field(preset, "description");
    let summary = if description.is_empty() {
        tr(
            "Аргументов: {count}",
            &[(
                "count",
                &preset["argCount"].as_u64().unwrap_or_default().to_string(),
            )],
        )
    } else {
        description.to_owned()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, name));
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(
            rect,
            theme.radius_small as u8,
            if selected {
                theme.card_hover
            } else {
                theme.nav_hover
            },
        );
    }
    if selected {
        let marker = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - 9.0),
            vec2(3.0, 18.0),
        );
        painter.rect_filled(marker, 2.0, theme.accent);
    }
    if active {
        painter.circle_filled(rect.left_top() + vec2(16.0, 16.0), 4.0, theme.success);
    }
    let normal_size = theme.font_normal as f32;
    let font = if selected || active {
        semibold(normal_size)
    } else {
        egui::FontId::proportional(normal_size)
    };
    for (left, top, height, text) in [
        (
            if active { 26.0 } else { 12.0 },
            6.0,
            21.0,
            RichText::new(name)
                .font(font)
                .color(if active { theme.success } else { theme.text }),
        ),
        (
            12.0,
            27.0,
            17.0,
            RichText::new(summary)
                .size(theme.font_small as f32)
                .color(theme.text_muted),
        ),
    ] {
        let bounds = egui::Rect::from_min_max(
            rect.left_top() + vec2(left, top),
            egui::pos2(rect.right() - 10.0, rect.top() + top + height),
        );
        let mut label = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bounds)
                .layout(Layout::left_to_right(Align::Center)),
        );
        label.add(
            egui::Label::new(text)
                .truncate()
                .selectable(false)
                .sense(Sense::hover()),
        );
    }
    response
}

fn argument_count(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count()
}

fn name_error(name: &str) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("Укажите имя пресета")
    } else if name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        Some("Недопустимые символы в имени")
    } else {
        None
    }
}

fn card(ui: &mut Ui, theme: &Theme, content: impl FnOnce(&mut Ui)) {
    card_with_margin(ui, theme, 12.0, content);
}

fn card_with_margin(ui: &mut Ui, theme: &Theme, margin: f32, content: impl FnOnce(&mut Ui)) {
    crate::shell::widgets::card_frame(theme, margin + 1.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

fn button(ui: &mut Ui, label: &str) -> bool {
    ui.button(tr(label, &[])).clicked()
}
fn title(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.label(RichText::new(tr(label, &[])).font(semibold(theme.font_title as f32)));
}
fn hint(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.label(
        RichText::new(tr(label, &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
}
fn field<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_editor_counts_only_arguments_and_rejects_path_components() {
        assert_eq!(
            argument_count("# Preset: test\n --filter-tcp=443\n\n  #comment\n--new\n"),
            2
        );
        assert!(name_error(" dir\\test ").is_some());
        assert!(name_error(" пример ").is_none());
    }

    #[test]
    fn preset_sidebar_stacks_at_minimum_width_and_keeps_fixed_rows() {
        assert_eq!(sidebar_width(540.0), None);
        assert_eq!(sidebar_width(780.0), Some(250.0));
        assert_eq!(sidebar_width(1180.0), Some(340.0));

        let ctx = egui::Context::default();
        let theme = Theme::resolve(&Default::default());
        crate::shell::theme::apply_fonts(&ctx, &theme);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            ui.set_width(250.0);
            let preset =
                serde_json::json!({"name": "Demo", "description": "Long description ".repeat(30)});
            let first = preset_row(ui, &theme, &preset, true, true);
            let second = preset_row(ui, &theme, &preset, false, false);
            assert_eq!(first.rect.height(), 50.0);
            assert_eq!(
                second.rect.top() - first.rect.bottom(),
                ui.spacing().item_spacing.y
            );
            assert!(ui.min_rect().width() <= 250.0);
        });
        output.textures_delta.clear();
    }
}
