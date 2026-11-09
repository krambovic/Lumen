//! Lossless repair of UTF-8 provider text exposed as single-byte HTTP headers.
//!
//! Port of `windows/xray_fluent/subscription_text.py`. Also hosts the shared
//! Python-value helpers (`py_str`, `py_truthy`, `py_or_str`) used by the other
//! ported modules so the JSON <-> Python semantics stay in one place.

use serde_json::Value;

/// Python `str()` over a decoded JSON value.
///
/// Scalars follow Python reprs (`True`, `None`, `1.0`); lists/objects fall back
/// to compact JSON (Python would render a `repr`, deviation is cosmetic).
pub(crate) fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                return int.to_string();
            }
            if let Some(uint) = number.as_u64() {
                return uint.to_string();
            }
            let float = number.as_f64().unwrap_or(0.0);
            if !float.is_finite() {
                return if float.is_nan() {
                    "nan".into()
                } else if float > 0.0 {
                    "inf".into()
                } else {
                    "-inf".into()
                };
            }
            // Python repr keeps a decimal point on integral floats (2.0 -> "2.0").
            if float.fract() == 0.0 && float.abs() < 1e16 {
                return format!("{float:.1}");
            }
            format!("{float}")
        }
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Python truthiness over a decoded JSON value.
pub(crate) fn py_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int != 0
            } else if let Some(uint) = number.as_u64() {
                uint != 0
            } else {
                number.as_f64().map(|float| float != 0.0).unwrap_or(false)
            }
        }
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Python `str(a or b or ... or "")` over an ordered list of decoded values.
pub(crate) fn py_or_str(values: &[Option<&Value>]) -> String {
    for value in values.iter().flatten() {
        if py_truthy(value) {
            return py_str(value);
        }
    }
    String::new()
}

/// Single-byte encodings tried when repairing mojibake, in Python codec order.
#[derive(Copy, Clone, PartialEq, Eq)]
enum HeaderEncoding {
    Latin1,
    Cp1252,
}

/// cp1252 reverse table: Unicode char -> byte, for bytes 0x80..=0x9F only.
/// The five undefined bytes (0x81, 0x8D, 0x8F, 0x90, 0x9D) are absent, which is
/// where Python's cp1252 encoder raises UnicodeEncodeError.
const CP1252_ENCODE: &[(u32, u8)] = &[
    (0x20AC, 0x80), // EURO SIGN
    (0x201A, 0x82), // SINGLE LOW-9 QUOTATION MARK
    (0x0192, 0x83), // LATIN SMALL LETTER F WITH HOOK
    (0x201E, 0x84), // DOUBLE LOW-9 QUOTATION MARK
    (0x2026, 0x85), // HORIZONTAL ELLIPSIS
    (0x2020, 0x86), // DAGGER
    (0x2021, 0x87), // DOUBLE DAGGER
    (0x02C6, 0x88), // MODIFIER LETTER CIRCUMFLEX ACCENT
    (0x2030, 0x89), // PER MILLE SIGN
    (0x0160, 0x8A), // LATIN CAPITAL LETTER S WITH CARON
    (0x2039, 0x8B), // SINGLE LEFT-POINTING ANGLE QUOTATION MARK
    (0x0152, 0x8C), // LATIN CAPITAL LIGATURE OE
    (0x017D, 0x8E), // LATIN CAPITAL LETTER Z WITH CARON
    (0x2018, 0x91), // LEFT SINGLE QUOTATION MARK
    (0x2019, 0x92), // RIGHT SINGLE QUOTATION MARK
    (0x201C, 0x93), // LEFT DOUBLE QUOTATION MARK
    (0x201D, 0x94), // RIGHT DOUBLE QUOTATION MARK
    (0x2022, 0x95), // BULLET
    (0x2013, 0x96), // EN DASH
    (0x2014, 0x97), // EM DASH
    (0x02DC, 0x98), // SMALL TILDE
    (0x2122, 0x99), // TRADE MARK SIGN
    (0x0161, 0x9A), // LATIN SMALL LETTER S WITH CARON
    (0x203A, 0x9B), // SINGLE RIGHT-POINTING ANGLE QUOTATION MARK
    (0x0153, 0x9C), // LATIN SMALL LIGATURE OE
    (0x017E, 0x9E), // LATIN SMALL LETTER Z WITH CARON
    (0x0178, 0x9F), // LATIN CAPITAL LETTER Y WITH DIAERESIS
];

