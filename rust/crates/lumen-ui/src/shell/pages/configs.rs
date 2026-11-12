//! Raw sing-box and Xray configuration editors from `ConfigsPage.qml`.

use std::sync::LazyLock;

use egui::{
    Align, Align2, Button, Color32, ComboBox, FontId, Frame, Id, Key, Layout, Modal, Modifiers,
    RichText, Sense, Stroke, Ui, vec2,
};
use lumen_core::i18n::tr;
use regex::Regex;
use serde_json::Value;

use super::PageContext;
use super::{code_editor, fluent_button};
use crate::shell::{
    theme::{Theme, semibold},
    win_native,
};
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
        ui.spacing_mut().item_spacing.y = theme.spacing as f32;
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 34.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                core_selector(ui, theme, &mut self.core);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(
                        RichText::new(tr("Конфиги", &[])).font(semibold(theme.font_title as f32)),
                    );
                });
            },
        );
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

        card(theme, 9.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            editor_toolbar(ui, theme, editor, core, bridge, self.pending.is_none());
            let footer = if editor.error_line().is_some() {
                theme.control_height
            } else {
                theme.font_small + 4
            };
            let height = (ui.available_height() - footer as f32 - 8.0).max(20.0);
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
                ui.spacing_mut().item_spacing.y = 16.0;
                fluent_button::style(ui);
                ui.label(
                    RichText::new(tr("Несохранённые изменения", &[]))
                        .font(semibold(theme.font_strong as f32)),
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

fn core_selector(ui: &mut Ui, theme: &Theme, core: &mut usize) {
    let (track, _) = ui.allocate_exact_size(vec2(216.0, 34.0), Sense::hover());
    ui.painter().rect(
        track,
        theme.radius_small as u8,
        theme.control_fill,
        Stroke::new(1.0, theme.border_solid),
        egui::StrokeKind::Inside,
    );
    for (index, (_, name)) in CORES.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            track.min + vec2(3.0 + 106.0 * index as f32, 3.0),
            vec2(104.0, 28.0),
        );
        let response = ui
            .interact(
                rect,
                ui.scope_id().with(("config-core", index)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            *core = index;
        }
        let active = *core == index;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, *name)
        });
        let fill = if active {
            theme.card
        } else if response.hovered() {
            theme.nav_hover
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if response.has_focus() {
            Stroke::new(1.0, theme.accent)
        } else if active {
            Stroke::new(1.0, theme.border_solid)
        } else {
            Stroke::NONE
        };
        ui.painter().rect(
            rect,
            theme.radius_small.saturating_sub(1) as u8,
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        let size = theme.font_normal as f32;
        let (font, color) = if active {
            (semibold(size), theme.text)
        } else {
            (FontId::proportional(size), theme.text_muted)
        };
        ui.painter()
            .text(rect.center(), Align2::CENTER_CENTER, *name, font, color);
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
    card(theme, 13.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        let control_height = theme.control_height as f32;
        let import = tr("Импорт", &[]);
        let action_width = ui
            .painter()
            .layout_no_wrap(
                import.clone(),
                FontId::proportional(theme.font_normal as f32),
                theme.text,
            )
            .size()
            .x
            + theme.font_normal as f32
            + 8.0
            + 28.0
            + 6.0
            + control_height;
        let combo_width = (ui.available_width() - 72.0 - 20.0 - action_width).max(50.0);
        ui.horizontal(|ui| {
            source_row_style(ui, control_height);
            field_label(ui, theme, "Конфиг");
            if let Some(value) = profile_picker(
                ui,
                "configs-active",
                &editor.configs,
                &editor.active_config,
                "Нет конфигов",
                combo_width,
            ) {
                action = Some(Replace::Config(value));
            }
            if ui
                .add(fluent_button::button(
                    theme,
                    "\u{e838}",
                    &tr("Папка", &[]),
                    false,
                ))
                .on_hover_text(tr("Открыть папку конфигов", &[]))
                .clicked()
                && let Some(path) = bridge.config_directory(core)
            {
                win_native::open_directory(&path);
            }
        });
        ui.horizontal(|ui| {
            source_row_style(ui, control_height);
            field_label(ui, theme, "Шаблон");
            if let Some(value) = profile_picker(
                ui,
                "configs-template",
                &editor.templates,
                &editor.active_template,
                "Шаблон не выбран",
                combo_width,
            ) {
                action = Some(Replace::Template(value));
            }
            if ui
                .add(fluent_button::button(theme, "\u{e8b5}", &import, false))
                .on_hover_text(tr("Импорт шаблона", &[]))
                .clicked()
            {
                action = Some(Replace::Import);
            }
            ui.spacing_mut().item_spacing.x = 6.0;
            if fluent_button::icon(
                ui,
                theme,
                "\u{e777}",
                &tr("Сбросить к шаблону", &[]),
                editor.has_template,
            )
            .clicked()
            {
                action = Some(Replace::Reset);
            }
        });
    });
    action
}

fn source_row_style(ui: &mut Ui, height: f32) {
    fluent_button::style(ui);
    ui.spacing_mut().interact_size.y = height;
    ui.spacing_mut().item_spacing.x = 10.0;
}

fn field_label(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.allocate_ui_with_layout(
        vec2(72.0, theme.control_height as f32),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(72.0);
            ui.add(
                egui::Label::new(RichText::new(tr(label, &[])).color(theme.text_muted)).truncate(),
            );
        },
    );
}

