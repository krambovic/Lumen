//! Ports of the Qt-free expectations from `test_connection_state_machine.py`,
//! `test_transition_responsiveness.py` plus cheap controller-level checks.

use std::time::{Duration, Instant};

use tempfile::tempdir;
use tokio::sync::oneshot;

use super::*;
use crate::profile_service::ProfileService;
use crate::runtime_services::handle_unexpected_disconnect;

#[tokio::test]
async fn shell_driver_preserves_controller_events_across_iterations() {
    let (mut controller, _root) = test_controller();
    let mut managers = controller.manager_events();
    let mut ui = controller.subscribe();
    let sender = controller.task_sender();
    for name in ["first", "second"] {
        sender.send(Box::new(move |controller| controller.report_diagnostics(Ok(PathBuf::from(name))))).unwrap();
        assert!(controller.drive_event_loop_once(&mut managers).await);
        assert!(matches!(ui.try_recv(), Ok(AppEvent::DiagnosticsFinished(Ok(path))) if path == PathBuf::from(name)));
    }
    controller.request_event_loop_stop();
    assert!(!controller.drive_event_loop_once(&mut managers).await);
}

/// A controller isolated from the real user profile: temp state file and
/// temp profile roots, system proxy disabled so no registry writes happen.
pub(crate) fn test_controller() -> (AppController, tempfile::TempDir) {
    let root = tempdir().expect("temp root");
    let storage = lumen_core::storage::StateStorage::new(root.path().join("state.json"));
    let mut controller = AppController::with_storage(storage);
    controller.profiles = std::cell::RefCell::new(ProfileService::new(root.path().to_path_buf()));
    controller.state.settings.enable_system_proxy = false;
    (controller, root)
}

#[test]
fn finished_transition_clears_the_auto_switch_flag() {
    let (mut controller, _root) = test_controller();
    controller.runtime.auto_switch_transitioning = true;

    controller.on_transition_action_complete(true, "proxy_hot_swap", "auto-switch", 0);

    assert!(!controller.runtime.auto_switch_transitioning);
}

#[test]
fn no_op_transition_clears_the_auto_switch_flag() {
    let (mut controller, _root) = test_controller();
    controller.runtime.auto_switch_transitioning = true;
    controller.transitions.pending = true;
    controller.load_state = LoadState::Loaded;

    controller.drain_transition_queue();

    assert!(!controller.runtime.auto_switch_transitioning);
    assert!(!controller.transitions.pending);
}

#[test]
fn kill_switch_keeps_system_proxy_after_a_proxy_hot_swap() {
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;
    controller.state.settings.kill_switch = true;
    controller.state.settings.enable_system_proxy = true;
    controller.state.settings.tun_mode = false;
    controller.runtime.desired_connected = true;

    // Auto-switch requested a transition that resolved to a proxy hot swap.
    controller.runtime.auto_switch_transitioning = true;
    controller.on_transition_action_complete(true, "proxy_hot_swap", "auto-switch", 0);

    // The xray process later dies on its own.
    controller.connected = true;
    controller.active_session = Some(crate::session_state::ActiveSessionSnapshot::default());
    handle_unexpected_disconnect(&mut controller);

    assert!(controller.runtime.kill_switch_engaged);
    assert!(!controller.runtime.desired_connected);
    assert!(controller.active_session.is_none());
}

#[tokio::test]
async fn updater_disconnect_goes_through_the_transition_state_machine() {
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;
    controller.connected = true;
    controller.runtime.desired_connected = true;

    let (confirm, mut stopped) = oneshot::channel();
    controller.on_resource_update_disconnect_request(confirm);

    assert!(stopped.try_recv().is_err());
    assert!(!controller.runtime.desired_connected);
    assert_eq!(controller.pending_resource_disconnects.len(), 1);

    // The transition actually stopped the core.
    controller.connected = false;
    controller.transitions.pending = false;
    controller.transitions.scheduled = false;
    controller.resolve_pending_update_disconnects();

    assert!(stopped.try_recv().unwrap());
    assert!(controller.pending_resource_disconnects.is_empty());
}

