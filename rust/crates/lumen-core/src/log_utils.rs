//! Log-line cleanup, classification and humanization for the Logs page.
//!
//! Port of `windows/xray_fluent/log_utils.py`.

use std::sync::LazyLock;

use chrono::{DateTime, Local};
use regex::Regex;
use serde_json::Value;

use crate::secret_scrubber::scrub_text;

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\x1b|\u{009b})\[[0-?]*[ -/]*[@-~]").unwrap());
static CONTROL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]").unwrap());
static SINGBOX_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[+-]\d{4}\s+)?\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\s+").unwrap()
});
static TRACE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[\d+\]\s*(?:\d+(?:\.\d+)?(?:ms|s)\s*)?").unwrap());
static PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:порт|port)\s+(\d{2,5})").unwrap());
static SINGBOX_ROUTINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)router:\s+(?:failed to search process:\s+(?:process not found|access is denied)|process dns packet:\s+unpack request:\s+bad question name:\s+dns:)",
    )
    .unwrap()
});
static SOURCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[([^\]]+)]\s*(.*)$").unwrap());
static EXIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:code|код(?:ом)?)\s+(-?\d+)").unwrap());
static ERROR_WORD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(error|critical|fatal|panic)\b").unwrap());
static WARN_WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(warning|warn)\b").unwrap());
static CORE_MISSING_A: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:xray|sing-box)(?:\.exe)?\s+(?:file\s+)?not found\b").unwrap());
static CORE_MISSING_B: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:xray|sing-box)\.exe\b.{0,80}\bno such file\b").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub details: String,
    pub action_id: String,
    pub action_label: String,
    pub search_text: String,
}

fn any_in(text: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|t| text.contains(t))
}

pub fn clean_log_text(value: &str) -> String {
    let scrubbed = scrub_text(&Value::String(value.to_string()));
    let text = ANSI_RE.replace_all(&scrubbed, "");
    let text = text.replace('\u{fffd}', "").replace('\u{001b}', "");
    let text = CONTROL_RE.replace_all(&text, "");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a sing-box/core line is routine runtime chatter.
pub fn is_routine_core_log(line: &str) -> bool {
    let text = clean_log_text(line).to_lowercase();
    if SINGBOX_ROUTINE_RE.is_match(&text) {
        return true;
    }
    if any_in(
        &text,
        &[
            "inbound connection from",
            "inbound connection to",
            "inbound packet connection from",
            "inbound packet connection to",
            "outbound connection to",
            "outbound packet connection",
            "connection upload closed",
            "connection download closed",
            "an existing connection was forcibly closed by the remote host",
            "wsarecv",
            "wsasend",
            "open interface take too much time to finish",
        ],
    ) {
        return true;
    }
    let has_error_token = any_in(&text, &["error", "failed", "timeout", "deadline", "fatal", "panic"]);
    !has_error_token
        && any_in(
            &text,
            &[
                "found process",
                "process_name=",
                "process_path=",
                "dns: exchanged",
                "exchanged for",
                "dns: lookup",
                "dns: cached",
                "dns: resolve",
                "dns: domain",
                "router: match[",
                "router: found",
                "sniffed ",
                "decided to ",
            ],
        )
}

fn split_source(text: &str) -> (String, String) {
    let Some(caps) = SOURCE_RE.captures(text) else {
        return ("Приложение".into(), text.into());
    };
    let tag = caps[1].trim().to_lowercase();
    let body = caps[2].trim().to_string();
    let source = match tag.as_str() {
        "xray" | "xray-error" => "Xray".to_string(),
        "singbox" | "sing-box" | "singbox-error" => "sing-box".into(),
        "tun" | "tun-host-swap" => "TUN".into(),
        "proxy" | "system-proxy" => "Прокси".into(),
        "zapret" | "zapret-error" => "Zapret".into(),
        "discord-proxy" | "droute" => "Discord Voice".into(),
        _ => tag,
    };
    (source, body)
}

fn exit_code(low: &str) -> Option<i64> {
    EXIT_RE.captures(low).and_then(|c| c[1].parse::<i64>().ok())
}

pub fn classify_log_level(text: &str) -> &'static str {
    let low = text.to_lowercase();
    if EXIT_RE.is_match(&low) {
        if exit_code(&low).unwrap_or(0) != 0 && !low.contains("expected") {
            return "error";
        }
        return "success";
    }
    if any_in(&low, &["[error]", "[critical]", "[fatal]", "[panic]"]) {
        return "error";
    }
    if any_in(&low, &["[warning]", "[warn]"]) {
        return "warning";
    }
    if any_in(
        &low,
        &["connection:", "handshake", "dial tcp", "unexpected http response status", "unexpected response status"],
    ) {
        return "warning";
    }
    if ERROR_WORD_RE.is_match(&low) && !low.contains("common/errors") {
        return "error";
    }
    if WARN_WORD_RE.is_match(&low) {
        return "warning";
    }
    if any_in(&low, &["failed", "не удалось"]) {
        return "error";
    }
    if any_in(&low, &["deprecated", "timeout", "timed out", "предупреж"]) {
        return "warning";
    }
    if low.contains("ошибк") {
        return "error";
    }
    if any_in(&low, &["ready", "started", "connected", "успеш", "готов", "подключено", "updated"]) {
        return "success";
    }
    "info"
}

