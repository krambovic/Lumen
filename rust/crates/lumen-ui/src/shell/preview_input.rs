//! Debug builds only: scripted input for visual comparison
//! (`rust/tools/uicmp`). `LUMEN_PREVIEW_INPUT` holds `;`-separated steps
//! `seconds,action,args...`, timed from the first frame:
//!
//! - `click,x,y` / `rclick,x,y` / `dclick,x,y`: press and release at a point
//! - `move,x,y`: hover
//! - `wheel,x,y,notches`: mouse wheel notches (negative scrolls down)
//! - `key,Name`: press and release an egui key (`Escape`, `Enter`, `Tab`...)
//! - `text,abc`: typed text
//!
//! Coordinates are egui points. The input never reaches other windows, so
//! the scripts can run while someone else uses the machine.
//!
//! `LUMEN_PREVIEW_INPUT_FILE` names a file to read the script from instead.
//! The clock starts when the file appears, so `uicmp.py` can write it once
//! the window is on screen and time its captures from the same moment.

use std::collections::VecDeque;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, pos2, vec2};

enum Step {
    Click(Pos2, PointerButton, u8),
    Move(Pos2),
    Wheel(Pos2, f32),
    Key(Key),
    Text(String),
}

pub(super) struct PreviewInput {
    /// The script file still to be read (`LUMEN_PREVIEW_INPUT_FILE`).
    pending_file: Option<std::path::PathBuf>,
    steps: VecDeque<(f64, Step)>,
    /// Events held back to the next frame (button releases), so egui sees
    /// a press and a release in separate passes like a real click.
    next_frame: Vec<Event>,
}

impl PreviewInput {
    pub(super) fn from_env() -> Option<Self> {
        if let Some(path) = std::env::var_os("LUMEN_PREVIEW_INPUT_FILE") {
            return Some(Self {
                pending_file: Some(path.into()),
                steps: VecDeque::new(),
                next_frame: Vec::new(),
            });
        }
        let script = std::env::var("LUMEN_PREVIEW_INPUT").ok()?;
        Some(Self {
            pending_file: None,
            steps: Self::parse(&script),
            next_frame: Vec::new(),
        })
    }

    fn parse(script: &str) -> VecDeque<(f64, Step)> {
        let mut steps = VecDeque::new();
        for item in script.split(';').map(str::trim).filter(|item| !item.is_empty()) {
            let parts: Vec<&str> = item.split(',').map(str::trim).collect();
            let Some(at) = parts.first().and_then(|value| value.parse::<f64>().ok()) else {
                continue;
            };
            let number = |index: usize| parts.get(index).and_then(|value| value.parse::<f32>().ok());
            let point = || Some(pos2(number(2)?, number(3)?));
            let step = match parts.get(1).copied() {
                Some("click") => point().map(|at| Step::Click(at, PointerButton::Primary, 1)),
                Some("dclick") => point().map(|at| Step::Click(at, PointerButton::Primary, 2)),
                Some("rclick") => point().map(|at| Step::Click(at, PointerButton::Secondary, 1)),
                Some("move") => point().map(Step::Move),
                Some("wheel") => point().zip(number(4)).map(|(at, dy)| Step::Wheel(at, dy)),
                Some("key") => parts.get(2).and_then(|name| Key::from_name(name)).map(Step::Key),
                Some("text") => parts.get(2).map(|text| Step::Text((*text).to_string())),
                _ => None,
            };
            if let Some(step) = step {
                steps.push_back((at, step));
            }
        }
        steps
    }

    /// Reads the script file once it exists. Returns false while waiting.
    pub(super) fn ready(&mut self) -> bool {
        let Some(path) = &self.pending_file else {
            return true;
        };
        let Ok(script) = std::fs::read_to_string(path) else {
            return false;
        };
        self.steps = Self::parse(&script);
        self.pending_file = None;
        true
    }

    /// Adds the due steps to this frame's input. Returns true while steps
    /// remain, so the caller keeps frames coming.
    pub(super) fn apply(&mut self, started: f64, raw: &mut RawInput) -> bool {
        raw.events.append(&mut self.next_frame);
        let now = raw.time.unwrap_or(0.0) - started;
        while self.steps.front().is_some_and(|(at, _)| *at <= now) {
            let (_, step) = self.steps.pop_front().expect("front checked");
            match step {
                Step::Click(at, button, count) => {
                    raw.events.push(Event::PointerMoved(at));
                    for _ in 0..count {
                        let press = |pressed| Event::PointerButton {
                            pos: at,
                            button,
                            pressed,
                            modifiers: Modifiers::NONE,
                        };
                        raw.events.push(press(true));
                        self.next_frame.push(press(false));
                    }
                }
                Step::Move(at) => raw.events.push(Event::PointerMoved(at)),
                Step::Wheel(at, dy) => {
                    raw.events.push(Event::PointerMoved(at));
                    raw.events.push(Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: vec2(0.0, dy),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Modifiers::NONE,
                    });
                }
                Step::Key(key) => {
                    let event = |pressed| Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    };
                    raw.events.push(event(true));
                    self.next_frame.push(event(false));
                }
                Step::Text(text) => raw.events.push(Event::Text(text)),
            }
        }
        !self.steps.is_empty() || !self.next_frame.is_empty()
    }
}
