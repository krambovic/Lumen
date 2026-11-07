"""Lossless repair of UTF-8 provider text exposed as single-byte HTTP headers."""
from __future__ import annotations


def repair_utf8_header(value: str) -> str:
    """Reverse Latin-1/Windows-1252 mojibake only when strict UTF-8 decoding succeeds.

    Python's HTTP parser maps header bytes to Latin-1. Some panels send raw
    UTF-8 instead of base64, and cached values may have passed through a second
    single-byte conversion. Never replace undecodable bytes or guess CP1251.
    Repair before strip(): UTF-8 continuation bytes can look like whitespace.
    """
    if len(value) > 64 * 1024:
        return value
    for _ in range(2):
        for encoding in ("latin-1", "cp1252"):
            try:
                repaired = value.encode(encoding).decode("utf-8")
            except (UnicodeEncodeError, UnicodeDecodeError):
                continue
            if repaired != value:
                value = repaired
                break
        else:
            break
    return value
