"""Расшифровка закрытых ссылок Happ (`happ://crypt`, `crypt2..crypt5`, `crypt5.1`).

Happ умеет делиться подписками в «зашифрованном» виде, чтобы адрес подписки не
был виден в самой ссылке. Формат восстановлен сообществом (ключи Happ извлечены
из APK и опубликованы в проектах-дешифраторах под открытыми лицензиями). Схема:

* ``crypt``..``crypt4`` — тело в base64, расшифровывается RSA-PKCS1v15 одним из
  четырёх приватных ключей (по одному на каждый режим), блоками по размеру ключа.
* ``crypt5`` (классический) — гибрид: несколько строковых перестановок,
  RSA-PKCS1v15 (ключ выбирается по 8-символьному «маркеру») отдаёт ключ
  ChaCha20-Poly1305, которым расшифровывается тело.
* ``crypt5.1`` — тот же гибрид с изменённой раскладкой полей и расширенным
  набором ключей; нужный ключ подбирается перебором.

Результат расшифровки — обычный текст: либо URL подписки (``https://...``),
либо готовый список ссылок (``vless://...`` и т.п.). Дальше он уходит в тот же
пайплайн импорта подписок, что и любой другой ответ провайдера.

Так как схема опирается на приватные ключи, извлечённые из Happ, публично
доступен только их ограниченный набор. Для ссылок ``crypt5.1`` с ключом, которого
нет в наборе, функция бросает :class:`HappKeyUnavailableError` с понятным
сообщением.
"""

from __future__ import annotations

import base64
import re

from cryptography.hazmat.primitives.asymmetric.padding import PKCS1v15
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.serialization import load_der_private_key

from .happ_crypt_keys import CRYPT5_KEYS_B64, PKCS1_KEYS_B64

HAPP_SCHEME = "happ://"

_CRYPT_PREFIXES: tuple[tuple[str, int], ...] = (
    ("crypt5/", 4),
    ("crypt4/", 3),
    ("crypt3/", 2),
    ("crypt2/", 1),
    ("crypt/", 0),
)


class HappDecryptError(Exception):
    """Не удалось расшифровать ссылку Happ (битый формат, неизвестная схема)."""


class HappKeyUnavailableError(HappDecryptError):
    """Нет приватного ключа под данную ссылку (например, формат ``crypt5.1``)."""


# ─────────────────────────────────────────────────────────────────────────────
#  Определение ссылок
# ─────────────────────────────────────────────────────────────────────────────


def is_happ_link(text: str) -> bool:
    return str(text or "").strip().lower().startswith(HAPP_SCHEME)


def _split_crypt_prefix(text: str) -> tuple[int, str] | None:
    """Возвращает ``(ordinal, payload)`` для happ://crypt*-ссылки, иначе ``None``.

    Схема (``happ://``) и токен режима (``cryptN/``) сопоставляются без учёта
    регистра; тело подписки сохраняется как есть.
    """
    body = str(text or "").strip()
    if body[:len(HAPP_SCHEME)].lower() != HAPP_SCHEME:
        return None
    path = body[len(HAPP_SCHEME):]
    for prefix, ordinal in _CRYPT_PREFIXES:
        if path[:len(prefix)].lower() == prefix:
            return ordinal, path[len(prefix):]
    return None


def is_happ_crypt_link(text: str) -> bool:
    return _split_crypt_prefix(text) is not None


# ─────────────────────────────────────────────────────────────────────────────
#  Строковые / байтовые помощники
# ─────────────────────────────────────────────────────────────────────────────


def _b64decode(text: str) -> bytes:
    """base64, терпимый к url-safe алфавиту и отсутствию паддинга."""
    compact = text.replace("-", "+").replace("_", "/")
    compact = "".join(compact.split())
    return base64.b64decode(compact + "=" * (-len(compact) % 4))


def _swap_pairs(text: str) -> str:
    """Меняет местами соседние символы попарно: ``ABCD`` → ``BADC``."""
    chars = list(text)
    for i in range(0, len(chars) - 1, 2):
        chars[i], chars[i + 1] = chars[i + 1], chars[i]
    return "".join(chars)


