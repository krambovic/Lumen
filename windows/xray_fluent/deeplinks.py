"""Parsing and local IPC helpers for the ``lumen:`` URL protocol."""
from __future__ import annotations

from dataclasses import dataclass
import json
from urllib.parse import parse_qs, unquote, urlsplit


MAX_DEEP_LINK_LENGTH = 32 * 1024
_TARGET_QUERY_KEYS = ("url", "subscription", "link", "config", "data")
_NAME_QUERY_KEYS = ("name", "title")
_ACTIONS = {"add", "import", "subscribe", "subscription", "install-config"}
_TARGET_SCHEMES = {"http", "https", "happ"}


class DeepLinkError(ValueError):
    """Raised when a Lumen deep link is present but is unsafe or malformed."""


@dataclass(frozen=True, slots=True)
class SubscriptionDeepLink:
    url: str
    name: str = ""


def _first_query_value(query: dict[str, list[str]], keys: tuple[str, ...]) -> str:
    for key in keys:
        values = query.get(key)
        if values:
            value = str(values[0] or "").strip()
            if value:
                return value
    return ""


def _validate_subscription_url(value: str) -> str:
    target = str(value or "").strip()
    if not target or len(target) > MAX_DEEP_LINK_LENGTH:
        raise DeepLinkError("subscription URL is empty or too long")
    if any(ord(char) < 32 for char in target):
        raise DeepLinkError("subscription URL contains control characters")
    parsed = urlsplit(target)
    if parsed.scheme.lower() not in _TARGET_SCHEMES:
        raise DeepLinkError("only http, https and happ subscription links are allowed")
    if not parsed.netloc:
        raise DeepLinkError("subscription URL has no host")
    return target


def parse_lumen_deep_link(value: str) -> SubscriptionDeepLink | None:
    """Parse a browser URL into a subscription import request.

    Canonical form::

        lumen://add?url=https%3A%2F%2Fexample.com%2Fsubscription&name=Example

    ``import``, ``subscribe``, ``subscription/add`` and ``install-config`` are
    accepted aliases. A percent-encoded URL may also be placed after the action
    in the path. A bare ``lumen://`` remains an activation-only link for old
    Windows toast notifications.
    """
    raw = str(value or "").strip()
    if not raw:
        return None
    if len(raw) > MAX_DEEP_LINK_LENGTH:
        raise DeepLinkError("deep link is too long")
    parsed = urlsplit(raw)
    if parsed.scheme.lower() != "lumen":
        raise DeepLinkError("unsupported deep-link scheme")

    host = parsed.netloc.strip().lower()
    decoded_path = unquote(parsed.path.lstrip("/")).strip()
    action = host
    path_payload = decoded_path
    if action == "subscription" and decoded_path:
        nested_action, separator, remainder = decoded_path.partition("/")
        if nested_action.lower() in _ACTIONS:
            action = f"subscription/{nested_action.lower()}"
            path_payload = remainder if separator else ""
    elif not action and decoded_path:
        path_action, separator, remainder = decoded_path.partition("/")
        action = path_action.lower()
        path_payload = remainder if separator else ""

    query = parse_qs(parsed.query, keep_blank_values=True)
    target = _first_query_value(query, _TARGET_QUERY_KEYS)
    name = "".join(
        char for char in _first_query_value(query, _NAME_QUERY_KEYS) if ord(char) >= 32
    )[:256]

    if not action and not target and not path_payload:
        return None
    base_action = action.split("/", 1)[-1]
    if base_action not in _ACTIONS:
        raise DeepLinkError("unsupported Lumen deep-link action")
    if not target and path_payload:
        target = path_payload.strip()
    return SubscriptionDeepLink(_validate_subscription_url(target), name)


def find_lumen_deep_link(arguments: list[str] | tuple[str, ...]) -> str:
    for argument in arguments:
        value = str(argument or "").strip()
        if value.lower().startswith("lumen:"):
            return value
    return ""


def encode_instance_message(arguments: list[str] | tuple[str, ...]) -> bytes:
    link = find_lumen_deep_link(arguments)
    payload = {"command": "deeplink" if link else "activate"}
    if link:
        payload["url"] = link
    return (json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")


def decode_instance_message(payload: bytes) -> str:
    """Return the deep link from IPC, or an empty string for activation-only."""
    raw = bytes(payload or b"")[: MAX_DEEP_LINK_LENGTH + 1024].strip()
    if not raw or raw == b"activate":
        return ""
    try:
        message = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return ""
    if not isinstance(message, dict) or message.get("command") != "deeplink":
        return ""
    link = str(message.get("url") or "").strip()
    return link if link.lower().startswith("lumen:") and len(link) <= MAX_DEEP_LINK_LENGTH else ""
