//! Fail-closed redaction for human logs, structured records and diagnostics.
//!
//! Port of `windows/xray_fluent/secret_scrubber.py` (scrub_text/scrub_value and
//! StreamingSecretScrubber). The logging-record integration (`scrub_record`,
//! `prepare_record`, `ScrubbingFilter`) is Python-logging specific and is not
//! ported; call [`scrub_text`]/[`scrub_value`] at the Rust log sink instead.

use std::collections::HashSet;
use std::sync::LazyLock;

use parking_lot::Mutex;
use regex::{Captures, Regex};
use serde_json::{Map, Value};

use crate::subscription::text::py_str;

pub const REDACTED: &str = "***";
const MAX_RECORD_CHARS: usize = 16 * 1024;
const MAX_ITEMS: usize = 128;
const MAX_DEPTH: usize = 8;

const UUID_PATTERN: &str = r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}";

// Logs sometimes contain share links copied from Markdown with an escaped
// colon/slashes (``vless\://``). Treat that spelling exactly like a URI;
// otherwise the userinfo credential before ``@`` would remain visible.
static URI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b[a-zA-Z][a-zA-Z0-9+.-]*\\?:(?:\\?/){2}[^\s<>"']+"#).unwrap()
});
static IDENTIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"\b(?:(?:{UUID_PATTERN})|[0-9a-fA-F]{{32,128}})\b")).unwrap()
});
// Only explicitly labelled request correlation IDs are diagnostic data.
// Unlabelled UUIDs, node credentials and installation IDs in log text stay hidden.
static TECHNICAL_ID_KEYS: [&str; 3] = ["request_id", "trace_id", "correlation_id"];
static TECHNICAL_ID_VALUE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^(?:{UUID_PATTERN}|[0-9a-fA-F]{{32}})\z")).unwrap());
// The Python pattern has a `(?<![\w.-])` lookbehind and a `(?![\w-])`
// lookahead; the regex crate does not support lookaround, so the boundaries
// are re-checked manually on each match (see preceded_by_boundary /
// followed_by_boundary).
static TECHNICAL_ID_ASSIGNMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?:request_id|trace_id|correlation_id)\s*[=:]\s*["']?(?P<value>{UUID_PATTERN}|[0-9a-fA-F]{{32}})"#
    ))
    .unwrap()
});
// Match the assignment syntax first, then classify its key. This also handles
// quoted keys inside multiply escaped JSON embedded in otherwise plain text.
// The key class equals the lookbehind's forbidden set, so Rust's leftmost
// match always starts on the same boundary Python's lookbehind enforces.
static ASSIGNMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?P<key>[\w.-]+)(?:\\*["'])?[ \t]*[:=][ \t]*"#).unwrap());
static PEM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)-----BEGIN [^-\r\n]*PRIVATE KEY-----.*?(?:-----END [^-\r\n]*PRIVATE KEY-----|\z)")
        .unwrap()
});
static PEM_BOUNDARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)-----((?:BEGIN)|(?:END)) [^-\r\n]*PRIVATE KEY-----").unwrap()
});
static AUTH_SCHEME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:bearer|basic)[ \t]+").unwrap());
static HTTP_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|CONNECT)\s+)/(?:[^\s"'<>]+)"#)
        .unwrap()
});
static HOME_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:[a-z]:[\\/]+Users[\\/]+|/(?:home|Users)/)[^\\/\s"']+"#).unwrap()
});
static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\x1b|\x{9b})\[[0-?]*[ -/]*[@-~]").unwrap());
static CONTROLS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]").unwrap());
static UNICODE_ESCAPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\+u([0-9a-fA-F]{4})").unwrap());

