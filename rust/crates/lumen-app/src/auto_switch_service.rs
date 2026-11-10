//! Automatic node switching based on active-profile health or sustained speed loss.

use lumen_core::active_profile_health::{HEALTH_MAX_AGE_SEC, HealthSample};
use lumen_core::metrics_api::monotonic_seconds;
use lumen_core::models::{AppState, Node};

use crate::node_runtime_service::fastest_node;
use crate::session_state::ActiveSessionSnapshot;

const HIGH_TICKS_REQUIRED: usize = 10;
const IDLE_BPS: f64 = 1024.0;
const HEALTH_FAILURES_REQUIRED: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSwitchReason {
    ProfileHealth,
    SpeedDrop,
}

impl AutoSwitchReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileHealth => "auto-switch: profile HTTP health",
            Self::SpeedDrop => "auto-switch: speed drop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSwitchEvent {
    Log(String),
    Status {
        level: &'static str,
        message: &'static str,
    },
    Triggered(String),
    SelectionChanged(String),
    SaveRequested,
    TransitionRequested(AutoSwitchReason),
}

pub struct AutoSwitchConnection<'a> {
    pub state: &'a mut AppState,
    pub active_session: Option<&'a ActiveSessionSnapshot>,
    pub connected: bool,
    pub switching: bool,
    pub reconnecting: bool,
    pub desired_connected: &'a mut bool,
}

pub struct AutoSwitchMetrics<'a> {
    pub down_bps: f64,
    pub up_bps: f64,
    pub health: Option<&'a HealthSample>,
}

#[derive(Default)]
struct HealthTracking {
    profile_id: String,
    failure_count: usize,
    last_sample_at: f64,
    traffic_seen_at: f64,
}

#[derive(Default)]
pub struct AutoSwitchService {
    health: HealthTracking,
    health_session_started_at: f64,
    low_since: Option<f64>,
    last_switch_at: Option<f64>,
    high_ticks: usize,
    active_download: bool,
    cycle_attempts: usize,
    exhausted: bool,
    transitioning: bool,
}

impl AutoSwitchService {
    pub fn reset_health_tracking(&mut self) {
        self.health = HealthTracking::default();
    }

    pub fn mark_session_started(&mut self) {
        self.health_session_started_at = monotonic_seconds();
    }

    pub fn reset_for_connection(&mut self, reset_cooldown: bool, reset_cycle: bool) {
        self.low_since = None;
        self.high_ticks = 0;
        self.active_download = false;
        self.reset_health_tracking();
        self.mark_session_started();
        if reset_cycle {
            self.cycle_attempts = 0;
            self.exhausted = false;
        }
        if reset_cooldown {
            self.last_switch_at = None;
        }
    }

    pub fn is_transitioning(&self) -> bool {
        self.transitioning
    }

    pub fn finish_transition(&mut self) {
        self.transitioning = false;
    }

    /// Emits events in controller order. SaveRequested carries the state after selection.
    pub fn check_auto_switch(
        &mut self,
        connection: &mut AutoSwitchConnection<'_>,
        metrics: AutoSwitchMetrics<'_>,
        emit: impl FnMut(AutoSwitchEvent, &AppState),
    ) {
        self.check_at(connection, metrics, monotonic_seconds(), emit);
    }

