//! Parsing and local IPC helpers for the ``lumen:`` URL protocol.

use std::collections::HashSet;

use serde_json::{json, Value};

pub const MAX_DEEP_LINK_LENGTH: usize = 32 * 1024;

const TARGET_QUERY_KEYS: [&str; 5] = ["url", "subscription", "link", "config", "data"];
const NAME_QUERY_KEYS: [&str; 2] = ["name", "title"];
const ACTIONS: [&str; 5] = ["add", "import", "subscribe", "subscription", "install-config"];
const TARGET_SCHEMES: [&str; 4] = ["http", "https", "happ", "incy"];

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct DeepLinkError(pub String);

#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionDeepLink {
    pub url: String,
    pub name: String,
}

fn first_query_value(query: &[(String, Vec<String>)], keys: &[&str]) -> String {
    for key in keys {
        if let Some((_, values)) = query.iter().find(|(name, _)| name == key) {
            if let Some(value) = values.first() {
                let value = value.trim();
                if !value.is_empty() {
                    return value.to_string();
                }
            }
        }
    }
    String::new()
}

fn validate_subscription_url(value: &str) -> Result<String, DeepLinkError> {
    let target = value.trim().to_string();
    if target.is_empty() || target.chars().count() > MAX_DEEP_LINK_LENGTH {
        return Err(DeepLinkError("subscription URL is empty or too long".to_string()));
    }
    if target.chars().any(|c| (c as u32) < 32) {
        return Err(DeepLinkError("subscription URL contains control characters".to_string()));
    }
    let parsed = urlsplit(&target);
    if !TARGET_SCHEMES.contains(&parsed.scheme.as_str()) {
        return Err(DeepLinkError(
            "only http, https, happ and incy subscription links are allowed".to_string(),
        ));
    }
    if parsed.netloc.is_empty() {
        return Err(DeepLinkError("subscription URL has no host".to_string()));
    }
    Ok(target)
}

/// Parse a browser URL into a subscription import request.
///
/// Canonical form: `lumen://add?url=https%3A%2F%2Fexample.com%2Fsubscription&name=Example`.
/// `import`, `subscribe`, `subscription/add` and `install-config` are accepted
/// aliases. A percent-encoded URL may also be placed after the action in the
/// path. A bare `lumen://` remains an activation-only link for old Windows
/// toast notifications.
pub fn parse_lumen_deep_link(value: &str) -> Result<Option<SubscriptionDeepLink>, DeepLinkError> {
    let raw = value.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.chars().count() > MAX_DEEP_LINK_LENGTH {
        return Err(DeepLinkError("deep link is too long".to_string()));
    }
    let parsed = urlsplit(raw);
    if parsed.scheme != "lumen" {
        return Err(DeepLinkError("unsupported deep-link scheme".to_string()));
    }

    let host = parsed.netloc.trim().to_lowercase();
    let decoded_path = unquote(parsed.path.trim_start_matches('/')).trim().to_string();
    let mut action = host.clone();
    let mut path_payload = decoded_path.clone();
    if action == "subscription" && !decoded_path.is_empty() {
        let (nested_action, separator, remainder) = partition(&decoded_path, '/');
        if ACTIONS.contains(&nested_action.to_lowercase().as_str()) {
            action = format!("subscription/{}", nested_action.to_lowercase());
            path_payload = if separator { remainder.to_string() } else { String::new() };
        }
    } else if action.is_empty() && !decoded_path.is_empty() {
        let (path_action, separator, remainder) = partition(&decoded_path, '/');
        action = path_action.to_lowercase();
        path_payload = if separator { remainder.to_string() } else { String::new() };
    }

    let query = parse_qs(&parsed.query, true);
    let target = first_query_value(&query, &TARGET_QUERY_KEYS);
    let name: String = first_query_value(&query, &NAME_QUERY_KEYS)
        .chars()
        .filter(|c| (*c as u32) >= 32)
        .take(256)
        .collect();

    if action.is_empty() && target.is_empty() && path_payload.is_empty() {
        return Ok(None);
    }
    let base_action = action.split_once('/').map(|(_, rest)| rest).unwrap_or(&action);
    if !ACTIONS.contains(&base_action) {
        return Err(DeepLinkError("unsupported Lumen deep-link action".to_string()));
    }
    let mut target = target;
    if target.is_empty() && !path_payload.is_empty() {
        target = path_payload.trim().to_string();
    }
    Ok(Some(SubscriptionDeepLink {
        url: validate_subscription_url(&target)?,
        name,
    }))
}

pub fn find_lumen_deep_link(arguments: &[String]) -> String {
    for argument in arguments {
        let value = argument.trim();
        if value.to_lowercase().starts_with("lumen:") {
            return value.to_string();
        }
    }
    String::new()
}

