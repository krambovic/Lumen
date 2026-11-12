use egui::{Align, Button, Frame, RichText, Sense, Stroke, TextEdit, Ui, vec2};

use crate::shell::theme::Theme;

pub(super) fn show(
    ui: &mut Ui,
    theme: &Theme,
    query: &mut String,
    placeholder: &str,
    clearable: bool,
) -> (egui::Response, bool) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme.control_height as f32),
        Sense::hover(),
    );
    // The focus state is only known after the edit is placed; reserve the
    // background slot now so the text paints above it.
    let background = ui.painter().add(egui::Shape::Noop);
    let border_slot = ui.painter().add(egui::Shape::Noop);
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "\u{e721}",
        egui::FontId::proportional(14.0),
        theme.text_faint,
    );
    let edit_rect = egui::Rect::from_min_max(
        rect.min + vec2(44.0, 0.0),
        rect.max
            - vec2(
                if !clearable || query.is_empty() {
                    10.0
                } else {
                    32.0
                },
                0.0,
            ),
    );
    let response = ui.place(
        edit_rect,
        TextEdit::singleline(query)
            .frame(Frame::NONE)
            .vertical_align(Align::Center)
            .margin(egui::Margin::ZERO)
            .hint_text(RichText::new(placeholder).color(theme.text_faint)),
    );
    let cleared = clearable
        && !query.is_empty()
        && ui
            .place(
                egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 32.0, rect.top()),
                    vec2(32.0, 32.0),
                ),
                Button::new("\u{e711}").frame(false),
            )
            .clicked();
    if cleared {
        query.clear();
        response.request_focus();
    }
    let border = if response.has_focus() {
        theme.accent
    } else {
        theme.border_solid
    };
    let radius = theme.radius_small as f32;
    ui.painter().set(
        background,
        egui::Shape::rect_filled(rect.shrink(1.0), (radius - 1.0).max(0.0), theme.control_fill),
    );
    ui.painter().set(
        border_slot,
        egui::Shape::rect_stroke(rect, radius, Stroke::new(1.0, border), egui::StrokeKind::Inside),
    );
    (response, cleared)
}