/// Encode text to `encoding` bytes the way Python's strict codec does.
/// Returns `None` where Python would raise UnicodeEncodeError.
fn encode_single_byte(text: &str, encoding: HeaderEncoding) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        match encoding {
            HeaderEncoding::Latin1 => {
                if code > 0xFF {
                    return None;
                }
                out.push(code as u8);
            }
            HeaderEncoding::Cp1252 => {
                if code <= 0x7F || (0xA0..=0xFF).contains(&code) {
                    out.push(code as u8);
                } else if let Some(&(_, byte)) =
                    CP1252_ENCODE.iter().find(|(mapped, _)| *mapped == code)
                {
                    out.push(byte);
                } else {
                    // C1 controls and non-Latin characters are undefined here.
                    return None;
                }
            }
        }
    }
    Some(out)
}

/// Reverse Latin-1/Windows-1252 mojibake only when strict UTF-8 decoding succeeds.
///
/// Python's HTTP parser maps header bytes to Latin-1. Some panels send raw
/// UTF-8 instead of base64, and cached values may have passed through a second
/// single-byte conversion. Never replace undecodable bytes or guess CP1251.
/// Repair before strip(): UTF-8 continuation bytes can look like whitespace.
pub fn repair_utf8_header(value: &str) -> String {
    // Python len() counts code points, not bytes.
    if value.chars().count() > 64 * 1024 {
        return value.to_string();
    }
    let mut current = value.to_string();
    for _ in 0..2 {
        let mut replaced = false;
        for encoding in [HeaderEncoding::Latin1, HeaderEncoding::Cp1252] {
            let Some(bytes) = encode_single_byte(&current, encoding) else {
                continue;
            };
            if let Ok(repaired) = std::str::from_utf8(&bytes) {
                if repaired != current {
                    current = repaired.to_string();
                    replaced = true;
                    break;
                }
            }
        }
        if !replaced {
            break;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interpret each UTF-8 byte of `text` as a Latin-1 character, the way
    /// Python's http.client exposes raw header bytes.
    fn as_latin1_chars(text: &str) -> String {
        text.bytes().map(|byte| byte as char).collect()
    }

    #[test]
    fn repairs_single_pass_mojibake() {
        let original = "Подписка истекла";
        assert_eq!(repair_utf8_header(&as_latin1_chars(original)), original);
    }

    #[test]
    fn repairs_double_encoded_mojibake() {
        let original = "Лимит устройств";
        let once = as_latin1_chars(original);
        let twice = as_latin1_chars(&once);
        assert_eq!(repair_utf8_header(&twice), original);
    }

    #[test]
    fn repairs_cp1252_only_characters() {
        // Bytes E2 80 85 read as chars: 'a-umlaut', euro, ellipsis. Latin-1
        // cannot encode the euro sign, so the cp1252 pass must handle it.
        let mojibake = "\u{00E2}\u{20AC}\u{2026}";
        assert_eq!(repair_utf8_header(mojibake), "\u{2005}");
    }

    #[test]
    fn keeps_unrepairable_text() {
        assert_eq!(repair_utf8_header("plain ascii"), "plain ascii");
        assert_eq!(repair_utf8_header("café"), "café");
        // U+0081 is a C1 control with no cp1252 encoding and produces invalid
        // UTF-8 from latin-1; both passes fail so the value stays as-is.
        assert_eq!(repair_utf8_header("\u{0081}"), "\u{0081}");
    }

    #[test]
    fn skips_over_long_values() {
        let long = "\u{00D0}\u{00B0}".repeat(64 * 1024); // 128 Ki chars of mojibake
        assert_eq!(repair_utf8_header(&long), long);
    }
}
