//! Connection transition decisions from a snapshot of controller state.

use crate::session_state::ActiveSessionSnapshot;

pub struct TransitionContext<'a> {
    pub desired_connected: bool,
    pub locked: bool,
    pub has_selected_node: bool,
    pub can_connect_without_selected_node: bool,
    pub connected: bool,
    pub blocked_transition_signature: &'a str,
    pub current_transition_signature: &'a str,
    pub active_session: Option<&'a ActiveSessionSnapshot>,
    pub can_apply_proxy_runtime_change: bool,
    pub can_tun_hot_swap: bool,
    pub can_proxy_hot_swap: bool,
}

pub fn needs_transition(context: &TransitionContext<'_>) -> bool {
    if !context.desired_connected {
        return context.connected;
    }
    if context.locked
        || (!context.has_selected_node && !context.can_connect_without_selected_node)
        || context.current_transition_signature == context.blocked_transition_signature
    {
        return false;
    }
    !context.connected
        || context.active_session.is_none_or(|session| {
            session.transition_signature != context.current_transition_signature
        })
}

pub fn compute_transition_action(context: &TransitionContext<'_>) -> Option<&'static str> {
    if !context.desired_connected {
        return context.connected.then_some("disconnect");
    }
    if context.locked || (!context.has_selected_node && !context.can_connect_without_selected_node)
    {
        return None;
    }
    let Some(session) = context.active_session.filter(|_| context.connected) else {
        return Some("connect");
    };
    if session.transition_signature == context.current_transition_signature {
        return None;
    }
    if context.can_apply_proxy_runtime_change {
        return Some("proxy_update");
    }
    if context.can_tun_hot_swap {
        return Some("tun_hot_swap");
    }
    if context.can_proxy_hot_swap {
        return Some("proxy_hot_swap");
    }
    Some("reconnect")
}

pub fn transition_status_text(action: &str) -> &'static str {
    match action {
        "connect" => "Подключение...",
        "disconnect" => "Отключение...",
        "proxy_update" => "Применение системного прокси...",
        "proxy_hot_swap" | "tun_hot_swap" => "Переключение сервера...",
        "reconnect" => "Переподключение...",
        _ => "Применение изменений...",
    }
}

pub fn can_apply_proxy_runtime_change(
    session: &ActiveSessionSnapshot,
    settings_tun_mode: bool,
    current_xray_layer_signature: &str,
    proxy_enabled: bool,
    proxy_bypass_lan: bool,
) -> bool {
    matches!(session.active_core.as_str(), "xray" | "singbox")
        && !session.hybrid
        && !session.tun_mode
        && !settings_tun_mode
        && session.xray_layer_signature == current_xray_layer_signature
        && (session.proxy_enabled != proxy_enabled || session.proxy_bypass_lan != proxy_bypass_lan)
}

pub fn can_proxy_hot_swap(
    session: &ActiveSessionSnapshot,
    settings_tun_mode: bool,
    socks_port: i64,
    http_port: i64,
    current_xray_layer_signature: &str,
) -> bool {
    session.active_core == "xray"
        && !session.tun_mode
        && !settings_tun_mode
        && session.socks_port == socks_port
        && session.http_port == http_port
        && session.xray_layer_signature != current_xray_layer_signature
}

