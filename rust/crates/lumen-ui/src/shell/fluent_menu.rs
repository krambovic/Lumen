//! `FluentMenu.qml`, `FluentMenuItem.qml` and `FluentMenuSeparator.qml` on top
//! of egui's popup and submenu logic.
//!
//! - Menu: `Theme.flyout` background, radius 8, 1 px `flyoutBorder`, 5 px
//!   padding, no shadow, 90 ms fade-in. `width` is the QML menu width
//!   (padding included).
//! - Item: 32 px row, 12 px side padding, a 28 px icon column (16 px wide
//!   cell, 14 px Segoe Fluent Icons glyph) when the menu shows icons, label
//!   at `Theme.fontNormal`, right-aligned hint at `Theme.fontSmall` in
//!   `textFaint`, red text and icon for destructive items, a radius 6
//!   `cardHover` highlight that fades in 90 ms.
//! - Separator: 1 px `divider` with 6 px side and 5 px vertical padding.
//! - Submenu: the same row with a 10 px chevron 10 px from the right edge
//!   (right padding 28); hovering opens it, and a `clickable` submenu also
//!   acts on a click on its own row.

use egui::{
    Align2, Color32, CornerRadius, FontId, Margin, Popup, Rect, Response, Sense, Shadow,
    Stroke, Ui, containers::menu::{MenuConfig, SubMenu}, pos2, vec2,
};

use super::motion;
use super::theme::Theme;

const ROW_HEIGHT: f32 = 32.0;

fn theme_id() -> egui::Id {
    egui::Id::new("lumen-fluent-menu-theme")
}

/// Makes the frame's theme available to widgets that are not handed one
/// (the shared text-field context menu).
pub(crate) fn publish_theme(ctx: &egui::Context, theme: &Theme) {
    ctx.data_mut(|data| data.insert_temp(theme_id(), std::sync::Arc::new(theme.clone())));
}

/// The theme published for this frame, if any.
pub(crate) fn published_theme(ctx: &egui::Context) -> Option<std::sync::Arc<Theme>> {
    ctx.data(|data| data.get_temp::<std::sync::Arc<Theme>>(theme_id()))
}
const PADDING: f32 = 5.0;

fn style_modifier(theme: &Theme) -> impl Fn(&mut egui::Style) + Send + Sync + 'static {
    let fill = theme.flyout;
    let border = theme.flyout_border;
    move |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        style.spacing.menu_margin = Margin::same(PADDING as i8);
        style.spacing.item_spacing = vec2(0.0, 0.0);
        style.visuals.menu_corner_radius = CornerRadius::same(8);
        style.visuals.window_fill = fill;
        style.visuals.window_stroke = Stroke::new(1.0, border);
        style.visuals.popup_shadow = Shadow::NONE;
    }
}

fn frame(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.flyout)
        .stroke(Stroke::new(1.0, theme.flyout_border))
        .corner_radius(8)
        .inner_margin(PADDING)
}

/// `enter: Transition { opacity 0 -> 1, 90 ms }`, keyed by the menu's ui id:
/// the clock restarts whenever the menu was not shown in the previous pass.
fn fade_in(ui: &mut Ui, theme: &Theme) {
    if theme.anim_fast == 0 {
        return;
    }
    let ctx = ui.ctx().clone();
    let id = ui.id().with("lumen-menu-fade");
    let now = ctx.input(|input| input.time);
    let pass = ctx.cumulative_pass_nr();
    let (start, last_pass) = ctx.data(|data| data.get_temp::<(f64, u64)>(id)).unwrap_or((now, 0));
    let start = if last_pass + 1 < pass { now } else { start };
    ctx.data_mut(|data| data.insert_temp(id, (start, pass)));
    let level = ((now - start) / 0.09).clamp(0.0, 1.0) as f32;
    if level < 1.0 {
        ui.multiply_opacity(level);
        ctx.request_repaint();
    }
}

/// A Fluent context menu on `response` (secondary click), `width` wide.
pub(crate) fn context_menu<R>(
    response: &Response,
    theme: &Theme,
    width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    Popup::context_menu(response)
        .style(style_modifier(theme))
        .frame(frame(theme))
        .show(|ui| {
            ui.set_width(width - 2.0 * PADDING);
            fade_in(ui, theme);
            content(ui)
        })
        .map(|inner| inner.inner)
}

/// A Fluent drop-down menu toggled by clicks on `response`.
pub(crate) fn dropdown<R>(
    response: &Response,
    theme: &Theme,
    width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    Popup::menu(response)
        .style(style_modifier(theme))
        .frame(frame(theme))
        .gap(4.0)
        .show(|ui| {
            ui.set_width(width - 2.0 * PADDING);
            fade_in(ui, theme);
            content(ui)
        })
        .map(|inner| inner.inner)
}

