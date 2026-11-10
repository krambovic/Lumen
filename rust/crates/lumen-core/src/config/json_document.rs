//! Shared inspection and diagnostics for runtime JSON documents.

use serde_json::Value;

pub fn config_has_proxy_outbound(payload: &Value) -> bool {
    payload["outbounds"]
        .as_array()
        .is_some_and(|outbounds| outbounds.iter().any(|outbound| outbound["tag"] == "proxy"))
}

pub fn format_json_error_message(text: &str, error: &serde_json::Error) -> String {
    let line_number = error.line();
    let line = text
        .lines()
        .nth(line_number.saturating_sub(1))
        .unwrap_or_default();
    let indent = line
        .bytes()
        .take(error.column().saturating_sub(1))
        .filter(|byte| byte & 0xc0 != 0x80)
        .count();
    let column = indent + 1;
    let message = error.to_string();
    let detail = message
        .rsplit_once(" at line ")
        .map_or(message.as_str(), |(detail, _)| detail);
    let mut output =
        format!("Ошибка синтаксиса JSON: {detail} (строка {line_number}, столбец {column})");
    if !line.is_empty() {
        output.push_str(&format!("\n{line}\n{}^", " ".repeat(indent)));
    }
    output
}
