//! Python-stdlib parity helpers for `link_parser.py`:
//! `urlsplit`, `unquote`/`parse_qs`, base64 decoding chains, `_get_param`,
//! subscription blob detection/splitting and small string utilities.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use base64::engine::general_purpose::GeneralPurpose;
use base64::engine::GeneralPurposeConfig;
use base64::Engine;
use regex::Regex;
use serde_json::Value;

use super::LinkParseError;

// ── Unicode whitespace exactly like Python `str.isspace()`/`str.split()` ──

pub fn is_py_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{1f}' | '\u{85}' | '\u{a0}')
        || ('\u{1680}'..='\u{200a}').contains(&c) && c != '\u{200b}'
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Python `str.splitlines()` (covers the full separator set).
pub fn py_splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut iter = text.char_indices().peekable();
    while let Some((pos, c)) = iter.next() {
        let consumed = match c {
            '\r' => {
                if iter.peek().map(|(_, n)| *n) == Some('\n') {
                    iter.next();
                    2
                } else {
                    1
                }
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => c.len_utf8(),
            _ => 0,
        };
        if consumed > 0 {
            lines.push(&text[start..pos]);
            start = pos + consumed;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Python `"".join(text.split())` — remove every whitespace character.
pub fn remove_all_whitespace(text: &str) -> String {
    text.chars().filter(|c| !is_py_space(*c)).collect()
}

// ── unquote / parse_qs ──

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Python `urllib.parse.unquote` (UTF-8, errors="replace").
pub fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut pending: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                if !pending.is_empty() {
                    out.push_str(&String::from_utf8_lossy(&pending));
                    pending.clear();
                }
                pending.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        pending.push(bytes[i]);
        i += 1;
    }
    if !pending.is_empty() {
        out.push_str(&String::from_utf8_lossy(&pending));
    }
    out
}

/// Python `urllib.parse.unquote_plus` (`+` becomes a space first).
pub fn unquote_plus(s: &str) -> String {
    unquote(&s.replace('+', " "))
}

/// Python `parse_qs(query, keep_blank_values=True)` as an ordered pair list.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for item in query.split('&') {
        if item.is_empty() {
            continue;
        }
        let (name_raw, value_raw) = match item.split_once('=') {
            Some((n, v)) => (n, v),
            None => (item, ""),
        };
        result.push((unquote_plus(name_raw), unquote_plus(value_raw)));
    }
    result
}

