//! Reconnect the active Lumen session after Windows sleep/hibernate
//! (port of `windows/xray_fluent/power_reconnect.py`).
//!
//! The Qt `QAbstractNativeEventFilter` becomes a plain state machine: the
//! caller feeds it native messages ([`WindowsPowerEventFilter::native_event_filter`])
//! and fires the single-shot timer from its own loop
//! ([`WindowsPowerEventFilter::fire_timer_if_due`]) or directly in tests.

use std::time::{Duration, Instant};

use windows::Win32::UI::WindowsAndMessaging::MSG;

pub const WM_POWERBROADCAST: u32 = 0x0218;
pub const WM_QUERYENDSESSION: u32 = 0x0011;
pub const WM_ENDSESSION: u32 = 0x0016;
pub const PBT_APMSUSPEND: u32 = 0x0004;
pub const PBT_APMRESUMECRITICAL: u32 = 0x0006;
pub const PBT_APMRESUMESUSPEND: u32 = 0x0007;
pub const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;

const RESUME_INITIAL_DELAY_MS: u64 = 4000;
const RESUME_RETRY_DELAYS_MS: [u64; 2] = [8000, 15000];
const RESUME_BUSY_RETRY_MS: u64 = 2000;
const MAX_RESUME_ATTEMPTS: usize = 1 + RESUME_RETRY_DELAYS_MS.len();

/// Controller surface the filter drives. Mirrors the duck-typed fields the
/// Python filter reads from and writes onto the AppController.
pub trait PowerReconnectController {
    // Read surface used by `should_arm_resume_reconnect`.
    fn reconnect_after_sleep(&self) -> bool {
        true
    }
    fn locked(&self) -> bool {
        false
    }
    fn shutting_down(&self) -> bool {
        false
    }
    fn system_shutdown(&self) -> bool {
        false
    }
    /// `controller._desired_connected`; `None` means the attribute is absent.
    fn desired_connected(&self) -> Option<bool> {
        None
    }
    fn connected(&self) -> bool {
        false
    }
    /// `controller._active_session is not None`.
    fn has_active_session(&self) -> bool {
        false
    }
    fn transition_active(&self) -> bool {
        false
    }
    fn transition_pending(&self) -> bool {
        false
    }

    // Control surface.
    fn set_system_shutdown(&mut self, _value: bool) {}
    /// Returns the error text on failure (the Python filter logs it).
    fn stop_active_connection_processes(&mut self, _disable_proxy: bool, _fast: bool) -> Result<(), String> {
        Ok(())
    }
    fn request_resume_reconnect(&mut self, _reason: &str) -> bool {
        false
    }
    fn log(&mut self, _message: &str) {}
}

/// Classify a WM_POWERBROADCAST wParam for platform-independent tests.
pub fn power_broadcast_event(wparam: u32) -> Option<&'static str> {
    if wparam == PBT_APMSUSPEND {
        return Some("suspend");
    }
    if wparam == PBT_APMRESUMECRITICAL
        || wparam == PBT_APMRESUMESUSPEND
        || wparam == PBT_APMRESUMEAUTOMATIC
    {
        return Some("resume");
    }
    None
}

/// `ctypes.cast(address, POINTER(_Message))`: read (message, wParam) from a
/// native MSG pointer; a null address yields None like the Python helper.
pub fn native_message(message: *const MSG) -> Option<(u32, usize)> {
    if message.is_null() {
        return None;
    }
    // SAFETY: Qt hands the filter a pointer to the live MSG for this event,
    // exactly like the Python `ctypes.cast` contract.
    let native = unsafe { &*message };
    Some((native.message, native.wParam.0))
}

/// Return whether the current session should be restored after resume.
pub fn should_arm_resume_reconnect<C: PowerReconnectController + ?Sized>(
    controller: &C,
) -> bool {
    if !controller.reconnect_after_sleep() {
        return false;
    }
    if controller.locked() {
        return false;
    }
    if controller.shutting_down() || controller.system_shutdown() {
        return false;
    }

    let desired_connected = match controller.desired_connected() {
        Some(desired) => desired,
        None => controller.connected(),
    };
    if !desired_connected {
        return false;
    }

    controller.connected() || controller.has_active_session()
}

/// Queue a bounded reconnect after a Windows suspend/resume cycle.
pub struct WindowsPowerEventFilter<'a, C: PowerReconnectController + ?Sized> {
    controller: &'a mut C,
    armed: bool,
    attempts: usize,
    busy_retries: usize,
    timer_active: bool,
    deadline: Option<Instant>,
}