def _block_pair_swap(text: str) -> str:
    """Каждый полный 4-символьный блок ``ABCD`` → ``CDAB``; хвост — как есть."""
    full = len(text) - (len(text) % 4)
    out: list[str] = []
    for offset in range(0, full, 4):
        out.append(text[offset + 2:offset + 4])
        out.append(text[offset:offset + 2])
    out.append(text[full:])
    return "".join(out)


# ─────────────────────────────────────────────────────────────────────────────
#  RSA / ChaCha20-Poly1305
# ─────────────────────────────────────────────────────────────────────────────

_key_cache: dict[str, object] = {}


def _wrap_pkcs1_in_pkcs8(der: bytes) -> bytes:
    """Оборачивает PKCS#1 RSAPrivateKey (DER) в PKCS#8 PrivateKeyInfo."""
    def _der_len(size: int) -> bytes:
        if size < 0x80:
            return bytes([size])
        raw = size.to_bytes((size.bit_length() + 7) // 8, "big")
        return bytes([0x80 | len(raw)]) + raw

    rsa_algo_id = bytes.fromhex("300d06092a864886f70d0101010500")
    octet = b"\x04" + _der_len(len(der)) + der
    body = b"\x02\x01\x00" + rsa_algo_id + octet
    return b"\x30" + _der_len(len(body)) + body


def _load_private_key(b64: str):
    cached = _key_cache.get(b64)
    if cached is not None:
        return cached
    der = base64.b64decode(b64)
    try:
        key = load_der_private_key(der, password=None)
    except ValueError:
        # crypt..crypt4 хранятся как PKCS#1 — оборачиваем в PKCS#8.
        key = load_der_private_key(_wrap_pkcs1_in_pkcs8(der), password=None)
    _key_cache[b64] = key
    return key


def _rsa_decrypt(b64_key: str, ciphertext: bytes) -> bytes:
    return _load_private_key(b64_key).decrypt(ciphertext, PKCS1v15())


def _chacha_decrypt(key: bytes, nonce: bytes, ciphertext: bytes) -> bytes:
    return ChaCha20Poly1305(key).decrypt(nonce, ciphertext, None)


# ─────────────────────────────────────────────────────────────────────────────
#  crypt..crypt4
# ─────────────────────────────────────────────────────────────────────────────


def _decrypt_crypt1to4(ordinal: int, payload: str) -> str:
    b64_key = PKCS1_KEYS_B64[ordinal]
    key = _load_private_key(b64_key)
    key_size = (key.key_size + 7) // 8
    cipher = _b64decode(payload)
    if not cipher or len(cipher) % key_size != 0:
        raise HappDecryptError("crypt: длина шифртекста не кратна размеру RSA-блока")
    parts = [
        key.decrypt(cipher[i:i + key_size], PKCS1v15())
        for i in range(0, len(cipher), key_size)
    ]
    return b"".join(parts).decode("utf-8")


# ─────────────────────────────────────────────────────────────────────────────
#  crypt5 (классический)
# ─────────────────────────────────────────────────────────────────────────────


def _finish_crypt5(nonce: bytes, url_b64: str, enc_str: str, key_b64: str) -> str:
    """Общий финал crypt5/crypt5.1: RSA → ключ ChaCha → расшифровка → URL."""
    rsa_plain = _rsa_decrypt(key_b64, _b64decode(enc_str)).decode("latin-1")
    chacha_key = _b64decode(_swap_pairs(rsa_plain))
    if len(chacha_key) != 32:
        raise HappDecryptError("crypt5: неверная длина ключа ChaCha20")
    intermediate = _chacha_decrypt(chacha_key, nonce, _b64decode(url_b64)).decode("utf-8")
    return _b64decode(_swap_pairs(intermediate)).decode("utf-8")


def _try_decrypt_crypt5_legacy(payload: str) -> str | None:
    """Классический crypt5: маркер (первые+последние 4 символа) → RSA-ключ.

    Возвращает ``None``, если маркер неизвестен либо структура не подходит, —
    тогда вызывающий пробует формат crypt5.1.
    """
    shuffled = _block_pair_swap(payload)
    if len(shuffled) < 8:
        return None
    marker = shuffled[:4] + shuffled[-4:]
    key_b64 = CRYPT5_KEYS_B64.get(marker)
    if key_b64 is None:
        return None
    body = shuffled[4:-4]
    if len(body) < 13:
        return None
    match = re.match(r"^\d+", body[12:])
    if not match:
        return None
    segment_len = int(match.group(0))
    packed = body[12 + match.end():]
    if len(packed) < 1 + segment_len:
        return None
    url_b64 = packed[1:1 + segment_len]
    enc_str = packed[1 + segment_len:]
    try:
        nonce = body[:12].encode("ascii")
        return _finish_crypt5(nonce, url_b64, enc_str, key_b64)
    except Exception:
        return None


# ─────────────────────────────────────────────────────────────────────────────
#  crypt5.1
# ─────────────────────────────────────────────────────────────────────────────


def _c51_block_pair_swap(region: str, length: int) -> str:
    out: list[str] = []
    for j in range(1, length + 1):
        block = (j - 1) // 4
        pos = (j - 1) % 4
        index = 4 * block + ((pos + 2) % 4)
        if index < len(region):
            out.append(region[index])
    return "".join(out)


def _c51_extract_nonce(payload: str) -> str:
    n = payload[4:16]
    return (
        n[2] + n[3] + n[0] + n[1] + n[6] + n[7]
        + n[4] + n[5] + n[10] + n[11] + n[8] + n[9]
    )


def _c51_selector(payload: str) -> str:
    if len(payload) < 10:
        return ""
    return (
        payload[2:4] + payload[0:2] + payload[-6:-4] + payload[-2:]
    ).lower()


def _c51_make_cipher_b64(enc_str: str, split_on_inner_equals: bool) -> str:
    trailing_start = len(re.sub(r"=+$", "", enc_str))
    eq_idx = enc_str.find("=")
    cipher_b64 = (
        enc_str[eq_idx + 1:]
        if split_on_inner_equals and 0 <= eq_idx < trailing_start
        else enc_str
    )
    cleaned = re.sub(r"^=+", "", cipher_b64)
    cleaned = re.sub(r"=+$", "", cleaned)
    return cleaned + "=" * (-len(cleaned) % 4)


def _c51_candidates(payload: str) -> list[tuple[str, str, str, bool]]:
    """Возможные разбиения payload на (nonce, url_b64, enc_str, split)."""
    nonce_str = _c51_extract_nonce(payload)
    candidates: list[tuple[str, str, str, bool]] = []
    seen: set[tuple] = set()

    def push(url_b64: str, enc_str: str, split: bool) -> None:
        if not url_b64 or len(enc_str) < 684:
            return
        key = (
            len(url_b64), url_b64[:16], url_b64[-16:], enc_str[:16], enc_str[-16:]
        )
        if key in seen:
            return
        seen.add(key)
        candidates.append((nonce_str, url_b64, enc_str, split))

    try:
        n = int(payload[18:20])
    except ValueError:
        n = 0
    if n > 0 and len(payload) >= 20 + n + 684:
        url_region = payload[20:20 + n]
        enc_region = payload[20 + n:20 + n + 684]
        skip = ((n - 1) // 4) * 4 + 1
        url_b64 = payload[17] + _c51_block_pair_swap(url_region, n - 1)
        enc_str = url_region[skip] + _c51_block_pair_swap(enc_region, 683)
        push(url_b64, enc_str, True)

    for trailer_len in range(4, 9):
        url_len = len(payload) - 20 - 684 - trailer_len
        if url_len <= 0:
            continue
        url_region = payload[20:20 + url_len]
        enc_region = payload[20 + url_len:20 + url_len + 684]
        if len(enc_region) != 684:
            continue
        url_b64 = _c51_block_pair_swap(url_region, url_len)
        enc_str = _c51_block_pair_swap(enc_region, 684)
        push(url_b64, enc_str, False)
        if url_b64.endswith("="):
            push(url_b64[1:] + "=", enc_str, False)

    return candidates


def _candidate_keys(selector: str) -> list[str]:
    """Ключи-кандидаты: сначала по селектору/семейству, затем все остальные."""
    keys: list[str] = []
    seen: set[str] = set()

    def add(value: str | None) -> None:
        if value and value not in seen:
            seen.add(value)
            keys.append(value)

    add(CRYPT5_KEYS_B64.get(selector))
    family = selector[:4]
    if family:
        for marker, value in CRYPT5_KEYS_B64.items():
            if marker.startswith(family):
                add(value)
    for value in CRYPT5_KEYS_B64.values():
        add(value)
    return keys


def _decrypt_crypt51(payload: str) -> str:
    selector = _c51_selector(payload)
    keys = _candidate_keys(selector)
    candidates = _c51_candidates(payload)
    preferred_values = {
        value
        for marker, value in CRYPT5_KEYS_B64.items()
        if marker == selector or (selector[:4] and marker.startswith(selector[:4]))
    }
    preferred_keys = [key for key in keys if key in preferred_values]
    fallback_keys = [key for key in keys if key not in preferred_values]

    def decrypt_with_keys(key_candidates: list[str]) -> str | None:
        for nonce_str, url_b64, enc_str, split in candidates:
            try:
                nonce = nonce_str.encode("ascii")
                cipher_b64 = _c51_make_cipher_b64(enc_str, split)
            except Exception:
                continue
            for key_b64 in key_candidates:
                try:
                    rsa_plain = _rsa_decrypt(key_b64, _b64decode(cipher_b64)).decode("latin-1")
                except Exception:
                    continue
                for shaped in (_swap_pairs(rsa_plain), rsa_plain):
                    try:
                        chacha_key = _b64decode(shaped)
                        if len(chacha_key) != 32:
                            continue
                        intermediate = _chacha_decrypt(
                            chacha_key, nonce, _b64decode(url_b64)
                        ).decode("utf-8")
                        return _b64decode(_swap_pairs(intermediate)).decode("utf-8")
                    except Exception:
                        continue
        return None

    decrypted = decrypt_with_keys(preferred_keys)
    if decrypted is not None:
        return decrypted

    # Fallback to Node.js emulation decryptor
    from pathlib import Path
    import shutil
    
    node_bin = shutil.which("node")
    if node_bin:
        emu_dir = Path(__file__).parent / "happ_emulator"
        cli_js = emu_dir / "decrypt_cli.js"
        if cli_js.exists():
            from .subprocess_utils import run_text_pumped, CREATE_NO_WINDOW, decode_output
            try:
                link = f"happ://crypt5/{payload}"
                result = run_text_pumped(
                    [node_bin, str(cli_js), link],
                    timeout=20.0,
                    creationflags=CREATE_NO_WINDOW,
                )
                if result.returncode == 0:
                    decrypted = decode_output(result.stdout).strip()
                    if decrypted:
                        return decrypted
            except Exception:
                pass

    decrypted = decrypt_with_keys(fallback_keys)
    if decrypted is not None:
        return decrypted

    raise HappKeyUnavailableError(
        "не удалось расшифровать happ://crypt5-ссылку: подходящий приватный ключ "
        f"отсутствует в наборе (формат crypt5.1, маркер «{selector}»)"
    )


def _decrypt_crypt5(payload: str) -> str:
    legacy = _try_decrypt_crypt5_legacy(payload)
    if legacy is not None:
        return legacy
    return _decrypt_crypt51(payload)


# ─────────────────────────────────────────────────────────────────────────────
#  Публичный API
# ─────────────────────────────────────────────────────────────────────────────


def decrypt_happ_link(link: str) -> str:
    """Расшифровывает ``happ://crypt*`` и возвращает URL/текст подписки.

    Бросает :class:`HappDecryptError` (или :class:`HappKeyUnavailableError`),
    если ссылку расшифровать нельзя.
    """
    parsed = _split_crypt_prefix(link)
    if parsed is None:
        raise HappDecryptError("неподдерживаемая ссылка (ожидалась happ://crypt*)")
    ordinal, payload = parsed
    if not payload:
        raise HappDecryptError("пустое тело happ://crypt-ссылки")
    try:
        if ordinal == 4:
            return _decrypt_crypt5(payload)
        return _decrypt_crypt1to4(ordinal, payload)
    except HappDecryptError:
        raise
    except Exception as exc:  # noqa: BLE001 - единый понятный класс ошибки
        raise HappDecryptError(f"ошибка расшифровки happ-ссылки: {exc}") from exc
