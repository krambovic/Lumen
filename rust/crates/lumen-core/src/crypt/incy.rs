//! Native decryption for Incy `incy://crypt1/` subscription links.
//!
//! The wire format is URL-safe base64 containing `nonce || ciphertext || tag`.
//! The authenticated plaintext is a UTF-8 JSON object with a non-empty `url`
//! field. Only the two 32-byte key-material slices used by the format are kept in
//! the application, so portable builds need no key file or JavaScript runtime.

use std::fmt::Write as _;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::{alphabet, Engine as _};
use sha2::{Digest, Sha256};

pub const INCY_SCHEME: &str = "incy://";
pub const INCY_CRYPT1_PREFIX: &str = "incy://crypt1/";

const KEYMAT_A_SLICE_B64: &str = "7odqBjr3BNe0CfGRDZcxzBQZCB7AiZOgEnBVaHPh7y0=";
const KEYMAT_B_SLICE_B64: &str = "z1EN9DsquX6phHju5fGxz4DjpIT8k2kxbM2H5Ut5l7c=";
const KEY_SALT: &[u8] = b"incydeepcrypt1v2026.06";
const KEY_FINGERPRINT: &str =
    "b6bf708471cc90043232967660aade86a50b4e57929db2e53c5fa34db624c08c";
const NONCE_SIZE: usize = 12;
const TAG_SIZE: usize = 16;
const MAX_WIRE_SIZE: usize = 1024 * 1024;

/// The Incy link is malformed, corrupted, or cannot be authenticated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct IncyDecryptError {
    pub message: String,
}

impl IncyDecryptError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Python `_decode_payload` использует строгий url-safe base64 (validate=True).
const URL_SAFE_ENGINE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

pub fn is_incy_link(text: &str) -> bool {
    text.trim().to_lowercase().starts_with(INCY_SCHEME)
}

pub fn is_incy_crypt_link(text: &str) -> bool {
    let body = text.trim();
    let head: String = body
        .chars()
        .take(INCY_CRYPT1_PREFIX.len())
        .collect::<String>()
        .to_lowercase();
    head == INCY_CRYPT1_PREFIX
}

/// Постоянное по времени сравнение байтов (аналог `hmac.compare_digest`).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn derive_key() -> Result<[u8; 32], IncyDecryptError> {
    let keymat_a = STANDARD_B64
        .decode(KEYMAT_A_SLICE_B64)
        .map_err(|err| IncyDecryptError::new(format!("Incy key material decode failed: {err}")))?;
    let keymat_b = STANDARD_B64
        .decode(KEYMAT_B_SLICE_B64)
        .map_err(|err| IncyDecryptError::new(format!("Incy key material decode failed: {err}")))?;
    let seed: Vec<u8> = KEY_SALT
        .iter()
        .chain(keymat_a.iter())
        .chain(keymat_b.iter())
        .copied()
        .collect();
    let key: [u8; 32] = Sha256::digest(&seed).into();
    let fingerprint = to_hex(&Sha256::digest(key));
    if !ct_eq(fingerprint.as_bytes(), KEY_FINGERPRINT.as_bytes()) {
        return Err(IncyDecryptError::new("Incy key fingerprint mismatch"));
    }
    Ok(key)
}

/// Стандартный base64 для констант ключевого материала.
const STANDARD_B64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

fn decode_payload(payload: &str) -> Result<Vec<u8>, IncyDecryptError> {
    let mut compact_len = 0usize;
    let compact: String = payload
        .chars()
        .filter(|ch| {
            let keep = !ch.is_whitespace();
            if keep {
                compact_len += 1;
            }
            keep
        })
        .collect();
    // rstrip("/") после удаления пробелов.
    let trimmed = compact.trim_end_matches('/');
    compact_len -= compact.len() - trimmed.len();
    let compact = trimmed;
    if compact.is_empty() {
        return Err(IncyDecryptError::new("empty incy://crypt1 payload"));
    }
    if compact_len > (MAX_WIRE_SIZE * 4 / 3 + 8) {
        return Err(IncyDecryptError::new("incy://crypt1 payload is too large"));
    }
    let mut padded = compact.to_string();
    for _ in 0..(4 - compact_len % 4) % 4 {
        padded.push('=');
    }
    URL_SAFE_ENGINE
        .decode(padded.as_bytes())
        .map_err(|_| IncyDecryptError::new("invalid incy://crypt1 base64 payload"))
}