    fn check_at(
        &mut self,
        connection: &mut AutoSwitchConnection<'_>,
        metrics: AutoSwitchMetrics<'_>,
        now: f64,
        mut emit: impl FnMut(AutoSwitchEvent, &AppState),
    ) {
        if !connection.state.settings.auto_switch_enabled
            || !connection.connected
            || connection.switching
            || connection.reconnecting
        {
            self.reset_health_tracking();
            return;
        }
        if connection.state.nodes.len() < 2 || self.exhausted {
            return;
        }
        let selected_id = match connection.active_session {
            Some(session) => session.node_id.as_deref(),
            None => connection.state.selected_node_id.as_deref(),
        }
        .unwrap_or_default();
        let cooldown = connection.state.settings.auto_switch_cooldown_sec as f64;
        if self.profile_health_failed(selected_id, &metrics, now)
            && self.cooldown_elapsed(now, cooldown)
            && self.perform_switch(
                connection,
                now,
                AutoSwitchReason::ProfileHealth,
                "[auto-switch] active profile HTTP health failed repeatedly → switching".into(),
                &mut emit,
            )
        {
            return;
        }

        let settings = &connection.state.settings;
        let down_bps = metrics.down_bps;
        if down_bps >= settings.auto_switch_threshold_kbps as f64 * 1024.0 {
            self.high_ticks = self.high_ticks.saturating_add(1);
            if self.high_ticks >= HIGH_TICKS_REQUIRED {
                self.active_download = true;
            }
            self.low_since = None;
            return;
        }
        self.high_ticks = 0;
        if !self.active_download {
            return;
        }
        if down_bps < IDLE_BPS {
            self.low_since = None;
            self.active_download = false;
            return;
        }
        let Some(low_since) = self.low_since else {
            self.low_since = Some(now);
            return;
        };
        let low_duration = now - low_since;
        if low_duration < settings.auto_switch_delay_sec as f64
            || !self.cooldown_elapsed(now, cooldown)
        {
            return;
        }
        let log = format!(
            "[auto-switch] speed {:.0} KB/s < {} KB/s for {low_duration:.0}s → switching",
            down_bps / 1024.0,
            settings.auto_switch_threshold_kbps,
        );
        self.perform_switch(connection, now, AutoSwitchReason::SpeedDrop, log, &mut emit);
    }

    fn profile_health_failed(
        &mut self,
        profile_id: &str,
        metrics: &AutoSwitchMetrics<'_>,
        now: f64,
    ) -> bool {
        if self.health.profile_id != profile_id {
            self.reset_health_tracking();
            self.health.profile_id = profile_id.to_owned();
        }
        let tracking = &mut self.health;
        if [metrics.down_bps, metrics.up_bps]
            .into_iter()
            .any(|bps| bps.is_finite() && bps > 0.0)
        {
            tracking.failure_count = 0;
            tracking.traffic_seen_at = now;
            return false;
        }
        let Some(sample) = metrics.health.filter(|sample| {
            let sampled = sample.checked_at;
            sampled.is_finite()
                && sampled > 0.0
                && (0.0..=HEALTH_MAX_AGE_SEC).contains(&(now - sampled))
                && !profile_id.is_empty()
                && sample.profile_id == profile_id
                && (sample.status.eq_ignore_ascii_case("HEALTHY")
                    || sample.status.eq_ignore_ascii_case("FAILED"))
                && sampled > tracking.traffic_seen_at
                && sampled > self.health_session_started_at
        }) else {
            tracking.failure_count = 0;
            return false;
        };
        if sample.checked_at <= tracking.last_sample_at {
            return false;
        }
        if sample.checked_at - tracking.last_sample_at > HEALTH_MAX_AGE_SEC {
            tracking.failure_count = 0;
        }
        tracking.last_sample_at = sample.checked_at;
        if sample.status.eq_ignore_ascii_case("HEALTHY") {
            tracking.failure_count = 0;
            return false;
        }
        tracking.failure_count = tracking.failure_count.saturating_add(1);
        tracking.failure_count >= HEALTH_FAILURES_REQUIRED
    }

    fn cooldown_elapsed(&self, now: f64, cooldown: f64) -> bool {
        self.last_switch_at
            .is_none_or(|last| now - last >= cooldown)
    }

