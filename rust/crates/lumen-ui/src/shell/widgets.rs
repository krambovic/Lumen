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

/// `Card.qml`. `padding` is the QML padding, border included.
///
/// A hoverable card (the QML default) answers the pointer like Card.qml:
/// over `animFast` its border turns from `borderSolid` to `divider`, a
/// 1.7% white (1.5% black in light themes) overlay fades in and a shadow
/// appears 3 px below it at 60% of `Theme.shadowOpacity`. The hover test
/// does not take input from the content.
pub(crate) fn card_frame(theme: &Theme, padding: f32) -> Card {
    Card {
        padding,
        hoverable: true,
        radius: theme.radius as f32,
        fill: theme.card,
        border: theme.border_solid,
        hot_border: theme.divider,
        overlay: if theme.dark {
            Color32::from_rgba_unmultiplied(255, 255, 255, 4)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 4)
        },
        shadow_alpha: theme.shadow_opacity as f32 * 0.6,
        fast_ms: theme.anim_fast as f32,
    }
}

pub(crate) struct Card {
    padding: f32,
    hoverable: bool,
    radius: f32,
    fill: Color32,
    border: Color32,
    hot_border: Color32,
    overlay: Color32,
    shadow_alpha: f32,
    fast_ms: f32,
}

impl Card {
    /// `hoverable: false`.
    pub(crate) fn hoverable(mut self, hoverable: bool) -> Self {
        self.hoverable = hoverable;
        self
    }

    pub(crate) fn show<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let id = ui.next_auto_id().with("lumen-card-hover");
        let background = ui.painter().add(egui::Shape::Noop);
        let inner = Frame::new().inner_margin(self.padding).show(ui, content);
        let rect = inner.response.rect;
        let hot = self.hoverable && ui.rect_contains_pointer(rect);
        let level = if self.hoverable {
            let ctx = ui.ctx().clone();
            let target = if hot { 1.0 } else { 0.0 };
            if self.fast_ms <= 0.0 {
                target
            } else {
                ctx.animate_value_with_time(id, target, self.fast_ms / 1000.0)
            }
        } else {
            0.0
        };
        let mut shapes: Vec<egui::Shape> = Vec::new();
        if level > 0.0 {
            // MultiEffect blurs the card's own alpha: a translucent card
            // casts a proportionally lighter shadow (and it shows through).
            let coverage = f32::from(self.fill.a()) / 255.0;
            let alpha = (self.shadow_alpha * coverage * level * 255.0).round().clamp(0.0, 255.0) as u8;
            shapes.push(
                egui::epaint::Shadow {
                    offset: [0, (3.0 * level).round() as i8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(alpha),
                }
                .as_shape(rect, self.radius)
                .into(),
            );
        }
        shapes.push(egui::Shape::rect_filled(rect, self.radius, self.fill));
        if level > 0.0 {
            shapes.push(egui::Shape::rect_filled(
                rect.shrink(1.0),
                (self.radius - 1.0).max(0.0),
                self.overlay.gamma_multiply(level),
            ));
        }
        let border = motion::mix(self.border, self.hot_border, level);
        shapes.push(egui::Shape::rect_stroke(
            rect,
            self.radius,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        ));
        ui.painter().set(background, egui::Shape::Vec(shapes));
        inner
    }
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
            response.fluent_tip(self.tip)
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

/// `FluentCombo.qml` chevron for stock `egui::ComboBox` fields.
pub(crate) fn combo_chevron(ui: &Ui, rect: Rect, _visuals: &egui::style::WidgetVisuals, _open: bool) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{e70d}",
        FontId::proportional(12.0),
        ui.visuals().widgets.noninteractive.fg_stroke.color,
    );
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
        // Centred on the arc's radius (egui strokes circles outside).
        painter.circle_stroke(center, radius - 2.0, Stroke::new(4.0, idle_track));
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

// ── BusyIndicator (Qt Quick Controls Universal) ─────────────────────────────

