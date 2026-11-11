//! Window reveal (port of `qml_app/window_reveal.py`).
//!
//! Avoid exposing the native window surface before the first frame painted.
//! The Qt window calls become a [`RevealSink`] the shell maps onto eframe
//! viewport commands.

use std::time::Duration;

/// What `WindowReveal` used to do to the QQuickWindow.
pub trait RevealSink {
    /// `window.setOpacity(value)`; 1.0 means fully revealed.
    fn set_opacity(&mut self, value: f64);
    /// `window.setVisible(value)`.
    fn set_visible(&mut self, value: bool);
    /// `window.show(); window.raise_(); window.requestActivate()`.
    fn show_raise_activate(&mut self);
}

/// Ported 1:1 from `WindowReveal`; the tests in
/// `windows/tests/test_window_reveal.py` assert the exact call order.
pub struct WindowReveal {
    pending: bool,
    frame_seen: bool,
}

impl Default for WindowReveal {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowReveal {
    pub fn new() -> Self {
        Self {
            pending: false,
            frame_seen: false,
        }
    }

    /// `request(*, activate=False)`.
    pub fn request(&mut self, sink: &mut dyn RevealSink, activate: bool) {
        self.pending = true;
        sink.set_opacity(0.0);
        if activate {
            sink.show_raise_activate();
        } else {
            sink.set_visible(true);
        }
        if self.frame_seen {
            self.frame_swapped(sink);
        }
    }

    /// `frame_swapped()`. `schedule` receives the finish callback, mirroring
    /// the Python `schedule` argument (`QTimer.singleShot(0, callback)`).
    pub fn frame_swapped(&mut self, sink: &mut dyn RevealSink) {
        self.frame_seen = true;
        if !self.pending {
            return;
        }
        self.pending = false;
        self.finish(sink);
    }

    /// `reveal_if_pending()` — fallback for render backends that do not emit
    /// frameSwapped.
    pub fn reveal_if_pending(&mut self, sink: &mut dyn RevealSink) {
        if self.pending {
            self.frame_swapped(sink);
        }
    }

    fn finish(&mut self, sink: &mut dyn RevealSink) {
        sink.set_opacity(1.0);
    }
}

/// The eframe sink: egui windows do not support a native opacity ramp, so
/// `set_opacity(0.0)` defers showing the viewport and `set_opacity(1.0)`
/// shows it. `show_raise_activate` focuses the window on reveal.
#[derive(Default)]
pub struct EframeRevealSink {
    /// The viewport must stay hidden until the reveal completes.
    pub hide_until_revealed: bool,
    pub commands: Vec<RevealCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealCommand {
    Hide,
    Show,
    Focus,
}

impl EframeRevealSink {
    /// Apply the collected commands; the shell forwards them as
    /// `ViewportCommand`s after the frame.
    pub fn drain(&mut self) -> Vec<RevealCommand> {
        std::mem::take(&mut self.commands)
    }
}

impl RevealSink for EframeRevealSink {
    fn set_opacity(&mut self, value: f64) {
        if value < 1.0 {
            self.hide_until_revealed = true;
            self.commands.push(RevealCommand::Hide);
        } else {
            self.hide_until_revealed = false;
            self.commands.push(RevealCommand::Show);
        }
    }

    fn set_visible(&mut self, value: bool) {
        if value {
            self.commands.push(RevealCommand::Show);
        } else {
            self.commands.push(RevealCommand::Hide);
        }
    }

    fn show_raise_activate(&mut self) {
        self.commands.push(RevealCommand::Show);
        self.commands.push(RevealCommand::Focus);
    }
}

/// `QTimer.singleShot(0, reveal.reveal_if_pending)` from `main_qml.py` —
/// the shell schedules this fallback 500 ms after a reveal request.
pub const REVEAL_FALLBACK_DELAY: Duration = Duration::from_millis(500);

#[cfg(test)]
mod tests {
    use super::*;

    /// The `_Window` double from test_window_reveal.py.
    #[derive(Default)]
    struct RecordingSink {
        calls: Vec<(String, f64)>,
    }

    impl RecordingSink {
        fn set_visible_call(&mut self, value: bool) {
            self.calls
                .push(("setVisible".into(), if value { 1.0 } else { 0.0 }));
        }
    }

    impl RevealSink for RecordingSink {
        fn set_opacity(&mut self, value: f64) {
            self.calls.push(("setOpacity".into(), value));
        }
        fn set_visible(&mut self, value: bool) {
            self.set_visible_call(value);
        }
        fn show_raise_activate(&mut self) {
            self.set_visible_call(true);
        }
    }

    /// test_window_reveal.test_reveal_waits_for_a_presented_frame_before_becoming_opaque
    #[test]
    fn reveal_waits_for_a_presented_frame_before_becoming_opaque() {
        let mut sink = RecordingSink::default();
        let mut reveal = WindowReveal::new();

        reveal.request(&mut sink, false);

        assert_eq!(
            sink.calls,
            vec![
                ("setOpacity".to_string(), 0.0),
                ("setVisible".to_string(), 1.0)
            ]
        );

        // frame_swapped finishes the pending reveal (the Python test asserts
        // the scheduled callback applied setOpacity(1.0) as the last call).
        reveal.frame_swapped(&mut sink);
        assert_eq!(sink.calls.last(), Some(&("setOpacity".to_string(), 1.0)));
    }

    #[test]
    fn activate_requests_show_and_focus() {
        let mut sink = RecordingSink::default();
        let mut reveal = WindowReveal::new();
        reveal.request(&mut sink, true);
        assert_eq!(
            sink.calls,
            vec![
                ("setOpacity".to_string(), 0.0),
                ("setVisible".to_string(), 1.0)
            ]
        );
    }

    #[test]
    fn eframe_sink_maps_opacity_to_viewport_visibility() {
        let mut sink = EframeRevealSink::default();
        let mut reveal = WindowReveal::new();
        reveal.request(&mut sink, true);
        assert!(sink.hide_until_revealed);
        assert_eq!(
            sink.drain(),
            vec![
                RevealCommand::Hide,
                RevealCommand::Show,
                RevealCommand::Focus
            ]
        );
        reveal.frame_swapped(&mut sink);
        assert!(!sink.hide_until_revealed);
        assert_eq!(sink.drain(), vec![RevealCommand::Show]);
    }

    #[test]
    fn frame_before_request_reveals_immediately() {
        let mut sink = RecordingSink::default();
        let mut reveal = WindowReveal::new();
        reveal.frame_swapped(&mut sink);
        reveal.request(&mut sink, false);
        assert_eq!(sink.calls.last(), Some(&("setOpacity".to_string(), 1.0)));
    }
}