/// Python `_first(query, key, default="")` over an ordered query pair list.
pub fn first_of(query: &[(String, String)], key: &str) -> String {
    query
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

// ── urlsplit (CPython semantics) ──

const SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.";
const UNSAFE_URL_BYTES: [char; 3] = ['\t', '\r', '\n'];

#[derive(Debug, Clone, Default)]
pub struct PyUrlSplit {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

impl PyUrlSplit {
    /// CPython `urllib.parse.urlsplit` (no IDNA/normalization beyond CPython's).
    pub fn parse(url: &str) -> Result<Self, LinkParseError> {
        // CPython strips ASCII tab/newline characters before parsing.
        let mut rest: String = url.chars().filter(|c| !UNSAFE_URL_BYTES.contains(c)).collect();
        if (rest.contains('[') && !rest.contains(']')) || (rest.contains(']') && !rest.contains('[')) {
            return Err(LinkParseError::new("Invalid IPv6 URL"));
        }
        let mut scheme = String::new();
        if let Some(colon) = rest.find(':') {
            if colon > 0 {
                let head = &rest[..colon];
                let first = head.chars().next().unwrap();
                if first.is_ascii_alphabetic() && head.chars().all(|c| SCHEME_CHARS.contains(c)) {
                    scheme = head.to_lowercase();
                    rest = rest[colon + 1..].to_string();
                }
            }
        }
        let mut netloc = String::new();
        if rest.starts_with("//") {
            let start = 2;
            let mut delim = rest.len();
            for c in ['/', '?', '#'] {
                if let Some(pos) = rest[start..].find(c) {
                    delim = delim.min(start + pos);
                }
            }
            netloc = rest[start..delim].to_string();
            rest = rest[delim..].to_string();
        }
        let mut fragment = String::new();
        if let Some(pos) = rest.find('#') {
            fragment = rest[pos + 1..].to_string();
            rest = rest[..pos].to_string();
        }
        let mut query = String::new();
        if let Some(pos) = rest.find('?') {
            query = rest[pos + 1..].to_string();
            rest = rest[..pos].to_string();
        }
        Ok(Self { scheme, netloc, path: rest, query, fragment })
    }

    pub fn username(&self) -> Option<&str> {
        let userinfo = self.netloc.rsplit_once('@')?.0;
        Some(userinfo.split_once(':').map(|(u, _)| u).unwrap_or(userinfo))
    }

    pub fn password(&self) -> Option<&str> {
        let userinfo = self.netloc.rsplit_once('@')?.0;
        userinfo.split_once(':').map(|(_, p)| p)
    }

    /// `parsed.hostname` — lowercased, IPv6 brackets stripped, never None.
    pub fn hostname(&self) -> String {
        let hostinfo = match self.netloc.rsplit_once('@') {
            Some((_, host)) => host,
            None => self.netloc.as_str(),
        };
        let host = if hostinfo.contains('[') {
            match (hostinfo.find('['), hostinfo.find(']')) {
                (Some(open), Some(close)) if close > open => &hostinfo[open + 1..close],
                _ => "",
            }
        } else {
            hostinfo.split_once(':').map(|(h, _)| h).unwrap_or(hostinfo)
        };
        host.to_lowercase()
    }

    /// `parsed.port` — raises the CPython ValueErrors as `LinkParseError`.
    pub fn port(&self) -> Result<Option<i64>, LinkParseError> {
        let hostinfo = match self.netloc.rsplit_once('@') {
            Some((_, host)) => host,
            None => self.netloc.as_str(),
        };
        let port_text = if hostinfo.contains('[') {
            match hostinfo.find(']') {
                Some(close) => hostinfo[close + 1..]
                    .split_once(':')
                    .map(|(_, p)| p)
                    .unwrap_or(""),
                None => "",
            }
        } else {
            hostinfo.split_once(':').map(|(_, p)| p).unwrap_or("")
        };
        if port_text.is_empty() {
            return Ok(None);
        }
        match py_parse_int(port_text) {
            Ok(value) => {
                if !(0..=65535).contains(&value) {
                    return Err(LinkParseError::new("Port out of range 0-65535"));
                }
                Ok(Some(value))
            }
            Err(()) => Err(LinkParseError::new(format!(
                "Port could not be cast to integer value as {port_text:?}"
            ))),
        }
    }
}

/// Python `int(str)` for ASCII decimal strings (accepts sign, surrounding
/// whitespace and digit-adjacent underscores).
pub fn py_parse_int(text: &str) -> Result<i64, ()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(());
    }
    let mut body = trimmed;
    let mut sign = 1i64;
    if let Some(rest) = body.strip_prefix('+') {
        body = rest;
    } else if let Some(rest) = body.strip_prefix('-') {
        body = rest;
        sign = -1;
    }
    if body.is_empty() {
        return Err(());
    }
    // Underscores are legal only between two digits.
    let bytes = body.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'_' {
            let prev_ok = i > 0 && bytes[i - 1].is_ascii_digit();
            let next_ok = i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
            if !prev_ok || !next_ok {
                return Err(());
            }
        }
    }
    let cleaned: String = body.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() || !cleaned.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    cleaned.parse::<i64>().map(|v| sign * v).map_err(|_| ())
}

// ── serde_json ↔ Python scalar semantics ──

/// Truthiness for an optional JSON value (`x.get(k)` convenience).
pub fn py_truthy_opt(value: Option<&Value>) -> bool {
    value.map(py_truthy).unwrap_or(false)
}

/// Python `type(value) is int` (bools and floats excluded).
pub fn is_py_int(value: &Value) -> bool {
    matches!(value, Value::Number(n) if n.is_i64() || n.is_u64())
}

/// Python truthiness for JSON values.
pub fn py_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python `int(value)` for JSON scalars; `str(value)` must go through
/// [`py_str`] first when Python code called `int(str(...))`.
pub fn py_int_value(value: &Value) -> Result<i64, LinkParseError> {
    match value {
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i)
            } else if let Some(u) = n.as_u64() {
                Ok(u.min(i64::MAX as u64) as i64)
            } else {
                Ok(n.as_f64().unwrap_or(0.0).trunc() as i64)
            }
        }
        Value::String(s) => py_parse_int(s).map_err(|_| {
            LinkParseError::new(format!("invalid literal for int() with base 10: {s:?}"))
        }),
        other => Err(LinkParseError::new(format!(
            "int() argument must be a string, a bytes-like object or a real number, not '{}'",
            python_type_name(other)
        ))),
    }
}