const KEY_PARTS: [&str; 21] = [
    "password",
    "passphrase",
    "secret",
    "token",
    "credential",
    "authorization",
    "cookie",
    "apikey",
    "privatekey",
    "publickey",
    "presharedkey",
    "hwid",
    "hardwareid",
    "machineid",
    "deviceid",
    "installid",
    "installationid",
    "clientid",
    "userid",
    "hash",
    "salt",
];
const KEY_EXACT: [&str; 12] = [
    "id",
    "pass",
    "pwd",
    "psk",
    "uuid",
    "shortid",
    "sid",
    "username",
    "email",
    "uri",
    "url",
    "path",
];

/// Never include an exception or repr fallback when string conversion fails.
pub fn safe_text(value: &Value) -> String {
    py_str(value)
}

pub fn is_sensitive_key(key: &str) -> bool {
    let normalized: String = key
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    normalized.ends_with("uri")
        || normalized.ends_with("url")
        || normalized.ends_with("path")
        || KEY_EXACT.contains(&normalized.as_str())
        || KEY_PARTS.iter().any(|part| normalized.contains(part))
}

/// Python lookbehind `(?<![\w.-])`: the previous character must not be a word
/// character, dot or dash (start of text counts as a boundary).
fn preceded_by_boundary(text: &str, start: usize) -> bool {
    match text[..start].chars().next_back() {
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-'),
        None => true,
    }
}

/// Python lookahead `(?![\w-])`: the next character must not be a word
/// character or dash (end of text counts as a boundary).
fn followed_by_boundary(text: &str, end: usize) -> bool {
    match text[end..].chars().next() {
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '-'),
        None => true,
    }
}

/// `\\+u00(22|27)` anchored at `start`: a Unicode-escaped quote in an otherwise
/// malformed JSON fragment has ambiguous boundaries. Never stop at a comma
/// inside its private value.
fn starts_with_escaped_quote(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() && bytes[index] == b'\\' {
        index += 1;
    }
    if index == 0 || bytes.len() < index + 5 {
        return false;
    }
    let rest = &bytes[index..];
    matches!(rest[0], b'u' | b'U')
        && rest[1] == b'0'
        && rest[2] == b'0'
        && rest[3] == b'2'
        && (rest[4] == b'2' || rest[4] == b'7')
}

/// `\\*["']` anchored at `start`, or the default `"` delimiter.
fn escaped_quote_prefix(bytes: &[u8]) -> String {
    let mut index = 0;
    while index < bytes.len() && bytes[index] == b'\\' {
        index += 1;
    }
    if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
        return String::from_utf8_lossy(&bytes[..=index]).into_owned();
    }
    "\"".to_string()
}

/// Consume a whole quoted (possibly escaped) value or a header value.
///
/// In an escaped JSON string, a delimiter with one backslash closes a value;
/// a quote with three backslashes belongs to the value. A token-only regular
/// expression is unsafe here and for 'Authorization: Bearer <credential>'.
/// All scans are byte-based: every special character involved is ASCII.
fn assignment_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if starts_with_escaped_quote(&bytes[start.min(len)..]) {
        return len;
    }
    let mut quote_at = start;
    while quote_at < len && bytes[quote_at] == b'\\' {
        quote_at += 1;
    }
    if quote_at < len && (bytes[quote_at] == b'"' || bytes[quote_at] == b'\'') {
        let quote = bytes[quote_at];
        let escape_depth = quote_at - start;
        let mut index = quote_at + 1;
        while index < len {
            let found = match bytes[index..].iter().position(|byte| *byte == quote) {
                Some(offset) => index + offset,
                None => return len, // Incomplete/rotated private value: fail closed.
            };
            let mut backslashes = 0;
            let mut preceding = found as isize - 1;
            while preceding >= start as isize && bytes[preceding as usize] == b'\\' {
                backslashes += 1;
                preceding -= 1;
            }
            if (escape_depth == 0 && backslashes % 2 == 0)
                || (escape_depth > 0 && backslashes == escape_depth)
            {
                let mut following = found + 1;
                while following < len && (bytes[following] == b' ' || bytes[following] == b'\t') {
                    following += 1;
                }
                if following < len
                    && !matches!(
                        bytes[following],
                        b',' | b';' | b'}' | b']' | b'\r' | b'\n'
                    )
                {
                    return len; // Malformed unescaped quote: hide the tail.
                }
                return found + 1;
            }
            index = found + 1;
        }
        return len;
    }
    // Credentials and auth schemes may contain spaces. Preserve fields after
    // strong delimiters, but never leave a credential after masking only Bearer.
    let mut end = start;
    while end < len && !matches!(bytes[end], b'\r' | b'\n' | b',' | b';' | b'}' | b']') {
        end += 1;
    }
    end
}

