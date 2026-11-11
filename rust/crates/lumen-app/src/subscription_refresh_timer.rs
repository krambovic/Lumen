//! Monotonic timer for periodic subscription refreshes.

use std::time::{Duration, Instant};

#[derive(Default)]
pub struct SubscriptionRefreshTimer {
    interval: Duration,
    deadline: Option<Instant>,
}

impl SubscriptionRefreshTimer {
    pub fn reconfigure(&mut self, minutes: i64, now: Instant) {
        let seconds = u64::try_from(minutes)
            .unwrap_or_default()
            .saturating_mul(60);
        self.interval = Duration::from_secs(seconds);
        self.deadline = if self.interval.is_zero() {
            None
        } else {
            now.checked_add(self.interval)
        };
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = now.checked_add(self.interval);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_restarts_on_configuration_and_coalesces_missed_ticks() {
        let start = Instant::now();
        let mut timer = SubscriptionRefreshTimer::default();
        timer.reconfigure(1, start);
        assert!(!timer.take_due(start + Duration::from_secs(59)));
        let late = start + Duration::from_secs(241);
        assert!(timer.take_due(late));
        assert!(!timer.take_due(late));
        assert_eq!(timer.deadline(), Some(late + Duration::from_secs(60)));
        timer.reconfigure(2, late);
        assert_eq!(timer.deadline(), Some(late + Duration::from_secs(120)));
        timer.reconfigure(0, late);
        assert!(timer.deadline().is_none());
    }
}