pub fn can_tun_hot_swap(
    session: &ActiveSessionSnapshot,
    settings_tun_mode: bool,
    has_selected_node: bool,
    current_tun_layer_signature: &str,
) -> bool {
    // Native sing-box uses the same selector in TUN and system-proxy mode.
    settings_tun_mode == session.tun_mode
        && has_selected_node
        && session.active_core == "singbox"
        && !session.hybrid
        && !session.clash_api_selector.is_empty()
        && session.tun_layer_signature == current_tun_layer_signature
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(session: Option<&ActiveSessionSnapshot>) -> TransitionContext<'_> {
        TransitionContext {
            desired_connected: true,
            locked: false,
            has_selected_node: true,
            can_connect_without_selected_node: false,
            connected: session.is_some(),
            blocked_transition_signature: "blocked",
            current_transition_signature: "requested",
            active_session: session,
            can_apply_proxy_runtime_change: true,
            can_tun_hot_swap: true,
            can_proxy_hot_swap: true,
        }
    }

    #[test]
    fn transition_checks_intent_node_lock_and_blocked_signature() {
        let mut context = context(None);
        assert!(needs_transition(&context));
        assert_eq!(compute_transition_action(&context), Some("connect"));
        context.current_transition_signature = "blocked";
        assert!(!needs_transition(&context));
        assert_eq!(compute_transition_action(&context), Some("connect"));
        context.current_transition_signature = "requested";
        context.has_selected_node = false;
        assert!(!needs_transition(&context));
        assert_eq!(compute_transition_action(&context), None);
        context.can_connect_without_selected_node = true;
        assert!(needs_transition(&context));
        context.locked = true;
        assert!(!needs_transition(&context));
        context.desired_connected = false;
        assert_eq!(compute_transition_action(&context), None);
        context.connected = true;
        assert!(needs_transition(&context));
        assert_eq!(compute_transition_action(&context), Some("disconnect"));
    }

    #[test]
    fn changed_session_prefers_runtime_update_then_selector_then_xray_swap() {
        let session = ActiveSessionSnapshot {
            transition_signature: "active".into(),
            ..Default::default()
        };
        let mut context = context(Some(&session));
        assert!(needs_transition(&context));
        assert_eq!(compute_transition_action(&context), Some("proxy_update"));
        context.can_apply_proxy_runtime_change = false;
        assert_eq!(compute_transition_action(&context), Some("tun_hot_swap"));
        context.can_tun_hot_swap = false;
        assert_eq!(compute_transition_action(&context), Some("proxy_hot_swap"));
        context.can_proxy_hot_swap = false;
        assert_eq!(compute_transition_action(&context), Some("reconnect"));
        context.current_transition_signature = "active";
        assert!(!needs_transition(&context));
        assert_eq!(compute_transition_action(&context), None);
        assert_eq!(transition_status_text("reconnect"), "Переподключение...");
        assert_eq!(transition_status_text("unknown"), "Применение изменений...");
    }

    #[test]
    fn proxy_updates_and_xray_swaps_require_matching_runtime_conditions() {
        let mut session = ActiveSessionSnapshot {
            active_core: "xray".into(),
            socks_port: 1080,
            http_port: 1081,
            xray_layer_signature: "same".into(),
            ..Default::default()
        };
        assert!(can_apply_proxy_runtime_change(
            &session, false, "same", true, false
        ));
        assert!(!can_apply_proxy_runtime_change(
            &session, false, "same", false, false
        ));
        assert!(!can_apply_proxy_runtime_change(
            &session, false, "changed", true, false
        ));
        assert!(can_proxy_hot_swap(&session, false, 1080, 1081, "changed"));
        assert!(!can_proxy_hot_swap(&session, false, 1080, 1082, "changed"));
        assert!(!can_proxy_hot_swap(&session, false, 1080, 1081, "same"));
        session.hybrid = true;
        assert!(!can_apply_proxy_runtime_change(
            &session, false, "same", true, false
        ));
        session.active_core = "singbox".into();
        assert!(!can_proxy_hot_swap(&session, false, 1080, 1081, "changed"));
    }

    #[test]
    fn singbox_selector_requires_a_node_and_preserves_tun_mode() {
        let mut session = ActiveSessionSnapshot {
            active_core: "singbox".into(),
            clash_api_selector: "proxy".into(),
            tun_layer_signature: "same".into(),
            ..Default::default()
        };
        assert!(can_tun_hot_swap(&session, false, true, "same"));
        assert!(!can_tun_hot_swap(&session, true, true, "same"));
        assert!(!can_tun_hot_swap(&session, false, true, "changed"));
        session.tun_mode = true;
        assert!(can_tun_hot_swap(&session, true, true, "same"));
        assert!(!can_tun_hot_swap(&session, true, false, "same"));
        session.clash_api_selector.clear();
        assert!(!can_tun_hot_swap(&session, true, true, "same"));
    }
}
