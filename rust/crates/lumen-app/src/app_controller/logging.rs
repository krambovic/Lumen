//! Domain-separated log routing, core log scrubbing and noisy-line filters
//! (`AppController._log`, `_classify_log`, `_on_xray_log`, core log queues).

use std::time::Instant;

use lumen_core::log_utils::{classify_log_level, clean_log_text};
use lumen_core::logging::{Level, LogRecord, APP_LOGGER_NAME, CORE_LOGGER_NAME, TRAFFIC_LOGGER_NAME};
use regex::Regex;
use std::sync::LazyLock;

use crate::app_controller::{AppController, AppEvent};

const CORE_TAGS: [&str; 12] = [
    "xray",
    "singbox",
    "sing-box",
    "tun",
    "zapret",
    "proxy",
    "core",
    "xray-error",
    "singbox-error",
    "zapret-error",
    "discord",
    "discord-proxy",
];

const TRAFFIC_TAGS: [&str; 9] = [
    "speed",
    "ping",
    "traffic",
    "metrics",
    "auto-switch",
    "autoswitch",
    "kill-switch",
    "killswitch",
    "health",
];

const TRAFFIC_PREFIXES: [&str; 6] = ["speed", "ping", "auto", "traffic", "kill", "health"];
const CORE_PREFIXES: [&str; 8] = [
    "xray",
    "singbox",
    "sing-box",
    "tun",
    "zapret",
    "proxy",
    "core",
    "discord",
];

static OUTBOUND_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"using outbound/([^\s:]+)").expect("valid regex"));

impl AppController {
    /// Python `_classify_log`: infer `(domain, level)` from a leading `[tag]`.
    pub(crate) fn classify_log(&self, line: &str) -> (&'static str, Level) {
        let text = line.trim_start();
        let mut tag = String::new();
        if text.starts_with('[') {
            if let Some(end) = text.find(']') {
                tag = text[1..end].trim().to_lowercase();
            }
        }
        let domain: &'static str =
            if TRAFFIC_TAGS.contains(&tag.as_str())
                || TRAFFIC_PREFIXES.iter().any(|prefix| tag.starts_with(prefix))
            {
                TRAFFIC_LOGGER_NAME
            } else if CORE_TAGS.contains(&tag.as_str())
                || CORE_PREFIXES.iter().any(|prefix| tag.starts_with(prefix))
            {
                CORE_LOGGER_NAME
            } else {
                APP_LOGGER_NAME
            };
        let level = if tag.contains("error") {
            Level::Error
        } else if tag.contains("warn") {
            Level::Warning
        } else {
            match classify_log_level(line) {
                "error" => Level::Error,
                "warning" => Level::Warning,
                _ => Level::Info,
            }
        };
        (domain, level)
    }

    /// Python `_log`: route a log line to its domain file + the UI.
    pub fn log(&mut self, line: &str) {
        let line = clean_log_text(line);
        if line.is_empty() {
            return;
        }
        self.recent_logs.append(line.clone());
        let (domain, level) = self.classify_log(&line);
        lumen_core::logging::emit(LogRecord::new(domain, level, &line));
        self.emit(AppEvent::LogLine(line));
    }

    /// Direct logger write used before `_log` exists (constructor paths and
    /// thread completions).
    pub(crate) fn log_raw(&self, line: &str) {
        lumen_core::logging::Logger::named(APP_LOGGER_NAME).info(line);
    }

    /// Python `_on_xray_log` (also sing-box and zapret runtime lines after
    /// `_on_core_log` prefixing).
    pub(crate) fn on_xray_log(&mut self, line: &str) {
        if self.is_noisy_local_proxy_log(line) {
            self.routine_log_count += 1;
            if self.routine_log_count % 50 == 0 {
                lumen_core::logging::Logger::named(CORE_LOGGER_NAME).info(&format!(
                    "[core] {} routine local proxy logs suppressed",
                    self.routine_log_count
                ));
            }
            return;
        }
        if self.state.settings.tun_mode && self.is_noisy_tun_log(line) {
            self.tun_log_count += 1;
            if self.tun_log_count % 200 == 0 {
                lumen_core::logging::Logger::named(CORE_LOGGER_NAME).info(&format!(
                    "[tun] {} internal/noisy logs hidden",
                    self.tun_log_count
                ));
            }
            return;
        }
        if self.collapse_repeated_connection_failure(line) {
            return;
        }
        self.log(line);
    }

