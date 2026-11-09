//! Расшифровка закрытых ссылок Happ (`happ://crypt`, `crypt2..crypt5`, `crypt5.1`).
//!
//! Happ умеет делиться подписками в «зашифрованном» виде, чтобы адрес подписки не
//! был виден в самой ссылке. Формат восстановлен сообществом (ключи Happ извлечены
//! из APK и опубликованы в проектах-дешифраторах под открытыми лицензиями). Схема:
//!
//! * `crypt`..`crypt4` — тело в base64, расшифровывается RSA-PKCS1v15 одним из
//!   четырёх приватных ключей (по одному на каждый режим), блоками по размеру ключа.
//! * `crypt5` (включая salted-вариант) — гибрид: строковые перестановки,
//!   RSA-PKCS1v15 (ключ выбирается по 8-символьному «маркеру») отдаёт ключ
//!   ChaCha20-Poly1305, которым расшифровывается тело.
//! * `crypt5.1` — тот же гибрид с изменённой раскладкой полей и расширенным
//!   набором ключей; нужный ключ подбирается перебором.
//!
//! Результат расшифровки — обычный текст: либо URL подписки (`https://...`),
//! либо готовый список ссылок (`vless://...` и т.п.). Дальше он уходит в тот же
//! пайплайн импорта подписок, что и любой другой ответ провайдера.
//!
//! Так как схема опирается на приватные ключи, извлечённые из Happ, публично
//! доступен только их ограниченный набор. Для ссылок `crypt5.1` с ключом, которого
//! нет в наборе, функция возвращает [`HappDecryptError`] в варианте
//! key-unavailable (аналог [`HappKeyUnavailableError`]) с понятным сообщением.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Mutex, OnceLock};

use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::{alphabet, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

use super::happ_keys::{crypt5_key, CRYPT5_KEYS_B64, PKCS1_KEYS_B64};

pub const HAPP_SCHEME: &str = "happ://";

/// Порядок сопоставления префиксов повторяет `_CRYPT_PREFIXES` в Python.
const CRYPT_PREFIXES: [(&str, usize); 5] = [
    ("crypt5/", 4),
    ("crypt4/", 3),
    ("crypt3/", 2),
    ("crypt2/", 1),
    ("crypt/", 0),
];

// ─────────────────────────────────────────────────────────────────────────────
//  Ошибки
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HappErrorKind {
    Decrypt,
    KeyUnavailable,
}

/// Не удалось расшифровать ссылку Happ (битый формат, неизвестная схема).
///
/// Python-аналог — `HappDecryptError`; для ошибки отсутствия ключа (аналог
/// `HappKeyUnavailableError`) см. [`HappDecryptError::is_key_unavailable`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HappDecryptError {
    pub message: String,
    kind: HappErrorKind,
}

/// Нет приватного ключа под данную ссылку (например, формат `crypt5.1`).
///
/// Обёртка над [`HappDecryptError`], повторяющая наследование
/// `HappKeyUnavailableError(HappDecryptError)` в Python.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct HappKeyUnavailableError(#[from] pub HappDecryptError);

impl HappDecryptError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: HappErrorKind::Decrypt,
        }
    }

    fn key_unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: HappErrorKind::KeyUnavailable,
        }
    }

    /// Python: `isinstance(exc, HappKeyUnavailableError)`.
    pub fn is_key_unavailable(&self) -> bool {
        self.kind == HappErrorKind::KeyUnavailable
    }

    /// Возвращает обёртку `HappKeyUnavailableError`, если это ошибка отсутствия ключа.
    pub fn into_key_unavailable(self) -> Option<HappKeyUnavailableError> {
        if self.kind == HappErrorKind::KeyUnavailable {
            Some(HappKeyUnavailableError(self))
        } else {
            None
        }
    }
}

/// Внутренняя ошибка шага: `Happ` уже финальная (пробрасывается как есть,
/// как `except HappDecryptError: raise`), `Other` оборачивается в общее
/// сообщение на выходе (как `except Exception as exc`).
#[derive(Debug)]
enum StepErr {
    Happ(HappDecryptError),
    Other(String),
}

