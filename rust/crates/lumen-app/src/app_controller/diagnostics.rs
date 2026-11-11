//! Diagnostics bundle export and upload (`AppController.build_diagnostics`).

use std::path::PathBuf;
use std::sync::Arc;

use lumen_core::models::AppState;

use crate::app_controller::AppController;
use crate::diagnostics_service::DiagnosticsExport;

impl AppController {
    /// Python `build_diagnostics`: capture state/logs/consent, then write and
    /// optionally upload on a background thread. The completed archive path is
    /// reported through a `ControllerTask` so callers can surface it.
    pub fn build_diagnostics(
        &mut self,
        include: Option<serde_json::Value>,
        upload: bool,
        logs: Option<Vec<String>>,
        on_done: Option<Box<dyn FnOnce(&mut AppController, Result<PathBuf, String>) + Send>>,
    ) {
        let source: AppState = self.state.clone();
        let log_snapshot = logs.unwrap_or_else(|| self.recent_logs.snapshot());
        let shutting_down = Arc::clone(&self.shutting_down);
        let request = DiagnosticsExport::capture(&source, &log_snapshot, include, upload);
        let tasks = self.tasks.0.clone();
        if let Ok(handle) = std::thread::Builder::new()
            .name("diagnostics-export".into())
            .spawn(move || {
                let result = (|| -> Result<PathBuf, String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime
                        .block_on(request.run(shutting_down.clone()))
                        .map_err(|error| error.to_string())
                })();
                let _ = tasks.send(Box::new(move |controller: &mut AppController| {
                    if let Some(on_done) = on_done {
                        on_done(controller, result);
                    }
                }));
            })
        {
            self.diagnostics_exports.push(handle);
        }
    }
}