pub fn encode_instance_message(arguments: &[String]) -> Vec<u8> {
    let link = find_lumen_deep_link(arguments);
    let payload = if !link.is_empty() {
        json!({"command": "deeplink", "url": link})
    } else {
        json!({"command": if should_start_in_tray(arguments, &link) { "tray" } else { "activate" }})
    };
    let mut bytes = serde_json::to_vec(&payload).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Only an OS autostart without a deep link should remain in the tray.
pub fn should_start_in_tray(arguments: &[String], initial_deep_link: &str) -> bool {
    let args = if arguments.len() > 1 { &arguments[1..] } else { arguments };
    let flags: HashSet<&str> = args.iter().map(|arg| arg.trim()).collect();
    initial_deep_link.is_empty() && flags.contains("--tray")
        && !(flags.contains("--relaunched") || flags.contains("--relaunch-as-admin"))
}

/// Parse local IPC payload into `(command, url)`.
///
/// Returns `("activate", "")` for standard activation or legacy payload,
/// `("tray", "")` for secondary autostart notifications that must stay in
/// tray, `("deeplink", url)` for deep links.
pub fn parse_instance_message(payload: &[u8]) -> (String, String) {
    let bound = payload.len().min(MAX_DEEP_LINK_LENGTH + 1024);
    let raw = trim_ascii_whitespace(&payload[..bound]);
    if raw.is_empty() || raw == b"activate" {
        return ("activate".to_string(), String::new());
    }
    let text = match std::str::from_utf8(raw) {
        Ok(text) => text,
        Err(_) => return ("activate".to_string(), String::new()),
    };
    let message: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return ("activate".to_string(), String::new()),
    };
    let Some(message) = message.as_object() else {
        return ("activate".to_string(), String::new());
    };
    let command = message
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if command == "deeplink" {
        let link = message.get("url").and_then(Value::as_str).unwrap_or("").trim();
        if link.to_lowercase().starts_with("lumen:") && link.chars().count() <= MAX_DEEP_LINK_LENGTH {
            return ("deeplink".to_string(), link.to_string());
        }
        return ("activate".to_string(), String::new());
    }
    if command == "tray" {
        return ("tray".to_string(), String::new());
    }
    ("activate".to_string(), String::new())
}

/// Return the deep link from IPC, or an empty string for activation/tray-only.
pub fn decode_instance_message(payload: &[u8]) -> String {
    let (command, link) = parse_instance_message(payload);
    if command == "deeplink" {
        link
    } else {
        String::new()
    }
}

// ── urllib.parse equivalents (lenient, Python-compatible) ──

struct SplitUrl {
    scheme: String,
    netloc: String,
    path: String,
    query: String,
}

