//! Python `json.dumps` compatible serializer (separators, `ensure_ascii`,
//! `indent`, float repr). Key order follows the `serde_json::Map` order.

use serde_json::Value;

/// `json.dumps(value, ensure_ascii=..., indent=...)`.
pub fn dumps(value: &Value, ensure_ascii: bool, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_value(&mut out, value, ensure_ascii, indent, 0);
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
        return if value > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let sci = format!("{:e}", value);
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-4..16).contains(&exponent) {
        let plain = format!("{}", value);
        if plain.contains('.') { plain } else { plain + ".0" }
    } else {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", mantissa, sign, exponent.abs())
    }
}

fn newline(out: &mut String, indent: usize, level: usize) {
    out.push('\n');
    out.push_str(&" ".repeat(indent * level));
}

fn write_value(out: &mut String, value: &Value, ensure_ascii: bool, indent: Option<usize>, level: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                out.push_str(&number.to_string());
            } else {
                out.push_str(&float_repr(number.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(text) => write_string(out, text, ensure_ascii),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(if indent.is_some() { "," } else { ", " });
                }
                if let Some(width) = indent {
                    newline(out, width, level + 1);
                }
                write_value(out, item, ensure_ascii, indent, level + 1);
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
                    out.push_str(if indent.is_some() { "," } else { ", " });
                }
                if let Some(width) = indent {
                    newline(out, width, level + 1);
                }
                write_string(out, key, ensure_ascii);
                out.push_str(": ");
                write_value(out, item, ensure_ascii, indent, level + 1);
            }
            if let Some(width) = indent {
                newline(out, width, level);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_and_indented() {
        let value = json!({"a": [1, 2.5, "п"], "b": {}});
        assert_eq!(dumps(&value, true, None), "{\"a\": [1, 2.5, \"\\u043f\"], \"b\": {}}");
        assert_eq!(dumps(&value, false, None), "{\"a\": [1, 2.5, \"п\"], \"b\": {}}");
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