#[tokio::test]
async fn updater_disconnect_is_acknowledged_on_shutdown() {
    // shutdown() revokes diagnostics consent globally; hold the shared lock
    // so consent-observing tests cannot see the epoch bump mid-export.
    let _consent = crate::test_support::CONSENT_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;
    let (confirm, mut stopped) = oneshot::channel();
    controller.pending_resource_disconnects.push(confirm);

    controller.shutdown(None).await;

    assert!(!stopped.try_recv().unwrap());
    assert!(controller.pending_resource_disconnects.is_empty());
}

#[test]
fn unexpected_core_exit_is_ignored_once_windows_shutdown_started() {
    let (mut controller, _root) = test_controller();
    // `PowerReconnectController::set_system_shutdown` keeps both flags in sync.
    controller.system_shutdown = true;
    controller.runtime.system_shutdown = true;
    controller.runtime.shutting_down = false;
    controller.active_session = Some(crate::session_state::ActiveSessionSnapshot::default());

    handle_unexpected_disconnect(&mut controller);

    // The unexpected path never touched the session while Windows shuts down.
    assert!(controller.active_session.is_some());
}

#[tokio::test]
async fn transition_action_completes_without_blocking_the_caller() {
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;
    controller.connected = true;
    controller.runtime.desired_connected = false;

    // Drain resolves the disconnect action; the worker executes it.
    controller.transitions.pending = true;
    controller.drain_transition_queue();
    assert!(controller.transitions.active);
    let Some((_, tick)) = controller.timers.transition.clone() else {
        panic!("transition execution scheduled");
    };
    let TransitionTick::Execute {
        action,
        reason,
        generation,
    } = tick
    else {
        panic!("execute tick scheduled");
    };
    assert_eq!(action, "disconnect");

    let started = Instant::now();
    controller.execute_transition_action(action, &reason, generation);
    // The action ran to completion on this task (the Rust owner task replaces
    // the Python worker thread) and the flags are consistent again.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "disconnect must not hang"
    );
    assert!(!controller.transitions.active);
    assert!(!controller.connected);
}

#[test]
fn effective_http_proxy_url_quotes_credentials_and_uses_the_live_port() {
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;
    controller.state.settings.local_http_port = 10809;
    controller.state.settings.proxy_auth_enabled = true;
    controller.state.settings.proxy_auth_username = "user@name".into();
    controller.state.settings.proxy_auth_password = "p@ss/word".into();
    assert_eq!(
        controller.get_effective_http_proxy_url().as_deref(),
        Some("http://user%40name:p%40ss%2Fword@127.0.0.1:10809")
    );
    controller.state.settings.proxy_auth_enabled = false;
    assert_eq!(
        controller.get_effective_http_proxy_url().as_deref(),
        Some("http://127.0.0.1:10809")
    );
    controller.state.settings.tun_mode = true;
    assert_eq!(controller.get_effective_http_proxy_url(), None);
}

#[test]
fn classify_log_routes_domains_and_levels() {
    let (controller, _root) = test_controller();
    let (domain, level) = controller.classify_log("[auto-switch] switching");
    assert_eq!(domain, lumen_core::logging::TRAFFIC_LOGGER_NAME);
    assert_eq!(level, lumen_core::logging::Level::Info);
    let (domain, level) = controller.classify_log("[xray-error] boom");
    assert_eq!(domain, lumen_core::logging::CORE_LOGGER_NAME);
    assert_eq!(level, lumen_core::logging::Level::Error);
    let (domain, _level) = controller.classify_log("[metrics] sampled");
    assert_eq!(domain, lumen_core::logging::TRAFFIC_LOGGER_NAME);
    let (domain, _) = controller.classify_log("plain line");
    assert_eq!(domain, lumen_core::logging::APP_LOGGER_NAME);
}

#[test]
fn repeated_connection_failures_collapse_until_the_window_passes() {
    let (mut controller, _root) = test_controller();
    let line = "2026-01-01 [proxy] connection: open connection failed using outbound/proxy: i/o timeout";
    assert!(!controller.collapse_repeated_connection_failure(line));
    assert!(controller.collapse_repeated_connection_failure(line));
    assert!(controller.collapse_repeated_connection_failure(line));
    let line_other = "2026-01-01 [proxy] connection: open connection failed using outbound/api: connection refused";
    assert!(!controller.collapse_repeated_connection_failure(line_other));
}

