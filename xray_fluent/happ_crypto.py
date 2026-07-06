"""Local decryption of encrypted HAPP subscription links.

The Happ VPN client lets providers hand out *encrypted* subscription links so
that the real subscription URL (or config list) is hidden from casual copying.
These links look like::

    happ://crypt/<payload>     happ://crypt2/<payload>   happ://crypt3/<payload>
    happ://crypt4/<payload>    happ://crypt5/<payload>

Happ ships the decryption keys inside the client and decrypts everything
locally — no network round-trip is needed. Lumen does exactly the same here so
that any Happ subscription (including encrypted ones, and encrypted ones that
*also* rely on HWID binding / device limits) can be imported.

Formats
-------
* ``crypt`` .. ``crypt4`` — the payload is base64 of one or more RSA blocks.
  Each block is RSA/PKCS#1 v1.5 decrypted with a built-in private key and the
  plaintext blocks are concatenated.
* ``crypt5`` — a newer format that wraps a ChaCha20-Poly1305 payload: a short
  marker selects an RSA key, the RSA block yields the ChaCha20 key, and the
  body is decrypted with ChaCha20-Poly1305. A couple of fixed character
  permutations (``m4831f`` / ``m4842j`` / ``permute4``) obfuscate the layout.

The decrypted result is usually an ``https://`` subscription URL, but some
providers embed the config list directly. Callers should treat the output as
"whatever a Happ subscription body could be" and feed it back through the
normal import pipeline.
"""

from __future__ import annotations

import base64
from typing import TYPE_CHECKING

from .happ_keys import CRYPT5_MARKER_KEYS_B64, CRYPT_NATIVE_KEYS_B64

if TYPE_CHECKING:  # pragma: no cover - typing only
    from cryptography.hazmat.primitives.asymmetric.rsa import RSAPrivateKey

# Ordered longest-first so that "crypt5/" is matched before "crypt/".
_CRYPT_PREFIXES: tuple[tuple[str, int], ...] = (
    ("crypt5/", 5),
    ("crypt4/", 4),
    ("crypt3/", 3),
    ("crypt2/", 2),
    ("crypt/", 1),
)

_HAPP_SCHEME = "happ://"

# Lazily-loaded private key caches (keyed by their base64 material).
_native_keys: list[RSAPrivateKey] | None = None
_crypt5_keys: dict[str, RSAPrivateKey] = {}


class HappDecryptError(ValueError):
    """Raised when an encrypted HAPP link cannot be decrypted."""


def is_happ_crypt_link(text: object) -> bool:
    """True if ``text`` is an encrypted ``happ://crypt*`` deep link."""
    value = str(text or "").strip()
    lowered = value.lower()
    if not lowered.startswith(_HAPP_SCHEME):
        return False
    path = value[len(_HAPP_SCHEME):]
    return any(path.lower().startswith(prefix) for prefix, _ in _CRYPT_PREFIXES)


def decrypt_happ_link(link: str) -> str:
    """Decrypt a ``happ://crypt*`` link and return the embedded string.

    Raises :class:`HappDecryptError` when the link is not a recognised encrypted
    HAPP link or when decryption fails (unknown key/marker, malformed payload,
    missing ``cryptography`` dependency).
    """
    value = str(link or "").strip()
    if not value.lower().startswith(_HAPP_SCHEME):
        raise HappDecryptError("not a happ:// link")
    path = value[len(_HAPP_SCHEME):]
    for prefix, mode in _CRYPT_PREFIXES:
        if path.lower().startswith(prefix):
            payload = path[len(prefix):].strip()
            if not payload:
                raise HappDecryptError("empty happ crypt payload")
            if mode == 5:
                return _decrypt_crypt5(payload)
            return _decrypt_rsa(mode - 1, payload)
    raise HappDecryptError("unsupported happ crypt format")


# --------------------------------------------------------------------------- #
# Low-level helpers
# --------------------------------------------------------------------------- #
def _b64_decode(text: str) -> bytes:
    """Decode base64 that may be standard or URL-safe and unpadded."""
    text = text.strip()
    for variant in (text, text.rstrip("=")):
        for decoder in (base64.b64decode, base64.urlsafe_b64decode):
            padded = variant + "=" * ((4 - len(variant) % 4) % 4)
            try:
                return decoder(padded)
            except Exception:  # noqa: BLE001 - try the next alphabet/padding
                continue
    raise HappDecryptError("invalid base64 payload")


def _shuffle_blocks(text: str, block_size: int, order: list[int]) -> str:
    data = text.encode("utf-8", errors="surrogatepass")
    full = len(data) // block_size * block_size
    out = bytearray()
    for i in range(0, full, block_size):
        block = data[i:i + block_size]
        for idx in order:
            out.append(block[idx])
    out += data[full:]
    return out.decode("utf-8", errors="surrogatepass")


def _m4831f(text: str) -> str:
    return _shuffle_blocks(text, 6, [1, 3, 5, 0, 2, 4])


