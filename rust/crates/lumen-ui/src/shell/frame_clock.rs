//! Frame timestamps on the display's vblank grid.
//!
//! egui stamps a frame with the moment it starts, so animation steps follow
//! the jitter of event handling and frame work. Qt Quick advances its
//! animations by the vsync interval. `FrameClock` replaces the frame time
//! with the vblank at which the frame will be shown (DWM composition
//! timing), so consecutive frames are exactly one or more refresh periods
//! apart. Without DWM timing the time is left alone.

use egui::RawInput;

use super::win_native;

#[derive(Default)]
pub(super) struct FrameClock {
    /// egui time minus QPC time, fixed at the first frame.
    offset: Option<f64>,
    last: f64,
}

impl FrameClock {
    pub(super) fn apply(&mut self, raw: &mut RawInput) {
        let Some(time) = raw.time else {
            return;
        };
        let Some((next_vblank, period, now)) = win_native::vblank_timing() else {
            return;
        };
        let mut offset = *self.offset.get_or_insert(time - now);
        // Both clocks are QPC based; re-anchor only if they ever disagree.
        if (now + offset - time).abs() > 0.05 {
            offset = time - now;
            self.offset = Some(offset);
        }
        // Never step backwards; a second pass within one refresh period
        // keeps the same timestamp.
        let paced = (next_vblank + offset).max(self.last);
        self.last = paced;
        raw.time = Some(paced);
        raw.predicted_dt = period as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_times_never_go_backwards() {
        let mut clock = FrameClock::default();
        let mut previous = 0.0;
        for step in 0..20 {
            let mut raw = RawInput { time: Some(step as f64 * 0.004), ..Default::default() };
            clock.apply(&mut raw);
            let time = raw.time.unwrap();
            assert!(time >= previous);
            previous = time;
        }
    }
}