impl<'a, C: PowerReconnectController + ?Sized> WindowsPowerEventFilter<'a, C> {
    pub fn new(controller: &'a mut C) -> Self {
        Self {
            controller,
            armed: false,
            attempts: 0,
            busy_retries: 0,
            timer_active: false,
            deadline: None,
        }
    }

    pub fn close(&mut self) {
        self.stop_timer();
        self.armed = false;
        self.attempts = 0;
        self.busy_retries = 0;
    }

    /// Qt `nativeEventFilter`; always reports the message as unhandled, like
    /// the Python filter's `(False, 0)` return.
    pub fn native_event_filter(&mut self, event_type: &[u8], message: *const MSG) -> bool {
        if !matches!(event_type, b"windows_generic_MSG" | b"windows_dispatcher_MSG") {
            return false;
        }

        let Some((message_id, wparam)) = native_message(message) else {
            return false;
        };
        if message_id == WM_QUERYENDSESSION {
            self.mark_system_shutdown(false);
            return false;
        }
        if message_id == WM_ENDSESSION {
            if wparam != 0 {
                self.mark_system_shutdown(true);
            } else {
                self.cancel_system_shutdown();
            }
            return false;
        }
        // Keep accepting message_id=0 for compatibility with lightweight
        // native-message test doubles used by older integrations.
        if (message_id != 0 && message_id != WM_POWERBROADCAST) || wparam == 0 {
            return false;
        }
        match power_broadcast_event(wparam as u32) {
            Some("suspend") => {
                self.stop_timer();
                self.attempts = 0;
                self.armed = should_arm_resume_reconnect(self.controller);
                if self.armed {
                    self.log("[power] suspend detected; resume reconnect armed");
                }
            }
            Some("resume") => {
                if !self.armed {
                    // A resume without a suspend observed by this process is not
                    // enough evidence that the active session needs reconnecting.
                    // Windows can emit resume notifications for unrelated power
                    // or device changes.
                    return false;
                }
                if self.timer_active || self.attempts > 0 {
                    self.log("[power] duplicate resume ignored; recovery already pending");
                    return false;
                }
                self.busy_retries = 0;
                self.log("[power] resume detected; waiting for network");
                self.schedule(RESUME_INITIAL_DELAY_MS);
            }
            _ => {}
        }
        false
    }

    /// Single-shot timer arming (`QTimer.start`).
    pub fn schedule(&mut self, delay_ms: u64) {
        if self.armed {
            self.timer_active = true;
            self.deadline = Some(Instant::now() + Duration::from_millis(delay_ms.max(0)));
        }
    }

    pub fn stop_timer(&mut self) {
        self.timer_active = false;
        self.deadline = None;
    }

    pub fn timer_is_active(&self) -> bool {
        self.timer_active
    }

    /// Drive the timer from an external loop: fires `_on_timer` once the
    /// scheduled delay elapsed and reports whether it fired.
    pub fn fire_timer_if_due(&mut self) -> bool {
        if !self.timer_active {
            return false;
        }
        let Some(deadline) = self.deadline else {
            self.timer_active = false;
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }
        // Single-shot: the timer deactivates itself when it fires.
        self.timer_active = false;
        self.deadline = None;
        self.on_timer();
        true
    }

    /// `_on_timer`, callable directly (the tests do exactly that).
    pub fn on_timer(&mut self) {
        if !self.armed || !should_arm_resume_reconnect(self.controller) {
            self.close();
            return;
        }

        if self.transition_busy() {
            self.busy_retries += 1;
            if self.busy_retries >= MAX_RESUME_ATTEMPTS {
                self.log("[power] resume reconnect skipped; transition stayed busy");
                self.close();
                return;
            }
            self.schedule(RESUME_BUSY_RETRY_MS);
            return;
        }

        if self.attempts > 0 && self.controller.connected() {
            self.log("[power] resume reconnect already restored the session");
            self.close();
            return;
        }

        let request = self.controller.request_resume_reconnect("system resume");
        if !request {
            self.log("[power] resume reconnect skipped");
            self.close();
            return;
        }

        self.attempts += 1;
        self.busy_retries = 0;
        self.log(&format!(
            "[power] resume reconnect attempt {}/{}",
            self.attempts, MAX_RESUME_ATTEMPTS
        ));
        if self.attempts >= MAX_RESUME_ATTEMPTS {
            self.log("[power] resume reconnect attempts exhausted");
            self.close();
            return;
        }
        let delay = RESUME_RETRY_DELAYS_MS[self.attempts - 1];
        self.schedule(delay);
    }

