//! Background diagnostics export from a snapshot of the controller state.

use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use chrono::Local;
use lumen_core::constants::{APP_VERSION, DIAGNOSTICS_UPLOAD_URL, log_dir};
use lumen_core::diagnostics::{DiagnosticsError, EngineVersionProbe, export_diagnostics_in};
use lumen_core::diagnostics_uploader::{get_upload_epoch, upload_bundle, uploads_allowed};
use lumen_core::models::AppState;
use lumen_engines::singbox::manager::get_singbox_version;
use lumen_engines::xray::manager::get_xray_version;
use serde_json::Value;

pub struct DiagnosticsExport {
    state: AppState,
    logs: Vec<String>,
    include: Option<Value>,
    upload: bool,
    epoch: u64,
}

impl DiagnosticsExport {
    /// Capture state, logs and consent before queuing work on a background thread.
    pub fn capture(
        state: &AppState,
        logs: &[String],
        include: Option<Value>,
        upload: bool,
    ) -> Self {
        let epoch = get_upload_epoch();
        Self {
            state: state.clone(),
            logs: logs.to_vec(),
            include,
            upload,
            epoch,
        }
    }

    /// Write the archive off the UI thread and queue any authorized upload.
    pub async fn run(self, shutting_down: Arc<AtomicBool>) -> anyhow::Result<PathBuf> {
        let archive = tokio::task::spawn_blocking(move || {
            self.build_in(&log_dir(), &shutting_down, &engine_version)
        })
        .await??;
        Ok(archive)
    }

    fn build_in(
        self,
        directory: &Path,
        shutting_down: &AtomicBool,
        probe: EngineVersionProbe<'_>,
    ) -> Result<PathBuf, DiagnosticsError> {
        let will_upload = self.upload
            && self.state.settings.diagnostics_upload_enabled
            && !DIAGNOSTICS_UPLOAD_URL.is_empty()
            && uploads_allowed(self.epoch);
        let cancelled = || {
            shutting_down.load(Ordering::Acquire) || (will_upload && !uploads_allowed(self.epoch))
        };
        let stamp = Local::now().format("%Y%m%d_%H%M%S_%6f");
        let id = uuid::Uuid::new_v4().simple();
        let path = directory.join(format!("diagnostics_{stamp}_{id}.zip"));
        let bundle = export_diagnostics_in(
            directory,
            &path,
            &self.state,
            &self.logs,
            self.include.as_ref(),
            Some(&cancelled),
            probe,
        )?;
        if will_upload && !cancelled() {
            upload_bundle(
                DIAGNOSTICS_UPLOAD_URL,
                &bundle,
                APP_VERSION,
                Some(self.epoch),
            );
        }
        Ok(bundle)
    }
}

fn engine_version(engine: &str, path: &str) -> Option<String> {
    match engine {
        "xray" => get_xray_version(path),
        "sing-box" => get_singbox_version(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Read;

    #[test]
    fn export_uses_captured_logs_and_selected_sections() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let mut logs = vec!["original sample".into()];
        let request = DiagnosticsExport::capture(
            &AppState::default(),
            &logs,
            Some(json!({"recent": true})),
            false,
        );
        logs[0] = "later sample".into();
        let path = request.build_in(dir.path(), &AtomicBool::new(false), &|engine, _| {
            (engine == "xray").then(|| "Xray 25.10.15".into())
        })?;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        assert_eq!(archive.len(), 2);
        let mut recent = String::new();
        archive
            .by_name("recent_logs.txt")?
            .read_to_string(&mut recent)?;
        assert_eq!(recent, "original sample");
        let meta: Value = serde_json::from_reader(archive.by_name("meta.json")?)?;
        assert_eq!(meta["runtime"]["engines"]["xray"], "25.10.15");
        Ok(())
    }

    #[test]
    fn shutdown_cancels_queued_export_before_probing_or_writing() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let request = DiagnosticsExport::capture(&AppState::default(), &[], None, true);
        let result = request.build_in(dir.path(), &AtomicBool::new(true), &|_, _| {
            panic!("must not probe during shutdown")
        });
        assert!(matches!(result, Err(DiagnosticsError::Cancelled)));
        assert!(dir.path().read_dir()?.next().is_none());
        Ok(())
    }
}
