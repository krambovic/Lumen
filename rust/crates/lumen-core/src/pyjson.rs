//! Python `json.dumps` compatible serializer (separators, `ensure_ascii`,
//! `indent`, float repr). Key order follows the `serde_json::Map` order.

use serde_json::Value;
use unicode_general_category::{GeneralCategory, get_general_category};

/// `json.dumps(value, ensure_ascii=..., indent=...)`.
pub fn dumps(value: &Value, ensure_ascii: bool, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_value(&mut out, value, ensure_ascii, indent, false, 0);
    out
}

/// `json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))`.
pub fn dumps_compact_sorted_ascii(mut value: Value) -> String {
    value.sort_all_objects();
    let mut out = String::new();
    write_value(&mut out, &value, true, None, true, 0);
    out
}

fn push_unicode_escape(out: &mut String, unit: u32) {
    out.push_str(&format!("\\u{:04x}", unit));
}

pub fn write_string(out: &mut String, text: &str, ensure_ascii: bool) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => push_unicode_escape(out, c as u32),
            c if ensure_ascii && (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    push_unicode_escape(out, *unit as u32);
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python `float.__repr__`.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    let sci = format!("{:e}", value);
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-4..16).contains(&exponent) {
        let plain = format!("{}", value);
        if plain.contains('.') {
            plain
        } else {
            plain + ".0"
        }
    } else {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", mantissa, sign, exponent.abs())
    }
}

fn newline(out: &mut String, indent: usize, level: usize) {
    out.push('\n');
    out.push_str(&" ".repeat(indent * level));
}

fn write_number(out: &mut String, number: &serde_json::Number) {
    if number.is_i64() || number.is_u64() {
        out.push_str(&number.to_string());
    } else {
        out.push_str(&float_repr(number.as_f64().unwrap_or(0.0)));
    }
}

fn write_value(
    out: &mut String,
    value: &Value,
    ensure_ascii: bool,
    indent: Option<usize>,
    compact: bool,
    level: usize,
) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => write_number(out, number),
        Value::String(text) => write_string(out, text, ensure_ascii),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(if compact || indent.is_some() {
                        ","
                    } else {
                        ", "
                    });
                }
                if let Some(width) = indent {
                    newline(out, width, level + 1);
                }
                write_value(out, item, ensure_ascii, indent, compact, level + 1);
            }
            if let Some(width) = indent {
                newline(out, width, level);
            }
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(if compact || indent.is_some() {
                        ","
                    } else {
                        ", "
                    });
                }
                if let Some(width) = indent {
                    newline(out, width, level + 1);
                }
                write_string(out, key, ensure_ascii);
                out.push_str(if compact { ":" } else { ": " });
                write_value(out, item, ensure_ascii, indent, compact, level + 1);
            }
            if let Some(width) = indent {
                newline(out, width, level);
            }
            out.push('}');
        }
    }
}

/// Python `repr` for a JSON value in the map order available in Rust.
pub(crate) fn repr(value: &Value) -> String {
    let mut out = String::new();
    write_repr(&mut out, value);
    out
}

fn write_repr(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Number(number) => write_number(out, number),
        Value::String(text) => write_repr_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_repr(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_repr_string(out, key);
                out.push_str(": ");
                write_repr(out, item);
            }
            out.push('}');
        }
    }
}