fn is_valid_scheme(head: &str) -> bool {
    let mut chars = head.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    head.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Minimal `urlsplit`: scheme, netloc (after `//`) and path/query. Fragments
/// are irrelevant for Lumen links and dropped.
fn urlsplit(raw: &str) -> SplitUrl {
    let mut scheme = String::new();
    let mut rest = raw;
    if let Some(idx) = raw.find(':') {
        let head = &raw[..idx];
        if is_valid_scheme(head) {
            scheme = head.to_ascii_lowercase();
            rest = &raw[idx + 1..];
        }
    }
    let mut netloc = String::new();
    let mut path_rest = rest;
    if let Some(after) = rest.strip_prefix("//") {
        let end = after
            .find(|c| c == '/' || c == '?' || c == '#')
            .unwrap_or(after.len());
        netloc = after[..end].to_string();
        path_rest = &after[end..];
    }
    let end = path_rest
        .find(|c| c == '?' || c == '#')
        .unwrap_or(path_rest.len());
    let path = path_rest[..end].to_string();
    let query = path_rest[end..]
        .strip_prefix('?')
        .map(|q| match q.find('#') {
            Some(hash) => q[..hash].to_string(),
            None => q.to_string(),
        })
        .unwrap_or_default();
    SplitUrl { scheme, netloc, path, query }
}

fn partition(text: &str, sep: char) -> (&str, bool, &str) {
    match text.find(sep) {
        Some(idx) => (&text[..idx], true, &text[idx + 1..]),
        None => (text, false, ""),
    }
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let is_ws = |b: &u8| b.is_ascii_whitespace();
    let start = bytes.iter().position(|b| !is_ws(b)).unwrap_or(bytes.len());
    let end = bytes.iter().rposition(|b| !is_ws(b)).map_or(0, |i| i + 1);
    &bytes[start..end]
}

/// Python `unquote`: `%XX` sequences decode into bytes; runs are re-encoded as
/// UTF-8 with replacement characters for invalid sequences.
fn unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unquote_plus(text: &str) -> String {
    unquote(&text.replace('+', " "))
}

/// Python `parse_qs(query, keep_blank_values=keep_blank)`.
fn parse_qs(query: &str, keep_blank: bool) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    if query.is_empty() {
        return result;
    }
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key_raw, value_raw) = match pair.find('=') {
            Some(idx) => (&pair[..idx], &pair[idx + 1..]),
            None => (pair, ""),
        };
        if value_raw.is_empty() && !keep_blank {
            continue;
        }
        let key = unquote_plus(key_raw);
        let value = unquote_plus(value_raw);
        match result.iter_mut().find(|(name, _)| *name == key) {
            Some((_, values)) => values.push(value),
            None => result.push((key, vec![value])),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(link: &str) -> SubscriptionDeepLink {
        parse_lumen_deep_link(link).unwrap().unwrap()
    }

    /// Test helper: Python `quote(value, safe="")`.
    fn quote_all(value: &str) -> String {
        let mut out = String::new();
        for &byte in value.as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char)
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }

    #[test]
    fn canonical_subscription_deep_link() {
        let target = "https://example.com/sub?token=secret&client=lumen";
        let link = format!(
            "lumen://add?url={}&name=My%20VPN",
            quote_all(target)
        );
        let request = parse(&link);
        assert_eq!(request.url, target);
        assert_eq!(request.name, "My VPN");
    }

    #[test]
    fn supported_aliases() {
        for link in [
            "lumen://import?url=https%3A%2F%2Fexample.com%2Fsub",
            "lumen://subscribe/https%3A%2F%2Fexample.com%2Fsub",
            "lumen://add/https://example.com/sub",
            "lumen://subscription/add?subscription=happ%3A%2F%2Fcrypt%2Fpayload",
            "lumen://subscription/add?subscription=incy%3A%2F%2Fcrypt1%2Fpayload",
            "lumen:install-config?link=https%3A%2F%2Fexample.com%2Fsub",
        ] {
            let request = parse(link);
            assert!(
                request.url == "https://example.com/sub"
                    || request.url == "happ://crypt/payload"
                    || request.url == "incy://crypt1/payload",
                "unexpected url {}",
                request.url
            );
        }
    }

    #[test]
    fn bare_lumen_link_only_activates() {
        assert_eq!(parse_lumen_deep_link("lumen://").unwrap(), None);
    }

    #[test]
    fn rejects_unsafe_or_unknown() {
        for link in [
            "lumen://add?url=file%3A%2F%2FC%3A%2Fsecret.txt",
            "lumen://add?url=javascript%3Aalert%281%29",
            "lumen://unknown?url=https%3A%2F%2Fexample.com",
        ] {
            assert!(parse_lumen_deep_link(link).is_err());
        }
    }

    #[test]
    fn single_instance_message_preserves_deep_link() {
        let link = "lumen://add?url=https%3A%2F%2Fexample.com%2Fsub";
        let payload = encode_instance_message(&["Lumen.exe".to_string(), link.to_string()]);

        assert_eq!(find_lumen_deep_link(&["Lumen.exe".to_string(), link.to_string()]), link);
        assert_eq!(decode_instance_message(&payload), link);
        assert_eq!(decode_instance_message(b"activate"), "");
    }

    #[test]
    fn encode_instance_message_tray_and_activation() {
        let tray_payload = encode_instance_message(&["Lumen.exe".to_string(), "--tray".to_string()]);
        assert_eq!(parse_instance_message(&tray_payload), ("tray".to_string(), String::new()));
        assert_eq!(decode_instance_message(&tray_payload), "");

        let normal_payload = encode_instance_message(&["Lumen.exe".to_string()]);
        assert_eq!(parse_instance_message(&normal_payload), ("activate".to_string(), String::new()));
        assert_eq!(decode_instance_message(&normal_payload), "");

        let relaunch_payload = encode_instance_message(&[
            "Lumen.exe".to_string(),
            "--tray".to_string(),
            "--relaunch-as-admin".to_string(),
        ]);
        assert_eq!(parse_instance_message(&relaunch_payload), ("activate".to_string(), String::new()));

        let link = "lumen://add?url=https%3A%2F%2Fexample.com%2Fsub";
        let deeplink_tray_payload = encode_instance_message(&[
            "Lumen.exe".to_string(),
            "--tray".to_string(),
            link.to_string(),
        ]);
        assert_eq!(parse_instance_message(&deeplink_tray_payload), ("deeplink".to_string(), link.to_string()));
        assert_eq!(decode_instance_message(&deeplink_tray_payload), link);
    }

    #[test]
    fn parse_instance_message_edge_cases() {
        let act = ("activate".to_string(), String::new());
        assert_eq!(parse_instance_message(b""), act);
        assert_eq!(parse_instance_message(b"activate"), act);
        assert_eq!(parse_instance_message(b"invalid json"), act);
        assert_eq!(parse_instance_message(br#"{"command": "unknown"}"#), act);
        assert_eq!(parse_instance_message(br#"{"command": "tray"}"#), ("tray".to_string(), String::new()));
        assert_eq!(parse_instance_message(br#"{"command": "activate"}"#), act);
    }
}
