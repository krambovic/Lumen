//! Routing presets, process rules, service overrides and domain rules.

use std::path::Path;

use egui::{Button, ComboBox, Frame, Id, Modal, RichText, ScrollArea, Stroke, TextEdit, Ui};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::PageContext;
use crate::shell::{theme::Theme, win_native};
use crate::view_models::AppBridge;

const ACTIONS: &[(&str, &str)] = &[
    ("direct", "Прямой"),
    ("proxy", "Прокси"),
    ("block", "Блокировка"),
];
const DEFAULT_ROUTES: &[(&str, &str)] = &[("proxy", "Прокси"), ("direct", "Напрямую")];

const HELP: &str = "Быстрые пресеты зависят от регионального профиля в настройках сети. Пресет «Всё кроме региона» пускает ресурсы выбранной страны напрямую, а остальной трафик — через прокси.\n\nСервисы:\n«Прокси» — всегда вести сервис через VPN/прокси.\n«Прямой» — всегда вести сервис напрямую.\n\nПравила по приложениям работают только в режиме TUN.";

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
    input: String,
    running: Vec<String>,
    actions: [String; 3],
}

impl Default for RoutingPage {
    fn default() -> Self {
        Self {
            dialog: None,
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
        ScrollArea::vertical().id_salt("routing-page").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Маршрутизация", &[])).size(theme.font_title as f32).strong());
                if button(ui, "Справка") { self.open(Dialog::Help); }
            });
            self.behavior(ui, theme, bridge);

            section(ui, theme, "Приложения");
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Добавить .exe") { self.open(Dialog::Process); }
                if button(ui, "Добавить папку") { self.open(Dialog::Folder); }
                if button(ui, "Из запущенных") {
                    self.open(Dialog::Running);
                    self.running = bridge.running_processes();
                    self.input = self.running.first().cloned().unwrap_or_default();
                }
            });
            rules(ui, theme, bridge, true);
            if !bridge.tun_mode() {
                ui.label(RichText::new(tr("В режиме системного прокси правила по приложениям не работают — включите TUN, чтобы перехватывать трафик приложений.", &[])).color(theme.warning));
            }

            services(ui, theme, bridge);
            section(ui, theme, "Домены и IP");
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Добавить") { self.open(Dialog::Domain); }
                if button(ui, "Импорт") { self.open(Dialog::Import); }
                if button(ui, "Из файла") { import_file(bridge); }
                if button(ui, "Экспорт") && let Some(payload) = bridge.export_domain_rules() {
                    ui.ctx().copy_text(payload);
                }
            });
            rules(ui, theme, bridge, false);
        });
        if let Some(dialog) = self.dialog {
            self.render_dialog(page.ctx, theme, bridge, dialog);
        }
    }

    fn open(&mut self, dialog: Dialog) {
        self.dialog = Some(dialog);
        self.input.clear();
    }

    fn behavior(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let active = bridge.active_routing_preset_id();
        let custom = bridge.custom_routing_presets();
        card(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    caption(ui, theme, "Быстрые пресеты маршрутизации");
                    if let Some(id) =
                        presets(ui, "built-in", &bridge.regional_routing_presets(), &active)
                    {
                        bridge.apply_routing_preset(&id);
                    }
                });
                ui.vertical(|ui| {
                    caption(ui, theme, "Свои пресеты маршрутизации");
                    ui.add_enabled_ui(!custom.is_empty(), |ui| {
                        if let Some(id) = presets(ui, "custom", &custom, &active) {
                            bridge.apply_custom_routing_preset(&id);
                        }
                    });
                });
            });
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Сохранить текущий пресет") {
                    self.open(Dialog::Save);
                }
                if ui
                    .add_enabled(
                        custom.iter().any(|preset| field(preset, "id") == active),
                        Button::new(tr("Удалить пресет", &[])),
                    )
                    .clicked()
                {
                    bridge.delete_routing_preset(&active);
                }
            });
            ui.separator();
            let mut bypass = bridge.bypass_lan();
            if ui
                .checkbox(&mut bypass, tr("Обход локальной сети", &[]))
                .changed()
            {
                bridge.set_bypass_lan(bypass);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("По умолчанию (TUN)", &[]));
                if let Some(action) = action_combo(
                    ui,
                    "default-outbound",
                    &bridge.tun_default_outbound(),
                    DEFAULT_ROUTES,
                ) {
                    bridge.set_tun_default_outbound(&action);
                }
            });
            caption(
                ui,
                theme,
                "Маршрут для трафика, который не совпал ни с одним правилом.",
            );
            ui.separator();
            let tun = bridge.tun_mode();
            let mut discord = bridge.discord_proxy();
            if ui
                .add_enabled(!tun, egui::Checkbox::new(&mut discord, "Discord Voice"))
                .changed()
            {
                bridge.set_discord_proxy(discord);
            }
            caption(
                ui,
                theme,
                if tun {
                    "Недоступно при включенном TUN — трафик Discord уже идет через VPN"
                } else {
                    "Голос и стримы Discord через SOCKS5 без TUN"
                },
            );
        });
    }

    fn render_dialog(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        dialog: Dialog,
    ) {
        let modal = Modal::new(Id::unique("routing-dialog")).show(ctx, |ui| {
            ui.set_width(480.0_f32.min((ctx.content_rect().width() - 52.0).max(0.0)));
            section(ui, theme, dialog.title());
            caption(ui, theme, dialog.description());
            match dialog {
                Dialog::Help => {}
                Dialog::Import => {
                    ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut self.input)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(7)
                                .hint_text(
                                    "example.com|proxy\nads.example.net|block\n10.0.0.0/8|direct",
                                ),
                        )
                    });
                }
                Dialog::Running => {
                    if let Some(process) = combo(
                        ui,
                        "running-process",
                        &self.input,
                        self.running
                            .iter()
                            .map(|name| (name.as_str(), name.clone())),
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
                    ui.add(
                        TextEdit::singleline(&mut self.input)
                            .desired_width(f32::INFINITY)
                            .hint_text(hint),
                    );
                }
            }
            if dialog == Dialog::Domain && !self.input.trim().is_empty() {
                let preview = bridge.normalize_domain_rule_input(&self.input);
                if let Some((text, valid)) = domain_preview(&preview) {
                    ui.label(RichText::new(text).color(if valid {
                        theme.text_muted
                    } else {
                        theme.danger
                    }));
                }
            }
            if let Some(index) = dialog.action_index() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr("Действие", &[]));
                    if let Some(action) =
                        action_combo(ui, "dialog-action", &self.actions[index], ACTIONS)
                    {
                        self.actions[index] = action;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                let cancel = dialog != Dialog::Help && button(ui, "Отмена");
                let confirm = match dialog {
                    Dialog::Help => "Понятно",
                    Dialog::Save => "Сохранить",
                    Dialog::Import => "Импорт",
                    _ => "Добавить",
                };
                let confirm = ui
                    .add_enabled(
                        dialog == Dialog::Help || !self.input.trim().is_empty(),
                        Button::new(tr(confirm, &[])),
                    )
                    .clicked();
                (confirm, cancel)
            })
            .inner
        });
        let (confirm, cancel) = modal.inner;
        if confirm {
            let input = self.input.trim();
            let action = dialog
                .action_index()
                .map(|index| self.actions[index].as_str())
                .unwrap_or("proxy");
            match dialog {
                Dialog::Help => {}
                Dialog::Save => bridge.save_routing_preset(input),
                Dialog::Process | Dialog::Folder | Dialog::Running => {
                    bridge.add_process_rule(input, action)
                }
                Dialog::Domain => bridge.add_domain_rule(input, action),
                Dialog::Import => bridge.import_domain_rules(&self.input),
            }
        }
        if confirm || cancel || modal.should_close() {
            self.dialog = None;
        }
    }
}

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
    card(ui, theme, |ui| {
        ui.horizontal_wrapped(|ui| {
            caption(ui, theme, title);
            caption(ui, theme, "Действие");
        });
        ui.separator();
        if rows.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(tr(empty, &[]));
                caption(ui, theme, hint);
            });
        }
        for row in rows {
            let value = field(&row, key);
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Удалить") {
                    if processes {
                        bridge.remove_process_rule(value);
                    } else {
                        bridge.remove_domain_rule(value);
                    }
                }
                if let Some(action) = action_combo(ui, (key, value), field(&row, "action"), ACTIONS)
                {
                    if processes {
                        bridge.add_process_rule(value, &action);
                    } else {
                        bridge.add_domain_rule(value, &action);
                    }
                }
                ui.add(egui::Label::new(value).truncate())
                    .on_hover_text(value);
            });
            ui.separator();
        }
    });
}