    fn perform_switch(
        &mut self,
        connection: &mut AutoSwitchConnection<'_>,
        now: f64,
        reason: AutoSwitchReason,
        log: String,
        emit: &mut impl FnMut(AutoSwitchEvent, &AppState),
    ) -> bool {
        let state = &mut *connection.state;
        let max_attempts = state.nodes.len().saturating_sub(1).max(1);
        if self.cycle_attempts >= max_attempts {
            self.exhausted = true;
            self.low_since = None;
            self.reset_health_tracking();
            self.active_download = false;
            emit(
                AutoSwitchEvent::Status {
                    level: "warning",
                    message: "Автопереключение остановлено: все серверы уже проверены",
                },
                state,
            );
            emit(
                AutoSwitchEvent::Log(
                    "[auto-switch] exhausted all nodes for current session".into(),
                ),
                state,
            );
            return false;
        }
        let Some(next_node) = get_next_node_for_auto_switch(state) else {
            return false;
        };
        let next_id = next_node.id.clone();
        let next_name = next_node.name.clone();
        self.low_since = None;
        self.reset_health_tracking();
        self.last_switch_at = Some(now);
        self.active_download = false;
        self.cycle_attempts += 1;
        self.transitioning = true;
        emit(AutoSwitchEvent::Log(log), state);
        emit(AutoSwitchEvent::Triggered(next_name), state);
        state.selected_node_id = Some(next_id.clone());
        emit(AutoSwitchEvent::SelectionChanged(next_id), state);
        emit(AutoSwitchEvent::SaveRequested, state);
        *connection.desired_connected = true;
        emit(AutoSwitchEvent::TransitionRequested(reason), state);
        true
    }
}

pub fn get_next_node_for_auto_switch(state: &AppState) -> Option<&Node> {
    let current_id = state.selected_node_id.as_deref();
    let group = state
        .nodes
        .iter()
        .find(|node| Some(node.id.as_str()) == current_id)
        .map(node_group);
    let scoped: Vec<_> = state
        .nodes
        .iter()
        .filter(|node| group.is_none_or(|group| node_group(node) == group))
        .collect();
    if scoped.len() < 2 {
        return None;
    }
    let alive = scoped
        .iter()
        .copied()
        .filter(|node| Some(node.id.as_str()) != current_id && node.is_alive == Some(true));
    if let Some(node) = fastest_node(alive) {
        return Some(node);
    }
    let next = scoped
        .iter()
        .position(|node| Some(node.id.as_str()) == current_id)
        .map_or(0, |index| (index + 1) % scoped.len());
    let node = scoped[next];
    (Some(node.id.as_str()) != current_id).then_some(node)
}

