"""Test-only helpers that build encrypted ``happ://crypt*`` links.

Production code only ever *decrypts* HAPP links, so the encryption side lives
here (in the test suite) and is used to generate deterministic fixtures for the
decryption round-trip tests. This mirrors exactly what a HAPP panel does when it
hands out an encrypted subscription link.
"""

from __future__ import annotations

import base64
import os

from cryptography.hazmat.primitives.asymmetric.padding import PKCS1v15
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.serialization import load_der_private_key

from xray_fluent import happ_crypto as hc
from xray_fluent.happ_keys import CRYPT5_MARKER_KEYS_B64, CRYPT_NATIVE_KEYS_B64

_PREFIX_BY_MODE = {1: "crypt", 2: "crypt2", 3: "crypt3", 4: "crypt4"}


def _load(encoded: str):
    return load_der_private_key(base64.b64decode(encoded), password=None)


def encrypt_crypt(text: str, mode: int) -> str:
    """Build a ``happ://crypt``..``crypt4`` link that decrypts back to ``text``."""
    key = _load(CRYPT_NATIVE_KEYS_B64[mode - 1])
    pub = key.public_key()
    key_size = (key.key_size + 7) // 8
    max_chunk = key_size - 11  # PKCS#1 v1.5 overhead
    data = text.encode("utf-8")
    blob = b""
    for i in range(0, len(data), max_chunk):
        blob += pub.encrypt(data[i:i + max_chunk], PKCS1v15())
    return f"happ://{_PREFIX_BY_MODE[mode]}/" + base64.b64encode(blob).decode()


def encrypt_crypt5(text: str, marker: str = "axrtpjmw") -> str:
    """Build a ``happ://crypt5/`` link that decrypts back to ``text``."""
    key = _load(CRYPT5_MARKER_KEYS_B64[marker])
    pub = key.public_key()

    step3 = base64.b64encode(text.encode("utf-8")).decode()
    step2 = hc._m4842j(step3)  # ChaCha plaintext

    chacha_key = os.urandom(32)
    nonce_str = "".join(chr(65 + (i % 26)) for i in range(12))
    nonce = nonce_str.encode()
    encrypted = ChaCha20Poly1305(chacha_key).encrypt(nonce, step2.encode(), None)
    enc_seg = base64.b64encode(encrypted).decode()

    rsa_plain = hc._m4842j(base64.b64encode(chacha_key).decode())
    rsa_ct = base64.b64encode(pub.encrypt(rsa_plain.encode(), PKCS1v15())).decode()

    rest = f"{len(enc_seg)}!{enc_seg}{rsa_ct}"  # '!' is the throwaway packed[0]
    body = nonce_str + rest
    shuffled = marker[:4] + body + marker[4:]
    payload = hc._permute4(shuffled)
    return "happ://crypt5/" + payload