fn profile_picker(
    ui: &mut Ui,
    id: &str,
    items: &[Value],
    active: &str,
    empty: &str,
    width: f32,
) -> Option<String> {
    let selected = items
        .iter()
        .find(|item| field(item, "value") == active)
        .or_else(|| items.first());
    let label = selected
        .map(|item| field(item, "label").to_string())
        .unwrap_or_else(|| tr(empty, &[]));
    let mut selection = None;
    ui.add_enabled_ui(!items.is_empty(), |ui| {
        ComboBox::from_id_salt(id).icon(crate::shell::widgets::combo_chevron)
            .selected_text(label)
            .width(width)
            .truncate()
            .icon(|ui, rect, visuals, _| {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "\u{e70d}",
                    FontId::proportional(12.0),
                    visuals.text_color(),
                );
            })
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
    let keyboard_save =
        shortcuts && ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::S));
    let (revert, format, validate, save, apply) = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width(), theme.control_height as f32),
            Layout::right_to_left(Align::Center),
            |ui| {
                fluent_button::style(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                let apply = ui
                    .add(fluent_button::button(
                        theme,
                        "\u{e768}",
                        &tr("Применить", &[]),
                        true,
                    ))
                    .on_hover_text(tr("Сохранить и перезапустить ядро с этим конфигом", &[]))
                    .clicked();
                let save = ui
                    .add(fluent_button::button(
                        theme,
                        "\u{e74e}",
                        &tr("Сохранить", &[]),
                        false,
                    ))
                    .on_hover_text("Ctrl+S")
                    .clicked();
                let validate = ui
                    .add(fluent_button::button(
                        theme,
                        "\u{e73e}",
                        &tr("Проверить", &[]),
                        false,
                    ))
                    .clicked();
                let format = fluent_button::icon(
                    ui,
                    theme,
                    "\u{e8e5}",
                    &tr("Форматировать JSON", &[]),
                    !editor.text.is_empty(),
                )
                .clicked();
                let revert = fluent_button::icon(
                    ui,
                    theme,
                    "\u{e7a7}",
                    &tr("Отменить несохранённые правки", &[]),
                    editor.dirty(),
                )
                .clicked();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(RichText::new("\u{e8a5}").size(16.0).color(theme.text_muted));
                    let file = if editor.file.is_empty() {
                        "--"
                    } else {
                        editor.file.strip_prefix("//?/").unwrap_or(&editor.file)
                    };
                    let dirty = tr("Изменено", &[]);
                    let font = FontId::proportional(theme.font_small as f32);
                    let badge_width = ui
                        .painter()
                        .layout_no_wrap(dirty.clone(), font.clone(), theme.warning)
                        .size()
                        .x
                        + 14.0;
                    let show_badge = editor.dirty() && ui.available_width() > badge_width + 36.0;
                    let remaining = (ui.available_width()
                        - if show_badge { badge_width + 6.0 } else { 0.0 })
                    .clamp(0.0, 320.0);
                    let file_font = semibold(theme.font_normal as f32);
                    let measure = |text: &str| {
                        ui.painter()
                            .layout_no_wrap(text.to_owned(), file_font.clone(), theme.text)
                            .size()
                            .x
                    };
                    let caption = elide_path(file, remaining, measure);
                    ui.add(egui::Label::new(RichText::new(caption).font(file_font)).truncate())
                        .on_hover_text(file);
                    if show_badge {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(badge_width, 18.0), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 9.0, theme.warning.gamma_multiply(0.16));
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            dirty,
                            font,
                            theme.warning,
                        );
                    }
                });
                (revert, format, validate, save, apply)
            },
        )
        .inner;
    if revert {
        editor.text.clone_from(&editor.saved_text);
    }
    if format {
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
    if validate {
        editor.status(&bridge.validate_config(core, &editor.text));
    }
    if save || keyboard_save {
        let saved = bridge.save_config(core, &editor.text, &editor.saved_text);
        editor.saved(&saved);
    }
    if apply {
        let status = bridge.apply_config(core, &editor.text, &editor.saved_text);
        editor.status(&status);
        if editor.level != "error" {
            editor.saved_text.clone_from(&editor.text);
        }
    }
}

fn elide_path(path: &str, width: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(path) <= width {
        return path.to_owned();
    }
    let mut boundaries: Vec<_> = path.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(path.len());
    let len = boundaries.len() - 1;
    let mut low = 0;
    let mut high = len;
    while low < high {
        let kept = (low + high).div_ceil(2);
        let prefix = kept / 2;
        let suffix = len - (kept - prefix);
        let candidate = format!(
            "{}…{}",
            &path[..boundaries[prefix]],
            &path[boundaries[suffix]..]
        );
        if measure(&candidate) <= width {
            low = kept;
        } else {
            high = kept - 1;
        }
    }
    let prefix = low / 2;
    let suffix = len - (low - prefix);
    format!(
        "{}…{}",
        &path[..boundaries[prefix]],
        &path[boundaries[suffix]..]
    )
}

fn editor_status(ui: &mut Ui, theme: &Theme, editor: &mut Editor) {
    let error_line = editor.error_line();
    let height = if error_line.is_some() {
        theme.control_height
    } else {
        theme.font_small + 4
    } as f32;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let count = code_editor::line_count(&editor.text).to_string();
            ui.label(
                RichText::new(tr("Строк: {count}", &[("count", &count)]))
                    .color(theme.text_faint)
                    .size(theme.font_small as f32),
            );
            if let Some(line) = error_line
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
                vec2(ui.available_width(), height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if !editor.message.is_empty() {
                        let glyph = match editor.level.as_str() {
                            "success" => "\u{e73e}",
                            "warning" => "\u{e7ba}",
                            "error" => "\u{ea39}",
                            _ => "\u{e946}",
                        };
                        ui.label(RichText::new(glyph).size(14.0).color(color));
                    }
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
        },
    );
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
