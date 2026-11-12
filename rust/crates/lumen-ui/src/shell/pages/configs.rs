//! Raw sing-box and Xray configuration editors from `ConfigsPage.qml`.

use std::sync::LazyLock;

use egui::{Button, ComboBox, Frame, Id, Key, Modal, Modifiers, RichText, Stroke, Ui};
use lumen_core::i18n::tr;
use regex::Regex;
use serde_json::Value;

use super::PageContext;
use super::code_editor;
use crate::shell::{theme::Theme, win_native};
use crate::view_models::AppBridge;

const CORES: [(&str, &str); 2] = [("singbox", "sing-box"), ("xray", "Xray")];

#[derive(Default)]
struct Editor {
    text: String,
    saved_text: String,
    file: String,
    configs: Vec<Value>,
    templates: Vec<Value>,
    active_config: String,
    active_template: String,
    has_template: bool,
    level: String,
    message: String,
    jump_to: Option<usize>,
    document: u64,
}

impl Editor {
    fn load(state: &Value) -> Self {
        let text = field(state, "text").to_string();
        Self {
            saved_text: text.clone(),
            text,
            file: field(state, "fileLabel").into(),
            configs: state["configItems"].as_array().cloned().unwrap_or_default(),
            templates: state["templateItems"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            active_config: field(state, "activeConfig").into(),
            active_template: field(state, "activeTemplate").into(),
            has_template: state["hasTemplate"].as_bool().unwrap_or(false),
            level: field(state, "statusLevel").into(),
            message: field(state, "statusMessage").into(),
            jump_to: None,
            document: 0,
        }
    }

    fn dirty(&self) -> bool {
        self.text != self.saved_text
    }

    fn status(&mut self, state: &Value) {
        self.level = field(state, "statusLevel").into();
        self.message = field(state, "statusMessage").into();
        let file = field(state, "fileLabel");
        if !file.is_empty() {
            self.file = file.into();
        }
    }

    fn saved(&mut self, state: &Value) {
        if field(state, "statusLevel") == "error" {
            self.status(state);
        } else {
            let document = self.document;
            *self = Self::load(state);
            self.document = document;
        }
    }

    fn error_line(&self) -> Option<usize> {
        static LINE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)(?:строк[аеи]|\bline)\s*:?\s*(\d+)").unwrap());
        if self.level != "error" {
            return None;
        }
        LINE.captures(&self.message)?
            .get(1)?
            .as_str()
            .parse()
            .ok()
            .filter(|&line| line > 0)
    }
}

enum Replace {
    Config(String),
    Template(String),
    Import,
    Reset,
}

#[derive(Default)]
pub(super) struct ConfigsPage {
    core: usize,
    editors: [Option<Editor>; CORES.len()],
    pending: Option<Replace>,
}

impl ConfigsPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr("Конфиги", &[]))
                    .size(theme.font_title as f32)
                    .strong(),
            );
            for (index, (_, label)) in CORES.iter().enumerate() {
                ui.selectable_value(&mut self.core, index, *label);
            }
        });
        let (core, name) = CORES[self.core];
        let editor =
            self.editors[self.core].get_or_insert_with(|| Editor::load(&bridge.load_config(core)));
        let replacement = sources(ui, theme, editor, bridge, core);
        if let Some(action) = replacement {
            if editor.dirty() {
                self.pending = Some(action);
            } else {
                replace(editor, action, core, name, bridge);
            }
        }

        card(theme, 10.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            editor_toolbar(ui, theme, editor, core, bridge, self.pending.is_none());
            let footer = theme.control_height.max(28) as f32 + ui.spacing().item_spacing.y;
            let height = (ui.available_height() - footer).max(30.0);
            code_editor::show(
                ui,
                theme,
                (core, editor.document),
                &format!("Raw {name}.json"),
                &mut editor.text,
                height,
                editor.jump_to.take(),
            );
            editor_status(ui, theme, editor);
        });

        if self.pending.is_some() {
            let dialog = Modal::new(Id::unique("configs-discard")).show(page.ctx, |ui| {
                ui.set_width(380.0_f32.min((page.ctx.content_rect().width() - 48.0).max(0.0)));
                ui.label(
                    RichText::new(tr("Несохранённые изменения", &[]))
                        .size(theme.font_strong as f32)
                        .strong(),
                );
                ui.label(tr(
                    "В текущем редакторе есть несохранённые изменения. Отменить их и продолжить?",
                    &[],
                ));
                ui.horizontal_wrapped(|ui| {
                    let cancel = ui.button(tr("Отмена", &[])).clicked();
                    let discard = ui
                        .add(Button::new(tr("Отменить изменения", &[])).fill(theme.danger_fill))
                        .clicked();
                    (cancel, discard)
                })
                .inner
            });
            let (cancel, discard) = dialog.inner;
            if discard && let Some(action) = self.pending.take() {
                replace(editor, action, core, name, bridge);
            } else if cancel || dialog.should_close() {
                self.pending = None;
            }
        }
    }
}