/// `int(x if truthy(x) else fallback)` — mirrors `int(payload.get(k) or fb)`.
pub fn py_int_or(value: Option<&Value>, fallback: i64) -> Result<i64, LinkParseError> {
    match value {
        Some(v) if py_truthy(v) => py_int_value(v),
        _ => Ok(fallback),
    }
}

/// Python `str(value)` for JSON scalars.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True".into() } else { "False".into() },
        Value::Null => "None".into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
                    format!("{f:.1}")
                } else {
                    n.to_string()
                }
            }
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Python `str(value or fallback)`.
pub fn py_str_or(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(v) if py_truthy(v) => py_str(v),
        _ => fallback.to_string(),
    }
}

/// Python `type(value).__name__` approximation for JSON values.
pub fn python_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int"
            } else {
                "float"
            }
        }
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Python `_to_bool(str(value))`.
pub fn to_bool_value(value: Option<&Value>) -> bool {
    match value {
        Some(v) => to_bool_str(&py_str(v)),
        None => false,
    }
}

pub fn to_bool_str(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

// ── base64 ──

fn b64_engine() -> &'static GeneralPurpose {
    static ENGINE: OnceLock<GeneralPurpose> = OnceLock::new();
    ENGINE.get_or_init(|| {
        // Python's a2b_base64 (strict_mode=False) ignores non-canonical
        // trailing bits, so allow them for parity.
        GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            GeneralPurposeConfig::new()
                .with_decode_allow_trailing_bits(true)
                .with_decode_padding_mode(base64::engine::DecodePaddingMode::RequireCanonical),
        )
    })
}

/// Python `b64decode(..., altchars=b"-_")`: translate the URL-safe pair onto
/// the standard alphabet first; original `+`/`/` stay valid exactly like in
/// the Python decoder.
fn b64_decode_translated(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let translated: String = input
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    b64_engine().decode(translated.as_bytes())
}

fn is_b64_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '-' | '_')
}

/// Python `base64.urlsafe_b64decode(data)` with default `validate=False`:
/// non-alphabet characters are silently discarded, then padding is applied.
pub fn decode_b64(data: &str) -> Result<String, LinkParseError> {
    let data = data.trim();
    let cleaned: String = data.chars().filter(|c| is_b64_char(*c)).collect();
    let mut padded = cleaned;
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let raw = b64_decode_translated(&padded)
        .map_err(|_| LinkParseError::new("invalid base64 data"))?;
    Ok(decode_text_encodings(&raw))
}

/// Python `base64.b64decode(..., validate=True)` with caller-provided padding:
/// strict alphabet, no excess data, canonical padding runs.
pub fn decode_b64_strict(clean: &str) -> Result<Vec<u8>, LinkParseError> {
    let mut padded = clean.to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    // Mirror Python's a2b_base64 checks that the base64 crate reports
    // differently: a lone data character before padding is invalid.
    let data_chars = padded.chars().take_while(|c| *c != '=').count();
    if data_chars % 4 == 1 {
        return Err(LinkParseError::new(
            "Invalid base64-encoded string: number of data characters (1) cannot be 1 more than a multiple of 4",
        ));
    }
    b64_decode_translated(&padded)
        .map_err(|e| LinkParseError::new(format!("base64 decode failed: {e}")))
}

/// Python `bytes.decode("utf-8")` then `"cp1251"`, `"gb18030"`, `"latin-1"`
/// fallbacks and a final replace-decode (link_parser `_decode_b64`).
pub fn decode_text_encodings(raw: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(raw) {
        return text.to_string();
    }
    if let Some(text) = decode_cp1251(raw) {
        return text;
    }
    // gb18030 fallback omitted (would need the full mapping table); latin-1
    // always succeeds and preserves the byte values.
    raw.iter().map(|b| *b as char).collect()
}

