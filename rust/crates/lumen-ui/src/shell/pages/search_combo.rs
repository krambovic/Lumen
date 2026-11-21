//! `SearchCombo.qml` (a FluentCombo look-alike whose drop-down starts with a
//! search field) and the settings speed-test preset flyout. Both drop-downs
//! share `popup_rows`: 34 px rows, a `cardHover` highlight inset into the
//! row and a 3 px accent bar on the current item.

use crate::shell::wheel::GlideRoom;
use egui::{
    Align2, Area, Color32, CursorIcon, Frame, Id, Key, Modifiers, Order, Rect, ScrollArea, Sense,
    Stroke, StrokeKind, Ui, pos2, vec2,
};
use lumen_core::i18n::tr;

use crate::shell::theme::Theme;
use crate::shell::widgets;

const ROW_HEIGHT: f32 = 34.0;
/// `enter: Transition { NumberAnimation { property: "opacity"; duration: 90 } }`.
const FADE: f64 = 0.09;

#[derive(Clone, Default)]
struct State {
    open: bool,
    opened_at: f64,
    query: String,
    /// Highlighted position in the filtered list (`list.currentIndex`).
    highlight: Option<usize>,
    open_up: bool,
    room: f32,
    focus_search: bool,
    scroll_to_highlight: bool,
}

pub(super) struct SearchCombo<'a> {
    id: Id,
    labels: Vec<&'a str>,
    current: Option<usize>,
    placeholder: &'a str,
    search_placeholder: String,
    width: f32,
    popup_min_width: f32,
    enabled: bool,
}

impl<'a> SearchCombo<'a> {
    pub(super) fn new(ui: &Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, labels: Vec<&'a str>, current: Option<usize>) -> Self {
        Self {
            id: ui.make_persistent_id(("search-combo", id_salt)),
            labels,
            current,
            placeholder: "",
            search_placeholder: tr("Поиск серверов…", &[]),
            width: 220.0,
            popup_min_width: 280.0,
            enabled: true,
        }
    }