    fn transition_busy(&self) -> bool {
        self.controller.transition_active() || self.controller.transition_pending()
    }

    fn mark_system_shutdown(&mut self, stop_cores: bool) {
        self.stop_timer();
        self.armed = false;
        self.attempts = 0;
        self.busy_retries = 0;
        self.controller.set_system_shutdown(true);
        self.log("[power] Windows shutdown detected; reconnect disabled");
        if stop_cores {
            match self.controller.stop_active_connection_processes(true, true) {
                Ok(()) => {
                    self.log("[power] owned core processes stopped for Windows shutdown");
                }
                Err(exc) => {
                    self.log(&format!("[power] failed to stop owned core processes: {exc}"));
                }
            }
        }
    }

    fn cancel_system_shutdown(&mut self) {
        if !self.controller.shutting_down() {
            self.controller.set_system_shutdown(false);
        }
        self.log("[power] Windows shutdown cancelled");
    }

    fn log(&mut self, message: &str) {
        self.controller.log(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestController {
        connected: bool,
        desired_connected: bool,
        reconnect_after_sleep: bool,
        locked: bool,
        shutting_down: bool,
        system_shutdown: bool,
        transition_active: bool,
        has_active_session: bool,
        requests: Vec<String>,
        stopped: Vec<(bool, bool)>,
        /// Simulates a session torn down between reconnect attempts.
        simulate_restore: bool,
    }

    impl TestController {
        fn new(connected: bool) -> Self {
            Self {
                connected,
                desired_connected: true,
                reconnect_after_sleep: true,
                locked: false,
                shutting_down: false,
                system_shutdown: false,
                transition_active: false,
                has_active_session: false,
                requests: Vec::new(),
                stopped: Vec::new(),
                simulate_restore: false,
            }
        }
    }

    impl PowerReconnectController for TestController {
        fn reconnect_after_sleep(&self) -> bool {
            self.reconnect_after_sleep
        }
        fn locked(&self) -> bool {
            self.locked
        }
        fn shutting_down(&self) -> bool {
            self.shutting_down
        }
        fn system_shutdown(&self) -> bool {
            self.system_shutdown
        }
        fn desired_connected(&self) -> Option<bool> {
            Some(self.desired_connected)
        }
        fn connected(&self) -> bool {
            self.connected && !(self.simulate_restore && !self.requests.is_empty())
        }
        fn has_active_session(&self) -> bool {
            self.has_active_session
        }
        fn transition_active(&self) -> bool {
            self.transition_active
        }
        fn set_system_shutdown(&mut self, value: bool) {
            self.system_shutdown = value;
        }
        fn stop_active_connection_processes(
            &mut self,
            disable_proxy: bool,
            fast: bool,
        ) -> Result<(), String> {
            self.stopped.push((disable_proxy, fast));
            Ok(())
        }
        fn request_resume_reconnect(&mut self, reason: &str) -> bool {
            self.requests.push(reason.to_string());
            true
        }
        fn log(&mut self, _message: &str) {}
    }

    #[test]
    fn power_broadcast_events_distinguish_suspend_and_resume() {
        assert_eq!(power_broadcast_event(PBT_APMSUSPEND), Some("suspend"));
        assert_eq!(power_broadcast_event(PBT_APMRESUMESUSPEND), Some("resume"));
        assert_eq!(power_broadcast_event(PBT_APMRESUMEAUTOMATIC), Some("resume"));
        assert_eq!(power_broadcast_event(0xFFFF), None);
    }

    #[test]
    fn resume_reconnect_arms_only_for_an_intentionally_connected_session() {
        let make = |connected: bool, desired: bool, sleep: bool| {
            let mut controller = TestController::new(connected);
            controller.desired_connected = desired;
            controller.reconnect_after_sleep = sleep;
            controller
        };
        assert!(should_arm_resume_reconnect(&make(true, true, true)));
        assert!(!should_arm_resume_reconnect(&make(false, false, true)));
        assert!(!should_arm_resume_reconnect(&make(true, false, true)));
        assert!(should_arm_resume_reconnect(&make(true, true, true)));
        assert!(!should_arm_resume_reconnect(&make(true, true, false)));
    }

    #[test]
    fn resume_reconnect_does_not_arm_when_locked_or_shutting_down() {
        let mut controller = TestController::new(true);
        controller.locked = true;
        assert!(!should_arm_resume_reconnect(&controller));
        controller.locked = false;
        controller.shutting_down = true;
        assert!(!should_arm_resume_reconnect(&controller));
    }

    #[test]
    fn filter_uses_one_request_and_stops_after_restore() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        filter.armed = true;
        filter.on_timer();
        assert_eq!(filter.controller.requests, vec!["system resume"]);
        assert_eq!(filter.attempts, 1);

        filter.stop_timer();
        filter.on_timer();
        assert_eq!(filter.controller.requests, vec!["system resume"]);
        assert!(!filter.armed);
        filter.close();
    }

    #[test]
    fn filter_ignores_resume_without_a_matching_suspend() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        let mut raw = MSG::default();
        raw.message = WM_POWERBROADCAST;
        raw.wParam = windows::Win32::Foundation::WPARAM(PBT_APMRESUMESUSPEND as usize);
        let handled = filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(!handled);
        assert!(!filter.armed);
        assert!(!filter.timer_is_active());
        assert!(filter.controller.requests.is_empty());
        filter.close();
    }

