//! Protocol-aware server creation and editing shared by Dashboard and Nodes.

use egui::{
    Align, Align2, Color32, Context, FontId, Frame, Id, Layout, Modal, Rect, ScrollArea, Sense,
    Stroke, TextEdit, Ui, Vec2, pos2, vec2,
};
use lumen_core::i18n::tr;
use serde_json::{Map, Value, json};

use crate::shell::theme::Theme;
use crate::shell::widgets::{self, ButtonKind, FluentButton};
use crate::view_models::AppBridge;

#[derive(Default)]
pub(super) struct NodeEditor {
    draft: Option<Draft>,
}

struct Draft {
    node: Option<String>,
    name: String,
    group: String,
    protocol: String,
    specs: Vec<Value>,
    values: Map<String, Value>,
    hint: String,
}

impl Draft {
    fn new(node: Option<String>, data: &Value) -> Self {
        let mut draft = Self {
            node,
            name: field(data, "name").into(),
            group: field(data, "group").into(),
            protocol: String::new(),
            specs: Vec::new(),
            values: Map::new(),
            hint: String::new(),
        };
        draft.load_protocol(data);
        draft
    }

    fn load_protocol(&mut self, data: &Value) {
        self.protocol = field(data, "protocolKey").into();
        self.hint = field(data, "editHint").into();
        self.specs = data["protocolFields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        self.values = self
            .specs
            .iter()
            .map(|spec| (field(spec, "key").into(), spec["value"].clone()))
            .collect();
    }

    fn payload(&self) -> Value {
        let mut data = self.values.clone();
        data.insert("name".into(), json!(self.name));
        data.insert("group".into(), json!(self.group));
        data.insert("protocol".into(), json!(self.protocol));
        Value::Object(data)
    }
}

impl NodeEditor {
    pub(super) fn edit(&mut self, bridge: &AppBridge, id: &str) {
        self.draft = Some(Draft::new(Some(id.into()), &bridge.node_edit_fields(id)));
    }

    pub(super) fn create(&mut self, bridge: &AppBridge, group: &str) {
        let group = if group.is_empty() { "Default" } else { group };
        let mut draft = Draft::new(None, &bridge.manual_node_fields("vless", group));
        draft.name.clear();
        draft.group = group.into();
        self.draft = Some(draft);
    }

    pub(super) fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    pub(super) fn render(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        bridge: &mut AppBridge,
    ) -> Option<String> {
        let draft = self.draft.as_mut()?;
        let creating = draft.node.is_none();
        let screen = ctx.content_rect();
        // `width: min(760, overlay - 40)`, `height: min(720, overlay - 48)`.
        let width = 760.0_f32.min(screen.width() - 40.0).max(0.0);
        let height = 720.0_f32.min(screen.height() - 48.0).max(0.0);
        let control = theme.control_height as f32;
        let modal = Modal::new(Id::unique("node-editor"))
            // `Overlay.modal: Qt.rgba(0, 0, 0, 0.4)`.
            .backdrop_color(Color32::from_black_alpha(102))
            .frame(
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                ui.set_min_size(vec2(width, height));
                ui.set_max_size(vec2(width, height));
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let top = ui.cursor().top();
                // Title: margins 20 / 20, top 18, bottom 12, bold fontTitle.
                ui.add_space(18.0);
                let title = tr(if creating { "Новый сервер" } else { "Редактирование сервера" }, &[]);
                let galley = ui.painter().layout(
                    title,
                    crate::shell::theme::bold(theme.font_title as f32),
                    theme.text,
                    width - 40.0,
                );
                let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
                ui.painter().galley(pos2(rect.left() + 20.0, rect.top()), galley, theme.text);
                ui.add_space(12.0);

                // The form scrolls between the title and the button row
                // (12 px above it, 20 px below).
                let buttons = 12.0 + control + 20.0;
                let form_height = (height - (ui.cursor().top() - top) - buttons).max(40.0);
                let (form_rect, _) = ui.allocate_exact_size(vec2(width, form_height), Sense::hover());
                let mut form_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_max(
                            pos2(form_rect.left() + 20.0, form_rect.top()),
                            pos2(form_rect.right() - 14.0, form_rect.bottom()),
                        ))
                        .layout(Layout::top_down(Align::LEFT)),
                );
                ScrollArea::vertical()
                    .id_salt("node-editor-form")
                    .auto_shrink([false, false])
                    .max_height(form_height)
                    .show(&mut form_ui, |ui| {
                        // FluentScrollBar sits in the 14 px right margin.
                        ui.set_width(ui.available_width());
                        form(ui, theme, bridge, draft, creating);
                    });