    pub(super) fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = text;
        self
    }

    pub(super) fn search_placeholder(mut self, text: String) -> Self {
        self.search_placeholder = text;
        self
    }

    pub(super) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub(super) fn popup_min_width(mut self, width: f32) -> Self {
        self.popup_min_width = width;
        self
    }

    pub(super) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns the index the user picked (`activated(index)`); the current
    /// index is never changed here, as in QML.
    pub(super) fn show(self, ui: &mut Ui, theme: &Theme) -> Option<usize> {
        let ctx = ui.ctx().clone();
        let mut state = ctx.data_mut(|data| data.get_temp::<State>(self.id).unwrap_or_default());
        let enabled = self.enabled && ui.is_enabled();
        let height = theme.control_height as f32;
        let (field, response) = ui.allocate_exact_size(vec2(self.width, height), Sense::click());
        let response = if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response };
        if !enabled {
            state.open = false;
        }

        // Field: `controlFill` / hover / pressed with a 110 ms colour
        // animation, the accent border while the drop-down is open.
        let hover = ctx.animate_bool_with_time(self.id.with("hover"), enabled && response.hovered(), 0.11);
        let pressed =
            ctx.animate_bool_with_time(self.id.with("pressed"), enabled && response.is_pointer_button_down_on(), 0.11);
        let fill = lerp(lerp(theme.control_fill, theme.control_fill_hover, hover), theme.control_fill_pressed, pressed);
        let opacity = if enabled { 1.0 } else { 0.5 };
        let fade = |color: Color32| color.gamma_multiply(opacity);
        let painter = ui.painter();
        let radius = theme.radius_small as f32;
        painter.rect(
            field,
            radius,
            fade(fill),
            Stroke::new(1.0, fade(if state.open { theme.accent } else { theme.border_solid })),
            StrokeKind::Inside,
        );
        let chevron = painter.layout_no_wrap(
            "\u{e70d}".to_owned(),
            egui::FontId::proportional(12.0),
            theme.text_muted,
        );
        let chevron_left = field.right() - 12.0 - chevron.size().x;
        painter.galley(
            pos2(chevron_left, field.center().y - chevron.size().y / 2.0),
            chevron,
            fade(theme.text_muted),
        );
        let selected = self.current.and_then(|index| self.labels.get(index).copied());
        let text = selected.unwrap_or(self.placeholder);
        let color = if selected.is_some() { theme.text } else { theme.text_faint };
        let galley = widgets::elided(
            ui,
            text,
            egui::FontId::proportional(theme.font_normal as f32),
            color,
            chevron_left - 8.0 - (field.left() + 12.0),
        );
        ui.painter().galley(
            pos2(field.left() + 12.0, field.center().y - galley.size().y / 2.0),
            galley,
            fade(color),
        );

        let now = ctx.input(|input| input.time);
        if enabled && response.clicked() {
            if state.open {
                state.open = false;
            } else {
                // `onAboutToShow`: open upwards when there is no room below.
                let window = ctx.content_rect();
                let below = window.bottom() - field.bottom() - 12.0;
                let above = field.top() - window.top() - 12.0;
                let want = (self.labels.len() as f32 * ROW_HEIGHT + height + 6.0 + 12.0).min(380.0);
                state.open_up = below < want && above > below;
                state.room = if state.open_up { above } else { below };
                // `onOpened`: clear the search, refilter, focus it.
                state.open = true;
                state.opened_at = now;
                state.query.clear();
                state.highlight = Some(self.current.unwrap_or(0)).filter(|_| !self.labels.is_empty());
                state.focus_search = true;
                state.scroll_to_highlight = true;
            }
        }

        let mut chosen = None;
        if state.open {
            chosen = self.popup(ui, theme, field, &mut state, now);
        }
        ctx.data_mut(|data| data.insert_temp(self.id, state));
        chosen
    }

    fn popup(&self, ui: &Ui, theme: &Theme, field: Rect, state: &mut State, now: f64) -> Option<usize> {
        let ctx = ui.ctx().clone();
        let height = theme.control_height as f32;
        let filtered = filter(&self.labels, &state.query);
        let width = self.width.max(self.popup_min_width);
        let small = egui::FontId::proportional(theme.font_small as f32);
        let small_height = ui.fonts_mut(|fonts| fonts.row_height(&small));
        let list_height = (filtered.len() as f32 * ROW_HEIGHT)
            .min(300.0)
            .min((state.room - height - 6.0 - 12.0).max(68.0));
        let body = if filtered.is_empty() { 6.0 + small_height + 8.0 } else { list_height };
        let total = (12.0 + height + 6.0 + body).min(380.0);
        let x = field.left() + (field.width() - width).min(0.0);
        let y = if state.open_up { field.top() - 4.0 - total } else { field.bottom() + 4.0 };

        // Keyboard: Up / Down move the highlight, Enter picks, Escape closes
        // (`navigationKeys: true` on the search field).
        let (up, down, enter, escape) = ctx.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::ArrowUp),
                input.consume_key(Modifiers::NONE, Key::ArrowDown),
                input.consume_key(Modifiers::NONE, Key::Enter),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if !filtered.is_empty() {
            let last = filtered.len() - 1;
            let position = state.highlight.unwrap_or(0).min(last);
            if down {
                state.highlight = Some((position + 1).min(last));
                state.scroll_to_highlight = true;
            } else if up {
                state.highlight = Some(position.saturating_sub(1));
                state.scroll_to_highlight = true;
            }
        }
        let mut chosen = None;
        if enter && let Some(position) = state.highlight {
            chosen = filtered.get(position).copied();
        }
        if escape {
            state.open = false;
        }

        let opacity = (((now - state.opened_at) / FADE) as f32).clamp(0.0, 1.0);
        if opacity < 1.0 {
            ctx.request_repaint();
        }
        let area = Area::new(self.id.with("popup"))
            .order(Order::Foreground)
            // Qt popups with `margins: -1` are not pushed inside the window.
            .constrain(false)
            .fixed_pos(pos2(x, y))
            .show(&ctx, |ui| {
                ui.multiply_opacity(opacity);
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(8)
                    .inner_margin(5)
                    .show(ui, |ui| {
                        let inner = width - 12.0;
                        ui.set_width(inner);
                        ui.spacing_mut().item_spacing = vec2(0.0, 6.0);
                        let before = state.query.clone();
                        let (search, _) = super::search_field::show(
                            ui,
                            theme,
                            &mut state.query,
                            &self.search_placeholder,
                            true,
                        );
                        if std::mem::take(&mut state.focus_search) {
                            search.request_focus();
                        }
                        if state.query != before {
                            // `refilter`: keep the current item highlighted.
                            let filtered = filter(&self.labels, &state.query);
                            state.highlight = if filtered.is_empty() {
                                None
                            } else {
                                Some(
                                    self.current
                                        .and_then(|current| filtered.iter().position(|index| *index == current))
                                        .unwrap_or(0),
                                )
                            };
                            state.scroll_to_highlight = true;
                        }
                        let filtered = filter(&self.labels, &state.query);
                        if filtered.is_empty() {
                            ui.add_space(0.0);
                            let (rect, _) = ui.allocate_exact_size(vec2(inner, 6.0 + small_height + 8.0), Sense::hover());
                            ui.painter().text(
                                pos2(rect.center().x, rect.top() + 6.0),
                                Align2::CENTER_TOP,
                                tr("Ничего не найдено", &[]),
                                small.clone(),
                                theme.text_faint,
                            );
                            return None;
                        }
                        ScrollArea::vertical()
                            .id_salt(self.id.with("list"))
                            .max_height(list_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let labels: Vec<&str> = filtered.iter().map(|index| self.labels[*index]).collect();
                                let selected = self.current.and_then(|current| filtered.iter().position(|index| *index == current));
                                let rows = popup_rows(
                                    ui,
                                    theme,
                                    &labels,
                                    selected,
                                    state.highlight,
                                    2.0,
                                    std::mem::take(&mut state.scroll_to_highlight),
                                );
                                // `onHoveredChanged: if (hovered) list.currentIndex = index`.
                                if let Some(hovered) = rows.hovered {
                                    state.highlight = Some(hovered);
                                }
                                rows.clicked.map(|position| filtered[position])
                            }).glide_room(ui)
                            .inner
                    })
                    .inner
            });
        if let Some(index) = area.inner {
            chosen = Some(index);
        }
        // `closePolicy` default: a press outside the popup closes it (the
        // field's own click toggles it).
        let pressed_outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pointer| !area.response.rect.contains(pointer) && !field.contains(pointer))
        });
        if pressed_outside {
            state.open = false;
        }
        if chosen.is_some() {
            state.open = false;
        }
        chosen
    }
}

