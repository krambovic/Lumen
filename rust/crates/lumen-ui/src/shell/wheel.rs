//! Mouse-wheel glide of `FluentScroll.qml`: every notch moves the target by
//! `wheelStep` (92 px) and the view eases to it over `glide` (540 ms,
//! `Easing.OutQuint`). egui's own wheel handling steps 40 px per notch and
//! settles in about 100 ms, which reads as a jump next to the QML client.
//!
//! The glide rewrites line-unit wheel events in the raw input into small
//! point-unit deltas, so every scroll area follows it without changes.
//!
//! QML clamps the animation target to `0..maxY`, so a glide that runs into
//! the end of the content decelerates onto it. Scroll areas report how far
//! they can still move ([`GlideRoom::glide_room`]); the innermost one under
//! the pointer bounds the next target. Unreported areas keep the unclamped
//! glide, which stops abruptly at the end.

use egui::{
    Context, Event, Id, Modifiers, MouseWheelSource, MouseWheelUnit, RawInput, Rect, TouchPhase, Ui,
    Vec2, scroll_area::ScrollAreaOutput,
};

use super::motion::ease_out_quint;

const WHEEL_STEP: f32 = 92.0;
const GLIDE_SECONDS: f64 = 0.54;
/// egui adds its own smoothing to point deltas of 8 px and more.
const MAX_EVENT_DELTA: f32 = 7.0;

#[derive(Default)]
pub(crate) struct WheelGlide {
    /// Start, end and already emitted travel of the running animation.
    from: Vec2,
    to: Vec2,
    emitted: Vec2,
    start: f64,
    running: bool,
    modifiers: Modifiers,
    source: Option<MouseWheelSource>,
    /// Points the hovered scroll area could still move up and down after the
    /// last frame, in wheel-delta direction (positive = towards the top).
    room: Option<(f32, f32)>,
}

/// Room of the innermost scroll area under the pointer, per pass.
#[derive(Clone, Copy)]
struct Room {
    pass: u64,
    rect: Rect,
    up: f32,
    down: f32,
}

fn room_id() -> Id {
    Id::new("lumen-wheel-glide-room")
}

/// Reports a scroll area's remaining travel to the wheel glide.
pub(crate) trait GlideRoom {
    fn glide_room(self, ui: &Ui) -> Self;
}

impl<R> GlideRoom for ScrollAreaOutput<R> {
    fn glide_room(self, ui: &Ui) -> Self {
        let ctx = ui.ctx();
        // Only when the pointer is over this area's layer: a popup list
        // over the page must not take the page's room.
        let inside = ctx.pointer_hover_pos().is_some_and(|pos| {
            self.inner_rect.contains(pos) && ctx.layer_id_at(pos) == Some(ui.layer_id())
        });
        let max = (self.content_size.y - self.inner_rect.height()).max(0.0);
        // An area that cannot scroll leaves the wheel to its parent.
        if inside && max > 0.5 {
            let pass = ctx.cumulative_pass_nr();
            let room = Room {
                pass,
                rect: self.inner_rect,
                up: self.state.offset().y.clamp(0.0, max),
                down: (max - self.state.offset().y).max(0.0),
            };
            ctx.data_mut(|data| {
                let keep = data.get_temp::<Room>(room_id()).is_some_and(|old| {
                    old.pass == pass && old.rect.area() <= room.rect.area()
                });
                if !keep {
                    data.insert_temp(room_id(), room);
                }
            });
        }
        self
    }
}

impl WheelGlide {
    /// Take the room the scroll areas reported during the last pass.
    pub(crate) fn read_room(&mut self, ctx: &Context) {
        let last = ctx.cumulative_pass_nr().saturating_sub(1);
        self.room = ctx
            .data(|data| data.get_temp::<Room>(room_id()))
            .filter(|room| room.pass >= last)
            .map(|room| (room.up, room.down));
    }

    fn position(&self, now: f64) -> Vec2 {
        let progress = ((now - self.start) / GLIDE_SECONDS).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * ease_out_quint(progress)
    }

