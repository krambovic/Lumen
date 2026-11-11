//! Lumen UI (egui frontend).
#![recursion_limit = "256"]

pub mod shell;
pub mod view_models;

/// Entry point used by `main.rs`; the shell owns the eframe application.
pub fn run() -> i32 {
    shell::run()
}