fn sources(
    ui: &mut Ui,
    theme: &Theme,
    editor: &Editor,
    bridge: &mut AppBridge,
    core: &str,
) -> Option<Replace> {
    let mut action = None;
    card(theme, 14.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Конфиг", &[]));
            if let Some(value) = profile_picker(
                ui,
                "configs-active",
                &editor.configs,
                &editor.active_config,
                "Нет конфигов",
            ) {
                action = Some(Replace::Config(value));
            }
            if ui
                .button(tr("Папка", &[]))
                .on_hover_text(tr("Открыть папку конфигов", &[]))
                .clicked()
                && let Some(path) = bridge.config_directory(core)
            {
                win_native::open_directory(&path);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Шаблон", &[]));
            if let Some(value) = profile_picker(
                ui,
                "configs-template",
                &editor.templates,
                &editor.active_template,
                "Шаблон не выбран",
            ) {
                action = Some(Replace::Template(value));
            }
            if ui
                .button(tr("Импорт", &[]))
                .on_hover_text(tr("Импорт шаблона", &[]))
                .clicked()
            {
                action = Some(Replace::Import);
            }
            if ui
                .add_enabled(editor.has_template, Button::new(tr("Сбросить", &[])))
                .on_hover_text(tr("Сбросить к шаблону", &[]))
                .clicked()
            {
                action = Some(Replace::Reset);
            }
        });
    });
    action
}

fn profile_picker(
    ui: &mut Ui,
    id: &str,
    items: &[Value],
    active: &str,
    empty: &str,
) -> Option<String> {
    let selected = items
        .iter()
        .find(|item| field(item, "value") == active)
        .or_else(|| items.first());
    let label = selected
        .map(|item| field(item, "label").to_string())
        .unwrap_or_else(|| tr(empty, &[]));
    let width = (ui.available_width() * 0.52).clamp(110.0, 360.0);
    let mut selection = None;
    ui.add_enabled_ui(!items.is_empty(), |ui| {
        ComboBox::from_id_salt(id)
            .selected_text(label)
            .width(width)
            .truncate()
            .show_ui(ui, |ui| {
                for item in items {
                    let path = field(item, "value");
                    if ui
                        .selectable_label(path == active, field(item, "label"))
                        .clicked()
                    {
                        selection = Some(path.into());
                        ui.close();
                    }
                }
            });
    });
    selection
}

fn replace(editor: &mut Editor, action: Replace, core: &str, name: &str, bridge: &mut AppBridge) {
    let state = match action {
        Replace::Config(path) => bridge.select_config(core, &path),
        Replace::Template(path) => bridge.select_template(core, &path),
        Replace::Reset => bridge.reset_config(core),
        Replace::Import => {
            let directory = bridge.config_template_directory(core);
            let title = tr("Импортировать {title} template", &[("title", name)]);
            match win_native::pick_json_file(&directory, &title) {
                Ok(Some(path)) => bridge.import_config_template(core, &path),
                Ok(None) => return,
                Err(error) => {
                    editor.level = "error".into();
                    editor.message = error.to_string();
                    return;
                }
            }
        }
    };
    editor.saved(&state);
    if editor.level != "error" {
        editor.document = editor.document.wrapping_add(1);
    }
}