    /// Replace wheel notches in `raw` with this frame's share of the glide.
    /// Returns `true` while the animation needs further frames.
    pub(crate) fn apply(&mut self, raw: &mut RawInput, animated: bool, zoom: Modifiers) -> bool {
        let now = raw.time.unwrap_or(0.0);
        let mut notches = Vec2::ZERO;
        raw.events.retain(|event| match event {
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta,
                phase: TouchPhase::Move,
                source,
                modifiers,
            } if !modifiers.matches_any(zoom) => {
                notches += *delta;
                self.modifiers = *modifiers;
                self.source = Some(*source);
                false
            }
            _ => true,
        });
        if notches != Vec2::ZERO {
            // `scrollAnim.to = (running ? to : contentY) - notches * wheelStep`.
            let current = if self.running { self.position(now) } else { self.emitted };
            let base = if self.running { self.to } else { current };
            self.from = current;
            self.to = base + notches * WHEEL_STEP;
            if let Some((up, down)) = self.room {
                // `Math.max(0, Math.min(maxY, ...))`: the area has consumed
                // everything emitted so far.
                self.to.y = self.to.y.clamp(self.emitted.y - down, self.emitted.y + up);
            }
            self.start = if animated { now } else { now - GLIDE_SECONDS };
            self.running = true;
        }
        if !self.running {
            return false;
        }
        let position = self.position(now);
        let mut delta = position - self.emitted;
        self.emitted = position;
        if now - self.start >= GLIDE_SECONDS {
            self.running = false;
            // Keep the accumulators small; only differences matter.
            self.from = Vec2::ZERO;
            self.to = Vec2::ZERO;
            self.emitted = Vec2::ZERO;
        }
        let source = self.source.unwrap_or(MouseWheelSource::Unknown);
        while delta.length() > 0.01 {
            let step = if delta.length() > MAX_EVENT_DELTA {
                delta * (MAX_EVENT_DELTA / delta.length())
            } else {
                delta
            };
            delta -= step;
            raw.events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: step,
                phase: TouchPhase::Move,
                source,
                modifiers: self.modifiers,
            });
        }
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notch(lines: f32) -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, lines),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        }
    }

    fn travelled(raw: &RawInput) -> f32 {
        raw.events
            .iter()
            .map(|event| match event {
                Event::MouseWheel { unit: MouseWheelUnit::Point, delta, .. } => {
                    assert!(delta.length() <= MAX_EVENT_DELTA + 0.001);
                    delta.y
                }
                other => panic!("unexpected event {other:?}"),
            })
            .sum()
    }

    #[test]
    fn a_notch_travels_one_wheel_step_over_the_glide() {
        let mut glide = WheelGlide::default();
        let mut total = 0.0;
        let mut running = true;
        let mut frame = 0;
        while running {
            let mut raw = RawInput { time: Some(frame as f64 / 60.0), ..Default::default() };
            if frame == 0 {
                raw.events.push(notch(-1.0));
            }
            running = glide.apply(&mut raw, true, Modifiers::CTRL);
            total += travelled(&raw);
            frame += 1;
        }
        assert!((total + WHEEL_STEP).abs() < 0.1, "travelled {total}");
        // 540 ms at 60 Hz.
        assert!((32..=34).contains(&frame), "frames {frame}");
    }

    #[test]
    fn the_target_stops_at_the_end_of_the_content() {
        // FluentScroll: `Math.max(0, Math.min(maxY, base - notches * step))`.
        let mut glide = WheelGlide::default();
        glide.room = Some((0.0, 30.0));
        let mut total = 0.0;
        let mut first_frame = 0.0;
        for frame in 0..60 {
            let mut raw = RawInput { time: Some(frame as f64 / 60.0), ..Default::default() };
            if frame == 0 {
                raw.events.push(notch(-1.0));
            }
            glide.apply(&mut raw, true, Modifiers::CTRL);
            let step = travelled(&raw);
            if frame == 0 {
                first_frame = step;
            }
            total += step;
        }
        assert!((total + 30.0).abs() < 0.1, "travelled {total}");
        // It eases onto the end instead of running at full speed into it.
        assert!(first_frame.abs() < 6.0, "first frame {first_frame}");
    }

    #[test]
    fn a_second_notch_extends_the_running_target() {
        let mut glide = WheelGlide::default();
        let mut total = 0.0;
        for frame in 0..200 {
            let mut raw = RawInput { time: Some(frame as f64 / 60.0), ..Default::default() };
            if frame == 0 || frame == 6 {
                raw.events.push(notch(1.0));
            }
            glide.apply(&mut raw, true, Modifiers::CTRL);
            total += travelled(&raw);
        }
        assert!((total - 2.0 * WHEEL_STEP).abs() < 0.1, "travelled {total}");
    }

    #[test]
    fn zoom_wheel_and_disabled_animations_are_not_glided() {
        let mut glide = WheelGlide::default();
        let mut raw = RawInput { time: Some(1.0), ..Default::default() };
        raw.events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, 1.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::CTRL,
        });
        assert!(!glide.apply(&mut raw, true, Modifiers::CTRL));
        assert_eq!(raw.events.len(), 1);

        let mut raw = RawInput { time: Some(2.0), ..Default::default() };
        raw.events.push(notch(1.0));
        assert!(!glide.apply(&mut raw, false, Modifiers::CTRL));
        assert!((travelled(&raw) - WHEEL_STEP).abs() < 0.1);
    }
}
