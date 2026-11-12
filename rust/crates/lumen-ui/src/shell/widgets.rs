//! Fluent controls shared by the pages: ports of `AccentButton.qml`,
//! `FluentCombo.qml`, `Card.qml`, `StatusRing.qml` and the small text
//! components (`SectionTitle`, `BigValue`).

use std::hash::Hash;
use std::sync::Arc;

use egui::{
    Color32, CursorIcon, FontId, Frame, Galley, Id, Popup, PopupCloseBehavior, Rect, Response,
    ScrollArea, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use super::motion;
use super::theme::{Theme, qcolor_darker, qcolor_lighter, semibold};

/// Segoe UI ascent in em units; aligns baselines of differently sized runs.
const ASCENT_EM: f32 = 1.079;

pub(crate) fn galley(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.to_owned(), font, color)
}

/// Single-line text elided with an ellipsis (`elide: Text.ElideRight`).
pub(crate) fn elided(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

/// A QML `Rectangle` with a border: the 1 px border replaces the fill along
/// the edge instead of blending over it.
pub(crate) fn bordered_rect(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    fill: Color32,
    border: Color32,
) {
    painter.rect_filled(rect.shrink(1.0), (radius - 1.0).max(0.0), fill);
    painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// `Card.qml` background. `padding` is the QML padding, border included.
pub(crate) fn card_frame(theme: &Theme, padding: f32) -> Frame {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(padding - 1.0)
}

/// `SectionTitle`: `fontStrong`, DemiBold.
pub(crate) fn section_title(ui: &mut Ui, theme: &Theme, text: &str) -> Response {
    let galley = galley(ui, text, semibold(theme.font_strong as f32), theme.text);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, theme.text);
    response
}

/// `BigValue`: a DemiBold number with a muted unit on the same baseline.
pub(crate) struct BigValue {
    number: Arc<Galley>,
    unit: Arc<Galley>,
    unit_drop: f32,
}

impl BigValue {
    pub(crate) fn new(ui: &Ui, theme: &Theme, value: &str, unit: &str, pixel_size: f32) -> Self {
        let unit_size = theme.font_small as f32 + 1.0;
        Self {
            number: galley(ui, value, semibold(pixel_size), theme.text),
            unit: galley(ui, unit, FontId::proportional(unit_size), theme.text_muted),
            unit_drop: (pixel_size - unit_size) * ASCENT_EM,
        }
    }

    pub(crate) fn size(&self) -> egui::Vec2 {
        vec2(
            self.number.size().x + 6.0 + self.unit.size().x,
            self.number.size().y,
        )
    }

    pub(crate) fn paint(&self, painter: &egui::Painter, pos: egui::Pos2) {
        painter.galley(pos, self.number.clone(), Color32::WHITE);
        painter.galley(
            pos + vec2(self.number.size().x + 6.0, self.unit_drop),
            self.unit.clone(),
            Color32::WHITE,
        );
    }

    pub(crate) fn show(&self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size(), Sense::hover());
        self.paint(ui.painter(), rect.min);
        response
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    Accent,
    Ghost,
    Danger,
}

/// `AccentButton.qml`.
pub(crate) struct FluentButton<'a> {
    kind: ButtonKind,
    glyph: &'a str,
    text: &'a str,
    icon_only: bool,
    width: Option<f32>,
    tip: &'a str,
    enabled: bool,
}

impl<'a> FluentButton<'a> {
    pub(crate) fn new(kind: ButtonKind, glyph: &'a str, text: &'a str) -> Self {
        Self {
            kind,
            glyph,
            text,
            icon_only: false,
            width: None,
            tip: "",
            enabled: true,
        }
    }