/// One phase of `QQuickUniversalBusyIndicatorNode`: duration (ms), rotation
/// range (degrees) and an optional cubic Bezier easing (`None` = linear).
const BUSY_PHASES: [(f32, f32, f32, Option<[f32; 4]>); 6] = [
    (433.0, -110.0, 10.0, Some([0.02, 0.33, 0.38, 0.77])),
    (767.0, 10.0, 93.0, None),
    (417.0, 93.0, 205.0, Some([0.57, 0.17, 0.95, 0.75])),
    (400.0, 205.0, 357.0, Some([0.00, 0.19, 0.07, 0.72])),
    (766.0, 357.0, 439.0, None),
    (434.0, 439.0, 585.0, Some([0.00, 0.00, 0.95, 0.37])),
];
const BUSY_INTERVAL: f32 = 167.0;
const BUSY_TOTAL: f32 = 4052.0;

/// `QEasingCurve::BezierSpline` with one segment from (0,0) to (1,1).
fn cubic_bezier(points: [f32; 4], progress: f32) -> f32 {
    let [x1, y1, x2, y2] = points;
    let axis = |t: f32, a: f32, b: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    // x(t) is monotonic for these control points: bisect for t.
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if axis(mid, x1, x2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    axis((low + high) / 2.0, y1, y2)
}

/// Rotation (degrees) of each dot at `time_ms` into the 4052 ms loop, or
/// `None` while the dot is hidden (`updateCurrentTime`).
fn busy_dot_rotations(time_ms: f32, count: usize) -> Vec<Option<f32>> {
    (0..count)
        .map(|index| {
            let begin = index as f32 * BUSY_INTERVAL;
            let end = BUSY_TOTAL - (BUSY_PHASES.len() - index - 1) as f32 * BUSY_INTERVAL;
            if time_ms < begin || time_ms > end {
                return None;
            }
            let (mut phase, mut remain, mut elapsed) = (0, time_ms, 0.0);
            while phase < BUSY_PHASES.len() - 1 {
                if remain <= BUSY_PHASES[phase].0 + begin {
                    break;
                }
                remain -= BUSY_PHASES[phase].0;
                elapsed += BUSY_PHASES[phase].0;
                phase += 1;
            }
            let (duration, from, to, curve) = BUSY_PHASES[phase];
            let shift = (index * count) as f32;
            let progress = ((time_ms - elapsed - begin) / duration).clamp(0.0, 1.0);
            let value = curve.map_or(progress, |points| cubic_bezier(points, progress));
            Some((from - shift) + ((to - shift) - (from - shift)) * value)
        })
        .collect()
}

/// The Universal-style `BusyIndicator` (Windows 8 progress ring): five
/// accent dots, or six from 60 px, that chase each other around a ring.
/// Port of `QQuickUniversalBusyIndicatorNode::sync` / `updateCurrentTime`.
pub(crate) fn busy_indicator(ui: &Ui, rect: Rect, color: Color32) {
    let size = rect.width().min(rect.height());
    let count = if size < 60.0 { 5 } else { 6 };
    let diameter = size / 10.0;
    let offset = (size - diameter * 2.0) / std::f32::consts::PI;
    // The dot's centre in the node space before rotation (45° down-right).
    let base = egui::vec2(offset + diameter / 2.0, offset + diameter / 2.0);
    let time_ms = ((ui.input(|input| input.time) * 1000.0) % BUSY_TOTAL as f64) as f32;
    let painter = ui.painter();
    for rotation in busy_dot_rotations(time_ms, count).into_iter().flatten() {
        let (sin, cos) = rotation.to_radians().sin_cos();
        let point = rect.center() + egui::vec2(base.x * cos - base.y * sin, base.x * sin + base.y * cos);
        painter.circle_filled(point, diameter / 2.0, color);
    }
    ui.ctx().request_repaint();
}

#[cfg(test)]
mod busy_indicator_tests {
    use super::*;

    #[test]
    fn dots_start_one_interval_apart_and_follow_the_qt_phases() {
        let at_start = busy_dot_rotations(0.0, 5);
        assert_eq!(at_start[0], Some(-110.0));
        assert!(at_start[1..].iter().all(Option::is_none));
        // The first dot reaches the end of phase 0 (10°) after 433 ms.
        let rotation = busy_dot_rotations(433.0, 5)[0].unwrap();
        assert!((rotation - 10.0).abs() < 0.01);
        // Dot 1 starts 167 ms later, shifted back by count degrees.
        let rotation = busy_dot_rotations(167.0, 5)[1].unwrap();
        assert!((rotation - (-115.0)).abs() < 0.01);
        assert!((cubic_bezier([0.0, 0.19, 0.07, 0.72], 1.0) - 1.0).abs() < 1e-4);
    }
}

// ── ProgressBar, indeterminate (Qt Quick Controls Universal) ────────────────

const BAR_TOTAL: f32 = 3917.0;
const BAR_INTERVAL: f32 = 167.0;
const BAR_VISIBLE: f32 = 3000.0;
const BAR_DOTS: usize = 5;
const BAR_DOT: f32 = 4.0;
/// `(duration ms, from, to)`: border phases are absolute pixels (linear),
/// ellipse phases are fractions of the width (eased).
const BAR_BORDER: [(f32, f32, f32); 4] =
    [(500.0, -50.0, 0.0), (1500.0, 0.0, 0.0), (1000.0, 0.0, 100.0), (917.0, 100.0, 100.0)];
const BAR_ELLIPSE: [(f32, f32, f32); 4] = [
    (1000.0, 0.0, 1.0 / 3.0),
    (1000.0, 1.0 / 3.0, 1.0 / 3.0),
    (1000.0, 1.0 / 3.0, 2.0 / 3.0),
    (1000.0, 1.0 / 3.0, 2.0 / 3.0),
];

fn bar_phase(phases: &[(f32, f32, f32); 4], time: f32, begin: f32) -> ((f32, f32, f32), f32) {
    let (mut index, mut remain, mut elapsed) = (0, time, 0.0);
    while index < phases.len() - 1 {
        if remain <= phases[index].0 + begin {
            break;
        }
        remain -= phases[index].0;
        elapsed += phases[index].0;
        index += 1;
    }
    let phase = phases[index];
    (phase, ((time - elapsed - begin) / phase.0).clamp(0.0, 1.0))
}

/// The x offsets of the visible dots at `time_ms` into the 3917 ms loop
/// (`QQuickUniversalProgressBarNode::updateCurrentTime`).
fn progress_dot_offsets(time_ms: f32, width: f32) -> Vec<f32> {
    let container = -34.0 + 0.435_222 * width * time_ms / BAR_TOTAL;
    (0..BAR_DOTS)
        .filter_map(|index| {
            let begin = index as f32 * BAR_INTERVAL;
            if time_ms < begin || time_ms > BAR_VISIBLE + begin {
                return None;
            }
            let ((_, from, to), progress) = bar_phase(&BAR_BORDER, time_ms, begin);
            let border = from + (to - from) * progress;
            let ((_, from, to), progress) = bar_phase(&BAR_ELLIPSE, time_ms, begin);
            let eased = cubic_bezier([0.4, 0.0, 0.6, 1.0], progress);
            let ellipse = (from + (to - from) * eased) * width;
            let base = (BAR_DOTS - index - 1) as f32 * (BAR_DOT + 4.0);
            Some(container + border + ellipse + base)
        })
        .collect()
}

/// `ProgressBar { indeterminate: true }` in the Universal style: five 4 px
/// accent dots that sweep across the bar, clipped to it.
pub(crate) fn indeterminate_progress(ui: &Ui, rect: Rect, color: Color32) {
    let time_ms = ((ui.input(|input| input.time) * 1000.0) % BAR_TOTAL as f64) as f32;
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    for x in progress_dot_offsets(time_ms, rect.width()) {
        painter.circle_filled(
            egui::pos2(rect.left() + x + BAR_DOT / 2.0, rect.center().y),
            BAR_DOT / 2.0,
            color,
        );
    }
    ui.ctx().request_repaint();
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn dots_enter_from_the_left_and_rest_in_the_well() {
        // At the start only the first dot is visible, left of the bar.
        let start = progress_dot_offsets(0.0, 280.0);
        assert_eq!(start.len(), 1);
        assert!(start[0] < 0.0);
        // Mid-loop all five dots sit near a third of the width.
        let well = progress_dot_offsets(1800.0, 280.0);
        assert_eq!(well.len(), 5);
        assert!(well.iter().all(|x| (60.0..160.0).contains(x)));
    }
}



// ── FluentToolTip ───────────────────────────────────────────────────────────

/// `FluentToolTip.qml` as an extension of `Response`: after 450 ms of hover a
/// flyout (radius 8, 1 px `flyoutBorder`, 12 / 8 px padding, `fontNormal`
/// text, at most 4 lines) fades in over 100 ms, centred 4 px above the
/// widget and kept 8 px inside the window; it hides after 6 s.
pub(crate) trait FluentTip {
    fn fluent_tip(self, text: impl Into<String>) -> Self;
    /// With a custom delay in seconds (SettingsPage `InfoIcon` waits 250 ms).
    fn fluent_tip_after(self, text: impl Into<String>, delay: f64) -> Self;
}

impl FluentTip for Response {
    fn fluent_tip(self, text: impl Into<String>) -> Self {
        show_fluent_tip(&self, text.into(), 0.45);
        self
    }

    fn fluent_tip_after(self, text: impl Into<String>, delay: f64) -> Self {
        show_fluent_tip(&self, text.into(), delay);
        self
    }
}

fn show_fluent_tip(response: &Response, text: String, delay: f64) {
    let ctx = response.ctx.clone();
    let id = response.id.with("lumen-fluent-tip");
    let now = ctx.input(|input| input.time);
    if !response.hovered() || text.is_empty() || ctx.dragged_id().is_some() {
        ctx.data_mut(|data| data.remove::<f64>(id));
        return;
    }
    let start = ctx.data_mut(|data| *data.get_temp_mut_or_insert_with(id, || now));
    let elapsed = now - start;
    if elapsed < delay {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(delay - elapsed));
        return;
    }
    // `timeout: 6000` after the tip appears.
    if elapsed > delay + 6.0 {
        return;
    }
    let Some(theme) = super::fluent_menu::published_theme(&ctx) else {
        return;
    };
    let animated = theme.anim_fast > 0;
    let opacity = if animated { (((elapsed - delay) / 0.1) as f32).clamp(0.0, 1.0) } else { 1.0 };
    if opacity < 1.0 {
        ctx.request_repaint();
    } else {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((delay + 6.0 - elapsed).max(0.0)));
    }
    let screen = ctx.content_rect();
    let font = FontId::proportional(theme.font_normal as f32);
    let max_text = (screen.width() - 16.0 - 24.0).max(40.0);
    let mut job = egui::text::LayoutJob::simple(text, font, theme.text, max_text);
    job.wrap.max_rows = 4;
    let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
    let size = galley.size() + vec2(24.0, 16.0);
    let anchor = response.rect;
    let mut x = (anchor.center().x - size.x / 2.0).round();
    let mut y = anchor.top() - size.y - 4.0;
    if y < screen.top() + 8.0 {
        // The popup engine flips it below when there is no room above.
        y = anchor.bottom() + 4.0;
    }
    x = x.clamp(screen.left() + 8.0, (screen.right() - 8.0 - size.x).max(screen.left() + 8.0));
    let rect = Rect::from_min_size(pos2(x, y), size);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id));
    let fade = |color: Color32| color.gamma_multiply(opacity);
    painter.rect(rect, 8.0, fade(theme.flyout), Stroke::new(1.0, fade(theme.flyout_border)), StrokeKind::Inside);
    painter.galley_with_override_text_color(rect.min + vec2(12.0, 8.0), galley, fade(theme.text));
}