def _inverse_m4831f(text: str) -> str:
    return _shuffle_blocks(text, 6, [3, 0, 4, 1, 5, 2])


def _m4842j(text: str) -> str:
    return _shuffle_blocks(text, 2, [1, 0])


def _permute4(text: str) -> str:
    return _shuffle_blocks(text, 4, [2, 3, 0, 1])


def _load_der_private_key(encoded: str) -> RSAPrivateKey:
    try:
        from cryptography.hazmat.primitives.serialization import load_der_private_key
    except Exception as exc:  # pragma: no cover - dependency guard
        raise HappDecryptError(f"cryptography is required for HAPP decryption: {exc}") from exc
    try:
        return load_der_private_key(base64.b64decode(encoded), password=None)
    except Exception as exc:  # noqa: BLE001
        raise HappDecryptError(f"failed to load HAPP key: {exc}") from exc


def _native_key(mode: int) -> RSAPrivateKey:
    global _native_keys
    if _native_keys is None:
        _native_keys = [_load_der_private_key(k) for k in CRYPT_NATIVE_KEYS_B64]
    if not 0 <= mode < len(_native_keys):
        raise HappDecryptError(f"no HAPP key for crypt mode {mode}")
    return _native_keys[mode]


def _crypt5_key(marker: str) -> RSAPrivateKey:
    cached = _crypt5_keys.get(marker)
    if cached is not None:
        return cached
    encoded = CRYPT5_MARKER_KEYS_B64.get(marker)
    if not encoded:
        raise HappDecryptError(f"unknown crypt5 marker: {marker}")
    key = _load_der_private_key(encoded)
    _crypt5_keys[marker] = key
    return key


def _rsa_decrypt(key: RSAPrivateKey, ciphertext: bytes) -> bytes:
    from cryptography.hazmat.primitives.asymmetric.padding import PKCS1v15

    try:
        return key.decrypt(ciphertext, PKCS1v15())
    except Exception as exc:  # noqa: BLE001
        raise HappDecryptError(f"RSA decryption failed: {exc}") from exc


def _decrypt_rsa(mode: int, payload: str) -> str:
    """crypt .. crypt4: base64 -> concatenated RSA/PKCS1v15 blocks."""
    key = _native_key(mode)
    cipher = _b64_decode(payload)
    key_size = (key.key_size + 7) // 8
    if not cipher or len(cipher) % key_size != 0:
        raise HappDecryptError("malformed RSA payload")
    plaintext = bytearray()
    for i in range(0, len(cipher), key_size):
        plaintext += _rsa_decrypt(key, bytes(cipher[i:i + key_size]))
    try:
        return plaintext.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise HappDecryptError(f"decrypted payload is not valid UTF-8: {exc}") from exc


def _decrypt_crypt5(payload: str) -> str:
    """crypt5: marker -> RSA key -> ChaCha20-Poly1305 body."""
    try:
        from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
    except Exception as exc:  # pragma: no cover - dependency guard
        raise HappDecryptError(f"cryptography is required for HAPP decryption: {exc}") from exc

    shuffled = _permute4(_inverse_m4831f(_m4831f(payload)))
    if len(shuffled) < 8:
        raise HappDecryptError("crypt5 payload too short")
    marker = shuffled[:4] + shuffled[-4:]
    body = shuffled[4:-4]
    if len(body) < 13:
        raise HappDecryptError("crypt5 body too short")

    nonce = body[:12].encode("utf-8", errors="surrogatepass")
    rest = body[12:]
    digit_count = len(rest) - len(rest.lstrip("0123456789"))
    if digit_count == 0:
        raise HappDecryptError("crypt5 segment length missing")
    segment_len = int(rest[:digit_count])
    packed = rest[digit_count:]
    if len(packed) < 1 + segment_len:
        raise HappDecryptError("crypt5 encrypted segment truncated")
    encrypted_segment = packed[1:1 + segment_len]
    rsa_ciphertext = packed[1 + segment_len:]

    rsa_plain = _rsa_decrypt(_crypt5_key(marker), _b64_decode(rsa_ciphertext)).decode("utf-8")
    chacha_key = _b64_decode(_m4842j(rsa_plain))
    if len(chacha_key) != 32:
        raise HappDecryptError(f"invalid ChaCha20 key length: {len(chacha_key)}")

    try:
        plaintext = ChaCha20Poly1305(chacha_key).decrypt(nonce, _b64_decode(encrypted_segment), None)
    except Exception as exc:  # noqa: BLE001
        raise HappDecryptError(f"ChaCha20 decryption failed: {exc}") from exc
    try:
        step2 = plaintext.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise HappDecryptError(f"crypt5 plaintext is not valid UTF-8: {exc}") from exc
    # Final layer: the ChaCha plaintext is m4842j-obfuscated base64 of the URL.
    return _b64_decode(_m4842j(step2)).decode("utf-8")