    /// Square ghost icon button with a hover tooltip (`iconOnly: true`).
    pub(crate) fn icon(glyph: &'a str, tip: &'a str) -> Self {
        Self {
            icon_only: true,
            tip,
            ..Self::new(ButtonKind::Ghost, glyph, "")
        }
    }

    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub(crate) fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let height = theme.control_height as f32;
        let size = theme.font_normal as f32;
        let standard = self.kind == ButtonKind::Ghost;
        let mut ink = if standard {
            theme.text
        } else {
            theme.accent_text
        };
        if !self.enabled {
            ink = ink.gamma_multiply(0.45);
        }
        let glyph =
            (!self.glyph.is_empty()).then(|| galley(ui, self.glyph, FontId::proportional(size), ink));
        let label_font = if standard {
            FontId::proportional(size)
        } else {
            semibold(size)
        };
        let label = (!self.icon_only && !self.text.is_empty())
            .then(|| galley(ui, self.text, label_font, ink));
        let gap = if glyph.is_some() && label.is_some() {
            8.0
        } else {
            0.0
        };
        let content = glyph.as_ref().map_or(0.0, |galley| galley.size().x)
            + gap
            + label.as_ref().map_or(0.0, |galley| galley.size().x);
        let width = self.width.unwrap_or(if self.icon_only {
            height
        } else {
            content + 28.0
        });
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        if ui.is_rect_visible(rect) {
            let ctx = ui.ctx().clone();
            let hovered = self.enabled && response.hovered();
            let down = self.enabled && response.is_pointer_button_down_on();
            let target = if standard {
                if !self.enabled {
                    Color32::TRANSPARENT
                } else if down {
                    theme.control_fill_pressed
                } else if hovered {
                    theme.control_fill_hover
                } else {
                    theme.control_fill
                }
            } else {
                let base = if self.kind == ButtonKind::Danger {
                    theme.danger_fill
                } else {
                    theme.accent
                };
                let shade = |light: i32, dark: i32| {
                    if theme.dark {
                        qcolor_lighter(base, light)
                    } else {
                        qcolor_darker(base, dark)
                    }
                };
                if !self.enabled {
                    base.gamma_multiply(0.4)
                } else if down {
                    shade(118, 112)
                } else if hovered {
                    shade(110, 106)
                } else {
                    base
                }
            };
            let fill = motion::follow_color(
                &ctx,
                theme,
                response.id.with("fill"),
                target,
                110.0,
                motion::linear,
            );
            let scale = motion::follow(
                &ctx,
                theme,
                response.id.with("scale"),
                if down { 0.97 } else { 1.0 },
                90.0,
                motion::ease_out_cubic,
            );
            let body = Rect::from_center_size(rect.center(), rect.size() * scale);
            let radius = theme.radius_small as f32;
            if standard {
                bordered_rect(ui.painter(), body, radius, fill, theme.border_solid);
            } else {
                ui.painter().rect_filled(body, radius, fill);
            }
            let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
            let mut x = (body.center().x - content / 2.0).max(body.left() + 4.0);
            if let Some(glyph) = glyph {
                painter.galley(
                    pos2(x, body.center().y - glyph.size().y / 2.0),
                    glyph.clone(),
                    ink,
                );
                x += glyph.size().x + gap;
            }
            if let Some(label) = label {
                painter.galley(pos2(x, body.center().y - label.size().y / 2.0), label, ink);
            }
        }
        if !self.tip.is_empty() && (self.icon_only || self.text.is_empty()) {
            response.on_hover_text(self.tip)
        } else {
            response
        }
    }
}

