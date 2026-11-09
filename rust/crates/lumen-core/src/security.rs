//! Password hashing, DPAPI wrappers and passphrase encryption (`security.py`).
//! The `XFENC1` passphrase format is a Fernet token (AES-128-CBC + HMAC-SHA256)
//! with a PBKDF2-HMAC-SHA256-derived key, matching the Python client exactly.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const ENCRYPTED_PREFIX: &str = "XFENC1";
pub const PASSWORD_PBKDF2_ITERATIONS: u32 = 250_000;
pub const PASSPHRASE_PBKDF2_ITERATIONS: u32 = 480_000;
pub const DPAPI_ENTROPY: &[u8] = b"xray-fluent";

type CbcEncrypt = cbc::Encryptor<aes::Aes128>;
type CbcDecrypt = cbc::Decryptor<aes::Aes128>;
type HmacSha256 = Hmac<Sha256>;

fn pbkdf2_sha256(passphrase: &str, salt: &[u8], iterations: u32, out_len: usize) -> Vec<u8> {
    let mut out = vec![0u8; out_len];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, iterations, &mut out);
    out
}

pub fn create_password_hash(password: &str, salt: Option<&[u8]>) -> anyhow::Result<(String, String)> {
    let owned_salt;
    let salt: &[u8] = match salt {
        Some(s) => s,
        None => {
            owned_salt = rand::Rng::r#gen::<[u8; 16]>(&mut rand::thread_rng());
            &owned_salt
        }
    };
    let digest = pbkdf2_sha256(password, salt, PASSWORD_PBKDF2_ITERATIONS, 32);
    Ok((
        STANDARD.encode(&digest),
        STANDARD.encode(salt),
    ))
}

pub fn verify_password(password: &str, expected_hash: &str, salt_b64: &str) -> bool {
    if expected_hash.is_empty() || salt_b64.is_empty() {
        return false;
    }
    let salt = match STANDARD.decode(salt_b64) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let digest = pbkdf2_sha256(password, &salt, PASSWORD_PBKDF2_ITERATIONS, 32);
    let expected = match STANDARD.decode(expected_hash) {
        Ok(e) => e,
        Err(_) => return false,
    };
    constant_time_eq(&digest, &expected)
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn derive_fernet_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let derived = pbkdf2_sha256(passphrase, salt, PASSPHRASE_PBKDF2_ITERATIONS, 32);
    let mut key = [0u8; 32];
    key.copy_from_slice(&derived);
    key
}

/// Minimal Fernet-compatible encrypt (version 0x80, timestamp, AES-128-CBC, HMAC-SHA256).
fn fernet_encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<String> {
    let signing_key = &key[..16];
    let enc_key = &key[16..];

    let mut iv = [0u8; 16];
    rand::Rng::fill(&mut rand::thread_rng(), &mut iv);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let mut head = Vec::with_capacity(9 + 16);
    head.push(0x80u8);
    head.extend_from_slice(&timestamp.to_be_bytes());
    head.extend_from_slice(&iv);

    let ciphertext = CbcEncrypt::new(enc_key.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext);

    let mut mac = HmacSha256::new_from_slice(signing_key)?;
    mac.update(&head);
    mac.update(&ciphertext);
    let tag = mac.finalize().into_bytes();

    let mut token = head;
    token.extend_from_slice(&ciphertext);
    token.extend_from_slice(&tag);
    // Python Fernet emits unpadded URL-safe base64.
    Ok(URL_SAFE_NO_PAD.encode(token))
}

fn fernet_decrypt(key: &[u8; 32], token: &str) -> anyhow::Result<Vec<u8>> {
    let raw = URL_SAFE_NO_PAD.decode(token.trim_end_matches('='))?;
    if raw.len() < 57 || raw[0] != 0x80 {
        anyhow::bail!("wrong passphrase or corrupted data");
    }
    let signing_key = &key[..16];
    let enc_key = &key[16..];

    let body_len = raw.len() - 32;
    let mut mac = HmacSha256::new_from_slice(signing_key)?;
    mac.update(&raw[..body_len]);
    let expected_tag = mac.finalize().into_bytes();
    if !constant_time_eq(&expected_tag, &raw[body_len..]) {
        anyhow::bail!("wrong passphrase or corrupted data");
    }

    let head = &raw[..25];
    let iv: [u8; 16] = head[9..25].try_into()?;
    let plaintext = CbcDecrypt::new(enc_key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&raw[25..body_len])?;
    Ok(plaintext)
}

pub fn encrypt_with_passphrase(data: &[u8], passphrase: &str) -> anyhow::Result<String> {
    let salt: [u8; 16] = rand::Rng::r#gen(&mut rand::thread_rng());
    let key = derive_fernet_key(passphrase, &salt);
    let token = fernet_encrypt(&key, data)?;
    Ok(format!(
        "{ENCRYPTED_PREFIX}:{}:{}",
        STANDARD.encode(salt),
        token
    ))
}

pub fn decrypt_with_passphrase(encrypted: &str, passphrase: &str) -> anyhow::Result<Vec<u8>> {
    let parts: Vec<&str> = encrypted.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != ENCRYPTED_PREFIX {
        anyhow::bail!("Invalid encrypted format");
    }
    let salt = STANDARD.decode(parts[1])?;
    let key = derive_fernet_key(passphrase, &salt);
    fernet_decrypt(&key, parts[2])
}

pub fn is_passphrase_encrypted(text: &str) -> bool {
    text.starts_with(&format!("{ENCRYPTED_PREFIX}:"))
}

// ── DPAPI ──

pub fn protect_data(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    lumen_platform::dpapi::protect(data, DPAPI_ENTROPY)
}

pub fn unprotect_data(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    lumen_platform::dpapi::unprotect(data, DPAPI_ENTROPY)
}

pub fn encode_encrypted(data: &[u8]) -> anyhow::Result<String> {
    Ok(STANDARD.encode(protect_data(data)?))
}

pub fn decode_encrypted(value: &str) -> anyhow::Result<Vec<u8>> {
    let raw = STANDARD.decode(value.trim())?;
    unprotect_data(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fernet_roundtrip() {
        let payload = b"hello lumen state {\"a\":1}";
        let encrypted = encrypt_with_passphrase(payload, "пароль test").unwrap();
        assert!(is_passphrase_encrypted(&encrypted));
        let decrypted = decrypt_with_passphrase(&encrypted, "пароль test").unwrap();
        assert_eq!(decrypted, payload);
        assert!(decrypt_with_passphrase(&encrypted, "wrong").is_err());
    }

    #[test]
    fn password_hash_roundtrip() {
        let (hash, salt) = create_password_hash("s3cret", None).unwrap();
        assert!(verify_password("s3cret", &hash, &salt));
        assert!(!verify_password("other", &hash, &salt));
    }
}
