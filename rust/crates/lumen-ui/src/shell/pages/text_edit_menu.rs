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
    let mut action = None;

    response.context_menu(|ui| {
        let (has_undo, has_redo) = if editable {
            let current = (selection.unwrap_or_default(), text.to_owned());
            let undoer = state.undoer();
            (undoer.has_undo(&current), undoer.has_redo(&current))
        } else {
            (false, false)
        };
        ui.set_min_width(170.0);
        if ui
            .add_enabled(editable && has_undo, Button::new(tr("Отменить", &[])))
            .clicked()
        {
            action = Some("undo");
            ui.close();
        }
        if ui
            .add_enabled(editable && has_redo, Button::new(tr("Повторить", &[])))
            .clicked()
        {
            action = Some("redo");
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(editable && has_selection, Button::new(tr("Вырезать", &[])))
            .clicked()
        {
            action = Some("cut");
            ui.close();
        }
        if ui
            .add_enabled(has_selection, Button::new(tr("Копировать", &[])))
            .clicked()
        {
            action = Some("copy");
            ui.close();
        }
        if ui
            .add_enabled(editable, Button::new(tr("Вставить", &[])))
            .clicked()
        {
            action = Some("paste");
            ui.close();
        }
        if ui
            .add_enabled(editable && has_selection, Button::new(tr("Удалить", &[])))
            .clicked()
        {
            action = Some("delete");
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_text, Button::new(tr("Выделить всё", &[])))
            .clicked()
        {
            action = Some("select_all");
            ui.close();
        }
    });

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