                ui.add_space(12.0);
                let (bar, _) = ui.allocate_exact_size(vec2(width, control), Sense::hover());
                let mut bar_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar.shrink2(vec2(20.0, 0.0)))
                        .layout(Layout::right_to_left(Align::Center)),
                );
                bar_ui.spacing_mut().item_spacing.x = 10.0;
                let save = FluentButton::new(ButtonKind::Accent, "", &tr("Сохранить", &[]))
                    .show(&mut bar_ui, theme)
                    .clicked();
                let cancel = FluentButton::new(ButtonKind::Ghost, "", &tr("Отмена", &[]))
                    .show(&mut bar_ui, theme)
                    .clicked();
                ui.add_space(20.0);
                (cancel, save)
            });
        let (cancel, save) = modal.inner;
        // `closePolicy: Popup.CloseOnEscape`: a click on the backdrop keeps
        // the editor open.
        let escape = modal.should_close() && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if save {
            let payload = draft.payload();
            if let Some(id) = &draft.node {
                bridge.save_node_edit(id, &payload);
                self.draft = None;
            } else {
                let id = bridge.create_manual_node(&payload);
                if !id.is_empty() {
                    self.draft = None;
                    return Some(id);
                }
            }
        } else if cancel || escape {
            self.draft = None;
        }
        None
    }
}