fn scrub_assignments(text: &str) -> String {
    let mut pieces: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for caps in ASSIGNMENT_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        if whole.start() < cursor {
            continue;
        }
        let key = caps.name("key").unwrap().as_str();
        if !is_sensitive_key(key) {
            continue;
        }
        pieces.push(text[cursor..whole.end()].to_string());
        let start = whole.end();
        let end = assignment_end(text, start);
        // Preserve the delimiter style, including escaped quotes in JSON logs.
        let delimiter = escaped_quote_prefix(&text.as_bytes()[start.min(text.len())..]);
        pieces.push(format!("{delimiter}{REDACTED}{delimiter}"));
        cursor = end;
    }
    pieces.push(text[cursor..].to_string());
    pieces.concat()
}

fn scrub_auth_schemes(text: &str) -> String {
    let mut pieces: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    for matched in AUTH_SCHEME_RE.find_iter(text) {
        if matched.start() < cursor {
            continue;
        }
        pieces.push(text[cursor..matched.start()].to_string());
        pieces.push("[REDACTED AUTH]".to_string());
        cursor = assignment_end(text, matched.end());
    }
    pieces.push(text[cursor..].to_string());
    pieces.concat()
}

/// Decode only non-delimiter unicode characters in JSON fragments. Turning
/// escaped quotes into bare quotes before parsing could expose value tails.
fn decode_unicode_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for caps in UNICODE_ESCAPE_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        let hex = caps.get(1).unwrap();
        out.push_str(&text[last..whole.start()]);
        let code = u32::from_str_radix(hex.as_str(), 16).unwrap_or(0);
        match char::from_u32(code) {
            Some(decoded)
                if decoded.is_alphanumeric() || matches!(decoded, '_' | '-' | '/' | '.' | ':') =>
            {
                out.push(decoded);
            }
            _ => out.push_str(whole.as_str()),
        }
        last = whole.end();
    }
    out.push_str(&text[last..]);
    out
}

fn strip_ansi_and_controls(text: &str) -> String {
    CONTROLS_RE
        .replace_all(&ANSI_RE.replace_all(text, ""), "")
        .into_owned()
}

/// The stateless text scrubber. `depth` mirrors Python's `_depth` parameter.
pub fn scrub_text_depth(value: &str, depth: usize) -> String {
    let text = strip_ansi_and_controls(value);
    if depth > MAX_DEPTH {
        return "[REDACTED DEPTH]".into();
    }
    // Decode complete JSON before redacting keys/values. This covers nested
    // JSON strings and unicode escapes without mis-parsing escaped value quotes.
    let stripped = text.trim();
    if !stripped.is_empty()
        && matches!(stripped.chars().next(), Some('{') | Some('[') | Some('"'))
        && stripped.chars().count() <= 4 * 1024 * 1024
    {
        if let Ok(parsed) = serde_json::from_str::<Value>(stripped) {
            return serde_json::to_string(&scrub_value_depth(&parsed, depth + 1))
                .unwrap_or_else(|_| "[REDACTED OBJECT]".into());
        }
    }
    let text = decode_unicode_escapes(&text);
    let text = PEM_RE.replace_all(&text, "[REDACTED KEY]").into_owned();
    let text = URI_RE.replace_all(&text, "[REDACTED URI]").into_owned();
    let text = HTTP_PATH_RE
        .replace_all(&text, "${1}/[REDACTED]")
        .into_owned();
    let text = HOME_PATH_RE.replace_all(&text, "[REDACTED HOME]").into_owned();
    let text = scrub_assignments(&text);
    let text = scrub_auth_schemes(&text);
    let technical: HashSet<(usize, usize)> = TECHNICAL_ID_ASSIGNMENT_RE
        .captures_iter(&text)
        .filter_map(|caps| {
            let value = caps.name("value")?;
            if !preceded_by_boundary(&text, caps.get(0).unwrap().start()) {
                return None;
            }
            if !followed_by_boundary(&text, value.end()) {
                return None;
            }
            Some((value.start(), value.end()))
        })
        .collect();
    IDENTIFIER_RE
        .replace_all(&text, |caps: &Captures| {
            let whole = caps.get(0).unwrap();
            if technical.contains(&(whole.start(), whole.end())) {
                whole.as_str().to_string()
            } else {
                "[REDACTED ID]".to_string()
            }
        })
        .into_owned()
}