/// One `FluentMenuItem`.
pub(crate) struct Item<'a> {
    glyph: &'a str,
    text: &'a str,
    hint: &'a str,
    danger: bool,
    enabled: bool,
}

impl<'a> Item<'a> {
    pub(crate) fn new(glyph: &'a str, text: &'a str) -> Self {
        Self { glyph, text, hint: "", danger: false, enabled: true }
    }

    pub(crate) fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub(crate) fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Paints the row; a click closes the menu and returns true.
    /// `icons` is the menu's `showIcons` (reserve the icon column).
    pub(crate) fn show(self, ui: &mut Ui, theme: &Theme, icons: bool) -> bool {
        let response = row(ui, theme, self.enabled, |ui, rect| {
            paint_content(ui, theme, rect, &self, icons, 12.0);
        });
        let clicked = self.enabled && response.clicked();
        if clicked {
            ui.close();
        }
        clicked
    }
}

/// `FluentMenuSeparator`.
pub(crate) fn separator(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 11.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_size(pos2(rect.left() + 6.0, rect.top() + 5.0), vec2(rect.width() - 12.0, 1.0)),
        0.0,
        theme.divider,
    );
}

/// A nested `FluentMenu` with its own `width` and `showIcons`. With
/// `clickable`, a click on the row returns true and closes the menu
/// (`onActivated`); hovering opens the submenu either way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submenu(
    ui: &mut Ui,
    theme: &Theme,
    icons: bool,
    glyph: &str,
    title: &str,
    width: f32,
    clickable: bool,
    content: impl FnOnce(&mut Ui),
) -> bool {
    let item = Item::new(glyph, title);
    let response = row(ui, theme, true, |ui, rect| {
        paint_content(ui, theme, rect, &item, icons, 28.0);
        ui.painter().text(
            pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            "\u{e76c}",
            FontId::proportional(10.0),
            theme.text_muted,
        );
    });
    SubMenu::new()
        .config(MenuConfig::new().style(style_modifier(theme)))
        .show(ui, &response, |ui| {
            ui.set_width(width - 2.0 * PADDING);
            fade_in(ui, theme);
            content(ui);
        });
    let activated = clickable && response.clicked_by(egui::PointerButton::Primary);
    if activated {
        ui.close();
    }
    activated
}

fn row(ui: &mut Ui, theme: &Theme, enabled: bool, paint: impl FnOnce(&Ui, Rect)) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), sense);
    let highlighted = enabled && (response.hovered() || response.has_focus());
    let fill = motion::follow_color(
        ui.ctx(),
        theme,
        response.id.with("menu-row-fill"),
        if highlighted { theme.card_hover } else { Color32::TRANSPARENT },
        90.0,
        motion::linear,
    );
    if fill.a() > 0 {
        ui.painter().rect_filled(rect, 6.0, fill);
    }
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
    }
    paint(ui, rect);
    response
}

fn paint_content(ui: &Ui, theme: &Theme, rect: Rect, item: &Item<'_>, icons: bool, right_padding: f32) {
    let painter = ui.painter();
    let left = rect.left() + 12.0;
    let icon_column = icons || !item.glyph.is_empty();
    let ink = if !item.enabled {
        theme.text_faint
    } else if item.danger {
        theme.danger
    } else {
        theme.text
    };
    if icon_column && !item.glyph.is_empty() {
        let color = if !item.enabled {
            theme.text_faint
        } else if item.danger {
            theme.danger
        } else {
            theme.text_muted
        };
        painter.text(
            pos2(left + 8.0, rect.center().y),
            Align2::CENTER_CENTER,
            item.glyph,
            FontId::proportional(14.0),
            color,
        );
    }
    let label_left = left + if icon_column { 28.0 } else { 0.0 };
    let mut right = rect.right() - right_padding;
    if !item.hint.is_empty() {
        let hint = painter.layout_no_wrap(
            item.hint.to_owned(),
            FontId::proportional(theme.font_small as f32),
            theme.text_faint,
        );
        let size = hint.size();
        painter.galley(pos2(right - size.x, rect.center().y - size.y / 2.0), hint, theme.text_faint);
        right -= size.x + 16.0;
    }
    let label = super::widgets::elided(
        ui,
        item.text,
        FontId::proportional(theme.font_normal as f32),
        ink,
        (right - label_left).max(0.0),
    );
    let size = label.size();
    painter.galley(pos2(label_left, rect.center().y - size.y / 2.0), label, ink);
}