    /// Python `_collapse_repeated_connection_failure`: keep the first core
    /// failure and periodic counts, not every retry.
    pub(crate) fn collapse_repeated_connection_failure(&mut self, line: &str) -> bool {
        let low = line.to_lowercase();
        if !low.contains("connection: open connection") {
            return false;
        }
        let outbound = OUTBOUND_TAG.captures(&low);
        let reason = [
            "i/o timeout",
            "connection refused",
            "network is unreachable",
            "requested address is not valid in its context",
        ]
        .into_iter()
        .find(|item| low.contains(item))
        .unwrap_or("");
        let Some(outbound) = outbound else {
            return false;
        };
        if reason.is_empty() {
            return false;
        }
        let key = (outbound[1].to_owned(), reason.to_owned());
        let now = monotonic_seconds();
        // Bound state for long sessions with many changing outbound tags.
        if self.repeated_connection_failures.len() > 64 {
            self.repeated_connection_failures.clear();
        }
        let Some(previous) = self.repeated_connection_failures.get(&key).copied() else {
            self.repeated_connection_failures.insert(key, (now, 0));
            return false;
        };
        let (started, hidden) = previous;
        if now - started < 30.0 {
            self.repeated_connection_failures
                .insert(key, (started, hidden + 1));
            return true;
        }
        if hidden > 0 {
            self.log(&format!(
                "[core] {hidden} repeated connection failures hidden in 30 s (outbound/{}, {reason})",
                key.0
            ));
        }
        self.repeated_connection_failures.insert(key, (now, 0));
        false
    }

    /// Python `_queue_core_log`: scrub complete producer lines before
    /// truncation/drop and before any emission.
    pub(crate) fn queue_core_log(&mut self, source: &'static str, line: &str) {
        let safe = self
            .core_log_streams
            .get(source)
            .map(|scrubber| {
                scrubber.scrub(&serde_json::Value::String(line.to_owned()))
            })
            .unwrap_or_else(|| line.to_owned());
        self.pending_core_logs.append((source, safe));
    }

    /// Python `_flush_core_logs` (50 ms timer).
    pub(crate) fn flush_core_logs(&mut self) {
        self.timers.core_log_flush = Some(Instant::now() + std::time::Duration::from_millis(50));
        for (source, line) in self.pending_core_logs.drain(Some(256)) {
            self.on_core_log(source, &line);
        }
    }

    /// Python `_on_core_log`.
    pub(crate) fn on_core_log(&mut self, source: &str, line: &str) {
        let clean = line.trim();
        if clean.is_empty() {
            return;
        }
        let lower = clean.to_lowercase();
        if lower.starts_with("[tun]") || lower.starts_with("[xray]") || lower.starts_with("[singbox]")
        {
            self.on_xray_log(clean);
        } else {
            self.on_xray_log(&format!("[{source}] {clean}"));
        }
    }

    /// Python `_is_noisy_local_proxy_log`.
    pub(crate) fn is_noisy_local_proxy_log(&self, line: &str) -> bool {
        let text = line.to_lowercase();
        if text.contains("proxy/http: failed to write response")
            && (text.contains("wsasend")
                || text.contains("wsarecv")
                || text.contains(
                    "connection was aborted by the software in your host machine",
                ))
        {
            return true;
        }
        text.contains("127.0.0.1")
            && text.contains(
                "an established connection was aborted by the software in your host machine",
            )
    }

    /// Python `_is_noisy_tun_log`.
    pub(crate) fn is_noisy_tun_log(&self, line: &str) -> bool {
        let text = line.to_lowercase();
        // Show real sing-box connection logs (like v2rayN with the sing-box
        // core). Only hide the app's own internal polling (metrics/clash API
        // on loopback) and low-level Windows socket spam.
        if text.contains("__app_") {
            return true;
        }
        text.contains("wsarecv") || text.contains("wsasend")
    }

    /// Python `_log_network_context`.
    pub(crate) fn log_network_context(&mut self) {
        let net = lumen_core::diagnostics::collect_network_context();
        let ipv4 = bool_field(&net, "ipv4_internet");
        let ipv6 = bool_field(&net, "ipv6_internet");
        let dns = net
            .get("system_dns")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        lumen_core::logging::Logger::named(APP_LOGGER_NAME).info(&format!(
            "[network-diagnostic] IPv4 Internet: {}, IPv6 Internet: {}, DNS: {}",
            if ipv4 { "OK" } else { "FAIL" },
            if ipv6 { "OK" } else { "FAIL" },
            if dns.is_empty() { "None" } else { &dns },
        ));
    }

    /// Python `_on_xray_error`.
    pub(crate) fn on_xray_error(&mut self, message: &str) {
        self.log(&format!("[xray-error] {message}"));
        self.log_network_context();
        self.set_connection_status("error", message, "error");
    }

    /// Python `_on_singbox_error`.
    pub(crate) fn on_singbox_error(&mut self, message: &str) {
        self.log(&format!("[singbox-error] {message}"));
        self.log_network_context();
        self.set_connection_status("error", message, "error");
    }

    /// Python constructor step: report a previous native crash (faulthandler).
    pub(crate) fn log_previous_native_crash(&self) {
        let path = lumen_core::constants::log_dir().join("faulthandler.prev");
        if !path.is_file() {
            return;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            let content = content.trim();
            if !content.is_empty() {
                lumen_core::logging::Logger::named(APP_LOGGER_NAME).error(&format!(
                    "[app] Native crash traceback detected from the previous run:\n{content}"
                ));
            }
        }
        let _ = std::fs::remove_file(&path);
    }
}

fn bool_field(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn monotonic_seconds() -> f64 {
    lumen_core::metrics_api::monotonic_seconds()
}