fn editor_toolbar(
    ui: &mut Ui,
    theme: &Theme,
    editor: &mut Editor,
    core: &str,
    bridge: &mut AppBridge,
    shortcuts: bool,
) {
    ui.horizontal_wrapped(|ui| {
        let file = if editor.file.is_empty() {
            "--"
        } else {
            &editor.file
        };
        ui.add(egui::Label::new(RichText::new(file).strong()).truncate());
        if editor.dirty() {
            ui.label(RichText::new(tr("Изменено", &[])).color(theme.warning));
        }
    });
    let keyboard_save =
        shortcuts && ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::S));
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                editor.dirty(),
                Button::new(tr("Отменить несохранённые правки", &[])),
            )
            .clicked()
        {
            editor.text.clone_from(&editor.saved_text);
        }
        if ui
            .add_enabled(
                !editor.text.is_empty(),
                Button::new(tr("Форматировать JSON", &[])),
            )
            .clicked()
        {
            match format_json(&editor.text) {
                Ok(formatted) if formatted != editor.text => {
                    editor.text = formatted;
                    editor.level = "info".into();
                    editor.message = tr("JSON отформатирован — не забудьте сохранить", &[]);
                }
                Ok(_) => {}
                Err(_) => editor.status(&bridge.validate_config(core, &editor.text)),
            }
        }
        if ui.button(tr("Проверить", &[])).clicked() {
            editor.status(&bridge.validate_config(core, &editor.text));
        }
        if ui
            .button(tr("Сохранить", &[]))
            .on_hover_text("Ctrl+S")
            .clicked()
            || keyboard_save
        {
            let saved = bridge.save_config(core, &editor.text, &editor.saved_text);
            editor.saved(&saved);
        }
        let apply = Button::new(RichText::new(tr("Применить", &[])).color(theme.accent_text))
            .fill(theme.accent);
        if ui
            .add(apply)
            .on_hover_text(tr("Сохранить и перезапустить ядро с этим конфигом", &[]))
            .clicked()
        {
            let status = bridge.apply_config(core, &editor.text, &editor.saved_text);
            editor.status(&status);
            if editor.level != "error" {
                editor.saved_text.clone_from(&editor.text);
            }
        }
    });
}

fn editor_status(ui: &mut Ui, theme: &Theme, editor: &mut Editor) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let count = code_editor::line_count(&editor.text).to_string();
        ui.label(
            RichText::new(tr("Строк: {count}", &[("count", &count)]))
                .color(theme.text_faint)
                .size(theme.font_small as f32),
        );
        if let Some(line) = editor.error_line()
            && ui
                .button(tr("К строке {line}", &[("line", &line.to_string())]))
                .clicked()
        {
            editor.jump_to = Some(line);
        }
        let (message, color) = if editor.message.is_empty() {
            (
                if editor.dirty() {
                    tr("Есть несохранённые изменения", &[])
                } else {
                    String::new()
                },
                theme.text_muted,
            )
        } else {
            let color = match editor.level.as_str() {
                "success" => theme.success,
                "warning" => theme.warning,
                "error" => theme.danger,
                _ => theme.text_muted,
            };
            (editor.message.clone(), color)
        };
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(message)
                            .color(color)
                            .size(theme.font_small as f32),
                    )
                    .truncate(),
                );
            },
        );
    });
}

fn card(theme: &Theme, margin: f32) -> Frame {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(margin)
}

fn field<'a>(state: &'a Value, key: &str) -> &'a str {
    state[key].as_str().unwrap_or_default()
}

fn format_json(text: &str) -> serde_json::Result<String> {
    serde_json::from_str::<Value>(text)?;
    static TOKENS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""(?:\\.|[^"\\])*"|[{}\[\],:]|[^\s{}\[\],:"]+"#).unwrap());
    let mut tokens = TOKENS
        .find_iter(text)
        .map(|token| token.as_str())
        .peekable();
    let mut output = String::new();
    let mut depth = 0;
    let mut previous = "";
    let newline = |output: &mut String, depth: usize| {
        output.push('\n');
        output.push_str(&"  ".repeat(depth));
    };
    while let Some(token) = tokens.next() {
        match token {
            "{" | "[" => {
                output.push_str(token);
                depth += 1;
                if !matches!(tokens.peek(), Some(&"}" | &"]")) {
                    newline(&mut output, depth);
                }
            }
            "}" | "]" => {
                depth -= 1;
                if !matches!(previous, "{" | "[") {
                    newline(&mut output, depth);
                }
                output.push_str(token);
            }
            "," => {
                output.push(',');
                newline(&mut output, depth);
            }
            ":" => output.push_str(": "),
            _ => output.push_str(token),
        }
        previous = token;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_save_keeps_previous_text_for_dirty_check_and_retry() {
        let mut editor = Editor::load(&serde_json::json!({"text":"{}"}));
        editor.text = "{broken".into();
        editor.saved(
            &serde_json::json!({"text":"{broken","statusLevel":"error","statusMessage":"line 1"}),
        );
        assert_eq!(editor.saved_text, "{}");
        assert!(editor.dirty());
        assert_eq!(editor.error_line(), Some(1));
    }

    #[test]
    fn formatting_keeps_key_order_and_json_string_contents() {
        let input = r#"{"z":[{},"a,\"б"],"a":true}"#;
        let expected = "{\n  \"z\": [\n    {},\n    \"a,\\\"б\"\n  ],\n  \"a\": true\n}";
        assert_eq!(format_json(input).unwrap(), expected);
        assert!(format_json("{broken").is_err());
    }
}
