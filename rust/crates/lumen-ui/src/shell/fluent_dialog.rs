//! `FluentDialog.qml`: the shared WinUI-style content dialog.
//!
//! A `width` px flyout (radius 10, 1 px `flyoutBorder`, no shadow, no enter
//! animation as in the Universal `Dialog`) over a 45% black backdrop. The
//! title is `fontStrong` DemiBold with 20 px side, 18 px top and 2 px bottom
//! padding; the content gets 20 px sides, 12 px top and 18 px bottom; the
//! footer right-aligns ghost Cancel, danger and accent OK buttons 10 px
//! apart with 6 px above, 18 px below and 20 px to the right. Escape and a
//! press outside reject the dialog.

use egui::{Align, Color32, Context, FontId, Frame, Id, Layout, Margin, Modal, Sense, Stroke, Ui, vec2};

use super::theme::{Theme, semibold};
use super::widgets::{ButtonKind, FluentButton};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogAction {
    None,
    Accept,
    Reject,
    Danger,
}

pub(crate) struct FluentDialog {
    id: Id,
    title: String,
    width: f32,
    ok: Option<String>,
    ok_enabled: bool,
    cancel: Option<String>,
    danger: Option<String>,
    padding: Margin,
}

impl FluentDialog {
    /// `okText` defaults to "Добавить" and `cancelText` to "Отмена", as in QML.
    pub(crate) fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, width: f32) -> Self {
        Self {
            id: Id::unique(("lumen-fluent-dialog", id)),
            title: title.into(),
            width,
            ok: Some(lumen_core::i18n::tr("Добавить", &[])),
            ok_enabled: true,
            cancel: Some(lumen_core::i18n::tr("Отмена", &[])),
            danger: None,
            // `padding: 20`, `topPadding: 12`; the Universal style sets
            // `verticalPadding: 18`, which wins over `padding` for the bottom.
            padding: Margin { left: 20, right: 20, top: 12, bottom: 18 },
        }
    }

    pub(crate) fn ok(mut self, text: impl Into<String>) -> Self {
        self.ok = Some(text.into());
        self
    }

    /// `showOkButton: false`.
    pub(crate) fn no_ok(mut self) -> Self {
        self.ok = None;
        self
    }

    pub(crate) fn ok_enabled(mut self, enabled: bool) -> Self {
        self.ok_enabled = enabled;
        self
    }

    pub(crate) fn cancel(mut self, text: impl Into<String>) -> Self {
        self.cancel = Some(text.into());
        self
    }

    /// `showCancelButton: false`.
    pub(crate) fn no_cancel(mut self) -> Self {
        self.cancel = None;
        self
    }

    /// `dangerText` (shown when not empty).
    pub(crate) fn danger(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.danger = (!text.is_empty()).then_some(text);
        self
    }

    /// Content padding (`padding` / `topPadding` overrides).
    pub(crate) fn padding(mut self, padding: Margin) -> Self {
        self.padding = padding;
        self
    }

    pub(crate) fn show(self, ctx: &Context, theme: &Theme, content: impl FnOnce(&mut Ui)) -> DialogAction {
        let screen = ctx.content_rect();
        let width = self.width.min((screen.width() - 40.0).max(120.0));
        let mut action = DialogAction::None;
        let modal = Modal::new(self.id)
            .backdrop_color(Color32::from_black_alpha(115))
            .frame(
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                if !self.title.is_empty() {
                    ui.add_space(18.0);
                    let galley = ui.painter().layout(
                        self.title.clone(),
                        semibold(theme.font_strong as f32),
                        theme.text,
                        width - 40.0,
                    );
                    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
                    ui.painter().galley(egui::pos2(rect.left() + 20.0, rect.top()), galley, theme.text);
                    ui.add_space(2.0);
                }
                Frame::new().inner_margin(self.padding).show(ui, |ui| {
                    ui.set_width(width - f32::from(self.padding.left) - f32::from(self.padding.right));
                    content(ui);
                });
                if self.ok.is_some() || self.cancel.is_some() {
                    ui.add_space(6.0);
                    let height = theme.control_height as f32;
                    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                    let mut buttons = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row.shrink2(vec2(20.0, 0.0)))
                            .layout(Layout::right_to_left(Align::Center)),
                    );
                    buttons.spacing_mut().item_spacing.x = 10.0;
                    if let Some(ok) = &self.ok
                        && FluentButton::new(ButtonKind::Accent, "", ok)
                            .enabled(self.ok_enabled)
                            .show(&mut buttons, theme)
                            .clicked()
                    {
                        action = DialogAction::Accept;
                    }
                    if let Some(danger) = &self.danger
                        && FluentButton::new(ButtonKind::Danger, "", danger).show(&mut buttons, theme).clicked()
                    {
                        action = DialogAction::Danger;
                    }
                    if let Some(cancel) = &self.cancel
                        && FluentButton::new(ButtonKind::Ghost, "", cancel).show(&mut buttons, theme).clicked()
                    {
                        action = DialogAction::Reject;
                    }
                    ui.add_space(18.0);
                }
            });
        if action == DialogAction::None && modal.should_close() {
            action = DialogAction::Reject;
        }
        action
    }
}

/// Wrapped dialog body text (`wrapMode: Text.WordWrap`) at the given width.
pub(crate) fn body_text(ui: &mut Ui, text: &str, font: FontId, color: Color32) {
    let width = ui.available_width();
    let galley = ui.painter().layout(text.to_owned(), font, color, width);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}

/// Wrapped text with `lineHeight` as a factor of the font's line height
/// (`Text.ProportionalHeight`).
pub(crate) fn body_text_spaced(ui: &mut Ui, text: &str, font: FontId, color: Color32, line_height: f32) {
    let width = ui.available_width();
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            line_height: Some(row * line_height),
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}
