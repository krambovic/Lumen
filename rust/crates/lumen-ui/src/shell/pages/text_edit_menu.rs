//! Shared editing context menu for egui text fields and read-only text.

use egui::{Button, Event, Key, Response, TextEdit, Ui, text::CCursorRange};
use lumen_core::i18n::tr;

pub(super) fn show(
    ui: &mut Ui,
    editor: TextEdit<'_>,
    id_salt: impl egui::AsIdSalt,
    editable: bool,
    reset_state: bool,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    if reset_state {
        ui.ctx().data_mut(|data| {
            data.remove::<egui::text_edit::TextEditState>(id);
            data.remove::<Vec<Event>>(id.with("pending-editor-event"));
        });
    }
    inject_pending_event(ui, id);
    let output = editor.id(id).show(ui);
    let text = output.galley.text();
    let response = output.response.response;
    let mut state = output.state;
    let selection = state.cursor.char_range();
    let has_selection = selection.is_some_and(|range| !range.is_empty());
    let has_text = !text.is_empty();
    // TextEditContextMenu.qml: a 190 px FluentMenu without icons. Unit
    // tests have no published theme and fall back to egui's own menu.
    let theme = crate::shell::fluent_menu::published_theme(ui.ctx());
    let menu = |ui: &mut Ui| {
        let (has_undo, has_redo) = if editable {
            let current = (selection.unwrap_or_default(), text.to_owned());
            let undoer = state.undoer();
            (undoer.has_undo(&current), undoer.has_redo(&current))
        } else {
            (false, false)
        };
        let entry = |ui: &mut Ui, label: &str, enabled: bool| match theme.as_deref() {
            Some(theme) => crate::shell::fluent_menu::Item::new("", &tr(label, &[]))
                .enabled(enabled)
                .show(ui, theme, false),
            None => {
                let clicked = ui.add_enabled(enabled, Button::new(tr(label, &[]))).clicked();
                if clicked {
                    ui.close();
                }
                clicked
            }
        };
        let separator = |ui: &mut Ui| match theme.as_deref() {
            Some(theme) => crate::shell::fluent_menu::separator(ui, theme),
            None => {
                ui.separator();
            }
        };
        let mut chosen = None;
        if entry(ui, "Отменить", editable && has_undo) {
            chosen = Some("undo");
        }
        if entry(ui, "Повторить", editable && has_redo) {
            chosen = Some("redo");
        }
        separator(ui);
        if entry(ui, "Вырезать", editable && has_selection) {
            chosen = Some("cut");
        }
        if entry(ui, "Копировать", has_selection) {
            chosen = Some("copy");
        }
        if entry(ui, "Вставить", editable) {
            chosen = Some("paste");
        }
        if entry(ui, "Удалить", editable && has_selection) {
            chosen = Some("delete");
        }
        separator(ui);
        if entry(ui, "Выделить всё", has_text) {
            chosen = Some("select_all");
        }
        chosen
    };
    let chosen = match theme.as_deref() {
        Some(theme) => crate::shell::fluent_menu::context_menu(&response, theme, 190.0, menu),
        None => egui::Popup::context_menu(&response)
            .show(|ui| {
                ui.set_min_width(170.0);
                menu(ui)
            })
            .map(|inner| inner.inner),
    };
    let action = chosen.flatten();

    if let Some(action) = action {
        match action {
            "undo" | "redo" => {
                let mut modifiers = egui::Modifiers::COMMAND;
                modifiers.shift = action == "redo";
                queue_edit_event(ui, response.id, key_event(Key::Z, modifiers));
            }
            "select_all" => {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::select_all_str(text)));
                state.store(ui.ctx(), response.id);
            }
            "copy" => {
                queue_edit_event(ui, response.id, Event::Copy);
            }
            "cut" => {
                queue_edit_event(ui, response.id, Event::Cut);
            }
            "delete" => {
                queue_edit_event(
                    ui,
                    response.id,
                    key_event(Key::Delete, egui::Modifiers::NONE),
                );
            }
            "paste" => {
                if let Some(value) = crate::shell::clipboard::read_text() {
                    queue_edit_event(ui, response.id, Event::Paste(value));
                }
            }
            _ => unreachable!(),
        }
        response.request_focus();
    }
    response
}

fn key_event(key: Key, modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn queue_edit_event(ui: &Ui, id: egui::Id, event: Event) {
    ui.ctx().data_mut(|data| {
        data.insert_temp(id.with("pending-editor-event"), vec![event]);
    });
    ui.ctx().request_repaint();
}

fn inject_pending_event(ui: &Ui, id: egui::Id) {
    if ui.ctx().memory(|memory| memory.focused() != Some(id)) {
        return;
    }
    let pending = ui
        .ctx()
        .data_mut(|data| data.remove_temp::<Vec<Event>>(id.with("pending-editor-event")));
    if let Some(event) = pending.and_then(|mut events| events.pop()) {
        ui.ctx().input_mut(|input| input.events.push(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_paste_replaces_the_selection_in_the_focused_field() {
        let ctx = egui::Context::default();
        let mut text = "abcd".to_owned();

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let id = ui.make_persistent_id("subscription-url-field");
            ui.ctx().memory_mut(|memory| memory.request_focus(id));
            let mut state = egui::text_edit::TextEditState::default();
            state.cursor.set_char_range(Some(CCursorRange::two(
                egui::text::CCursor::new(1),
                egui::text::CCursor::new(3),
            )));
            state.store(ui.ctx(), id);
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    id.with("pending-editor-event"),
                    vec![Event::Paste("X".into())],
                );
            });
            show(
                ui,
                TextEdit::singleline(&mut text),
                "subscription-url-field",
                true,
                false,
            );
        });
        output.textures_delta.clear();

        assert_eq!(text, "aXd");
    }

    #[test]
    fn queued_copy_exports_selected_read_only_text() {
        let ctx = egui::Context::default();
        let mut text = "строка журнала";
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let id = ui.make_persistent_id("read-only-log");
            ui.ctx().memory_mut(|memory| memory.request_focus(id));
            let mut state = egui::text_edit::TextEditState::default();
            state.cursor.set_char_range(Some(CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(6),
            )));
            state.store(ui.ctx(), id);
            queue_edit_event(ui, id, Event::Copy);
            show(
                ui,
                TextEdit::multiline(&mut text),
                "read-only-log",
                false,
                false,
            );
        });
        output.textures_delta.clear();

        assert!(output.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(value) if value == "строка")
        }));
    }
}