    #[test]
    fn native_filter_ignores_non_power_messages_with_power_like_wparam() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        let mut raw = MSG::default();
        raw.message = 0x0201; // WM_LBUTTONDOWN
        raw.wParam = windows::Win32::Foundation::WPARAM(PBT_APMRESUMEAUTOMATIC as usize);
        let handled = filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(!handled);
        assert!(!filter.armed);
        assert!(!filter.timer_is_active());
        filter.close();
    }

    #[test]
    fn filter_does_not_reset_an_inflight_recovery() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        filter.armed = true;
        filter.attempts = 1;
        filter.schedule(30_000);
        let mut raw = MSG::default();
        raw.message = WM_POWERBROADCAST;
        raw.wParam = windows::Win32::Foundation::WPARAM(PBT_APMRESUMEAUTOMATIC as usize);
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert_eq!(filter.attempts, 1);
        assert!(filter.timer_is_active());
        filter.close();
    }

    #[test]
    fn filter_waits_for_an_existing_transition_before_requesting() {
        let mut controller = TestController::new(true);
        controller.transition_active = true;
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        filter.armed = true;
        filter.on_timer();
        assert!(filter.controller.requests.is_empty());
        assert_eq!(filter.attempts, 0);
        assert!(filter.timer_is_active());
        filter.close();
    }

    #[test]
    fn native_shutdown_message_marks_controller_before_core_exit() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        let mut raw = MSG::default();
        raw.message = WM_QUERYENDSESSION;
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(filter.controller.system_shutdown);
        assert!(filter.controller.stopped.is_empty());
        assert!(!filter.armed);
        assert!(!filter.timer_is_active());

        filter.controller.system_shutdown = false;
        raw.message = WM_ENDSESSION;
        raw.wParam = windows::Win32::Foundation::WPARAM(1);
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(filter.controller.system_shutdown);
        assert_eq!(filter.controller.stopped, vec![(true, true)]);
        filter.close();
    }

    #[test]
    fn native_shutdown_cancel_restores_normal_controller_state() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        let mut raw = MSG::default();
        raw.message = WM_QUERYENDSESSION;
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(filter.controller.system_shutdown);

        raw.message = WM_ENDSESSION;
        raw.wParam = windows::Win32::Foundation::WPARAM(0);
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(!filter.controller.system_shutdown);
        assert!(filter.controller.desired_connected);
        filter.close();
    }

    #[test]
    fn native_power_callback_arms_and_schedules_resume() {
        let mut controller = TestController::new(true);
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        let mut raw = MSG::default();
        raw.message = WM_POWERBROADCAST;
        raw.wParam = windows::Win32::Foundation::WPARAM(PBT_APMSUSPEND as usize);
        let handled = filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(!handled);
        assert!(filter.armed);

        raw.wParam = windows::Win32::Foundation::WPARAM(PBT_APMRESUMESUSPEND as usize);
        filter.native_event_filter(b"windows_generic_MSG", &raw);
        assert!(filter.timer_is_active());
        filter.close();
    }

    #[test]
    fn attempts_exhaust_after_the_three_scheduled_tries() {
        let mut controller = TestController::new(true);
        controller.simulate_restore = true;
        controller.has_active_session = true;
        let mut filter = WindowsPowerEventFilter::new(&mut controller);
        filter.armed = true;
        for _ in 0..MAX_RESUME_ATTEMPTS {
            filter.on_timer();
        }
        assert_eq!(filter.controller.requests.len(), MAX_RESUME_ATTEMPTS);
        assert!(!filter.armed);
        filter.close();
    }
}