/// `scrub_text(value)` from the Python module.
pub fn scrub_text(value: &Value) -> String {
    scrub_text_depth(&safe_text(value), 0)
}

/// `scrub_value(value)` from the Python module: bounded JSON-safe data; deep
/// structures and unknown shapes fail closed.
pub fn scrub_value_depth(value: &Value, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::String("[REDACTED DEPTH]".into());
    }
    match value {
        Value::Object(map) => {
            let mut result = Map::new();
            for (index, (key, item)) in map.iter().enumerate() {
                if index >= MAX_ITEMS {
                    result.insert("[TRUNCATED]".into(), Value::Bool(true));
                    break;
                }
                let safe_key = scrub_text_depth(key, depth + 1);
                if TECHNICAL_ID_KEYS.contains(&key.as_str())
                    && item
                        .as_str()
                        .map(|text| TECHNICAL_ID_VALUE_RE.is_match(text))
                        .unwrap_or(false)
                {
                    result.insert(safe_key, item.clone());
                } else if is_sensitive_key(key) {
                    result.insert(safe_key, Value::String(REDACTED.into()));
                } else {
                    result.insert(safe_key, scrub_value_depth(item, depth + 1));
                }
            }
            Value::Object(result)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(MAX_ITEMS)
                .map(|item| scrub_value_depth(item, depth + 1))
                .collect(),
        ),
        Value::String(text) => Value::String(scrub_text_depth(text, depth + 1)),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

/// `scrub_value(value)` from the Python module.
pub fn scrub_value(value: &Value) -> Value {
    scrub_value_depth(value, 0)
}

/// Scrub complete raw lines BEFORE any stateless scrubber removes markers.
///
/// Own one instance per raw producer (not per line). An unterminated PEM
/// suppresses subsequent lines until its END; no timeout guesses or raw
/// buffering. Nested/interleaved blocks fail closed. Delimiter fragments
/// split across calls are not supported: frame complete lines upstream.
pub struct StreamingSecretScrubber {
    open_keys: Mutex<i64>,
}

impl Default for StreamingSecretScrubber {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingSecretScrubber {
    pub fn new() -> Self {
        Self {
            open_keys: Mutex::new(0),
        }
    }

    pub fn scrub(&self, value: &Value) -> String {
        let text = strip_ansi_and_controls(&safe_text(value));
        let mut guard = self.open_keys.lock();
        let mut pieces: Vec<String> = Vec::new();
        let mut cursor = 0usize;
        for caps in PEM_BOUNDARY_RE.captures_iter(&text) {
            let whole = caps.get(0).unwrap();
            if *guard == 0 {
                pieces.push(scrub_text_depth(&text[cursor..whole.start()], 0));
            }
            pieces.push("[REDACTED KEY]".to_string());
            if caps.get(1).unwrap().as_str().eq_ignore_ascii_case("BEGIN") {
                *guard += 1;
            } else if *guard > 0 {
                *guard -= 1;
            }
            cursor = whole.end();
        }
        if *guard != 0 {
            pieces.push("[REDACTED KEY]".to_string());
        } else {
            pieces.push(scrub_text_depth(&text[cursor..], 0));
        }
        let safe: String = pieces.concat();
        // Track/redact the ENTIRE input before bounding a queued UI line.
        if safe.chars().count() > MAX_RECORD_CHARS {
            format!(
                "{} [TRUNCATED]",
                safe.chars().take(MAX_RECORD_CHARS).collect::<String>()
            )
        } else {
            safe
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scrubs_sample_log_line_with_password_and_uuid() {
        let line = "login ok user=550e8400-e29b-41d4-a716-446655440000 password=hunter2";
        let scrubbed = scrub_text(&json!(line));
        assert!(!scrubbed.contains("hunter2"));
        assert!(scrubbed.contains("password=\"***\""), "got: {scrubbed}");
        assert!(!scrubbed.contains("550e8400"));
        assert!(scrubbed.contains("[REDACTED ID]"), "got: {scrubbed}");
        // The non-sensitive assignment key and its prefix survive.
        assert!(scrubbed.starts_with("login ok user="), "got: {scrubbed}");
    }

    #[test]
    fn hides_tail_after_unescaped_quote() {
        let line = "auth password=\"hunter2\" next-field";
        let scrubbed = scrub_text(&json!(line));
        assert!(!scrubbed.contains("hunter2"));
        assert!(!scrubbed.contains("next-field")); // fail-closed tail
        assert!(scrubbed.contains("password=\"***\""), "got: {scrubbed}");
    }

    #[test]
    fn redacts_pem_blocks() {
        let line = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQ\n-----END PRIVATE KEY-----\nplain tail";
        let scrubbed = scrub_text(&json!(line));
        assert!(scrubbed.contains("[REDACTED KEY]"), "got: {scrubbed}");
        assert!(!scrubbed.contains("MIIEvQ"));
        assert!(scrubbed.contains("plain tail"), "got: {scrubbed}");
    }

    #[test]
    fn keeps_labelled_request_ids() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let line = format!("request_id={uuid} done");
        let scrubbed = scrub_text(&json!(line));
        assert!(scrubbed.contains(uuid), "got: {scrubbed}");
        // The same UUID unlabelled stays hidden.
        let scrubbed = scrub_text(&json!(format!("trace {uuid} done")));
        assert!(!scrubbed.contains(uuid));
        assert!(scrubbed.contains("[REDACTED ID]"), "got: {scrubbed}");
    }

    #[test]
    fn redacts_uris_bearer_and_home_paths() {
        let scrubbed = scrub_text(&json!("GET /api/v1/users/42 HTTP/1.1 see https://example.com/a?token=1"));
        assert!(scrubbed.contains("GET /[REDACTED]"), "got: {scrubbed}");
        assert!(scrubbed.contains("[REDACTED URI]"), "got: {scrubbed}");
        let scrubbed = scrub_text(&json!("Authorization Bearer abc.def.ghi"));
        assert!(scrubbed.contains("[REDACTED AUTH]"), "got: {scrubbed}");
        assert!(!scrubbed.contains("abc.def"));
        let scrubbed = scrub_text(&json!("home C:\\Users\\alice\\notes.txt"));
        assert!(scrubbed.contains("[REDACTED HOME]"), "got: {scrubbed}");
    }

    #[test]
    fn scrub_value_redacts_structured_keys_and_keeps_technical_ids() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let value = json!({
            "request_id": uuid,
            "password": "hunter2",
            "nested": {"api_token": "abc123", "total": 5},
            "items": [1, 2, 3]
        });
        let scrubbed = scrub_value(&value);
        assert_eq!(scrubbed["request_id"], json!(uuid));
        assert_eq!(scrubbed["password"], "***");
        assert_eq!(scrubbed["nested"]["api_token"], "***");
        assert_eq!(scrubbed["nested"]["total"], 5);
        assert_eq!(scrubbed["items"], json!([1, 2, 3]));
    }

    #[test]
    fn scrub_value_fails_closed_on_depth_and_item_count() {
        let mut deep = json!(1);
        for _ in 0..20 {
            deep = json!({"a": deep});
        }
        // The raw leaf never survives; nesting stops at the depth cap. At the
        // innermost level Python's scrub_text-based key also becomes the
        // "[REDACTED DEPTH]" placeholder.
        let scrubbed = scrub_value(&deep);
        let mut cursor = &scrubbed;
        let mut levels = 0;
        while let Value::Object(map) = cursor {
            levels += 1;
            cursor = map.values().next().expect("nested value");
        }
        assert_eq!(cursor, &json!("[REDACTED DEPTH]"));
        assert!(levels <= MAX_DEPTH + 1, "levels: {levels}");
        let mut wide = Map::new();
        for index in 0..200 {
            wide.insert(format!("k{index}"), json!(index));
        }
        let scrubbed = scrub_value(&Value::Object(wide));
        assert_eq!(scrubbed["[TRUNCATED]"], json!(true));
        assert!(scrubbed.as_object().unwrap().len() <= MAX_ITEMS + 1);
    }

    #[test]
    fn streaming_scrubber_suppresses_unterminated_pem() {
        let scrubber = StreamingSecretScrubber::new();
        // Python appends one marker inside the boundary loop and one more
        // while a block is still open, so a BEGIN-only line carries two.
        assert_eq!(
            scrubber.scrub(&json!("-----BEGIN RSA PRIVATE KEY-----")),
            "[REDACTED KEY][REDACTED KEY]"
        );
        // While a key is open, every raw line is suppressed.
        assert_eq!(
            scrubber.scrub(&json!("MIIEvQ secret material")),
            "[REDACTED KEY]"
        );
        assert_eq!(
            scrubber.scrub(&json!("-----END RSA PRIVATE KEY-----")),
            "[REDACTED KEY]"
        );
        assert_eq!(scrubber.scrub(&json!("back to normal")), "back to normal");
        // Nested blocks fail closed but recover on balanced END markers.
        assert_eq!(
            scrubber.scrub(&json!("-----BEGIN PRIVATE KEY-----")),
            "[REDACTED KEY][REDACTED KEY]"
        );
        assert_eq!(
            scrubber.scrub(&json!("-----BEGIN PRIVATE KEY-----")),
            "[REDACTED KEY][REDACTED KEY]"
        );
        assert_eq!(
            scrubber.scrub(&json!("-----END PRIVATE KEY-----")),
            "[REDACTED KEY][REDACTED KEY]" // first END closes one of two blocks
        );
        assert_eq!(
            scrubber.scrub(&json!("-----END PRIVATE KEY-----")),
            "[REDACTED KEY]" // balanced now; only the boundary marker remains
        );
        assert_eq!(scrubber.scrub(&json!("clear again")), "clear again");
    }

    #[test]
    fn streaming_scrubber_truncates_long_lines() {
        let scrubber = StreamingSecretScrubber::new();
        let long = "x".repeat(MAX_RECORD_CHARS + 100);
        let scrubbed = scrubber.scrub(&json!(long));
        assert!(scrubbed.ends_with(" [TRUNCATED]"));
        assert!(scrubbed.chars().count() <= MAX_RECORD_CHARS + " [TRUNCATED]".len());
    }

    #[test]
    fn is_sensitive_key_matches_python() {
        assert!(is_sensitive_key("password"));
        // "P@ssw0rd!" normalizes to "pssw0rd", which is not a sensitive key;
        // the assignment scrubber relies on plain "password"-style keys.
        assert!(!is_sensitive_key("P@ssw0rd!"));
        assert!(is_sensitive_key("PassWord"));
        assert!(is_sensitive_key("api_token"));
        assert!(is_sensitive_key("hwid"));
        assert!(is_sensitive_key("imageUrl"));
        assert!(is_sensitive_key("path"));
        assert!(!is_sensitive_key("nickname"));
        assert!(!is_sensitive_key("request_id"));
    }
}
