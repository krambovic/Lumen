//! Preset management and server-scoped Zapret bindings.

use std::path::Path;

use egui::{
    Button, ComboBox, Frame, Id, Key, Modal, Modifiers, RichText, ScrollArea, Stroke, TextEdit, Ui,
};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::{PageContext, code_editor};
use crate::shell::{theme::Theme, win_native};
use crate::view_models::AppBridge;

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
}

impl ZapretPage {
    pub(super) fn on_hidden(&mut self) {
        self.loaded = false;
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
            ScrollArea::vertical().id_salt("zapret-page").auto_shrink([false, false]).show(ui, |ui| {
                title(ui, theme, "Zapret");
                hint(ui, theme, "Zapret запускает winws2.exe с выбранным пресетом аргументов, чтобы обходить DPI-замедления YouTube, Discord и других сервисов. Работает независимо от VPN/прокси и требует прав администратора.");
                if !bridge.is_admin() {
                    ui.label(RichText::new(tr("Lumen запущен без прав администратора — Zapret не сможет запуститься.", &[])).color(theme.warning));
                }
                if ui.available_width() >= 720.0 {
                    ui.columns(2, |columns| {
                        self.render_list(&mut columns[0], theme, bridge, running, active);
                        self.render_details(&mut columns[1], theme, bridge, running);
                    });
                } else {
                    self.render_list(ui, theme, bridge, running, active);
                    self.render_details(ui, theme, bridge, running);
                }
                self.render_bindings(ui, theme, bridge);
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
    ) {
        card(ui, theme, |ui| {
            ui.add(
                TextEdit::singleline(&mut self.query)
                    .hint_text(tr("Поиск пресета", &[]))
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Новый пресет") {
                    self.open_editor(bridge, "");
                }
                if button(ui, "Импорт из файла") {
                    self.import(bridge);
                }
            });
            if let Some(error) = &self.import_error {
                ui.label(RichText::new(error).color(theme.danger));
            }
            let query = self.query.trim().to_lowercase();
            let mut selected = None;
            let mut visible = false;
            ScrollArea::vertical()
                .id_salt("zapret-presets")
                .max_height(270.0)
                .show(ui, |ui| {
                    for info in &self.presets {
                        let name = field(info, "name");
                        let description = field(info, "description");
                        if !name.to_lowercase().contains(&query)
                            && !description.to_lowercase().contains(&query)
                        {
                            continue;
                        }
                        visible = true;
                        let label = if running && active == name {
                            RichText::new(name).color(theme.success)
                        } else {
                            RichText::new(name)
                        };
                        let row = ui
                            .selectable_label(self.selected == name, label)
                            .on_hover_text(description);
                        if !description.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(description)
                                        .color(theme.text_muted)
                                        .size(theme.font_small as f32),
                                )
                                .truncate(),
                            );
                        }
                        if row.clicked() || row.double_clicked() {
                            selected = Some((name.to_string(), row.double_clicked()));
                        }
                    }
                });
            if let Some((name, activate)) = selected {
                self.select(bridge, &name);
                if activate && !(running && active == name) {
                    bridge.start_zapret(&name);
                }
            }
            if !visible {
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
            }
            let count = self.presets.len().to_string();
            ui.label(
                RichText::new(tr("Пресетов: {count}", &[("count", &count)]))
                    .color(theme.text_faint),
            );
        });
    }

    fn render_details(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        running: bool,
    ) {
        card(ui, theme, |ui| {
            let Some(preset) = self
                .presets
                .iter()
                .find(|preset| field(preset, "name") == self.selected)
            else {
                hint(ui, theme, "Выберите пресет в списке");
                return;
            };
            ui.label(
                RichText::new(&self.selected)
                    .size(theme.font_strong as f32)
                    .strong(),
            );
            let description = field(preset, "description");
            if !description.is_empty() {
                ui.label(description);
            }
            let count = preset["argCount"].as_u64().unwrap_or_default().to_string();
            let mut info = tr("Аргументов: {count}", &[("count", &count)]);
            if !field(preset, "modified").is_empty() {
                info.push_str(&format!(
                    "  ·  {}{}",
                    tr("Изменён: ", &[]),
                    field(preset, "modified")
                ));
            }
            ui.label(RichText::new(info).color(theme.text_faint));
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Удалить") {
                    self.confirm = Some(Confirm::Delete);
                }
                if button(ui, "Изменить") {
                    self.open_editor(bridge, &self.selected.clone());
                }
                let label = if running {
                    "Остановить"
                } else {
                    "Запустить"
                };
                if ui
                    .add(Button::new(tr(label, &[])).fill(if running {
                        theme.danger_fill
                    } else {
                        theme.accent
                    }))
                    .clicked()
                {
                    if running {
                        bridge.stop_zapret();
                    } else {
                        bridge.start_zapret(&self.selected);
                    }
                }
            });
            let mut preview = self.preview.as_str();
            code_editor::show(
                ui,
                theme,
                ("zapret-preview", &self.selected),
                "",
                &mut preview,
                230.0,
                None,
            );
        });
    }

    fn render_bindings(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        card(ui, theme, |ui| {
            ui.label(
                RichText::new(tr("Запуск вместе с сервером", &[]))
                    .size(theme.font_strong as f32)
                    .strong(),
            );
            hint(
                ui,
                theme,
                "Стратегия или пресет работают только для IP и порта сервера, пока вы к нему подключены. Смена во время подключения переподключает сервер.",
            );
            let bindings = bridge.zapret_server_bindings();
            if !bindings.is_empty() {
                ScrollArea::vertical()
                    .id_salt("zapret-bindings")
                    .max_height(180.0)
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
                                ui.label(RichText::new(&row.name).color(color));
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
                                    ) {
                                        bridge.set_zapret_server_binding(&row.node_id, &value);
                                    }
                                }
                                if button(ui, "Убрать привязку") {
                                    bridge.remove_zapret_server_binding(&row.node_id);
                                }
                            });
                        }
                    });
            }
            let servers: Vec<_> = bridge
                .zapret_server_options()
                .into_iter()
                .map(|server| (server.value, server.label))
                .collect();
            if !servers.iter().any(|item| item.0 == self.server) {
                self.server = servers
                    .first()
                    .map(|item| item.0.clone())
                    .unwrap_or_default();
                self.choice.clear();
            }
            ui.horizontal_wrapped(|ui| {
                let label = selected_label(&servers, &self.server)
                    .map(str::to_string)
                    .unwrap_or_else(|| tr("Все серверы уже привязаны", &[]));
                if let Some(server) = searchable(ui, "zapret-server", &label, &servers, "Поиск")
                {
                    self.server = server;
                    self.choice.clear();
                }
                let choices = if self.server.is_empty() {
                    Vec::new()
                } else {
                    bridge.zapret_binding_choices_for(&self.server)
                };
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
                ) {
                    self.choice = choice;
                }
                if ui
                    .add_enabled(
                        !self.server.is_empty() && !choices.is_empty(),
                        Button::new(tr("Привязать", &[])),
                    )
                    .clicked()
                {
                    bridge.set_zapret_server_binding(&self.server, &self.choice);
                }
            });
        });
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
        let mut save = false;
        let mut close = false;
        ui.horizontal_wrapped(|ui| {
            let heading = if fresh {
                tr("Новый пресет", &[])
            } else {
                format!("{}{}", tr("Редактирование: ", &[]), draft.saved.name)
            };
            ui.label(
                RichText::new(heading)
                    .size(theme.font_title as f32)
                    .strong(),
            );
            close = button(ui, "Отмена");
            save = ui
                .add_enabled(error.is_none(), Button::new(tr("Сохранить", &[])))
                .clicked();
        });
        card(ui, theme, |ui| {
            let name = ui.add(
                TextEdit::singleline(&mut draft.current.name)
                    .hint_text(tr("Имя пресета", &[]))
                    .desired_width(f32::INFINITY),
            );
            if draft.focus && fresh {
                name.request_focus();
            }
            if let Some(error) =
                name_error(&draft.current.name).filter(|_| !draft.current.name.is_empty())
            {
                ui.label(RichText::new(tr(error, &[])).color(theme.danger));
            }
            ui.add(
                TextEdit::singleline(&mut draft.current.description)
                    .hint_text(tr("Краткое описание (необязательно)", &[]))
                    .desired_width(f32::INFINITY),
            );
            let height = (ui.available_height() - 56.0).max(40.0);
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
            ui.horizontal_wrapped(|ui| {
                if !draft.metadata.is_empty() {
                    ui.label(RichText::new(&draft.metadata).color(theme.text_faint));
                }
                if draft.dirty() {
                    ui.label(RichText::new(tr("Изменено", &[])).color(theme.warning));
                }
                let count = argument_count(&draft.current.args).to_string();
                ui.label(tr("Аргументов: {count}", &[("count", &count)]));
            });
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
        let modal = Modal::new(Id::unique("zapret-confirm")).show(ctx, |ui| {
            ui.set_width(420.0_f32.min((ctx.content_rect().width() - 52.0).max(0.0)));
            title(ui, theme, heading);
            ui.label(message);
            ui.horizontal_wrapped(|ui| (button(ui, "Отмена"), button(ui, accept)))
                .inner
        });
        let (cancel, accept) = modal.inner;
        if accept {
            match confirm {
                Confirm::Delete => {
                    bridge.delete_preset(&self.selected);
                    self.refresh(bridge);
                }
                Confirm::Discard => self.draft = None,
            }
        }
        if cancel || accept || modal.should_close() {
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
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    ui.add_enabled_ui(!items.is_empty(), |ui| {
        ComboBox::from_id_salt(id)
            .width(190.0)
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
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
}

fn button(ui: &mut Ui, label: &str) -> bool {
    ui.button(tr(label, &[])).clicked()
}
fn title(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.label(
        RichText::new(tr(label, &[]))
            .size(theme.font_title as f32)
            .strong(),
    );
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
}