fn filter(labels: &[&str], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    labels
        .iter()
        .enumerate()
        .filter(|(_, label)| query.is_empty() || label.to_lowercase().contains(&query))
        .map(|(index, _)| index)
        .collect()
}

fn lerp(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

pub(super) struct Rows {
    pub(super) clicked: Option<usize>,
    pub(super) hovered: Option<usize>,
}

/// ItemDelegate rows (34 px, 12 px text padding): the background is inset by
/// `inset` and filled with `cardHover` on the highlighted row; the current
/// item gets a 3 px accent bar half the row tall, 1 px from the left.
pub(super) fn popup_rows(
    ui: &mut Ui,
    theme: &Theme,
    labels: &[&str],
    selected: Option<usize>,
    highlight: Option<usize>,
    inset: f32,
    scroll_to_highlight: bool,
) -> Rows {
    let mut rows = Rows { clicked: None, hovered: None };
    let font = egui::FontId::proportional(theme.font_normal as f32);
    let width = ui.available_width();
    for (position, label) in labels.iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
        if response.hovered() {
            rows.hovered = Some(position);
        }
        if response.clicked() {
            rows.clicked = Some(position);
        }
        if scroll_to_highlight && highlight == Some(position) {
            response.scroll_to_me(None);
        }
        let background = rect.shrink(inset);
        if highlight == Some(position) {
            ui.painter().rect_filled(background, theme.radius_small as f32, theme.card_hover);
        }
        if selected == Some(position) {
            let bar = Rect::from_center_size(
                pos2(background.left() + 1.0 + 1.5, background.center().y),
                vec2(3.0, background.height() * 0.5),
            );
            ui.painter().rect_filled(bar, 1.5, theme.accent);
        }
        let galley = widgets::elided(ui, label, font.clone(), theme.text, width - 24.0);
        ui.painter().galley(
            pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
            galley,
            theme.text,
        );
    }
    rows
}

