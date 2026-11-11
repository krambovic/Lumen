//! Lumen application orchestration: controller, transition engine, services.

pub mod app_controller;
pub mod app_update_check;
pub mod app_updater;
pub mod auto_switch_service;
pub mod config_documents;
pub mod config_profiles;
pub mod connection_service;
pub mod diagnostics_service;
pub mod health_listener;
pub mod live_metrics_worker;
pub mod node_runtime_service;
pub mod node_service;
pub mod profile_service;
mod probe_targets;
pub mod runtime_introspection;
pub mod runtime_services;
pub mod server_preflight;
pub mod session_state;
pub mod subscription_cache;
pub mod subscription_dispatcher;
mod subscription_lifecycle;
pub mod subscription_refresh_timer;
pub mod subscription_service;
pub mod subscription_worker;
pub mod signature_service;
#[cfg(windows)]
pub mod single_instance;
pub mod transition_engine;
pub mod update_service;
pub mod worker_service;
pub mod xray_runtime_service;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    /// Consent globals (the diagnostics upload epoch) are process-wide;
    /// Python tests run sequentially, Rust tests do not — tests that toggle
    /// or observe consent hold this lock.
    pub static CONSENT_LOCK: Mutex<()> = Mutex::new(());
}
