from __future__ import annotations

import base64
import ctypes
from ctypes import wintypes
import hashlib
import os
import sys

from cryptography.fernet import Fernet, InvalidToken
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
from cryptography.hazmat.primitives import hashes


if sys.platform == "win32":
    CRYPTPROTECT_UI_FORBIDDEN = 0x01
    _crypt32 = ctypes.windll.crypt32
    _kernel32 = ctypes.windll.kernel32
    _user32 = ctypes.windll.user32


class _DataBlob(ctypes.Structure):
    _fields_ = [
        ("cbData", wintypes.DWORD),
        ("pbData", ctypes.POINTER(ctypes.c_byte)),
    ]


def _to_blob(data: bytes) -> tuple[_DataBlob, ctypes.Array[ctypes.c_char]]:
    if not data:
        data = b""
    buffer = ctypes.create_string_buffer(data, len(data))
    blob = _DataBlob(
        len(data),
        ctypes.cast(buffer, ctypes.POINTER(ctypes.c_byte)),
    )
    return blob, buffer


def _from_blob(blob: _DataBlob) -> bytes:
    if not blob.cbData:
        return b""
    return ctypes.string_at(blob.pbData, blob.cbData)


def protect_data(data: bytes, entropy: bytes = b"xray-fluent") -> bytes:
    if sys.platform != "win32":
        return data

    in_blob, in_buffer = _to_blob(data)
    ent_blob, ent_buffer = _to_blob(entropy)
    out_blob = _DataBlob()

    result = _crypt32.CryptProtectData(
        ctypes.byref(in_blob),
        None,
        ctypes.byref(ent_blob),
        None,
        None,
        CRYPTPROTECT_UI_FORBIDDEN,
        ctypes.byref(out_blob),
    )
    if not result:
        raise ctypes.WinError()

    try:
        return _from_blob(out_blob)
    finally:
        _kernel32.LocalFree(out_blob.pbData)
        del in_buffer
        del ent_buffer


def unprotect_data(data: bytes, entropy: bytes = b"xray-fluent") -> bytes:
    if sys.platform != "win32":
        return data

    in_blob, in_buffer = _to_blob(data)
    ent_blob, ent_buffer = _to_blob(entropy)
    out_blob = _DataBlob()

    result = _crypt32.CryptUnprotectData(
        ctypes.byref(in_blob),
        None,
        ctypes.byref(ent_blob),
        None,
        None,
        CRYPTPROTECT_UI_FORBIDDEN,
        ctypes.byref(out_blob),
    )
    if not result:
        raise ctypes.WinError()

    try:
        return _from_blob(out_blob)
    finally:
        _kernel32.LocalFree(out_blob.pbData)
        del in_buffer
        del ent_buffer


def encode_encrypted(data: bytes) -> str:
    return base64.b64encode(protect_data(data)).decode("ascii")


def decode_encrypted(value: str) -> bytes:
    raw = base64.b64decode(value.encode("ascii"))
    return unprotect_data(raw)


def create_password_hash(password: str, salt: bytes | None = None) -> tuple[str, str]:
    if salt is None:
        salt = os.urandom(16)
    digest = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt, 250_000)
    return base64.b64encode(digest).decode("ascii"), base64.b64encode(salt).decode("ascii")


def verify_password(password: str, expected_hash: str, salt_b64: str) -> bool:
    if not expected_hash or not salt_b64:
        return False
    salt = base64.b64decode(salt_b64.encode("ascii"))
    digest = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt, 250_000)
    return base64.b64encode(digest).decode("ascii") == expected_hash


ENCRYPTED_PREFIX = "XFENC1"


def _derive_fernet_key(passphrase: str, salt: bytes) -> bytes:
    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=480_000)
    return base64.urlsafe_b64encode(kdf.derive(passphrase.encode("utf-8")))


def encrypt_with_passphrase(data: bytes, passphrase: str) -> str:
    salt = os.urandom(16)
    key = _derive_fernet_key(passphrase, salt)
    token = Fernet(key).encrypt(data)
    salt_b64 = base64.b64encode(salt).decode("ascii")
    return f"{ENCRYPTED_PREFIX}:{salt_b64}:{token.decode('ascii')}"


def decrypt_with_passphrase(encrypted: str, passphrase: str) -> bytes:
    parts = encrypted.split(":", 2)
    if len(parts) != 3 or parts[0] != ENCRYPTED_PREFIX:
        raise ValueError("Invalid encrypted format")
    salt = base64.b64decode(parts[1])
    token = parts[2].encode("ascii")
    key = _derive_fernet_key(passphrase, salt)
    try:
        return Fernet(key).decrypt(token)
    except InvalidToken:
        raise ValueError("Wrong passphrase or corrupted data")


def is_passphrase_encrypted(text: str) -> bool:
    return text.startswith(f"{ENCRYPTED_PREFIX}:")


if sys.platform == "win32":
    class _LastInputInfo(ctypes.Structure):
        _fields_ = [
            ("cbSize", wintypes.UINT),
            ("dwTime", wintypes.DWORD),
        ]


def get_idle_seconds() -> int:
    if sys.platform != "win32":
        return 0

    info = _LastInputInfo()
    info.cbSize = ctypes.sizeof(_LastInputInfo)
    if not _user32.GetLastInputInfo(ctypes.byref(info)):
        return 0

    tick = _kernel32.GetTickCount()
    elapsed = max(0, tick - info.dwTime)
    return int(elapsed / 1000)
