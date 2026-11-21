//! Page buttons. They used to be styled `egui::Button`s, which switch
//! colour at once (an accent fill set on the button even ignored hover);
//! they now draw `AccentButton.qml` through [`FluentButton`], whose fill
//! follows hover and press over 110 ms like the QML `ColorAnimation`.

use egui::{Response, Ui, Widget, vec2};

#[allow(unused_imports)]
use crate::shell::widgets::FluentTip;
use crate::shell::theme::Theme;
use crate::shell::widgets::{ButtonKind, FluentButton};

pub(crate) struct PageButton<'t> {
    theme: &'t Theme,
    glyph: String,
    label: String,
    accent: bool,
    danger: bool,
    min_width: f32,
}

impl PageButton<'_> {
    /// `kind: "danger"` (the old `.fill(theme.danger_fill)`).
    pub(crate) fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// At least this wide (the old `.min_size`).
    pub(crate) fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }
}

pub(crate) fn button<'t>(theme: &'t Theme, icon: &str, label: &str, accent: bool) -> PageButton<'t> {
    PageButton {
        theme,
        glyph: icon.to_owned(),
        label: label.to_owned(),
        accent,
        danger: false,
        min_width: 0.0,
    }
}

impl Widget for PageButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut button = if self.label.is_empty() {
            FluentButton::icon(&self.glyph, "")
        } else {
            let kind = if self.danger {
                ButtonKind::Danger
            } else if self.accent {
                ButtonKind::Accent
            } else {
                ButtonKind::Ghost
            };
            FluentButton::new(kind, &self.glyph, &self.label)
        };
        if self.min_width > self.theme.control_height as f32 && self.label.is_empty() {
            button = button.width(self.min_width);
        }
        button.enabled(ui.is_enabled()).show(ui, self.theme)
    }
}

pub(crate) fn style(ui: &mut Ui) {
    ui.spacing_mut().button_padding = vec2(14.0, 0.0);
}

pub(crate) fn icon(
    ui: &mut Ui,
    theme: &Theme,
    glyph: &str,
    tip: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled(enabled, button(theme, glyph, "", false))
        .fluent_tip(tip)
}