/// Decrypt an `incy://crypt1/` link and return its wrapped URL.
pub fn decrypt_incy_link(link: &str) -> Result<String, IncyDecryptError> {
    let body = link.trim();
    if !is_incy_crypt_link(body) {
        return Err(IncyDecryptError::new(
            "unsupported link (expected incy://crypt1/)",
        ));
    }

    let payload = &body[INCY_CRYPT1_PREFIX.len()..];
    let wire = decode_payload(payload)?;
    if wire.len() < NONCE_SIZE + TAG_SIZE + 1 {
        return Err(IncyDecryptError::new("incy://crypt1 payload is too short"));
    }
    let key = derive_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).expect("key is 32 bytes");
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&wire[..NONCE_SIZE]), &wire[NONCE_SIZE..])
        .map_err(|_| IncyDecryptError::new("incy://crypt1 authentication failed"))?;

    let text = String::from_utf8(plaintext)
        .map_err(|_| IncyDecryptError::new("incy://crypt1 plaintext is not valid JSON"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| IncyDecryptError::new("incy://crypt1 plaintext is not valid JSON"))?;
    let Some(object) = data.as_object() else {
        return Err(IncyDecryptError::new(
            "incy://crypt1 plaintext must be a JSON object",
        ));
    };
    let url = object
        .get("url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            IncyDecryptError::new("incy://crypt1 JSON has no non-empty 'url' field")
        })?;
    if url.trim().is_empty() {
        return Err(IncyDecryptError::new(
            "incy://crypt1 JSON has no non-empty 'url' field",
        ));
    }
    Ok(url.trim().to_string())
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;

    #[test]
    fn test_incy_link_detection() {
        assert!(is_incy_link("incy://anything"));
        assert!(is_incy_link("  INCY://crypt1/x "));
        assert!(!is_incy_link("https://example.com"));
        assert!(!is_incy_link(""));
        assert!(!is_incy_link("incy:/x"));

        assert!(is_incy_crypt_link("incy://crypt1/abc"));
        assert!(is_incy_crypt_link(" INCY://CRYPT1/abc "));
        assert!(!is_incy_crypt_link("incy://crypt2/abc"));
        assert!(!is_incy_crypt_link("incy://crypt"));
        assert!(!is_incy_crypt_link("incy://"));
    }

    /// Ключевой материал даёт заранее известный SHA-256 отпечаток.
    #[test]
    fn test_incy_key_fingerprint() {
        let key = derive_key().expect("derive_key");
        assert_eq!(key.len(), 32);
        assert_eq!(
            to_hex(&Sha256::digest(key)),
            KEY_FINGERPRINT,
        );
    }

    #[test]
    fn test_decrypt_incy_roundtrip() {
        let key = derive_key().unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce = [0u8; NONCE_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        // url с ведущим/хвостовым пробелом — проверяем strip() в ответе.
        let plaintext = r#"{"url": " https://example.com/sub?x=1 "}"#;
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .unwrap();

        let mut wire = Vec::with_capacity(NONCE_SIZE + encrypted.len());
        wire.extend_from_slice(&nonce);
        wire.extend_from_slice(&encrypted);

        let link = format!("incy://crypt1/{}", URL_SAFE_ENGINE.encode(&wire));
        let url = decrypt_incy_link(&link).unwrap();
        assert_eq!(url, "https://example.com/sub?x=1");

        // Вариант с паддингом и стандартными завершающими '/' после тела.
        let padded = format!(
            "incy://crypt1/{}///",
            URL_SAFE_ENGINE.encode(&wire)
        );
        assert_eq!(decrypt_incy_link(&padded).unwrap(), url);
    }

    #[test]
    fn test_incy_errors() {
        assert_eq!(
            decrypt_incy_link("incy://crypt2/abc").unwrap_err().message,
            "unsupported link (expected incy://crypt1/)"
        );
        assert_eq!(
            decrypt_incy_link("incy://crypt1/").unwrap_err().message,
            "empty incy://crypt1 payload"
        );
        assert_eq!(
            decrypt_incy_link("incy://crypt1/abc+def").unwrap_err().message,
            "invalid incy://crypt1 base64 payload"
        );
        assert_eq!(
            decrypt_incy_link("incy://crypt1/AAAA").unwrap_err().message,
            "incy://crypt1 payload is too short"
        );
        // Слишком большой payload: размер проверяется до декодирования base64.
        let huge = format!("incy://crypt1/{}", "A".repeat(MAX_WIRE_SIZE * 4 / 3 + 9));
        assert_eq!(
            decrypt_incy_link(&huge).unwrap_err().message,
            "incy://crypt1 payload is too large"
        );
    }

    #[test]
    fn test_incy_authentication_and_json_errors() {
        let key = derive_key().unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = [7u8; NONCE_SIZE];

        let build_link = |payload: &str| -> String {
            let encrypted = cipher
                .encrypt(Nonce::from_slice(&nonce), payload.as_bytes())
                .unwrap();
            let mut wire = Vec::new();
            wire.extend_from_slice(&nonce);
            wire.extend_from_slice(&encrypted);
            format!("incy://crypt1/{}", URL_SAFE_ENGINE.encode(&wire))
        };

        // Испорченный тег → ошибка аутентификации. Меняем байт wire-тега ДО
        // кодирования: правка последнего символа ссылки попала бы в паддинг-кваду.
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                br#"{"url": "https://example.com"}"#.as_slice(),
            )
            .unwrap();
        let mut wire = Vec::new();
        wire.extend_from_slice(&nonce);
        wire.extend_from_slice(&encrypted);
        let last = wire.len() - 1;
        wire[last] ^= 0x01;
        let tampered = format!("incy://crypt1/{}", URL_SAFE_ENGINE.encode(&wire));
        assert_eq!(
            decrypt_incy_link(&tampered).unwrap_err().message,
            "incy://crypt1 authentication failed"
        );

        // Не JSON / не объект / пустой url.
        assert_eq!(
            decrypt_incy_link(&build_link("not json")).unwrap_err().message,
            "incy://crypt1 plaintext is not valid JSON"
        );
        assert_eq!(
            decrypt_incy_link(&build_link(r#""abc""#))
                .unwrap_err()
                .message,
            "incy://crypt1 plaintext must be a JSON object"
        );
        assert_eq!(
            decrypt_incy_link(&build_link(r#"{"url": 42}"#))
                .unwrap_err()
                .message,
            "incy://crypt1 JSON has no non-empty 'url' field"
        );
        assert_eq!(
            decrypt_incy_link(&build_link(r#"{"url": "   "}"#))
                .unwrap_err()
                .message,
            "incy://crypt1 JSON has no non-empty 'url' field"
        );
        assert_eq!(
            decrypt_incy_link(&build_link(r#"{"other": "x"}"#))
                .unwrap_err()
                .message,
            "incy://crypt1 JSON has no non-empty 'url' field"
        );
    }
}
