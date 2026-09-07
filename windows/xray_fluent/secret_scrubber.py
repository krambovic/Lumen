"""Fail-closed redaction for human logs, structured records and diagnostics.

Redaction is a last line of defence, not permission to log arbitrary user data.
Unknown LogRecord extras are deliberately replaced, not stringified. Keep
structured diagnostic metadata on an allowlist at its point of collection.
"""
from __future__ import annotations

import json
import logging
import re
import traceback
from collections.abc import Mapping

_REDACTED = "***"
_MAX_RECORD_CHARS = 16 * 1024
_MAX_ITEMS = 128
_MAX_DEPTH = 8
# Logs sometimes contain share links copied from Markdown with an escaped
# colon/slashes (``vless\://``).  Treat that spelling exactly like a URI;
# otherwise the userinfo credential before ``@`` would remain visible.
_URI = re.compile(r"\b[a-zA-Z][a-zA-Z0-9+.-]*\\?:(?:\\?/){2}[^\s<>\"']+")
_UUID = re.compile(r"\b[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\b")
_LONG_ID = re.compile(r"\b[0-9a-fA-F]{32,128}\b")
_KEY_PARTS = (
    "password", "passphrase", "secret", "token", "credential", "authorization",
    "cookie", "apikey", "privatekey", "publickey", "presharedkey", "hwid",
    "hardwareid", "machineid", "deviceid", "installid", "installationid",
    "clientid", "userid", "hash", "salt",
)
_KEY_EXACT = {"id", "pass", "pwd", "psk", "uuid", "shortid", "sid", "username", "email", "uri", "url", "path"}
# Match the assignment syntax first, then classify its key. This also handles
# quoted keys inside multiply escaped JSON embedded in otherwise plain text.
_ASSIGNMENT = re.compile(
    r"(?P<key>[\w.-]+)(?:\\*[\"'])?[ \t]*[:=][ \t]*", re.UNICODE
)
_PEM = re.compile(
    r"-----BEGIN [^-\r\n]*PRIVATE KEY-----.*?(?:-----END [^-\r\n]*PRIVATE KEY-----|\Z)",
    re.DOTALL | re.IGNORECASE,
)
_HTTP_PATH = re.compile(
    r"(?i)(\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|CONNECT)\s+)/(?:[^\s\"'<>]+)"
)
_HOME_PATH = re.compile(
    r"(?i)(?:[a-z]:[\\/]+Users[\\/]+|/(?:home|Users)/)[^\\/\s\"']+"
)
_ANSI = re.compile(r"(?:\x1b|\u009b)\[[0-?]*[ -/]*[@-~]")
_CONTROLS = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_UNICODE_ESCAPE = re.compile(r"\\+u([0-9a-fA-F]{4})")
_STANDARD = frozenset(logging.LogRecord("", 0, "", 0, "", (), None).__dict__)
_NUMERIC = frozenset({"levelno", "lineno", "created", "msecs", "relativeCreated", "thread", "process"})
_TEXT = frozenset({"name", "levelname", "pathname", "filename", "module", "funcName", "threadName", "processName", "taskName"})
_DOMAINS = {"app", "core", "traffic"}


def safe_text(value: object) -> str:
    """Never include an exception or repr fallback when string conversion fails."""
    if value is None:
        return ""
    try:
        return str(value)
    except Exception:
        return "[UNPRINTABLE]"


def is_sensitive_key(key: object) -> bool:
    if not isinstance(key, str):
        return True
    normalized = re.sub(r"[^a-z0-9]", "", key.lower())
    return (normalized in _KEY_EXACT or any(part in normalized for part in _KEY_PARTS)
            or normalized.endswith(("uri", "url", "path")))


def _assignment_end(text: str, start: int) -> int:
    """Consume a whole quoted (possibly escaped) value or a header value.

    In an escaped JSON string, a delimiter with one backslash closes a value;
    a quote with three backslashes belongs to the value. A token-only regular
    expression is unsafe here and for 'Authorization: Bearer <credential>'.
    """
    quote_at = start
    while quote_at < len(text) and text[quote_at] == "\\":
        quote_at += 1
    if quote_at < len(text) and text[quote_at] in "\"'":
        quote = text[quote_at]
        escape_depth = quote_at - start
        index = quote_at + 1
        while index < len(text):
            index = text.find(quote, index)
            if index < 0:
                return len(text)  # Incomplete/rotated private value: fail closed.
            backslashes = 0
            preceding = index - 1
            while preceding >= start and text[preceding] == "\\":
                backslashes += 1
                preceding -= 1
            if (escape_depth == 0 and backslashes % 2 == 0) or (
                escape_depth > 0 and backslashes == escape_depth
            ):
                return index + 1
            index += 1
        return len(text)
    # Credentials and auth schemes may contain spaces. Preserve fields after
    # strong delimiters, but never leave a credential after masking only Bearer.
    end = start
    while end < len(text) and text[end] not in "\r\n,;}]":
        end += 1
    return end