/// Decode Windows-1251; `None` when the bytes hit the undefined slot (0x98).
fn decode_cp1251(raw: &[u8]) -> Option<String> {
    // Table dumped from CPython's `encodings.cp1251.decoding_table`.
    const CP1251_HIGH: [char; 128] = [
        // 0x80-0x87
        '\u{0402}', '\u{0403}', '\u{201A}', '\u{0453}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
        // 0x88-0x8F
        '\u{20AC}', '\u{2030}', '\u{0409}', '\u{2039}', '\u{040A}', '\u{040C}', '\u{040B}', '\u{040F}',
        // 0x90-0x97
        '\u{0452}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
        // 0x98-0x9F (0x98 undefined → decoder refuses)
        '\u{FFFD}', '\u{2122}', '\u{0459}', '\u{203A}', '\u{045A}', '\u{045C}', '\u{045B}', '\u{045F}',
        // 0xA0-0xA7
        '\u{00A0}', '\u{040E}', '\u{045E}', '\u{0408}', '\u{00A4}', '\u{0490}', '\u{00A6}', '\u{00A7}',
        // 0xA8-0xAF
        '\u{0401}', '\u{00A9}', '\u{0404}', '\u{00AB}', '\u{00AC}', '\u{00AD}', '\u{00AE}', '\u{0407}',
        // 0xB0-0xB7
        '\u{00B0}', '\u{00B1}', '\u{0406}', '\u{0456}', '\u{0491}', '\u{00B5}', '\u{00B6}', '\u{00B7}',
        // 0xB8-0xBF
        '\u{0451}', '\u{2116}', '\u{0454}', '\u{00BB}', '\u{0458}', '\u{0405}', '\u{0455}', '\u{0457}',
        // 0xC0-0xC7
        '\u{0410}', '\u{0411}', '\u{0412}', '\u{0413}', '\u{0414}', '\u{0415}', '\u{0416}', '\u{0417}',
        // 0xC8-0xCF
        '\u{0418}', '\u{0419}', '\u{041A}', '\u{041B}', '\u{041C}', '\u{041D}', '\u{041E}', '\u{041F}',
        // 0xD0-0xD7
        '\u{0420}', '\u{0421}', '\u{0422}', '\u{0423}', '\u{0424}', '\u{0425}', '\u{0426}', '\u{0427}',
        // 0xD8-0xDF
        '\u{0428}', '\u{0429}', '\u{042A}', '\u{042B}', '\u{042C}', '\u{042D}', '\u{042E}', '\u{042F}',
        // 0xE0-0xE7
        '\u{0430}', '\u{0431}', '\u{0432}', '\u{0433}', '\u{0434}', '\u{0435}', '\u{0436}', '\u{0437}',
        // 0xE8-0xEF
        '\u{0438}', '\u{0439}', '\u{043A}', '\u{043B}', '\u{043C}', '\u{043D}', '\u{043E}', '\u{043F}',
        // 0xF0-0xF7
        '\u{0440}', '\u{0441}', '\u{0442}', '\u{0443}', '\u{0444}', '\u{0445}', '\u{0446}', '\u{0447}',
        // 0xF8-0xFF
        '\u{0448}', '\u{0449}', '\u{044A}', '\u{044B}', '\u{044C}', '\u{044D}', '\u{044E}', '\u{044F}',
    ];
    let mut out = String::with_capacity(raw.len());
    for b in raw {
        out.push(match b {
            0x98 => return None,
            0..=0x7F => *b as char,
            _ => CP1251_HIGH[(*b - 0x80) as usize],
        });
    }
    Some(out)
}

// ── _get_param / dict param helpers ──

/// Python `_get_param(params, *keys, default="")`.
pub fn get_param(params: &BTreeMap<String, String>, keys: &[&str], default: &str) -> String {
    let mut empty_value: Option<String> = None;
    for key in keys {
        if let Some(value) = params.get(*key) {
            if !value.is_empty() {
                return value.clone();
            }
            if empty_value.is_none() {
                empty_value = Some(value.clone());
            }
        }
    }
    let lower_params: BTreeMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect();
    for key in keys {
        if let Some(value) = lower_params.get(&key.to_lowercase()) {
            if !value.is_empty() {
                return value.clone();
            }
            if empty_value.is_none() {
                empty_value = Some(value.clone());
            }
        }
    }
    empty_value.unwrap_or_else(|| default.to_string())
}