fn humanize(body: &str, level: &str) -> (String, String, String) {
    let low = body.to_lowercase();
    let pair = |m: &str, d: &str| (m.to_string(), format!("{d}{body}"), String::new());
    if let Some(caps) = PORT_RE.captures(body) {
        if any_in(&low, &["занят", "in use", "already", "bind"]) {
            let port = caps[1].to_string();
            return (
                format!("Локальный порт {port} уже используется другой программой."),
                format!("Закройте конфликтующее приложение или нажмите «Сменить порт». {body}"),
                format!("change-port:{port}"),
            );
        }
    }
    if low.contains("certificate verify failed") || low.contains("unknown authority") {
        return pair(
            "Не удалось проверить сертификат защищённого соединения.",
            "Проверьте дату Windows, антивирусную HTTPS-проверку и обновления корневых сертификатов. ",
        );
    }
    if any_in(&low, &["unknown field", "failed to load config", "decode config"]) {
        return pair(
            "Сетевое ядро не принимает текущий конфиг.",
            "Обновите ядро или сбросьте соответствующий конфиг на шаблон по умолчанию. ",
        );
    }
    if low.contains("rule-set") && any_in(&low, &["eof", "failed", "context canceled"]) {
        return pair(
            "Не удалось загрузить данные маршрутизации.",
            "Обновите GeoIP/GeoSite во вкладке обновлений и повторите подключение. ",
        );
    }
    if low.contains("tls handshake") && (low.contains("eof") || low.contains("failed")) {
        return pair(
            "Сервер оборвал TLS-соединение.",
            "Проверьте параметры Reality/XHTTP, время Windows и попробуйте другой сервер. ",
        );
    }
    if low.contains("tunnel not initialized") || low.contains("endpoint not initialized") {
        return pair(
            "Не удалось инициализировать подключение WARP/MASQUE.",
            "Проверьте доступ к Cloudflare и параметры импортированного профиля. Исходная причина обычно указана соседней строкой выше. ",
        );
    }
    let core_file_missing = low.contains("sing-box path is not configured")
        || low.contains("xray path is not configured")
        || CORE_MISSING_A.is_match(&low)
        || CORE_MISSING_B.is_match(&low);
    if core_file_missing {
        return pair(
            "Файл сетевого ядра не найден.",
            "Проверьте путь к ядру в настройках или переустановите Lumen. ",
        );
    }
    if any_in(&low, &["permission denied", "access is denied", "отказано в доступе"]) {
        return pair(
            "Windows не разрешила выполнить операцию.",
            "Перезапустите Lumen от имени администратора. ",
        );
    }
    if low.contains("no such host") || (low.contains("dns") && any_in(&low, &["failed", "timeout", "deadline"])) {
        return pair(
            "DNS не смог определить адрес сайта.",
            "Проверьте подключение, смените режим DNS или переподключите TUN. ",
        );
    }
    if low.contains("another instance")
        || low.contains("другой экземпляр")
        || (low.contains("windivert") && level == "error")
    {
        return pair(
            "Драйвер перехвата трафика уже занят.",
            "Закройте другой VPN/Zapret-клиент и повторите запуск. ",
        );
    }
    let abnormal_exit = exit_code(&low).is_some_and(|c| c != 0);
    if low.contains("exited") || low.contains("завершился") || abnormal_exit {
        return pair(
            "Сетевое ядро неожиданно остановилось.",
            "Откройте соседние ошибки в журнале; обычно причина указана строкой выше. ",
        );
    }
    (body.into(), body.into(), String::new())
}

pub fn parse_log_line(line: &str, timestamp: Option<DateTime<Local>>) -> LogEntry {
    let cleaned = clean_log_text(line);
    let (source, body) = split_source(&cleaned);
    let body = SINGBOX_PREFIX_RE.replace(&body, "").to_string();
    let body = TRACE_ID_RE.replace(&body, "").trim().to_string();
    let level = classify_log_level(&cleaned);
    let base = if body.is_empty() { cleaned.as_str() } else { body.as_str() };
    let (message, details, action_id) = humanize(base, level);
    let search_text = format!("{source} {message} {details}").to_lowercase();
    LogEntry {
        timestamp: timestamp.unwrap_or_else(Local::now).format("%H:%M:%S").to_string(),
        level: level.into(),
        action_label: if action_id.is_empty() { String::new() } else { "Сменить порт".into() },
        source,
        message,
        details,
        action_id,
        search_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_conflict_is_humanized() {
        let e = parse_log_line("[xray] Порт 10808 занят другим процессом", None);
        assert_eq!(e.source, "Xray");
        assert_eq!(e.action_id, "change-port:10808");
        assert_eq!(e.action_label, "Сменить порт");
        assert!(e.message.starts_with("Локальный порт 10808"));
    }

    #[test]
    fn levels_and_cleanup() {
        assert_eq!(classify_log_level("process exited with code 1"), "error");
        assert_eq!(classify_log_level("process exited with code 0"), "success");
        assert_eq!(classify_log_level("[Warning] something"), "warning");
        assert_eq!(classify_log_level("common/errors: x"), "info");
        assert_eq!(classify_log_level("Подключено"), "success");
        assert_eq!(clean_log_text("\x1b[31m red \x1b[0m  text\x07"), "red text");
        let e = parse_log_line("[singbox] +0300 2026-01-02 03:04:05 [123] 15ms router: hi", None);
        assert_eq!(e.source, "sing-box");
        assert_eq!(e.message, "router: hi");
        assert_eq!(parse_log_line("plain", None).source, "Приложение");
    }

    #[test]
    fn routine_lines() {
        assert!(is_routine_core_log("inbound connection from 127.0.0.1"));
        assert!(is_routine_core_log("dns: exchanged example.com"));
        assert!(!is_routine_core_log("dns: exchanged failed"));
    }
}
