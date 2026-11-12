//! Protocol-aware server creation and editing shared by Dashboard and Nodes.

use egui::{Button, ComboBox, Context, Id, Modal, RichText, ScrollArea, TextEdit, Ui};
use lumen_core::i18n::tr;
use serde_json::{Map, Value, json};

use crate::shell::theme::Theme;
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
        let modal = Modal::new(Id::unique("node-editor")).show(ctx, |ui| {
            ui.set_width(590.0_f32.min((ctx.content_rect().width() - 52.0).max(0.0)));
            let title = if creating {
                "Новый сервер"
            } else {
                "Редактирование сервера"
            };
            ui.label(
                RichText::new(tr(title, &[]))
                    .size(theme.font_title as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 148.0).max(100.0))
                .show(ui, |ui| {
                    input(ui, "Псевдоним", &mut draft.name, "");
                    input(ui, "Группа", &mut draft.group, "");
                    ui.label(tr("Протокол", &[]));
                    if creating {
                        let mut protocol = draft.protocol.clone();
                        ComboBox::from_id_salt("node-edit-protocol").icon(crate::shell::widgets::combo_chevron)
                            .selected_text(&protocol)
                            .show_ui(ui, |ui| {
                                for option in bridge.manual_node_protocols() {
                                    ui.selectable_value(&mut protocol, option.to_string(), option);
                                }
                            });
                        if protocol != draft.protocol {
                            draft
                                .load_protocol(&bridge.manual_node_fields(&protocol, &draft.group));
                        }
                    } else {
                        ui.label(&draft.protocol);
                    }
                    if !draft.hint.is_empty() {
                        ui.label(RichText::new(tr(&draft.hint, &[])).color(theme.text_muted));
                    }
                    ui.separator();
                    for spec in &draft.specs {
                        let key = field(spec, "key");
                        if !visible(spec, &draft.values) {
                            continue;
                        }
                        let label = spec["label"].as_str().unwrap_or(key);
                        let value = draft.values.entry(key.to_string()).or_insert(Value::Null);
                        match field(spec, "kind") {
                            "bool" => {
                                let mut enabled = value.as_bool().unwrap_or(false);
                                if ui.checkbox(&mut enabled, tr(label, &[])).changed() {
                                    *value = Value::Bool(enabled);
                                    if key == "amneziaEnabled"
                                        && enabled
                                        && draft.protocol == "wireguard"
                                    {
                                        amnezia_defaults(&mut draft.values);
                                    }
                                }
                            }
                            "combo" => {
                                ui.label(tr(label, &[]));
                                let options = spec["options"]
                                    .as_array()
                                    .map(Vec::as_slice)
                                    .unwrap_or_default();
                                let mut selected = string_value(value);
                                if !options
                                    .iter()
                                    .any(|option| option.as_str() == Some(&selected))
                                    && let Some(first) = options.first().and_then(Value::as_str)
                                {
                                    selected = first.into();
                                }
                                ComboBox::from_id_salt(("node-edit-field", key)).icon(crate::shell::widgets::combo_chevron)
                                    .selected_text(&selected)
                                    .show_ui(ui, |ui| {
                                        for option in options.iter().filter_map(Value::as_str) {
                                            ui.selectable_value(
                                                &mut selected,
                                                option.to_string(),
                                                option,
                                            );
                                        }
                                    });
                                *value = Value::String(selected);
                            }
                            _ => {
                                let mut text = string_value(value);
                                if input(ui, label, &mut text, field(spec, "placeholder")) {
                                    *value = Value::String(text);
                                }
                            }
                        }
                    }
                });
            ui.horizontal_wrapped(|ui| {
                let cancel = ui.button(tr("Отмена", &[])).clicked();
                let save = ui
                    .add(
                        Button::new(RichText::new(tr("Сохранить", &[])).color(theme.accent_text))
                            .fill(theme.accent),
                    )
                    .clicked();
                (cancel, save)
            })
            .inner
        });
        let (cancel, save) = modal.inner;
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
        } else if cancel || modal.should_close() {
            self.draft = None;
        }
        None
    }
}

fn input(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> bool {
    ui.label(tr(label, &[]));
    ui.add(
        TextEdit::singleline(value)
            .hint_text(tr(hint, &[]))
            .desired_width(f32::INFINITY),
    )
    .changed()
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