fn write_repr_string(out: &mut String, text: &str) {
    use GeneralCategory::*;
    use std::fmt::Write;

    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote || c == '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c != ' '
                && matches!(
                    get_general_category(c),
                    Control
                        | Format
                        | Unassigned
                        | PrivateUse
                        | Surrogate
                        | LineSeparator
                        | ParagraphSeparator
                        | SpaceSeparator
                ) =>
            {
                match c as u32 {
                    unit @ 0..=0xff => write!(out, "\\x{unit:02x}"),
                    unit @ 0x100..=0xffff => write!(out, "\\u{unit:04x}"),
                    unit => write!(out, "\\U{unit:08x}"),
                }
                .expect("writing to String cannot fail");
            }
            c => out.push(c),
        }
    }
    out.push(quote);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_and_indented() {
        let value = json!({"a": [1, 2.5, "п"], "b": {}});
        assert_eq!(
            dumps(&value, true, None),
            "{\"a\": [1, 2.5, \"\\u043f\"], \"b\": {}}"
        );
        assert_eq!(
            dumps(&value, false, None),
            "{\"a\": [1, 2.5, \"п\"], \"b\": {}}"
        );
        assert_eq!(dumps(&json!([1]), true, Some(2)), "[\n  1\n]");
        assert_eq!(dumps(&json!("😀"), true, None), "\"\\ud83d\\ude00\"");
    }

    #[test]
    fn floats() {
        assert_eq!(float_repr(1.0), "1.0");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1.5e-7), "1.5e-07");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(123456.789), "123456.789");
    }
}

// ── `json.loads` error messages ─────────────────────────────────────────────
//
// Lumen shows `str(JSONDecodeError)` to users when an import fails, e.g.
// "Expecting ',' delimiter: line 1 column 294 (char 293)". serde_json words
// and positions its errors differently, so after a failed serde parse the text
// is re-scanned the way CPython 3.13's C scanner (`_json.c`) does.

/// `serde_json::from_str`, with Python's `JSONDecodeError` text on failure.
pub fn loads(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| decode_error(text).unwrap_or_else(|| error.to_string()))
}

/// The `str(JSONDecodeError)` Python raises for `json.loads(text)`, or `None`
/// when Python accepts the text.
pub fn decode_error(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() == Some(&'\u{feff}') {
        return Some(format_error(&chars, "Unexpected UTF-8 BOM (decode using utf-8-sig)", 0));
    }
    let scanner = Scanner { s: &chars, depth: 0 };
    let start = scanner.skip_ws(0);
    let error = match scanner.scan_once(start) {
        Ok(end) => {
            let end = scanner.skip_ws(end);
            if end == chars.len() {
                return None;
            }
            ScanError::Message("Extra data", end)
        }
        Err(error) => error,
    };
    Some(match error {
        ScanError::Stop(pos) => format_error(&chars, "Expecting value", pos),
        ScanError::Message(message, pos) => format_error(&chars, message, pos),
    })
}

/// `JSONDecodeError.__str__`: "%s: line %d column %d (char %d)".
fn format_error(chars: &[char], message: &str, pos: usize) -> String {
    let before = &chars[..pos.min(chars.len())];
    let line = before.iter().filter(|c| **c == '\n').count() + 1;
    let column = match before.iter().rposition(|c| *c == '\n') {
        Some(newline) => pos - newline,
        None => pos + 1,
    };
    format!("{message}: line {line} column {column} (char {pos})")
}

enum ScanError {
    /// `StopIteration(idx)`: no JSON value starts here ("Expecting value").
    Stop(usize),
    Message(&'static str, usize),
}

struct Scanner<'a> {
    s: &'a [char],
    depth: usize,
}

const MAX_DEPTH: usize = 900;