/// `FluentCombo.qml`: returns the index picked this frame.
pub(crate) fn combo<S: AsRef<str>>(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: impl Hash + std::fmt::Debug,
    width: f32,
    items: &[S],
    current: Option<usize>,
    enabled: bool,
) -> Option<usize> {
    let height = theme.control_height as f32;
    let id = ui.make_persistent_id(id_salt);
    let (_, rect) = ui.allocate_space(vec2(width, height));
    let response = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let open = Popup::is_id_open(ui.ctx(), Popup::default_response_id(&response));
    if ui.is_rect_visible(rect) {
        let ctx = ui.ctx().clone();
        let target = if response.is_pointer_button_down_on() {
            theme.control_fill_pressed
        } else if response.hovered() {
            theme.control_fill_hover
        } else {
            theme.control_fill
        };
        let dim = if enabled { 1.0 } else { 0.5 };
        let fill = motion::follow_color(&ctx, theme, id.with("fill"), target, 110.0, motion::linear);
        let radius = theme.radius_small as f32;
        let border = if open || response.has_focus() {
            theme.accent
        } else {
            theme.border_solid
        };
        bordered_rect(
            ui.painter(),
            rect,
            radius,
            fill.gamma_multiply(dim),
            border.gamma_multiply(dim),
        );
        let text = current
            .and_then(|index| items.get(index))
            .map_or("", |item| item.as_ref());
        let label = elided(
            ui,
            text,
            FontId::proportional(theme.font_normal as f32),
            theme.text.gamma_multiply(dim),
            width - 46.0,
        );
        ui.painter().galley(
            pos2(rect.left() + 12.0, rect.center().y - label.size().y / 2.0),
            label,
            theme.text,
        );
        let chevron = galley(
            ui,
            "\u{e70d}",
            FontId::proportional(12.0),
            theme.text_muted.gamma_multiply(dim),
        );
        ui.painter().galley(
            pos2(
                rect.right() - 12.0 - chevron.size().x,
                rect.center().y - chevron.size().y / 2.0,
            ),
            chevron,
            theme.text_muted,
        );
    }

    let mut picked = None;
    Popup::from_toggle_button_response(&response)
        .gap(4.0)
        .width(width - 10.0)
        .close_behavior(PopupCloseBehavior::CloseOnClick)
        .frame(
            Frame::new()
                .fill(theme.flyout)
                .stroke(Stroke::new(1.0, theme.flyout_border))
                .corner_radius(8)
                .inner_margin(4.0),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ScrollArea::vertical().max_height(312.0).show(ui, |ui| {
                ui.set_min_width(width - 10.0);
                for (index, item) in items.iter().enumerate() {
                    let (row, row_response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                    let pill = row.shrink(3.0);
                    let radius = theme.radius_small as f32;
                    if row_response.hovered() {
                        ui.painter().rect_filled(pill, radius, theme.card_hover);
                    }
                    if current == Some(index) {
                        ui.painter().rect_filled(
                            Rect::from_center_size(
                                pos2(pill.left() + 2.5, pill.center().y),
                                vec2(3.0, pill.height() * 0.5),
                            ),
                            1.5,
                            theme.accent,
                        );
                    }
                    let label = elided(
                        ui,
                        item.as_ref(),
                        FontId::proportional(theme.font_normal as f32),
                        theme.text,
                        row.width() - 24.0,
                    );
                    ui.painter().galley(
                        pos2(row.left() + 12.0, row.center().y - label.size().y / 2.0),
                        label,
                        theme.text,
                    );
                    if row_response.clicked() {
                        picked = Some(index);
                    }
                }
            });
        });
    picked
}