/// `formColumn`: rows 10 px apart, each an `FLabel` (185 px, muted small,
/// wrapped) 14 px before its editor.
fn form(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, draft: &mut Draft, creating: bool) {
    let control = theme.control_height as f32;
    labeled(ui, theme, "Псевдоним", |ui, width| {
        text_field(ui, theme, "node-edit-name", &mut draft.name, "", width);
    });
    ui.add_space(10.0);
    labeled(ui, theme, "Группа", |ui, width| {
        text_field(ui, theme, "node-edit-group", &mut draft.group, "", width);
    });
    ui.add_space(10.0);
    labeled(ui, theme, "Протокол", |ui, width| {
        if creating {
            let protocols = bridge.manual_node_protocols();
            let current = protocols.iter().position(|option| *option == draft.protocol);
            if let Some(index) =
                widgets::combo(ui, theme, "node-edit-protocol", width, &protocols, current, true)
                && let Some(protocol) = protocols.get(index)
                && *protocol != draft.protocol
            {
                let protocol = protocol.to_string();
                draft.load_protocol(&bridge.manual_node_fields(&protocol, &draft.group));
            }
        } else {
            // Upper-case protocol name, 10 px in, centred on the row.
            let (rect, _) = ui.allocate_exact_size(vec2(width, control), Sense::hover());
            let text = if draft.protocol.is_empty() {
                "?".to_string()
            } else {
                draft.protocol.to_uppercase()
            };
            ui.painter().text(
                pos2(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(theme.font_normal as f32),
                theme.text,
            );
        }
    });
    if !draft.hint.is_empty() {
        ui.add_space(10.0);
        let width = ui.available_width();
        let galley = ui.painter().layout(
            tr(&draft.hint, &[]),
            FontId::proportional(theme.font_small as f32),
            theme.text_faint,
            width,
        );
        let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
        ui.painter().galley(rect.min, galley, theme.text_faint);
    }
    if !draft.specs.is_empty() {
        ui.add_space(10.0);
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, theme.divider);
    }
    let specs = draft.specs.clone();
    for spec in &specs {
        if !visible(spec, &draft.values) {
            continue;
        }
        ui.add_space(10.0);
        let key = field(spec, "key").to_string();
        let label = spec["label"].as_str().unwrap_or(&key).to_string();
        labeled(ui, theme, &label, |ui, width| match field(spec, "kind") {
            "bool" => {
                let (rect, _) = ui.allocate_exact_size(vec2(width, control), Sense::hover());
                let mut host = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let checked = draft.values.get(&key).and_then(Value::as_bool).unwrap_or(false);
                if let Some(enabled) = super::settings::switch_widget(&mut host, theme, checked, true) {
                    draft.values.insert(key.clone(), Value::Bool(enabled));
                    if key == "amneziaEnabled" && enabled && draft.protocol == "wireguard" {
                        amnezia_defaults(&mut draft.values);
                    }
                }
            }
            "combo" => {
                let options: Vec<&str> = spec["options"]
                    .as_array()
                    .map(|options| options.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                // `syncCurrentValue`: an unknown value falls back to the first option.
                let current = draft.values.get(&key).map(string_value).unwrap_or_default();
                let index = options.iter().position(|option| *option == current).unwrap_or(0);
                if let Some(option) = options.get(index) {
                    draft.values.insert(key.clone(), Value::String((*option).to_string()));
                }
                if let Some(picked) = widgets::combo(
                    ui,
                    theme,
                    ("node-edit-field", &key),
                    width,
                    &options,
                    Some(index),
                    true,
                ) && let Some(option) = options.get(picked)
                {
                    draft.values.insert(key.clone(), Value::String((*option).to_string()));
                }
            }
            _ => {
                let mut text = draft.values.get(&key).map(string_value).unwrap_or_default();
                if text_field(
                    ui,
                    theme,
                    ("node-edit-field", &key),
                    &mut text,
                    field(spec, "placeholder"),
                    width,
                ) {
                    draft.values.insert(key.clone(), Value::String(text));
                }
            }
        });
    }
    ui.add_space(4.0);
}

/// One `RowLayout { FLabel; editor }`: the label column is 185 px and
/// vertically centred against the editor.
fn labeled(ui: &mut Ui, theme: &Theme, label: &str, editor: impl FnOnce(&mut Ui, f32)) {
    let control = theme.control_height as f32;
    let width = ui.available_width();
    let galley = ui.painter().layout(
        tr(label, &[]),
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        185.0,
    );
    let height = galley.size().y.max(control);
    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().galley(
        pos2(row.left(), row.center().y - galley.size().y / 2.0),
        galley,
        theme.text_muted,
    );
    let editor_rect = Rect::from_min_max(
        pos2(row.left() + 185.0 + 14.0, row.center().y - control / 2.0),
        pos2(row.right(), row.center().y + control / 2.0),
    );
    let mut editor_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(editor_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    editor(&mut editor_ui, editor_rect.width().max(0.0));
}

/// `FField`: a control-height Fluent text field with 10 px side padding.
fn text_field(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: impl egui::AsIdSalt,
    value: &mut String,
    hint: &str,
    width: f32,
) -> bool {
    let editor = TextEdit::singleline(value)
        .hint_text(tr(hint, &[]))
        .font(FontId::proportional(theme.font_normal as f32))
        .desired_width(width - 20.0)
        .min_size(vec2(width, theme.control_height as f32))
        .margin(egui::Margin::symmetric(10, 0))
        .vertical_align(Align::Center)
        // FField background: `Theme.controlFill` (egui uses the darker
        // extreme background for text fields).
        .background_color(theme.control_fill);
    super::text_edit_menu::show(ui, editor, id_salt, true, false).changed()
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn string_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn visible(spec: &Value, values: &Map<String, Value>) -> bool {
    let Some(key) = spec["whenKey"].as_str() else {
        return true;
    };
    let Some(options) = spec["whenValues"].as_array() else {
        return true;
    };
    let current = values.get(key).map(string_value).unwrap_or_default();
    options
        .iter()
        .any(|option| option.as_str() == Some(&current))
}

fn amnezia_defaults(values: &mut Map<String, Value>) {
    for (key, default) in [
        ("awg_jc", json!(4)),
        ("awg_jmin", json!(40)),
        ("awg_jmax", json!(70)),
        ("awg_s1", json!(0)),
        ("awg_s2", json!(0)),
        ("awg_s3", json!(0)),
        ("awg_s4", json!(0)),
        ("awg_h1", json!("1")),
        ("awg_h2", json!("2")),
        ("awg_h3", json!("3")),
        ("awg_h4", json!("4")),
    ] {
        let value = values.entry(key).or_insert(Value::Null);
        if value.is_null() || value.as_str() == Some("") {
            *value = default;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_fields_and_defaults_preserve_explicit_wireguard_values() {
        let mut values = Map::from_iter([
            ("amneziaEnabled".into(), json!(true)),
            ("awg_jc".into(), json!(9)),
        ]);
        amnezia_defaults(&mut values);
        assert_eq!(values["awg_jc"], 9);
        assert_eq!(values["awg_jmin"], 40);
        assert!(visible(
            &json!({"whenKey":"amneziaEnabled", "whenValues":["true"]}),
            &values
        ));
    }
}