impl Scanner<'_> {
    fn at(&self, index: usize) -> Option<char> {
        self.s.get(index).copied()
    }

    fn skip_ws(&self, mut index: usize) -> usize {
        while matches!(self.at(index), Some(' ' | '\t' | '\n' | '\r')) {
            index += 1;
        }
        index
    }

    fn matches(&self, index: usize, word: &str) -> bool {
        let word: Vec<char> = word.chars().collect();
        self.s.get(index..index + word.len()) == Some(&word[..])
    }

    /// `scan_once_unicode`: the index after the value at `index`.
    fn scan_once(&self, index: usize) -> Result<usize, ScanError> {
        let length = self.s.len();
        let Some(c) = self.at(index) else {
            return Err(ScanError::Stop(index));
        };
        match c {
            '"' => return self.scan_string(index + 1),
            '{' | '[' => {
                if self.depth >= MAX_DEPTH {
                    // RecursionError; not a JSONDecodeError, keep serde's text.
                    return Err(ScanError::Message("maximum recursion depth exceeded", index));
                }
                let nested = Scanner { s: self.s, depth: self.depth + 1 };
                return if c == '{' { nested.parse_object(index + 1) } else { nested.parse_array(index + 1) };
            }
            'n' if index + 3 < length && self.matches(index, "null") => return Ok(index + 4),
            't' if index + 3 < length && self.matches(index, "true") => return Ok(index + 4),
            'f' if index + 4 < length && self.matches(index, "false") => return Ok(index + 5),
            'N' if index + 2 < length && self.matches(index, "NaN") => return Ok(index + 3),
            'I' if index + 7 < length && self.matches(index, "Infinity") => return Ok(index + 8),
            '-' if index + 8 < length && self.matches(index, "-Infinity") => return Ok(index + 9),
            _ => {}
        }
        self.match_number(index)
    }

    /// `_match_number_unicode`.
    fn match_number(&self, start: usize) -> Result<usize, ScanError> {
        let end_index = self.s.len() as isize - 1;
        let mut index = start;
        let digit = |i: usize| self.at(i).is_some_and(|c| c.is_ascii_digit());
        if self.at(index) == Some('-') {
            index += 1;
            if index as isize > end_index {
                return Err(ScanError::Stop(start));
            }
        }
        match self.at(index) {
            Some('1'..='9') => {
                index += 1;
                while digit(index) {
                    index += 1;
                }
            }
            Some('0') => index += 1,
            _ => return Err(ScanError::Stop(start)),
        }
        if (index as isize) < end_index && self.at(index) == Some('.') && digit(index + 1) {
            index += 2;
            while digit(index) {
                index += 1;
            }
        }
        if (index as isize) < end_index && matches!(self.at(index), Some('e' | 'E')) {
            let e_start = index;
            index += 1;
            if (index as isize) < end_index && matches!(self.at(index), Some('-' | '+')) {
                index += 1;
            }
            while digit(index) {
                index += 1;
            }
            if !digit(index - 1) {
                index = e_start;
            }
        }
        Ok(index)
    }

    /// `scanstring_unicode` (strict): `end` is just after the opening quote.
    fn scan_string(&self, mut end: usize) -> Result<usize, ScanError> {
        let length = self.s.len();
        let begin = end - 1;
        loop {
            let mut c = '\0';
            let mut next = end;
            while next < length {
                c = self.s[next];
                if c == '"' || c == '\\' {
                    break;
                }
                if (c as u32) <= 0x1f {
                    return Err(ScanError::Message("Invalid control character at", next));
                }
                next += 1;
            }
            if !(c == '"' || c == '\\') || next >= length {
                return Err(ScanError::Message("Unterminated string starting at", begin));
            }
            next += 1;
            if c == '"' {
                return Ok(next);
            }
            if next == length {
                return Err(ScanError::Message("Unterminated string starting at", begin));
            }
            let escape = self.s[next];
            if escape != 'u' {
                end = next + 1;
                if !matches!(escape, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') {
                    return Err(ScanError::Message("Invalid \\escape", end - 2));
                }
                continue;
            }
            next += 1;
            end = next + 4;
            if end >= length {
                return Err(ScanError::Message("Invalid \\uXXXX escape", next - 1));
            }
            let mut value = 0u32;
            while next < end {
                let Some(digit) = self.s[next].to_digit(16) else {
                    return Err(ScanError::Message("Invalid \\uXXXX escape", end - 5));
                };
                value = (value << 4) | digit;
                next += 1;
            }
            if (0xd800..=0xdbff).contains(&value)
                && end + 6 < length
                && self.s[next] == '\\'
                && self.s[next + 1] == 'u'
            {
                next += 2;
                end += 6;
                while next < end {
                    if self.s[next].to_digit(16).is_none() {
                        return Err(ScanError::Message("Invalid \\uXXXX escape", end - 5));
                    }
                    next += 1;
                }
            }
        }
    }

    /// `_parse_object_unicode`: `index` is just after `{`.
    fn parse_object(&self, index: usize) -> Result<usize, ScanError> {
        let mut index = self.skip_ws(index);
        if self.at(index) != Some('}') {
            loop {
                if self.at(index) != Some('"') {
                    return Err(ScanError::Message("Expecting property name enclosed in double quotes", index));
                }
                index = self.skip_ws(self.scan_string(index + 1)?);
                if self.at(index) != Some(':') {
                    return Err(ScanError::Message("Expecting ':' delimiter", index));
                }
                index = self.skip_ws(index + 1);
                index = self.skip_ws(self.scan_once(index)?);
                if self.at(index) == Some('}') {
                    break;
                }
                if self.at(index) != Some(',') {
                    return Err(ScanError::Message("Expecting ',' delimiter", index));
                }
                let comma = index;
                index = self.skip_ws(index + 1);
                if self.at(index) == Some('}') {
                    return Err(ScanError::Message("Illegal trailing comma before end of object", comma));
                }
            }
        }
        Ok(index + 1)
    }

    /// `_parse_array_unicode`: `index` is just after `[`.
    fn parse_array(&self, index: usize) -> Result<usize, ScanError> {
        let mut index = self.skip_ws(index);
        if self.at(index) != Some(']') {
            loop {
                index = self.skip_ws(self.scan_once(index)?);
                if self.at(index) == Some(']') {
                    break;
                }
                if self.at(index) != Some(',') {
                    return Err(ScanError::Message("Expecting ',' delimiter", index));
                }
                let comma = index;
                index = self.skip_ws(index + 1);
                if self.at(index) == Some(']') {
                    return Err(ScanError::Message("Illegal trailing comma before end of array", comma));
                }
            }
        }
        Ok(index + 1)
    }
}