/// SettingsPage `speedPresetBtn` + `speedPresetMenu`: a 36 px chevron button
/// and a 230 px non-modal flyout aligned to its right edge.
pub(super) fn preset_menu(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: &str,
    labels: &[&str],
    selected: Option<usize>,
) -> Option<usize> {
    let ctx = ui.ctx().clone();
    let id = ui.make_persistent_id(("preset-menu", id_salt));
    let mut state = ctx.data_mut(|data| data.get_temp::<State>(id).unwrap_or_default());
    let (button, response) = ui.allocate_exact_size(vec2(36.0, theme.control_height as f32), Sense::click());
    let fill = if response.is_pointer_button_down_on() {
        theme.control_fill_pressed
    } else if response.hovered() {
        theme.control_fill_hover
    } else {
        theme.control_fill
    };
    let radius = theme.radius_small as f32;
    ui.painter().rect(button, radius, fill, Stroke::new(1.0, theme.border_solid), StrokeKind::Inside);
    ui.painter().text(
        button.center(),
        Align2::CENTER_CENTER,
        "\u{e70d}",
        egui::FontId::proportional(12.0),
        theme.text_muted,
    );
    let now = ctx.input(|input| input.time);
    if response.clicked() {
        state.open = !state.open;
        state.opened_at = now;
    }
    let mut chosen = None;
    if state.open {
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            state.open = false;
        }
        let opacity = (((now - state.opened_at) / FADE) as f32).clamp(0.0, 1.0);
        if opacity < 1.0 {
            ctx.request_repaint();
        }
        let width = 230.0;
        let area = Area::new(id.with("popup"))
            .order(Order::Foreground)
            // Qt popups with `margins: -1` are not pushed inside the window.
            .constrain(false)
            .fixed_pos(pos2(button.right() - width, button.bottom() + 4.0))
            .show(&ctx, |ui| {
                ui.multiply_opacity(opacity);
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(8)
                    .inner_margin(3)
                    .show(ui, |ui| {
                        ui.set_width(width - 8.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // Plain ItemDelegates: the hover is the highlight.
                        let hovered = state.highlight;
                        let rows = popup_rows(ui, theme, labels, selected, hovered, 3.0, false);
                        state.highlight = rows.hovered;
                        rows.clicked
                    })
                    .inner
            });
        chosen = area.inner;
        // `CloseOnPressOutsideParent`: presses on the button toggle instead.
        let pressed_outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pointer| !area.response.rect.contains(pointer) && !button.contains(pointer))
        });
        if pressed_outside || chosen.is_some() {
            state.open = false;
        }
        if !area.response.contains_pointer() {
            state.highlight = None;
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
    chosen
}

#[cfg(test)]
mod tests {
    use super::filter;

    #[test]
    fn filter_matches_case_insensitively() {
        let labels = ["Alpha", "Germany", "test"];
        assert_eq!(filter(&labels, ""), vec![0, 1, 2]);
        assert_eq!(filter(&labels, "  GER "), vec![1]);
        assert!(filter(&labels, "zzz").is_empty());
    }
}
