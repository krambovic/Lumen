//! Limit forwarded core logs while leaving the process output drain intact.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;

static CRITICAL_LEVEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[\s\[])\s*(?:ERROR|FATAL|PANIC|WARN(?:ING)?)(?:\b|\])")
        .expect("valid core log level regex")
});

pub struct CoreLogLimiter {
    source: String,
    limit: usize,
    critical_limit: usize,
    window_started: Option<Instant>,
    visible: usize,
    critical_visible: usize,
    hidden: usize,
}

impl CoreLogLimiter {
    pub fn new(source: &str) -> Self {
        Self::with_limits(source, 40, 10)
    }

    pub fn with_limits(source: &str, limit: usize, critical_limit: usize) -> Self {
        Self {
            source: source.into(),
            limit: limit.max(1),
            critical_limit: critical_limit.max(1),
            window_started: None,
            visible: 0,
            critical_visible: 0,
            hidden: 0,
        }
    }

    pub fn reset(&mut self) {
        self.window_started = None;
        self.visible = 0;
        self.critical_visible = 0;
        self.hidden = 0;
    }

    /// Return the previous window's summary and whether to forward this line.
    pub fn admit(&mut self, line: &str) -> (Option<String>, bool) {
        self.admit_at(line, Instant::now())
    }

    fn admit_at(&mut self, line: &str, now: Instant) -> (Option<String>, bool) {
        let mut summary = None;
        if self
            .window_started
            .is_none_or(|start| now - start >= Duration::from_secs(1))
        {
            if self.hidden != 0 {
                summary = Some(format!(
                    "INFO [{}] {} additional core log lines hidden in the previous second",
                    self.source, self.hidden
                ));
            }
            self.reset();
            self.window_started = Some(now);
        }
        let (visible, limit) = if CRITICAL_LEVEL.is_match(line) {
            (&mut self.critical_visible, self.critical_limit)
        } else {
            (&mut self.visible, self.limit)
        };
        if *visible >= limit {
            self.hidden += 1;
            (summary, false)
        } else {
            *visible += 1;
            (summary, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_hidden_logs_and_refreshes_both_quotas() {
        let mut limiter = CoreLogLimiter::with_limits("xray", 2, 1);
        let now = Instant::now();
        assert_eq!(limiter.admit_at("info opened", now), (None, true));
        assert_eq!(limiter.admit_at("info routed", now), (None, true));
        assert_eq!(limiter.admit_at("info routed", now), (None, false));
        assert_eq!(
            limiter.admit_at("timestamp [wArN] warning", now),
            (None, true)
        );
        assert_eq!(limiter.admit_at("PANIC: failed", now), (None, false));

        let next = now + Duration::from_secs(1);
        assert_eq!(
            limiter.admit_at("ERROR failed", next),
            (
                Some(
                    "INFO [xray] 2 additional core log lines hidden in the previous second".into()
                ),
                true
            )
        );
        assert_eq!(limiter.admit_at("info opened", next), (None, true));
        limiter.reset();
        assert_eq!(limiter.admit_at("ERROR failed", next), (None, true));
    }
}