#[cfg(test)]
mod decode_error_tests {
    use super::*;

    #[test]
    fn messages_match_cpython() {
        // Expected strings were produced by CPython 3.13 `json.loads`.
        let cases = [
            ("", "Expecting value: line 1 column 1 (char 0)"),
            ("{not json", "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"),
            ("[1, 2", "Expecting ',' delimiter: line 1 column 6 (char 5)"),
            ("[1, 2,]", "Illegal trailing comma before end of array: line 1 column 6 (char 5)"),
            ("{\"a\": 1,}", "Illegal trailing comma before end of object: line 1 column 8 (char 7)"),
            ("{\"a\" 1}", "Expecting ':' delimiter: line 1 column 6 (char 5)"),
            ("{\"a\": }", "Expecting value: line 1 column 7 (char 6)"),
            ("\"abc", "Unterminated string starting at: line 1 column 1 (char 0)"),
            ("\"a\tb\"", "Invalid control character at: line 1 column 3 (char 2)"),
            ("\"\\x\"", "Invalid \\escape: line 1 column 2 (char 1)"),
            ("\"\\u12g4\"", "Invalid \\uXXXX escape: line 1 column 3 (char 2)"),
            ("{}\n x", "Extra data: line 2 column 2 (char 4)"),
            ("\u{feff}{}", "Unexpected UTF-8 BOM (decode using utf-8-sig): line 1 column 1 (char 0)"),
            ("[-]", "Expecting value: line 1 column 2 (char 1)"),
            ("nul", "Expecting value: line 1 column 1 (char 0)"),
            ("1E", "Extra data: line 1 column 2 (char 1)"),
            ("{\"ключ\": [1,\n 2 3]}", "Expecting ',' delimiter: line 2 column 4 (char 16)"),
        ];
        for (text, expected) in cases {
            assert_eq!(decode_error(text).as_deref(), Some(expected), "{text:?}");
        }
        for valid in ["{}", "[]", " 1 ", "1e5", "-0.5", "\"\\ud83d\\ude00\"", "NaN", "[Infinity, -Infinity]"] {
            assert_eq!(decode_error(valid), None, "{valid:?}");
        }
    }
}

/// `JSONDecodeError.msg` for `json.loads(text)`: the message without the
/// position suffix, or `None` when Python accepts the text.
pub fn decode_error_msg(text: &str) -> Option<String> {
    decode_error(text).map(|message| match message.rfind(": line ") {
        Some(index) => message[..index].to_string(),
        None => message,
    })
}