fn node_group(node: &Node) -> &str {
    if node.group.is_empty() {
        "Default"
    } else {
        &node.group
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, group: &str, speed: Option<f64>, ping: Option<i64>, alive: bool) -> Node {
        Node {
            id: id.into(),
            name: id.into(),
            group: group.into(),
            speed_mbps: speed,
            ping_ms: ping,
            is_alive: Some(alive),
            ..Node::default()
        }
    }

    fn failed(id: &str, checked_at: f64) -> HealthSample {
        HealthSample {
            status: "FAILED".into(),
            checked_at,
            profile_id: id.into(),
            ..Default::default()
        }
    }

    struct Fixture {
        service: AutoSwitchService,
        state: AppState,
        active_session: Option<ActiveSessionSnapshot>,
        desired: bool,
        events: Vec<(AutoSwitchEvent, Option<String>)>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut state = AppState {
                nodes: vec![
                    node("current", "Main", None, Some(9), true),
                    node("backup", "Main", Some(5.0), Some(40), true),
                ],
                selected_node_id: Some("current".into()),
                ..AppState::default()
            };
            state.settings.auto_switch_enabled = true;
            state.settings.auto_switch_threshold_kbps = 10;
            state.settings.auto_switch_delay_sec = 2;
            state.settings.auto_switch_cooldown_sec = 30;
            Self {
                service: AutoSwitchService::default(),
                state,
                active_session: None,
                desired: false,
                events: Vec::new(),
            }
        }

        fn tick(&mut self, now: f64, down: f64, up: f64, health: Option<&HealthSample>) {
            let mut connection = AutoSwitchConnection {
                state: &mut self.state,
                active_session: self.active_session.as_ref(),
                connected: true,
                switching: false,
                reconnecting: false,
                desired_connected: &mut self.desired,
            };
            self.service.check_at(
                &mut connection,
                AutoSwitchMetrics {
                    down_bps: down,
                    up_bps: up,
                    health,
                },
                now,
                |event, state| self.events.push((event, state.selected_node_id.clone())),
            );
        }

        fn failure(&mut self, id: &str, at: f64) {
            self.tick(at, 0.0, 0.0, Some(&failed(id, at)));
        }
    }

    #[test]
    fn auto_switch_stays_in_current_group_and_uses_speed_then_ping_then_rotation() {
        let mut fixture = Fixture::new();
        fixture
            .state
            .nodes
            .insert(1, node("other", "Other", Some(100.0), Some(1), true));
        fixture
            .state
            .nodes
            .push(node("low_latency", "Main", None, Some(5), true));
        assert_eq!(
            get_next_node_for_auto_switch(&fixture.state).unwrap().id,
            "backup"
        );
        fixture.state.nodes[2].speed_mbps = None;
        assert_eq!(
            get_next_node_for_auto_switch(&fixture.state).unwrap().id,
            "low_latency"
        );
        fixture.state.nodes[2].is_alive = Some(false);
        fixture.state.nodes[3].is_alive = Some(false);
        assert_eq!(
            get_next_node_for_auto_switch(&fixture.state).unwrap().id,
            "backup"
        );
        fixture.state.nodes.truncate(2);
        assert!(get_next_node_for_auto_switch(&fixture.state).is_none());
    }

    #[test]
    fn distinct_health_failures_switch_once_and_persist_before_requesting_transition() {
        let mut fixture = Fixture::new();
        fixture.failure("current", 1.0);
        fixture.tick(1.5, 0.0, 0.0, Some(&failed("current", 1.0)));
        fixture.failure("current", 2.0);
        assert!(fixture.events.is_empty());
        fixture.failure("current", 3.0);
        assert!(fixture.service.is_transitioning());
        assert!(fixture.desired);
        assert_eq!(fixture.state.selected_node_id.as_deref(), Some("backup"));
        assert_eq!(
            fixture.events,
            vec![
                (
                    AutoSwitchEvent::Log(
                        "[auto-switch] active profile HTTP health failed repeatedly → switching"
                            .into()
                    ),
                    Some("current".into())
                ),
                (
                    AutoSwitchEvent::Triggered("backup".into()),
                    Some("current".into())
                ),
                (
                    AutoSwitchEvent::SelectionChanged("backup".into()),
                    Some("backup".into())
                ),
                (AutoSwitchEvent::SaveRequested, Some("backup".into())),
                (
                    AutoSwitchEvent::TransitionRequested(AutoSwitchReason::ProfileHealth),
                    Some("backup".into())
                ),
            ]
        );
    }

    #[test]
    fn speed_drop_needs_ten_high_ticks_and_sustained_non_idle_traffic() {
        let mut fixture = Fixture::new();
        for tick in 1..10 {
            fixture.tick(f64::from(tick), 11_000.0, 0.0, None);
        }
        fixture.tick(10.0, 2_048.0, 0.0, None);
        fixture.tick(13.0, 2_048.0, 0.0, None);
        assert!(fixture.events.is_empty());
        for tick in 14..24 {
            fixture.tick(f64::from(tick), 11_000.0, 0.0, None);
        }
        fixture.tick(24.0, 2_048.0, 0.0, None);
        fixture.tick(25.0, 512.0, 0.0, None);
        fixture.tick(28.0, 2_048.0, 0.0, None);
        assert!(fixture.events.is_empty());
        for tick in 30..40 {
            fixture.tick(f64::from(tick), 11_000.0, 0.0, None);
        }
        fixture.tick(40.0, 2_048.0, 0.0, None);
        fixture.tick(42.0, 2_048.0, 0.0, None);
        assert!(matches!(
            fixture.events.last(),
            Some((
                AutoSwitchEvent::TransitionRequested(AutoSwitchReason::SpeedDrop),
                _
            ))
        ));
    }

    #[test]
    fn missing_unknown_invalid_and_wrong_profile_samples_break_the_failure_streak() {
        for sample in [
            None,
            Some(HealthSample {
                status: "UNKNOWN".into(),
                ..failed("current", 108.0)
            }),
            Some(failed("current", 50.0)),
            Some(failed("current", 200.0)),
            Some(failed("current", f64::NAN)),
            Some(failed("other", 108.0)),
        ] {
            let mut fixture = Fixture::new();
            fixture.failure("current", 100.0);
            fixture.failure("current", 104.0);
            assert_eq!(fixture.service.health.failure_count, 2);
            fixture.tick(108.0, 0.0, 0.0, sample.as_ref());
            assert_eq!(fixture.service.health.failure_count, 0);
            fixture.failure("current", 112.0);
            assert!(fixture.events.is_empty());
        }
    }

    #[test]
    fn active_session_and_traffic_take_precedence_over_failed_probe_results() {
        let mut fixture = Fixture::new();
        fixture.active_session = Some(ActiveSessionSnapshot {
            node_id: Some("active".into()),
            tun_mode: true,
            ..Default::default()
        });
        fixture.service.health_session_started_at = 100.0;
        fixture.failure("active", 100.0);
        fixture.failure("current", 101.0);
        assert_eq!(fixture.service.health.failure_count, 0);
        fixture.failure("active", 102.0);
        fixture.failure("active", 103.0);
        assert_eq!(fixture.service.health.failure_count, 2);
        fixture.tick(104.0, 0.0, 1.0, Some(&failed("active", 104.0)));
        fixture.tick(105.0, 0.0, 0.0, Some(&failed("active", 104.0)));
        assert_eq!(fixture.service.health.failure_count, 0);
        fixture.failure("active", 106.0);
        fixture.tick(
            107.0,
            0.0,
            0.0,
            Some(&HealthSample {
                status: "HEALTHY".into(),
                ..failed("active", 107.0)
            }),
        );
        assert_eq!(fixture.service.health.failure_count, 0);
        fixture.failure("active", 108.0);
        fixture.failure("active", 119.0);
        assert_eq!(fixture.service.health.failure_count, 1);
        assert!(fixture.events.is_empty());
    }

    #[test]
    fn automatic_reconnect_preserves_cooldown_and_attempt_limit() {
        let mut fixture = Fixture::new();
        for time in [100.0, 104.0, 108.0] {
            fixture.failure("current", time);
        }
        assert!(fixture.service.is_transitioning());
        fixture.service.reset_for_connection(false, false);
        fixture.service.finish_transition();
        fixture.events.clear();

        for time in [112.0, 116.0, 120.0, 124.0, 128.0, 132.0, 136.0] {
            fixture.failure("backup", time);
        }
        assert!(fixture.events.is_empty());
        fixture.failure("backup", 138.0);
        assert!(fixture.service.exhausted);
        assert!(!fixture.service.is_transitioning());
        assert!(matches!(
            fixture.events[0].0,
            AutoSwitchEvent::Status {
                level: "warning",
                ..
            }
        ));
        fixture.failure("backup", 139.0);
        assert_eq!(fixture.events.len(), 2);
        fixture.service.reset_for_connection(true, true);
        assert!(!fixture.service.exhausted);
        assert_eq!(fixture.service.cycle_attempts, 0);
        assert_eq!(fixture.service.last_switch_at, None);
    }
}