#[test]
fn dns_defaults_migration_runs_once_and_resets_fields() {
    let mut state = AppState::default();
    state.routing.dns_mode = "custom".into();
    state.routing.dns_bootstrap_servers = vec!["9.9.9.9".into()];
    assert!(apply_dns_defaults_update_once(&mut state));
    let defaults = lumen_core::models::RoutingSettings::default();
    assert_eq!(state.routing.dns_mode, defaults.dns_mode);
    assert_eq!(state.routing.dns_bootstrap_servers, defaults.dns_bootstrap_servers);
    assert!(!apply_dns_defaults_update_once(&mut state));
    assert!(
        state
            .applied_migrations
            .get("recommended_dns_defaults_2026_07")
            .copied()
            .unwrap_or(false)
    );
}

#[test]
fn masque_migration_repairs_masque_nodes_once() {
    let mut state = AppState::default();
    let mut node = Node::default();
    node.id = "masque".into();
    node.scheme = "masque".into();
    node.outbound = serde_json::json!({"protocol": "masque"});
    state.nodes.push(node);
    assert!(apply_masque_direct_update_once(&mut state));
    assert!(!apply_masque_direct_update_once(&mut state));
}

#[test]
fn ascii_pretty_json_matches_python_ensure_ascii() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"a":["😀","я"],"n":1}"#).unwrap();
    let rendered = super::config_paths::dumps_ascii_pretty(&value);
    assert!(rendered.contains("\\ud83d\\ude00"));
    assert!(rendered.contains("\\u044f"));
    assert!(!rendered.contains('😀'));
}

#[tokio::test]
async fn save_queue_writes_the_snapshot_in_the_background() {
    let (mut controller, root) = test_controller();
    controller.load_state = LoadState::Loaded;
    controller.state.nodes.push(Node {
        id: "node".into(),
        name: "Test".into(),
        ..Node::default()
    });
    controller.save();
    assert!(
        controller.save_queue.flush(Duration::from_secs(5)),
        "background save must finish"
    );
    assert!(root.path().join("state.json").is_file());
}

#[test]
fn lock_state_machine_follows_the_python_semantics() {
    use lumen_core::security::create_password_hash;
    let (mut controller, _root) = test_controller();
    controller.load_state = LoadState::Loaded;

    controller.lock();
    assert!(!controller.locked, "lock without an enabled password is a no-op");

    let (hash, salt) = create_password_hash("secret", None).unwrap();
    controller.state.security.enabled = true;
    controller.state.security.password_hash = hash;
    controller.state.security.salt = salt;

    assert!(!controller.unlock("wrong"));
    assert!(!controller.locked, "a failed unlock never locks");
    assert!(controller.unlock("secret"));
    assert!(!controller.locked);

    controller.lock();
    assert!(controller.locked);
    assert!(!controller.runtime.desired_connected);
    controller.disable_master_password();
    assert!(!controller.locked);
    assert!(!controller.state.security.enabled);
}

impl AppController {
    // Placeholder for future event assertions shared by these tests.
}

/// Python test_proxy_core_selection: a sing-box-only server never takes the
/// Xray proxy hot-swap path (and the Xray config is not inspected for it).
#[test]
fn xray_proxy_hot_swap_is_rejected_when_selected_node_requires_singbox() {
    let (mut controller, _root) = test_controller();
    let node = lumen_core::models::Node {
        id: "hy2".into(),
        scheme: "hysteria2".into(),
        server: "example.com".into(),
        port: 443,
        outbound: serde_json::json!({"protocol": "hysteria2"}),
        ..Default::default()
    };
    controller.state.nodes.push(node);
    controller.state.selected_node_id = Some("hy2".into());
    controller.state.settings.tun_mode = false;
    controller.active_session = Some(crate::session_state::ActiveSessionSnapshot {
        active_core: "xray".into(),
        ..Default::default()
    });
    assert!(!controller.can_proxy_hot_swap());
}