impl fmt::Display for StepErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepErr::Happ(err) => write!(f, "{err}"),
            StepErr::Other(msg) => write!(f, "{msg}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Определение ссылок
// ─────────────────────────────────────────────────────────────────────────────

pub fn is_happ_link(text: &str) -> bool {
    text.trim().to_lowercase().starts_with(HAPP_SCHEME)
}

/// Возвращает `(ordinal, payload)` для happ://crypt*-ссылки, иначе `None`.
///
/// Схема (`happ://`) и токен режима (`cryptN/`) сопоставляются без учёта
/// регистра; тело подписки сохраняется как есть.
fn split_crypt_prefix(text: &str) -> Option<(usize, String)> {
    let body = text.trim();
    let scheme_len = HAPP_SCHEME.len();
    let head: String = body.chars().take(scheme_len).collect();
    if head.to_lowercase() != HAPP_SCHEME {
        return None;
    }
    let path: Vec<char> = body.chars().skip(scheme_len).collect();
    for (prefix, ordinal) in CRYPT_PREFIXES {
        let prefix_len = prefix.chars().count();
        let head: String = path
            .iter()
            .take(prefix_len)
            .collect::<String>()
            .to_lowercase();
        if head == prefix {
            return Some((ordinal, path[prefix_len..].iter().collect()));
        }
    }
    None
}

pub fn is_happ_crypt_link(text: &str) -> bool {
    split_crypt_prefix(text).is_some()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Строковые / байтовые помощники
// ─────────────────────────────────────────────────────────────────────────────

/// base64, терпимый к url-safe алфавиту и отсутствию паддинга (Python `_b64decode`).
const B64_ENGINE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Python `_b64decode`: url-safe алфавит, удаление пробелов, восстановление паддинга.
fn b64decode(text: &str) -> Result<Vec<u8>, String> {
    let mut compact = String::with_capacity(text.len());
    let mut len = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '-' {
            compact.push('+');
        } else if ch == '_' {
            compact.push('/');
        } else {
            compact.push(ch);
        }
        len += 1;
    }
    for _ in 0..(4 - len % 4) % 4 {
        compact.push('=');
    }
    B64_ENGINE
        .decode(compact.as_bytes())
        .map_err(|err| format!("invalid base64: {err}"))
}

/// Python `str.encode("ascii")` — `None` при не-ASCII символах (ошибка кодирования).
fn ascii_str(text: &str) -> Option<Vec<u8>> {
    if text.is_ascii() {
        Some(text.as_bytes().to_vec())
    } else {
        None
    }
}

/// Python `bytes.decode("latin-1")` — каждый байт становится символом U+0000..U+00FF.
fn latin1_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| byte as char).collect()
}

/// Меняет местами соседние символы попарно: `ABCD` → `BADC`.
fn swap_pairs(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index + 1 < chars.len() {
        chars.swap(index, index + 1);
        index += 2;
    }
    chars.into_iter().collect()
}

/// Каждый полный 4-символьный блок `ABCD` → `CDAB`; хвост — как есть.
fn block_pair_swap(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let full = chars.len() - chars.len() % 4;
    let mut out = Vec::with_capacity(chars.len());
    let mut offset = 0;
    while offset < full {
        out.extend_from_slice(&chars[offset + 2..offset + 4]);
        out.extend_from_slice(&chars[offset..offset + 2]);
        offset += 4;
    }
    out.extend_from_slice(&chars[full..]);
    out.into_iter().collect()
}

// ─────────────────────────────────────────────────────────────────────────────
//  RSA / ChaCha20-Poly1305
// ─────────────────────────────────────────────────────────────────────────────

type KeyCache = HashMap<&'static str, Result<RsaPrivateKey, String>>;

fn key_cache() -> &'static Mutex<KeyCache> {
    static CACHE: OnceLock<Mutex<KeyCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Разбор приватного ключа из base64 DER (кэшируется, как `_key_cache` в Python).
fn load_private_key(b64: &'static str) -> Result<RsaPrivateKey, String> {
    let mut cache = key_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(parsed) = cache.get(b64) {
        return parsed.clone();
    }
    let parsed = (|| -> Result<RsaPrivateKey, String> {
        let der = B64_ENGINE
            .decode(b64.as_bytes())
            .map_err(|err| format!("invalid key base64: {err}"))?;
        // crypt..crypt4 хранятся как PKCS#1, crypt5 — как PKCS#8: пробуем оба формата
        // (Python: load_der_private_key, при неудаче — оборачивание PKCS#1 в PKCS#8).
        if let Ok(key) = RsaPrivateKey::from_pkcs8_der(&der) {
            return Ok(key);
        }
        RsaPrivateKey::from_pkcs1_der(&der).map_err(|err| format!("RSA key parse failed: {err}"))
    })();
    cache.insert(b64, parsed.clone());
    parsed
}

fn rsa_decrypt(key: &RsaPrivateKey, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    key.decrypt(Pkcs1v15Encrypt, ciphertext)
        .map_err(|err| format!("RSA decrypt failed: {err}"))
}