/// Python `_split_csv(value)`.
pub fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Python `_camel_to_snake(value)`.
pub fn camel_to_snake(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 4);
    for c in value.chars() {
        if c.is_uppercase() {
            result.push('_');
            result.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            result.push(c);
        }
    }
    result
        .strip_prefix('_')
        .map(|s| s.to_string())
        .unwrap_or(result)
}

/// Python `_clean_name(name, fallback)`.
pub fn clean_name(name: Option<&str>, fallback: String) -> String {
    let value = match name {
        Some(n) => unquote(n),
        None => String::new(),
    };
    let value = value.split('?').next().unwrap_or("").trim().to_string();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

// ── subscription blob detection ──

fn import_scheme_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            "(?i)(?:vless|vmess|trojan|ss|ssr|hysteria2|hysteria|hy2|hy|tuic|wireguard|wg|awg|\
             amneziawg|warp|naive\\+https|naive\\+quic|naive|mierus|mieru|masque|socks5|socks|\
             https|http|happ|incy|snell|juicity|anytls)://",
        )
        .expect("static import scheme regex")
    })
}

/// Python `_decode_subscription_blob(text)` — base64 whole-body subscription.
pub fn decode_subscription_blob(text: &str) -> Option<String> {
    let clean = remove_all_whitespace(text);
    if clean.chars().count() < 16
        || !clean
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '_' | '=' | '-'))
    {
        return None;
    }
    let mut padded = clean.clone();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let raw = match decode_b64_strict(&padded) {
        Ok(raw) => raw,
        Err(_) => return None,
    };
    // utf-8-sig: strip a leading BOM after decoding.
    let mut decoded = decode_text_encodings(&raw);
    if let Some(rest) = decoded.strip_prefix('\u{feff}') {
        decoded = rest.to_string();
    }
    let decoded = decoded.trim().to_string();
    if decoded != text
        && (decoded.contains("://") || decoded.starts_with('{') || decoded.starts_with('[') || decoded.contains("proxies:"))
    {
        Some(decoded)
    } else {
        None
    }
}

/// Python `_split_subscription_entries(text)`.
pub fn split_subscription_entries(text: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for raw in py_splitlines(text) {
        let mut line = raw.trim().to_string();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            line = rest.trim().to_string();
        }
        if line.starts_with('{') || line.starts_with('[') || line.starts_with('"') {
            result.push(line);
            continue;
        }
        let re = import_scheme_regex();
        let starts: Vec<usize> = re
            .find_iter(&line)
            .filter_map(|m| {
                let group = m.as_str().to_lowercase();
                let at = m.start();
                let prev_ok = at == 0
                    || line[..at]
                        .chars()
                        .next_back()
                        .map(|c| is_py_space(c) || matches!(c, ',' | ';' | '|'))
                        .unwrap_or(false);
                if !group.starts_with("http") || at == 0 || prev_ok {
                    Some(at)
                } else {
                    None
                }
            })
            .collect();
        if starts.len() > 1 {
            let mut bounds: Vec<usize> = starts.clone();
            bounds.push(line.len());
            for window in bounds.windows(2) {
                let segment = line[window[0]..window[1]].trim();
                result.push(segment.trim_end_matches([',', ';', '|']).to_string());
            }
        } else {
            result.push(line);
        }
    }
    result
}

// ── file reference (Windows url2pathname parity) ──

/// Python `nturl2path.url2pathname` (Windows variant).
pub fn url2pathname(path: &str) -> String {
    let url = path.replace(':', "|");
    if !url.contains('|') {
        let mut url = url.as_str();
        if url.starts_with("////") {
            url = &url[2..];
        }
        let joined = url.split('/').collect::<Vec<_>>().join("\\");
        return unquote(&joined);
    }
    let comps: Vec<&str> = url.splitn(2, '|').collect();
    let drive_char = comps[0].chars().next_back().unwrap_or('\0');
    if comps.len() != 2 || !drive_char.is_ascii_alphabetic() {
        return path.to_string();
    }
    let drive = drive_char.to_ascii_uppercase();
    let mut out = String::new();
    out.push(drive);
    out.push(':');
    for comp in comps[1].split('/') {
        if !comp.is_empty() {
            out.push('\\');
            out.push_str(&unquote(comp));
        }
    }
    out
}
