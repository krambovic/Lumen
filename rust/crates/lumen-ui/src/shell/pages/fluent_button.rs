use egui::{Button, FontId, Stroke, TextFormat, Ui, text::LayoutJob, vec2};

use crate::shell::theme::{Theme, semibold};

pub(super) fn button(theme: &Theme, icon: &str, label: &str, accent: bool) -> Button<'static> {
    let height = theme.control_height as f32;
    let color = if accent {
        theme.accent_text
    } else {
        theme.text
    };
    let size = theme.font_normal as f32;
    let mut text = LayoutJob::default();
    if !icon.is_empty() {
        text.append(
            icon,
            0.0,
            TextFormat::simple(FontId::proportional(size), color),
        );
    }
    let font = if accent {
        semibold(size)
    } else {
        FontId::proportional(size)
    };
    let gap = if icon.is_empty() { 0.0 } else { 8.0 };
    text.append(label, gap, TextFormat::simple(font, color));
    let width = if label.is_empty() { height } else { 0.0 };
    let mut button = Button::new(text).min_size(vec2(width, height)).truncate();
    if accent {
        button = button.fill(theme.accent).stroke(Stroke::NONE);
    }
    button
}

pub(super) fn style(ui: &mut Ui) {
    ui.spacing_mut().button_padding = vec2(14.0, 0.0);
}

pub(super) fn icon(
    ui: &mut Ui,
    theme: &Theme,
    glyph: &str,
    tip: &str,
    enabled: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::Vec2::ZERO;
        ui.add_enabled(enabled, button(theme, glyph, "", false))
            .on_hover_text(tip)
    })
    .inner
}