fn chacha_decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != 12 {
        return Err(format!(
            "invalid ChaCha20-Poly1305 nonce length: {}",
            nonce.len()
        ));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|err| format!("ChaCha20-Poly1305 key: {err}"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| "ChaCha20-Poly1305 authentication failed".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
//  crypt..crypt4
// ─────────────────────────────────────────────────────────────────────────────

fn decrypt_crypt1to4(ordinal: usize, payload: &str) -> Result<String, StepErr> {
    let key = load_private_key(PKCS1_KEYS_B64[ordinal]).map_err(StepErr::Other)?;
    decrypt_crypt1to4_with_key(&key, payload)
}

/// Блочная RSA-расшифровка: шифртекст — конкатенация блоков размером ключа.
/// Выделена отдельно, чтобы тесты могли гонять путь на сгенерированном ключе.
fn decrypt_crypt1to4_with_key(key: &RsaPrivateKey, payload: &str) -> Result<String, StepErr> {
    let key_size = key.size();
    let cipher = b64decode(payload).map_err(StepErr::Other)?;
    if cipher.is_empty() || cipher.len() % key_size != 0 {
        return Err(StepErr::Happ(HappDecryptError::new(
            "crypt: длина шифртекста не кратна размеру RSA-блока",
        )));
    }
    let mut plaintext = Vec::with_capacity(cipher.len());
    for block in cipher.chunks(key_size) {
        plaintext.extend_from_slice(&rsa_decrypt(key, block).map_err(StepErr::Other)?);
    }
    String::from_utf8(plaintext)
        .map_err(|err| StepErr::Other(format!("utf-8 decode failed: {err}")))
}

// ─────────────────────────────────────────────────────────────────────────────
//  crypt5 (классический)
// ─────────────────────────────────────────────────────────────────────────────

/// Общий финал crypt5/crypt5.1: RSA → ключ ChaCha → расшифровка → URL.
fn finish_crypt5(
    key: &RsaPrivateKey,
    nonce: &[u8],
    url_b64: &str,
    enc_str: &str,
    salt: Option<&[u8]>,
) -> Result<String, StepErr> {
    let enc_cipher = b64decode(enc_str).map_err(StepErr::Other)?;
    let rsa_plain_bytes = rsa_decrypt(key, &enc_cipher).map_err(StepErr::Other)?;
    let rsa_plain = latin1_string(&rsa_plain_bytes);
    let mut chacha_key = b64decode(&swap_pairs(&rsa_plain)).map_err(StepErr::Other)?;
    if chacha_key.len() != 32 {
        return Err(StepErr::Happ(HappDecryptError::new(
            "crypt5: неверная длина ключа ChaCha20",
        )));
    }
    if let Some(salt) = salt.filter(|salt| !salt.is_empty()) {
        for (index, value) in chacha_key.iter_mut().enumerate() {
            *value ^= salt[index % salt.len()];
        }
    }
    let url_cipher = b64decode(url_b64).map_err(StepErr::Other)?;
    let intermediate_bytes = chacha_decrypt(&chacha_key, nonce, &url_cipher).map_err(StepErr::Other)?;
    let intermediate = String::from_utf8(intermediate_bytes)
        .map_err(|err| StepErr::Other(format!("utf-8 decode failed: {err}")))?;
    let final_bytes = b64decode(&swap_pairs(&intermediate)).map_err(StepErr::Other)?;
    String::from_utf8(final_bytes).map_err(|err| StepErr::Other(format!("utf-8 decode failed: {err}")))
}

/// Current crypt5 layout, including the salted crypt5.2 variant.
///
/// Возвращает `None`, если маркер неизвестен либо структура не подходит, —
/// тогда вызывающий пробует формат crypt5.1.
fn try_decrypt_crypt5_legacy(payload: &str) -> Result<Option<String>, StepErr> {
    let shuffled = block_pair_swap(payload);
    let shuffled_chars: Vec<char> = shuffled.chars().collect();
    let shuffled_len = shuffled_chars.len();
    if shuffled_len < 8 {
        return Ok(None);
    }
    let marker: String = shuffled_chars[..4]
        .iter()
        .chain(shuffled_chars[shuffled_len - 4..].iter())
        .collect();
    let Some(key_b64) = crypt5_key(&marker) else {
        return Ok(None);
    };
    let body_chars: Vec<char> = shuffled_chars[4..shuffled_len - 4].to_vec();
    let body_len = body_chars.len();
    if body_len < 13 {
        return Ok(None);
    }
    // В Python nonce.encode("ascii") находится ВНЕ try — не-ASCII уходит наружу
    // как обычная ошибка, а не как «формат не подходит».
    let nonce_slice: String = body_chars[..12].iter().collect();
    let Some(nonce) = ascii_str(&nonce_slice) else {
        return Err(StepErr::Other(
            "'ascii' codec can't encode nonce: not all characters are ASCII".to_string(),
        ));
    };
    let prefer_salted = body_len > 22 && !body_chars[12].is_ascii_digit();
    for salted in [prefer_salted, !prefer_salted] {
        let attempt = || -> Option<String> {
            let (salt, position) = if salted {
                if body_len < 22 {
                    return None;
                }
                let salt: String = body_chars[14..22].iter().collect();
                let salt = ascii_str(&salt)?;
                (Some(salt), 22usize)
            } else {
                (None, 12usize)
            };
            let start = position;
            let mut position = position;
            while position < body_len && body_chars[position].is_ascii_digit() {
                position += 1;
            }
            if position == start {
                return None;
            }
            let digits: String = body_chars[start..position].iter().collect();
            let segment_len: usize = digits.parse().ok()?;
            let packed_len = body_len - position;
            let Some(needed) = segment_len.checked_add(1) else {
                return None;
            };
            if packed_len < needed {
                return None;
            }
            let url_b64: String = body_chars[position + 1..position + 1 + segment_len]
                .iter()
                .collect();
            let enc_str: String = body_chars[position + 1 + segment_len..].iter().collect();
            let key = load_private_key(key_b64).ok()?;
            finish_crypt5(&key, &nonce, &url_b64, &enc_str, salt.as_deref()).ok()
        };
        if let Some(result) = attempt() {
            return Ok(Some(result));
        }
    }
    Ok(None)
}

// ─────────────────────────────────────────────────────────────────────────────
//  crypt5.1
// ─────────────────────────────────────────────────────────────────────────────

/// Каждый полный 4-символьный блок `ABCD` → `CDAB` в первых `length` символах региона;
/// индексы за пределами региона пропускаются (как в Python).
fn c51_block_pair_swap(region: &[char], length: usize) -> String {
    let mut out = Vec::with_capacity(length);
    for j in 1..=length {
        let block = (j - 1) / 4;
        let pos = (j - 1) % 4;
        let index = 4 * block + ((pos + 2) % 4);
        if index < region.len() {
            out.push(region[index]);
        }
    }
    out.into_iter().collect()
}

/// Python `_c51_extract_nonce`: перестановка `n[2] n[3] n[0] n[1] n[6] n[7]
/// n[4] n[5] n[10] n[11] n[8] n[9]` над `payload[4:16]`.
fn c51_extract_nonce(payload: &[char]) -> String {
    if payload.len() < 16 {
        return String::new();
    }
    let n = &payload[4..16];
    [2, 3, 0, 1, 6, 7, 4, 5, 10, 11, 8, 9]
        .iter()
        .map(|&index| n[index])
        .collect()
}

/// Python `_c51_selector`: `payload[2:4] + payload[0:2] + payload[-6:-4] + payload[-2:]`.
fn c51_selector(payload: &[char]) -> String {
    if payload.len() < 10 {
        return String::new();
    }
    let len = payload.len();
    let selector: String = payload[2..4]
        .iter()
        .chain(payload[0..2].iter())
        .chain(payload[len - 6..len - 4].iter())
        .chain(payload[len - 2..].iter())
        .collect();
    selector.to_lowercase()
}

/// Python `_c51_make_cipher_b64`: опциональный разрез по внутреннему `=`,
/// очистка крайних `=` и восстановление паддинга.
fn c51_make_cipher_b64(enc_str: &str, split_on_inner_equals: bool) -> String {
    let chars: Vec<char> = enc_str.chars().collect();
    // trailing_start = len(re.sub(r"=+$", "", enc_str))
    let trailing_start = chars
        .iter()
        .rposition(|&ch| ch != '=')
        .map_or(0, |index| index + 1);
    let eq_idx = chars.iter().position(|&ch| ch == '=');
    let cipher_b64: &[char] = match (split_on_inner_equals, eq_idx) {
        (true, Some(idx)) if idx < trailing_start => &chars[idx + 1..],
        _ => &chars[..],
    };
    // cleaned = re.sub(r"^=+", ...); cleaned = re.sub(r"=+$", ...)
    let start = cipher_b64
        .iter()
        .position(|&ch| ch != '=')
        .unwrap_or(cipher_b64.len());
    let end = cipher_b64
        .iter()
        .rposition(|&ch| ch != '=')
        .map_or(0, |index| index + 1);
    let cleaned: &[char] = if start >= end { &[] } else { &cipher_b64[start..end] };
    let mut result: String = cleaned.iter().collect();
    for _ in 0..(4 - cleaned.len() % 4) % 4 {
        result.push('=');
    }
    result
}

/// Возможные разбиения payload на (nonce, url_b64, enc_str, split).
fn c51_candidates(payload: &[char]) -> Vec<(String, String, String, bool)> {
    let nonce_str = c51_extract_nonce(payload);
    let mut candidates: Vec<(String, String, String, bool)> = Vec::new();
    let mut seen: HashSet<(usize, String, String, String, String)> = HashSet::new();

    fn push(
        candidates: &mut Vec<(String, String, String, bool)>,
        seen: &mut HashSet<(usize, String, String, String, String)>,
        nonce_str: &str,
        url_b64: String,
        enc_str: String,
        split: bool,
    ) {
        if url_b64.is_empty() || enc_str.chars().count() < 684 {
            return;
        }
        let url_chars: Vec<char> = url_b64.chars().collect();
        let enc_chars: Vec<char> = enc_str.chars().collect();
        let key = (
            url_chars.len(),
            url_chars.iter().take(16).collect::<String>(),
            url_chars[url_chars.len().saturating_sub(16)..].iter().collect::<String>(),
            enc_chars.iter().take(16).collect::<String>(),
            enc_chars[enc_chars.len().saturating_sub(16)..].iter().collect::<String>(),
        );
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        candidates.push((nonce_str.to_string(), url_b64, enc_str, split));
    }

    let payload_len = payload.len();
    // n = int(payload[18:20]) — при ошибке разбора Python даёт 0; для payload
    // короче 20 символов последующая проверка длины всё равно не пройдёт.
    let n: i64 = if payload_len >= 20 {
        let segment: String = payload[18..20].iter().collect();
        segment.trim().parse().unwrap_or(0)
    } else {
        0
    };
    if n > 0 {
        let n = n as usize;
        if payload_len >= 20 + n + 684 {
            let url_region = &payload[20..20 + n];
            let enc_region = &payload[20 + n..20 + n + 684];
            let skip = if (n - 1) % 4 == 0 {
                n - 1
            } else {
                ((n - 1) / 4) * 4 + 1
            };
            if skip < url_region.len() {
                let url_b64 = payload[17].to_string() + &c51_block_pair_swap(url_region, n - 1);
                let enc_str = url_region[skip].to_string() + &c51_block_pair_swap(enc_region, 683);
                push(
                    &mut candidates,
                    &mut seen,
                    &nonce_str,
                    url_b64,
                    enc_str,
                    true,
                );
            }
        }
    }

    for trailer_len in 4..=8 {
        let url_len = payload_len
            .checked_sub(20 + 684 + trailer_len)
            .unwrap_or(0);
        if url_len == 0 {
            continue;
        }
        let url_region = &payload[20..20 + url_len];
        let enc_region = &payload[20 + url_len..20 + url_len + 684];
        if enc_region.len() != 684 {
            continue;
        }
        let url_b64 = c51_block_pair_swap(url_region, url_len);
        let enc_str = c51_block_pair_swap(enc_region, 684);
        push(
            &mut candidates,
            &mut seen,
            &nonce_str,
            url_b64.clone(),
            enc_str.clone(),
            false,
        );
        if url_b64.ends_with('=') {
            push(
                &mut candidates,
                &mut seen,
                &nonce_str,
                format!("{}=", &url_b64[1..]),
                enc_str,
                false,
            );
        }
    }

    candidates
}

/// Ключи-кандидаты: сначала по селектору/семейству, затем все остальные.
fn candidate_keys(selector: &str) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = Vec::new();
    let mut seen: HashSet<&'static str> = HashSet::new();

    fn add_unique(
        value: Option<&'static str>,
        keys: &mut Vec<&'static str>,
        seen: &mut HashSet<&'static str>,
    ) {
        if let Some(value) = value {
            if seen.insert(value) {
                keys.push(value);
            }
        }
    }

    add_unique(crypt5_key(selector), &mut keys, &mut seen);
    // Python selector[:4] — первые 4 символа (селектор может быть не-ASCII).
    let family: String = selector.chars().take(4).collect();
    if !family.is_empty() {
        for (marker, value) in CRYPT5_KEYS_B64 {
            if marker.starts_with(&family) {
                add_unique(Some(value), &mut keys, &mut seen);
            }
        }
    }
    for (_, value) in CRYPT5_KEYS_B64 {
        add_unique(Some(value), &mut keys, &mut seen);
    }
    keys
}

fn decrypt_with_keys(
    key_candidates: &[&'static str],
    candidates: &[(String, String, String, bool)],
) -> Option<String> {
    for (nonce_str, url_b64, enc_str, split) in candidates {
        let Some(nonce) = ascii_str(nonce_str) else {
            continue;
        };
        let cipher_b64 = c51_make_cipher_b64(enc_str, *split);
        let Ok(cipher) = b64decode(&cipher_b64) else {
            continue;
        };
        for key_b64 in key_candidates {
            let Ok(key) = load_private_key(*key_b64) else {
                continue;
            };
            let Ok(rsa_plain_bytes) = rsa_decrypt(&key, &cipher) else {
                continue;
            };
            let rsa_plain = latin1_string(&rsa_plain_bytes);
            let swapped = swap_pairs(&rsa_plain);
            for shaped in [swapped.as_str(), rsa_plain.as_str()] {
                let Ok(chacha_key) = b64decode(shaped) else {
                    continue;
                };
                if chacha_key.len() != 32 {
                    continue;
                }
                let Ok(url_cipher) = b64decode(url_b64) else {
                    continue;
                };
                let Ok(intermediate_bytes) =
                    chacha_decrypt(&chacha_key, &nonce, &url_cipher)
                else {
                    continue;
                };
                let Ok(intermediate) = String::from_utf8(intermediate_bytes) else {
                    continue;
                };
                let final_swapped = swap_pairs(&intermediate);
                let Ok(final_bytes) = b64decode(&final_swapped) else {
                    continue;
                };
                let Ok(final_text) = String::from_utf8(final_bytes) else {
                    continue;
                };
                return Some(final_text);
            }
        }
    }
    None
}

fn decrypt_crypt51(payload: &str) -> Result<String, StepErr> {
    let payload_chars: Vec<char> = payload.chars().collect();
    let selector = c51_selector(&payload_chars);
    let keys = candidate_keys(&selector);
    let candidates = c51_candidates(&payload_chars);
    let family: String = selector.chars().take(4).collect();
    let preferred_values: HashSet<&'static str> = CRYPT5_KEYS_B64
        .iter()
        .filter(|(marker, _)| {
            *marker == selector.as_str()
                || (!family.is_empty() && marker.starts_with(&family))
        })
        .map(|(_, value)| *value)
        .collect();
    let preferred_keys: Vec<&'static str> = keys
        .iter()
        .copied()
        .filter(|key| preferred_values.contains(key))
        .collect();
    let fallback_keys: Vec<&'static str> = keys
        .iter()
        .copied()
        .filter(|key| !preferred_values.contains(key))
        .collect();

    for key_candidates in [&preferred_keys, &fallback_keys] {
        if let Some(decrypted) = decrypt_with_keys(key_candidates, &candidates) {
            return Ok(decrypted);
        }
    }

    Err(StepErr::Happ(HappDecryptError::key_unavailable(format!(
        "не удалось расшифровать happ://crypt5-ссылку: подходящий приватный ключ \
         отсутствует в наборе (формат crypt5.1, маркер «{selector}»)"
    ))))
}

fn decrypt_crypt5(payload: &str) -> Result<String, StepErr> {
    if let Some(legacy) = try_decrypt_crypt5_legacy(payload)? {
        return Ok(legacy);
    }
    decrypt_crypt51(payload)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Публичный API
// ─────────────────────────────────────────────────────────────────────────────

/// Расшифровывает `happ://crypt*` и возвращает URL/текст подписки.
///
/// Возвращает [`HappDecryptError`] (в т.ч. вариант key-unavailable — аналог
/// [`HappKeyUnavailableError`]), если ссылку расшифровать нельзя.
pub fn decrypt_happ_link(link: &str) -> Result<String, HappDecryptError> {
    let Some((ordinal, payload)) = split_crypt_prefix(link) else {
        return Err(HappDecryptError::new(
            "неподдерживаемая ссылка (ожидалась happ://crypt*)",
        ));
    };
    if payload.is_empty() {
        return Err(HappDecryptError::new(
            "пустое тело happ://crypt-ссылки",
        ));
    }
    let result = if ordinal == 4 {
        decrypt_crypt5(&payload)
    } else {
        decrypt_crypt1to4(ordinal, &payload)
    };
    result.map_err(|err| match err {
        StepErr::Happ(err) => err,
        StepErr::Other(message) => {
            HappDecryptError::new(format!("ошибка расшифровки happ-ссылки: {message}"))
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
//  Тесты
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};
    use rand::rngs::OsRng;
    use rand::RngCore;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

    use super::*;

    fn b64encode(data: &[u8]) -> String {
        STANDARD.encode(data)
    }

    #[test]
    fn test_is_happ_link() {
        assert!(is_happ_link("happ://anything"));
        assert!(is_happ_link("  HAPP://Crypt2/abc\n"));
        assert!(is_happ_link("happ://"));
        assert!(!is_happ_link("https://example.com"));
        assert!(!is_happ_link(""));
        assert!(!is_happ_link("happ:/x"));
        assert!(!is_happ_link("hap"));
    }

    #[test]
    fn test_is_happ_crypt_link() {
        assert!(is_happ_crypt_link("happ://crypt/abc"));
        assert!(is_happ_crypt_link("HAPP://CRYPT5/abc"));
        assert!(is_happ_crypt_link("happ://crypt2/x"));
        assert!(is_happ_crypt_link("happ://crypt3/x"));
        assert!(is_happ_crypt_link("happ://crypt4/x"));
        assert!(!is_happ_crypt_link("happ://crypt"));
        assert!(!is_happ_crypt_link("happ://cryptx/abc"));
        assert!(!is_happ_crypt_link("happ://other/abc"));
        assert!(!is_happ_crypt_link("https://example.com"));
    }

    #[test]
    fn test_split_crypt_prefix_ordinals() {
        assert_eq!(
            split_crypt_prefix("happ://crypt5/BODY"),
            Some((4, "BODY".to_string()))
        );
        assert_eq!(
            split_crypt_prefix("Happ://Crypt2/BODY"),
            Some((1, "BODY".to_string()))
        );
        assert_eq!(
            split_crypt_prefix("happ://crypt/abc"),
            Some((0, "abc".to_string()))
        );
        assert_eq!(split_crypt_prefix("happ://crypt"), None);
        // Тело сохраняется как есть (с учётом регистра).
        assert_eq!(
            split_crypt_prefix("happ://CRYPT5/AbC"),
            Some((4, "AbC".to_string()))
        );
    }

    #[test]
    fn test_swap_pairs() {
        assert_eq!(swap_pairs("ABCD"), "BADC");
        assert_eq!(swap_pairs("ABC"), "BAC");
        assert_eq!(swap_pairs(""), "");
        assert_eq!(swap_pairs("a"), "a");
        assert_eq!(swap_pairs("абвг"), "багв");
    }

    #[test]
    fn test_block_pair_swap() {
        assert_eq!(block_pair_swap("ABCDEFGH"), "CDABGHEF");
        // Хвост меньше 4 символов — как есть.
        assert_eq!(block_pair_swap("ABCDE"), "CDABE");
        assert_eq!(block_pair_swap("ABC"), "ABC");
        assert_eq!(block_pair_swap(""), "");
    }

    #[test]
    fn test_b64decode_urlsafe_and_padding() {
        assert_eq!(b64decode("YWJj").unwrap(), b"abc");
        // Отсутствие паддинга.
        assert_eq!(b64decode("YQ").unwrap(), b"a");
        assert_eq!(b64decode("YWI").unwrap(), b"ab");
        // url-safe алфавит: "-_" → "+/".
        assert_eq!(b64decode("-_8").unwrap(), vec![0xFB, 0xFF]);
        assert_eq!(b64decode("-_8=").unwrap(), vec![0xFB, 0xFF]);
        // Пробелы удаляются.
        assert_eq!(b64decode("  YW\nJj\t").unwrap(), b"abc");
        // Длина, не кратная 4 после паддинга (len % 4 == 1), — ошибка.
        assert!(b64decode("AAAAA").is_err());
    }

    #[test]
    fn test_decrypt_happ_link_errors() {
        let err = decrypt_happ_link("https://example.com").unwrap_err();
        assert_eq!(err.message, "неподдерживаемая ссылка (ожидалась happ://crypt*)");
        assert!(!err.is_key_unavailable());

        let err = decrypt_happ_link("happ://crypt5/").unwrap_err();
        assert_eq!(err.message, "пустое тело happ://crypt-ссылки");

        let err = decrypt_happ_link("happ://crypt/qqq").unwrap_err();
        assert!(err.message.starts_with("ошибка расшифровки happ-ссылки:"));
        assert!(!err.is_key_unavailable());
    }

    /// RSA round-trip на сгенерированном ключе: multi-block crypt1-раскладка.
    #[test]
    fn test_synthetic_rsa_crypt1_roundtrip() {
        let private = RsaPrivateKey::new(&mut OsRng, 1024).expect("keygen");
        let public = private.to_public_key();
        let url = "vless://uuid@example.com:443?security=tls&sni=example.com#node1";
        // Два блока PKCS1v15 (максимум 117 байт открытого текста на блок).
        let mut cipher = Vec::new();
        for chunk in url.as_bytes().chunks(117) {
            let block = public
                .encrypt(&mut OsRng, Pkcs1v15Encrypt, chunk)
                .expect("encrypt");
            cipher.extend_from_slice(&block);
        }
        let key_size = private.size();
        assert_eq!(cipher.len() % key_size, 0);

        let decrypted = decrypt_crypt1to4_with_key(&private, &b64encode(&cipher)).unwrap();
        assert_eq!(decrypted, url);

        // Длина шифртекста, не кратная размеру блока, — ошибка из Python.
        let err = decrypt_crypt1to4_with_key(&private, &b64encode(&cipher[..1])).unwrap_err();
        match err {
            StepErr::Happ(e) => assert_eq!(
                e.message,
                "crypt: длина шифртекста не кратна размеру RSA-блока"
            ),
            other => panic!("expected Happ error, got: {other}"),
        }
    }

    /// Синтетический crypt5-финал: RSA отдаёт ключ ChaCha, salted и classic.
    #[test]
    fn test_synthetic_crypt5_finish_roundtrip() {
        let private = RsaPrivateKey::new(&mut OsRng, 1024).expect("keygen");
        let public = private.to_public_key();
        let url = "https://example.com/sub?token=1";
        let nonce = b"0123456789AB";

        let mut chacha_key = [0u8; 32];
        OsRng.fill_bytes(&mut chacha_key);
        let cipher = ChaCha20Poly1305::new_from_slice(&chacha_key).unwrap();

        let build_payload = |rsa_key: &[u8], chacha_effective: &[u8]| -> (String, String) {
            // intermediate = swap_pairs(base64(url)); шифруется ЭФФЕКТИВНЫМ ключом.
            let intermediate = swap_pairs(&b64encode(url.as_bytes()));
            let url_cipher = ChaCha20Poly1305::new_from_slice(chacha_effective)
                .unwrap()
                .encrypt(Nonce::from_slice(nonce), intermediate.as_bytes())
                .unwrap();
            let url_b64 = b64encode(&url_cipher);
            // RSA шифрует swap_pairs(base64(RAW-ключа)) как ASCII; finish сам
            // применит XOR salt к расшифрованному ключу.
            let rsa_plain = swap_pairs(&b64encode(rsa_key));
            let enc_cipher = public
                .encrypt(&mut OsRng, Pkcs1v15Encrypt, rsa_plain.as_bytes())
                .unwrap();
            (url_b64, b64encode(&enc_cipher))
        };

        // Classic: без salt (raw == effective).
        let (url_b64, enc_str) = build_payload(&chacha_key, &chacha_key);
        let decrypted = finish_crypt5(&private, nonce, &url_b64, &enc_str, None).unwrap();
        assert_eq!(decrypted, url);

        // Salted: finish XOR-ит ключ из RSA повторяющимся 8-байтовым salt.
        let salt = b"S4LT1_23";
        let mut effective = chacha_key;
        for (index, value) in effective.iter_mut().enumerate() {
            *value ^= salt[index % salt.len()];
        }
        let (url_b64, enc_str) = build_payload(&chacha_key, &effective);
        let decrypted = finish_crypt5(&private, nonce, &url_b64, &enc_str, Some(salt)).unwrap();
        assert_eq!(decrypted, url);

        // DER-серриализация ключа тоже валидна (проверка инфраструктуры теста).
        assert!(private.to_pkcs8_der().is_ok());
    }

    #[test]
    fn test_c51_block_pair_swap() {
        let region: Vec<char> = "ABCDEFGH".chars().collect();
        assert_eq!(c51_block_pair_swap(&region, 8), "CDABGHEF");
        assert_eq!(c51_block_pair_swap(&region, 5), "CDABG");
        assert_eq!(c51_block_pair_swap(&region, 0), "");
        // Индексы за пределами региона пропускаются.
        let short: Vec<char> = "AB".chars().collect();
        assert_eq!(c51_block_pair_swap(&short, 4), "AB");
    }

    #[test]
    fn test_c51_extract_nonce() {
        // payload[4:16] = "ABCDEFGHIJKL" → перестановка n[2]n[3]n[0]n[1]n[6]n[7]
        // n[4]n[5]n[10]n[11]n[8]n[9] = "CDAB" + "GHEF" + "KLIJ".
        let payload: Vec<char> = "XXXXABCDEFGHIJKLyyyyyyyy".chars().collect();
        assert_eq!(c51_extract_nonce(&payload), "CDABGHEFKLIJ");
        let short: Vec<char> = "XXXXABCDEFGHIJK".chars().collect();
        assert_eq!(c51_extract_nonce(&short), "");
    }

    #[test]
    fn test_c51_selector() {
        // "AABBCCDDEEFF": [2:4]="BB", [0:2]="AA", [-6:-4]="DD", [-2:]="FF".
        let payload: Vec<char> = "AABBCCDDEEFF".chars().collect();
        assert_eq!(c51_selector(&payload), "bbaaddff");
        let short: Vec<char> = "AABBCC".chars().collect();
        assert_eq!(c51_selector(&short), "");
    }

    #[test]
    fn test_c51_make_cipher_b64() {
        // Внутренний '=' при split=true: часть после него.
        assert_eq!(c51_make_cipher_b64("abc=def==", true), "def=");
        // split=false: вся строка, крайние '=' сняты, паддинг восстановлен.
        assert_eq!(c51_make_cipher_b64("abc=def==", false), "abc=def=");
        // Внутренний '=' за пределами «полезной» части — разреза нет.
        assert_eq!(c51_make_cipher_b64("ab==", true), "ab==");
        assert_eq!(c51_make_cipher_b64("YWJj", false), "YWJj");
        assert_eq!(c51_make_cipher_b64("YWJ", false), "YWJ=");
        assert_eq!(c51_make_cipher_b64("====", true), "");
    }

    #[test]
    fn test_candidate_keys_order() {
        let total_keys: usize = CRYPT5_KEYS_B64.len();
        let (first_marker, first_key) = CRYPT5_KEYS_B64[0];
        // Точное совпадение селектора — ключ первый.
        let keys = candidate_keys(first_marker);
        assert_eq!(keys[0], first_key);
        assert_eq!(keys.len(), total_keys);
        // Неизвестный селектор: все ключи в порядке словаря.
        let keys = candidate_keys("zzzzzzzz");
        assert_eq!(keys.len(), total_keys);
        // Семейный префикс (первые 4 символа маркера) — совпадения идут первыми.
        let family = &first_marker[..4];
        let family_count = CRYPT5_KEYS_B64
            .iter()
            .filter(|(marker, _)| marker.starts_with(family))
            .count();
        let keys = candidate_keys(&format!("{family}9999"));
        for key in &keys[..family_count] {
            assert!(CRYPT5_KEYS_B64
                .iter()
                .any(|(marker, value)| value == key && marker.starts_with(family)));
        }
    }

    /// Реальный crypt5.1-маркер без подходящего ключа даёт key-unavailable.
    #[test]
    fn test_crypt51_key_unavailable_error() {
        let payload = "AABBCCDDEEFFGGHHIIJJKKLLMMNNOOPPQQRRSSTTUUVVWWXXYYZZ0011223344";
        let err = decrypt_crypt51(payload).unwrap_err();
        match err {
            StepErr::Happ(e) => {
                assert!(e.is_key_unavailable());
                assert!(e.message.contains("формат crypt5.1"));
                assert!(e.message.starts_with("не удалось расшифровать happ://crypt5-ссылку:"));
            }
            other => panic!("expected Happ error, got: {other}"),
        }
        let err = decrypt_happ_link(&format!("happ://crypt5/{payload}")).unwrap_err();
        assert!(err.is_key_unavailable());
        // Обёртка восстанавливает Python-тип HappKeyUnavailableError.
        let wrapped = err.clone().into_key_unavailable().expect("wrapper");
        assert_eq!(wrapped.to_string(), err.message);
        assert_eq!(wrapped.0.message, err.message);
    }
}
