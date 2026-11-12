//! Preserve failure details for the GUI executable and its background threads.

use std::{
    backtrace::Backtrace,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    panic,
    path::{Path, PathBuf},
};

use lumen_core::{logging::get_logger, secret_scrubber::scrub_text};
use serde_json::Value;

pub(super) struct StartupDiagnostics {
    current: PathBuf,
    previous: PathBuf,
}

impl StartupDiagnostics {
    pub(super) fn install(log_dir: &Path) -> Self {
        let current = log_dir.join("faulthandler.log");
        let previous = log_dir.join("faulthandler.prev");
        let _ = fs::create_dir_all(log_dir);
        if current.metadata().is_ok_and(|file| file.len() > 0) {
            let _ = fs::rename(&current, &previous);
        }
        let panic_log = current.clone();
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            default_hook(info);
            let trace = Backtrace::force_capture();
            write_report(&panic_log, &format!("{info}\n{trace}"));
        }));
        Self { current, previous }
    }

    pub(super) fn report_previous(&self) {
        let mut details = Vec::new();
        if File::open(&self.previous)
            .and_then(|file| file.take(64 * 1024).read_to_end(&mut details))
            .is_ok()
        {
            let details = String::from_utf8_lossy(&details);
            if !details.trim().is_empty() {
                get_logger("app").error(format!(
                    "[app] Crash report from the previous run:\n{}",
                    details.trim()
                ));
            }
            let _ = fs::remove_file(&self.previous);
        }
    }

    pub(super) fn report_renderer_failure(&self, error: &eframe::Error) {
        let detail = format!("Unable to initialize the Lumen window: {error}");
        write_report(&self.current, &detail);
        eprintln!("{detail}");
    }
}

fn write_report(path: &Path, detail: &str) {
    let scrubbed = scrub_text(&Value::String(detail.to_owned()));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(format!("{scrubbed}\n").as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_crash_details_without_credentials() {
        let path = std::env::temp_dir().join(format!("lumen-crash-{}.log", uuid::Uuid::new_v4()));
        write_report(
            &path,
            "window initialization failed password=fixture-password",
        );
        write_report(&path, "worker thread panicked");

        let report = fs::read_to_string(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(report.contains("window initialization failed"));
        assert!(report.contains("worker thread panicked"));
        assert!(!report.contains("fixture-password"));
    }
}