/// A pointing-hand hit area for hand-painted rows and tabs.
pub(crate) fn hit(ui: &Ui, rect: Rect, id: Id) -> Response {
    ui.interact(rect, id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
}

#[derive(Clone, Copy)]
struct Settle {
    from: f32,
    to: f32,
    start: f64,
    duration: f32,
}

/// `StatusRing.qml`: idle track, transition arc that spins while busy and
/// closes into a full ring once connected.
#[derive(Default)]
pub(crate) struct StatusRing {
    initialized: bool,
    was_busy: bool,
    was_active: bool,
    direction: f32,
    spin_started: f64,
    busy_started: f64,
    settle: Option<Settle>,
}

impl StatusRing {
    const SPIN_PERIOD: f64 = 1.1;

    fn rotation(&mut self, now: f64, spinning: bool) -> f32 {
        if spinning {
            let phase = ((now - self.spin_started) % Self::SPIN_PERIOD) / Self::SPIN_PERIOD;
            return self.direction * phase as f32 * 360.0;
        }
        let Some(settle) = self.settle else {
            return 0.0;
        };
        let progress = if settle.duration <= 0.0 {
            1.0
        } else {
            ((now - settle.start) as f32 / settle.duration).clamp(0.0, 1.0)
        };
        if progress >= 1.0 {
            self.settle = None;
            return 0.0;
        }
        settle.from + (settle.to - settle.from) * motion::ease_out_cubic(progress)
    }

    pub(crate) fn show(
        &mut self,
        ui: &Ui,
        theme: &Theme,
        rect: Rect,
        active: bool,
        busy: bool,
        target_active: bool,
    ) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|input| input.time);
        let animated = theme.anim_fast > 0;
        if !self.initialized {
            self.initialized = true;
            self.was_busy = false;
            self.was_active = active;
        }
        if busy && !self.was_busy {
            self.settle = None;
            self.busy_started = now;
            self.spin_started = now;
            self.direction = if target_active { 1.0 } else { -1.0 };
        } else if !busy && (self.was_busy || active != self.was_active) {
            let mut current = self.rotation(now, self.was_busy);
            let fast_disconnect = now - self.busy_started < 0.45;
            let (to, duration) = if active {
                if current == 0.0 {
                    current = 0.01;
                }
                ((current / 360.0).ceil() * 360.0, 0.52)
            } else if fast_disconnect {
                ((current / 360.0).round() * 360.0, 0.22)
            } else {
                if current.abs() < 0.01 {
                    current = -0.01;
                }
                ((current / 360.0).floor() * 360.0, 0.52)
            };
            self.settle = Some(Settle {
                from: current,
                to,
                start: now,
                duration: if animated { duration } else { 0.0 },
            });
        }
        self.was_busy = busy;
        self.was_active = active;
        let rotation = self.rotation(now, busy);
        if busy || self.settle.is_some() {
            ctx.request_repaint();
        }

        let transition = if target_active {
            theme.warning
        } else {
            theme.danger
        };
        let idle_track = if theme.dark {
            Color32::from_white_alpha(26)
        } else {
            Color32::from_black_alpha(26)
        };
        let idle_dot = if theme.dark {
            theme.text_faint
        } else {
            Color32::from_black_alpha(92)
        };
        // (sweep, arc colour, dot colour, dot scale), each with its QML duration.
        let (sweep, arc, dot, scale) = if busy {
            ((216.0, 620.0), (transition, 520.0), (transition, 480.0), (1.03, 420.0))
        } else if active {
            ((360.0, 520.0), (theme.success, 460.0), (theme.success, 420.0), (1.10, 360.0))
        } else {
            ((0.0, 420.0), (transition, 180.0), (idle_dot, 360.0), (1.0, 340.0))
        };
        let id = Id::unique("lumen-status-ring");
        let standard = motion::ease_out_cubic;
        let sweep = motion::follow(&ctx, theme, id.with("sweep"), sweep.0, sweep.1, motion::ease_in_out_cubic);
        let arc_color = motion::follow_color(&ctx, theme, id.with("arc"), arc.0, arc.1, standard);
        let dot_color = motion::follow_color(&ctx, theme, id.with("dot"), dot.0, dot.1, standard);
        let dot_scale = motion::follow(&ctx, theme, id.with("scale"), scale.0, scale.1, standard);
        let arc_alpha = motion::toggle(&ctx, theme, id.with("arc-alpha"), sweep > 0.0, 120.0, standard);

        let painter = ui.painter();
        let center = rect.center();
        let radius = (rect.width().min(rect.height()) - 8.0) / 2.0;
        painter.circle_stroke(center, radius, Stroke::new(4.0, idle_track));
        if arc_alpha > 0.0 {
            let color = arc_color.gamma_multiply(arc_alpha);
            let sweep = sweep.max(0.01);
            let steps = ((sweep / 4.0).ceil() as usize).max(2);
            let points: Vec<egui::Pos2> = (0..=steps)
                .map(|step| {
                    let angle = (-90.0 + rotation + sweep * step as f32 / steps as f32).to_radians();
                    center + radius * vec2(angle.cos(), angle.sin())
                })
                .collect();
            // Round caps (`ShapePath.RoundCap`).
            painter.circle_filled(points[0], 2.0, color);
            painter.circle_filled(points[steps], 2.0, color);
            painter.add(Shape::line(points, Stroke::new(4.0, color)));
        }
        painter.circle_filled(center, 7.0 * dot_scale, dot_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_ring_spins_in_the_target_direction_and_settles_at_the_top() {
        let mut ring = StatusRing {
            direction: -1.0,
            spin_started: 0.0,
            ..StatusRing::default()
        };
        // A quarter of the 1.1 s period, counter-clockwise for a disconnect.
        assert!((ring.rotation(0.275, true) + 90.0).abs() < 0.01);
        ring.settle = Some(Settle {
            from: 90.0,
            to: 360.0,
            start: 1.0,
            duration: 0.52,
        });
        let midway = ring.rotation(1.26, false);
        assert!(midway > 90.0 && midway < 360.0);
        assert_eq!(ring.rotation(2.0, false), 0.0);
        assert!(ring.settle.is_none());
    }
}
