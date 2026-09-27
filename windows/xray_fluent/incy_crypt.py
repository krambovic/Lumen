"""Native decryption for Incy ``incy://crypt1/`` subscription links.

The wire format is URL-safe base64 containing ``nonce || ciphertext || tag``.
The authenticated plaintext is a UTF-8 JSON object with a non-empty ``url``
field. Only the two 32-byte key-material slices used by the format are kept in
the application, so portable builds need no key file or JavaScript runtime.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json

from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

INCY_SCHEME = "incy://"
INCY_CRYPT1_PREFIX = "incy://crypt1/"

_KEYMAT_A_SLICE_B64 = "7odqBjr3BNe0CfGRDZcxzBQZCB7AiZOgEnBVaHPh7y0="
_KEYMAT_B_SLICE_B64 = "z1EN9DsquX6phHju5fGxz4DjpIT8k2kxbM2H5Ut5l7c="
_KEY_SALT = b"incydeepcrypt1v2026.06"
_KEY_FINGERPRINT = "b6bf708471cc90043232967660aade86a50b4e57929db2e53c5fa34db624c08c"
_NONCE_SIZE = 12
_TAG_SIZE = 16
_MAX_WIRE_SIZE = 1024 * 1024


class IncyDecryptError(Exception):
    """The Incy link is malformed, corrupted, or cannot be authenticated."""


def is_incy_link(text: str) -> bool:
    return str(text or "").strip().lower().startswith(INCY_SCHEME)


def is_incy_crypt_link(text: str) -> bool:
    body = str(text or "").strip()
    return body[: len(INCY_CRYPT1_PREFIX)].lower() == INCY_CRYPT1_PREFIX


def _derive_key() -> bytes:
    seed = (
        _KEY_SALT
        + base64.b64decode(_KEYMAT_A_SLICE_B64)
        + base64.b64decode(_KEYMAT_B_SLICE_B64)
    )
    key = hashlib.sha256(seed).digest()
    fingerprint = hashlib.sha256(key).hexdigest()
    if not hmac.compare_digest(fingerprint, _KEY_FINGERPRINT):
        raise IncyDecryptError("Incy key fingerprint mismatch")
    return key


def _decode_payload(payload: str) -> bytes:
    compact = "".join(payload.split()).rstrip("/")
    if not compact:
        raise IncyDecryptError("empty incy://crypt1 payload")
    if len(compact) > (_MAX_WIRE_SIZE * 4 // 3 + 8):
        raise IncyDecryptError("incy://crypt1 payload is too large")
    try:
        return base64.b64decode(
            compact + "=" * (-len(compact) % 4),
            altchars=b"-_",
            validate=True,
        )
    except (ValueError, TypeError) as exc:
        raise IncyDecryptError("invalid incy://crypt1 base64 payload") from exc


def decrypt_incy_link(link: str) -> str:
    """Decrypt an ``incy://crypt1/`` link and return its wrapped URL."""
    body = str(link or "").strip()
    if not is_incy_crypt_link(body):
        raise IncyDecryptError("unsupported link (expected incy://crypt1/)")

    wire = _decode_payload(body[len(INCY_CRYPT1_PREFIX) :])
    if len(wire) < _NONCE_SIZE + _TAG_SIZE + 1:
        raise IncyDecryptError("incy://crypt1 payload is too short")
    try:
        plaintext = AESGCM(_derive_key()).decrypt(
            wire[:_NONCE_SIZE], wire[_NONCE_SIZE:], None
        )
    except InvalidTag as exc:
        raise IncyDecryptError("incy://crypt1 authentication failed") from exc

    try:
        data = json.loads(plaintext.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise IncyDecryptError("incy://crypt1 plaintext is not valid JSON") from exc
    if not isinstance(data, dict):
        raise IncyDecryptError("incy://crypt1 plaintext must be a JSON object")
    url = data.get("url")
    if not isinstance(url, str) or not url.strip():
        raise IncyDecryptError("incy://crypt1 JSON has no non-empty 'url' field")
    return url.strip()