def _scrub_assignments(text: str) -> str:
    pieces: list[str] = []
    cursor = 0
    for match in _ASSIGNMENT.finditer(text):
        if match.start() < cursor or not is_sensitive_key(match.group("key")):
            continue
        pieces.append(text[cursor:match.end()])
        start = match.end()
        end = _assignment_end(text, start)
        # Preserve the delimiter style, including escaped quotes in JSON logs.
        quote = re.match(r"\\*[\"']", text[start:])
        delimiter = quote.group(0) if quote else '"'
        pieces.append(delimiter + _REDACTED + delimiter)
        cursor = end
    pieces.append(text[cursor:])
    return "".join(pieces)


def scrub_text(value: object, *, _depth: int = 0) -> str:
    text = _CONTROLS.sub("", _ANSI.sub("", safe_text(value)))
    if _depth > _MAX_DEPTH:
        return "[REDACTED DEPTH]"
    # Decode complete JSON before redacting keys/values. This covers nested
    # JSON strings and unicode escapes without mis-parsing escaped value quotes.
    stripped = text.strip()
    if stripped and stripped[0] in '{["' and len(stripped) <= 4 * 1024 * 1024:
        try:
            parsed = json.loads(stripped)
        except (ValueError, RecursionError):
            pass
        else:
            return json.dumps(scrub_value(parsed, _depth=_depth + 1), ensure_ascii=False)
    # Decode only non-delimiter unicode characters in JSON fragments. Turning
    # escaped quotes into bare quotes before parsing could expose value tails.
    text = _UNICODE_ESCAPE.sub(
        lambda m: chr(int(m.group(1), 16))
        if chr(int(m.group(1), 16)).isalnum() or chr(int(m.group(1), 16)) in "_-/.:"
        else m.group(0),
        text,
    )
    text = _PEM.sub("[REDACTED KEY]", text)
    text = _URI.sub("[REDACTED URI]", text)
    text = _HTTP_PATH.sub(r"\1/[REDACTED]", text)
    text = _HOME_PATH.sub("[REDACTED HOME]", text)
    text = _scrub_assignments(text)
    text = _UUID.sub("[REDACTED ID]", text)
    return _LONG_ID.sub("[REDACTED ID]", text)


def scrub_value(value: object, *, _depth: int = 0):
    """Return bounded JSON-safe data; cycles and unknown objects fail closed."""
    if _depth > _MAX_DEPTH:
        return "[REDACTED DEPTH]"
    if isinstance(value, Mapping):
        result = {}
        for index, (key, item) in enumerate(value.items()):
            if index >= _MAX_ITEMS:
                result["[TRUNCATED]"] = True
                break
            safe_key = scrub_text(key, _depth=_depth + 1) if isinstance(key, str) else "[REDACTED KEY]"
            result[safe_key] = _REDACTED if is_sensitive_key(key) else scrub_value(item, _depth=_depth + 1)
        return result
    if isinstance(value, (list, tuple)):
        return [scrub_value(item, _depth=_depth + 1) for item in value[:_MAX_ITEMS]]
    if isinstance(value, str):
        return scrub_text(value, _depth=_depth + 1)
    if value is None or type(value) in (bool, int, float):
        return value
    if isinstance(value, bytes):
        return scrub_text(value.decode("utf-8", errors="replace"), _depth=_depth + 1)
    return "[REDACTED OBJECT]"


def _record_text(value: object) -> str:
    # Redact BEFORE truncating; otherwise a split key/escape can expose secrets.
    text = scrub_text(value)
    if len(text) > _MAX_RECORD_CHARS:
        return text[:_MAX_RECORD_CHARS] + " [TRUNCATED]"
    return text


def scrub_record(record: logging.LogRecord) -> logging.LogRecord:
    """Scrub in place so later sinks cannot retain raw args or traceback frames."""
    try:
        message = record.getMessage()
    except Exception:
        message = "[UNFORMATTABLE LOG MESSAGE]"
    try:
        exception = "".join(traceback.format_exception(*record.exc_info)) if record.exc_info else record.exc_text
    except Exception:
        exception = "[UNFORMATTABLE EXCEPTION]"
    stack = getattr(record, "stack_info", None)
    for key, value in list(record.__dict__.items()):
        if key in _TEXT:
            record.__dict__[key] = _record_text(value)
        elif key in _NUMERIC:
            if type(value) not in (int, float):
                record.__dict__[key] = 0
        elif key == "xdomain":
            record.__dict__[key] = value if isinstance(value, str) and value in _DOMAINS else ""
        elif key not in _STANDARD and key not in {"message", "asctime"}:
            # Do not call __str__ on application objects or keep nested secrets
            # alive in a queue. Placeholder values also keep custom formatters usable.
            record.__dict__[key] = "[REDACTED FIELD]"
    record.msg = _record_text(message)
    record.args = ()
    record.message = record.msg
    record.exc_info = None
    record.exc_text = _record_text(exception) if exception else None
    record.stack_info = _record_text(stack) if stack else None
    record.__dict__.pop("asctime", None)
    return record


def prepare_record(record: logging.LogRecord) -> logging.LogRecord:
    """Detach a base LogRecord snapshot before any asynchronous queue or formatter."""
    scrub_record(record)
    snapshot = logging.LogRecord("", logging.INFO, "", 0, "", (), None)
    snapshot.__dict__.update(record.__dict__)
    # A hostile custom instance attribute must not shadow base logging methods.
    for name in ("getMessage", "__str__", "__repr__"):
        snapshot.__dict__.pop(name, None)
    return snapshot


class ScrubbingFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        scrub_record(record)
        return True
