//! Python `base64` decoding semantics (`binascii.a2b_base64`).
//!
//! Python's non-validating decoders skip characters outside the alphabet and
//! stop at a complete padding run; the base64 crate rejects such input. Code
//! that mirrors a Python `b64decode`/`urlsafe_b64decode` call uses these to
//! accept and reject exactly what Python does, with CPython's messages.

/// CPython `binascii.a2b_base64(data, strict_mode=False)`, behind
/// `base64.b64decode` (and `urlsafe_b64decode` when `urlsafe`, which maps
/// `-`/`_` to `+`/`/` first). Characters outside the alphabet are skipped;
/// a complete `==`/`=` pad ends the data. The error texts are CPython's.
pub fn py_a2b_base64(data: &[u8], urlsafe: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    let mut quad_pos = 0usize;
    let mut pads = 0usize;
    let mut left = 0u32;
    let mut data_chars = 0usize;
    for &byte in data {
        let byte = match (urlsafe, byte) {
            (true, b'-') => b'+',
            (true, b'_') => b'/',
            _ => byte,
        };
        if byte == b'=' {
            pads += 1;
            if quad_pos >= 2 && quad_pos + pads >= 4 {
                return Ok(out);
            }
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        pads = 0;
        data_chars += 1;
        match quad_pos {
            0 => {
                quad_pos = 1;
                left = u32::from(value);
            }
            1 => {
                quad_pos = 2;
                out.push(((left << 2) | (u32::from(value) >> 4)) as u8);
                left = u32::from(value) & 0x0f;
            }
            2 => {
                quad_pos = 3;
                out.push(((left << 4) | (u32::from(value) >> 2)) as u8);
                left = u32::from(value) & 0x03;
            }
            _ => {
                quad_pos = 0;
                out.push(((left << 6) | u32::from(value)) as u8);
                left = 0;
            }
        }
    }
    match quad_pos {
        0 => Ok(out),
        1 => Err(format!(
            "Invalid base64-encoded string: number of data characters ({data_chars}) cannot be 1 more than a multiple of 4"
        )),
        _ => Err("Incorrect padding".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::py_a2b_base64;

    #[test]
    fn matches_cpython() {
        // base64.b64decode / urlsafe_b64decode results from CPython 3.13.
        assert_eq!(py_a2b_base64(b"aGk=", false).unwrap(), b"hi");
        assert_eq!(py_a2b_base64(b"aG!k=", false).unwrap(), b"hi");
        assert_eq!(py_a2b_base64(b"AA:BB===", true).unwrap(), vec![0, 0, 0x41]);
        assert_eq!(py_a2b_base64(b"_-8=", true).unwrap(), vec![0xff, 0xef]);
        assert_eq!(py_a2b_base64(b"aGk", false).unwrap_err(), "Incorrect padding");
        assert_eq!(
            py_a2b_base64(b"notbase64===", false).unwrap_err(),
            "Invalid base64-encoded string: number of data characters (9) cannot be 1 more than a multiple of 4"
        );
        assert_eq!(py_a2b_base64(b"aGk=trailing", false).unwrap(), b"hi");
    }
}