fn services(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    section(ui, theme, "Сервисы");
    card(ui, theme, |ui| {
        caption(
            ui,
            theme,
            "Быстрые переключатели для популярных сервисов (YouTube, Discord и др.). Выберите прямое подключение или прокси.",
        );
        let items = bridge.service_list();
        if items.is_empty() {
            caption(ui, theme, "Список сервисов пуст.");
        }
        for item in items {
            let id = field(&item, "id");
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(field(&item, "name"));
                if let Some(action) =
                    action_combo(ui, ("service", id), field(&item, "action"), &ACTIONS[..2])
                {
                    bridge.set_service_route(id, &action);
                }
            });
            let description = field(&item, "description");
            if !description.is_empty() {
                caption(ui, theme, description);
            }
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

fn presets(ui: &mut Ui, id: &str, items: &[Value], active: &str) -> Option<String> {
    let label = items
        .iter()
        .find(|item| field(item, "id") == active)
        .map_or("", |item| field(item, "name"));
    combo(
        ui,
        id,
        label,
        items
            .iter()
            .map(|item| (field(item, "id"), field(item, "name").into())),
    )
}

fn action_combo(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    active: &str,
    options: &[(&str, &str)],
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
    )
}

fn combo<'a>(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    label: &str,
    items: impl Iterator<Item = (&'a str, String)>,
) -> Option<String> {
    ComboBox::from_id_salt(id)
        .width(150.0)
        .truncate()
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (key, item) in items {
                if ui.selectable_label(item == label, item).clicked() {
                    ui.close();
                    return Some(key.into());
                }
            }
            None
        })
        .inner
        .flatten()
}

fn section(ui: &mut Ui, theme: &Theme, title: &str) {
    ui.label(
        RichText::new(tr(title, &[]))
            .size(theme.font_strong as f32)
            .strong(),
    );
}

fn caption(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(tr(text, &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
}

fn button(ui: &mut Ui, label: &str) -> bool {
    ui.button(tr(label, &[])).clicked()
}

fn card(ui: &mut Ui, theme: &Theme, content: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
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
