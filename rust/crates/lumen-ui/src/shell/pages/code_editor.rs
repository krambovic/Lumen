//! The plain text editor and fixed line-number gutter from `CodeEditor.qml`.

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{
    Align, Align2, FontId, Id, Pos2, Rect, ScrollArea, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};

use crate::shell::theme::Theme;

pub(super) fn line_count(text: &str) -> usize {
    text.bytes().filter(|&byte| byte == b'\n').count() + 1
}

fn line_cursor(text: &str, target: usize) -> CCursor {
    let index: usize = text
        .split_inclusive('\n')
        .take(target.saturating_sub(1))
        .map(|line| line.chars().count())
        .sum();
    CCursor::new(index)
}

pub(super) fn show(
    ui: &mut Ui,
    theme: &Theme,
    key: impl std::hash::Hash + std::fmt::Debug,
    hint: &str,
    text: &mut dyn egui::TextBuffer,
    height: f32,
    jump: Option<usize>,
) {
    let id = Id::unique(("lumen-config-code", key));
    let font = FontId::monospace(13.0);
    let digits = line_count(text.as_str()).ilog10() + 1;
    let gutter_width = (digits as f32 * 8.0 + 16.0).max(36.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (gutter, _) = ui.allocate_exact_size(vec2(gutter_width, height), Sense::hover());
        let width = ui.available_width().max(1.0);
        let jump =
            jump.map(|line| line_cursor(text.as_str(), line.clamp(1, line_count(text.as_str()))));
        if let Some(cursor) = jump {
            let mut state = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(CCursorRange::one(cursor)));
            TextEdit::store_state(ui.ctx(), id, state);
            ui.memory_mut(|memory| memory.request_focus(id));
        }

        let editor = ScrollArea::both()
            .id_salt(id)
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let painter = ui.painter().clone();
                let highlight = painter.add(egui::Shape::Noop);
                let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, _width| {
                    let job = LayoutJob::simple(
                        buffer.as_str().to_owned(),
                        font.clone(),
                        theme.text,
                        f32::INFINITY,
                    );
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let editor = TextEdit::multiline(text)
                    .id(id)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .font(font.clone())
                    .hint_text(hint)
                    .margin(vec2(8.0, 4.0))
                    .desired_width(width)
                    .desired_rows(1)
                    .layouter(&mut layouter)
                    .show(ui);
                if text.is_mutable()
                    && editor.response.has_focus()
                    && let Some(selection) = editor.cursor_range
                {
                    let cursor = editor
                        .galley
                        .pos_from_cursor(selection.primary)
                        .translate(editor.galley_pos.to_vec2());
                    let line =
                        Rect::from_x_y_ranges(painter.clip_rect().x_range(), cursor.y_range());
                    painter.set(
                        highlight,
                        egui::Shape::rect_filled(line, 0.0, theme.nav_hover),
                    );
                }
                if let Some(cursor) = jump {
                    let rect = editor
                        .galley
                        .pos_from_cursor(cursor)
                        .translate(editor.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect, Some(Align::Center));
                }
                editor
            });

        let clip = gutter.intersect(ui.clip_rect());
        let painter = ui.painter().with_clip_rect(clip);
        painter.rect_filled(gutter, 0.0, theme.bg);
        painter.vline(
            gutter.right() - 1.0,
            gutter.y_range(),
            Stroke::new(1.0, theme.divider),
        );
        let origin: Pos2 = editor.inner.galley_pos;
        let rows = &editor.inner.galley.rows;
        let first = rows.partition_point(|row| origin.y + row.rect().bottom() < clip.top());
        for (index, row) in rows.iter().enumerate().skip(first) {
            let y = origin.y + row.rect().top();
            if y > clip.bottom() {
                break;
            }
            painter.text(
                pos2(gutter.right() - 8.0, y),
                Align2::RIGHT_TOP,
                (index + 1).to_string(),
                font.clone(),
                theme.text_faint,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_positions_use_characters_and_count_the_empty_last_line() {
        let text = "🙂я\n{\n";
        assert_eq!(line_count(text), 3);
        assert_eq!(line_cursor(text, 2).index.0, 3);
        assert_eq!(line_cursor(text, 3).index.0, 5);
    }

    #[test]
    fn jumping_to_a_line_focuses_the_editor_in_a_bounded_viewport() {
        let ctx = egui::Context::default();
        let theme = Theme::resolve(&Default::default());
        let mut text = "{\"поле\": \"длинная строка\"}\n".repeat(80);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(420.0, 280.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            show(ui, &theme, "test", "", &mut text, 140.0, Some(55));
            assert!(ui.min_rect().height() <= 150.0);
        });
        output.textures_delta.clear();
        let id = Id::unique(("lumen-config-code", "test"));
        let state = TextEdit::load_state(&ctx, id).unwrap();
        assert_eq!(
            state.cursor.char_range().unwrap().primary.index,
            line_cursor(&text, 55).index
        );
        assert!(ctx.memory(|memory| memory.has_focus(id)));
    }
}
