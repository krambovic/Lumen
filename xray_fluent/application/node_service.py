from __future__ import annotations

import base64
import binascii
import json
import re
import uuid
from datetime import datetime, timezone
from functools import lru_cache
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, quote, unquote, urlparse

from PyQt6.QtCore import QTimer

from ..country_flags import detect_country
from ..happ_crypt import HappDecryptError, decrypt_happ_link, is_happ_crypt_link, is_happ_link
from ..link_parser import normalize_node_outbound, parse_links_text, validate_node_outbound
from ..models import DEFAULT_SUBSCRIPTION_HWID
from ..subscription_fetcher import (
    SubscriptionFetcherCancelled,
    fetch_subscription_http,
)

if TYPE_CHECKING:
    from ..app_controller import AppController


MAX_SUBSCRIPTION_BYTES = 8 * 1024 * 1024
HAPP_WINDOWS_USER_AGENT = "Happ/2.18.3/Windows/2606241603601"

# Parameters documented by Happ for premium subscriptions.  Keep the original
# kebab-case names in persisted metadata so providers and users can see exactly
# what the subscription requested, including platform-specific options.
HAPP_PREMIUM_PARAMETERS: tuple[str, ...] = (
    "new-url",
    "new-domain",
    "subscription-always-hwid-enable",
    "notification-subs-expire",
    "hide-settings",
    "server-address-resolve-enable",
    "server-address-resolve-dns-domain",
    "server-address-resolve-dns-ip",
    "subscription-autoconnect",
    "subscription-autoconnect-type",
    "subscription-ping-onopen-enabled",
    "subscription-auto-update-enable",
    "fragmentation-enable",
    "fragmentation-packets",
    "fragmentation-length",
    "fragmentation-interval",
    "ping-type",
    "check-url-via-proxy",
    "change-user-agent",
    "app-auto-start",
    "subscription-auto-update-open-enable",
    "per-app-proxy-mode",
    "per-app-proxy-list",
    "sniffing-enable",
    "subscriptions-collapse",
    "ping-result",
    "mux-enable",
    "mux-tcp-connections",
    "mux-xudp-connections",
    "mux-quic",
    "exclude-routes",
)
_HAPP_BODY_METADATA_KEYS = {
    "providerid",
    "provider-id",
    "profile-title",
    "support-url",
    "profile-web-page-url",
    "telegram-url",
    "announce",
    "announce-url",
    *HAPP_PREMIUM_PARAMETERS,
}


@lru_cache(maxsize=1)
def _windows_machine_hwid() -> str:
    """Return the stable Windows installation identifier without spawning a process."""
    try:
        import winreg
    except ImportError:
        return ""

    access = winreg.KEY_READ
    view_flags = [getattr(winreg, "KEY_WOW64_64KEY", 0), 0]
    for view_flag in dict.fromkeys(view_flags):
        try:
            with winreg.OpenKey(
                winreg.HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\Cryptography",
                0,
                access | view_flag,
            ) as key:
                raw_value, _ = winreg.QueryValueEx(key, "MachineGuid")
        except OSError:
            continue
        value = str(raw_value or "").strip().strip("{}")
        if not value or "\r" in value or "\n" in value:
            continue
        try:
            return str(uuid.UUID(value))
        except ValueError:
            return value[:256]
    return ""


def _resolve_subscription_hwid(hwid: str, *, use_real_hwid: bool) -> str:
    if use_real_hwid:
        machine_hwid = _windows_machine_hwid()
        if machine_hwid:
            return machine_hwid
    return str(hwid or DEFAULT_SUBSCRIPTION_HWID).strip()


class SubscriptionFetchCancelled(RuntimeError):
    pass


def _raise_if_subscription_cancelled(cancelled=None) -> None:
    if cancelled is not None and cancelled():
        raise SubscriptionFetchCancelled("загрузка подписки отменена")


def import_nodes_from_text(
    controller: AppController,
    text: str,
    *,
    group: str | None = None,
    auto_connect: bool | None = None,
    select_imported: bool = False,
) -> tuple[int, list[str]]:
    nodes, errors = parse_links_text(text)
    if not nodes:
        return 0, errors

    previous_selected_id = controller.state.selected_node_id
    existing_nodes = {
        (node.link, (node.group or "Default").strip().casefold())
        for node in controller.state.nodes
        if node.link
    }
    max_order = max((node.sort_order for node in controller.state.nodes), default=0)
    first_new_id: str | None = None
    added = 0
    for node in nodes:
        normalize_node_outbound(node)
        problem = validate_node_outbound(node)
        if problem:
            errors.append(problem)
            continue
        effective_group = str(group if group is not None else (node.group or "Default")).strip() or "Default"
        identity = (node.link, effective_group.casefold())
        if node.link and identity in existing_nodes:
            continue
        node.group = effective_group
        if not node.country_code:
            node.country_code = detect_country(node.name, node.server)
        max_order += 1
        node.sort_order = max_order
        controller.state.nodes.append(node)
        if node.link:
            existing_nodes.add(identity)
        if first_new_id is None:
            first_new_id = node.id
        added += 1

    if first_new_id and select_imported:
        controller.state.selected_node_id = first_new_id

    selection_changed = controller.state.selected_node_id != previous_selected_id
    if selection_changed:
        controller._reset_auto_switch_state(reset_cooldown=True, reset_cycle=True)

    controller.nodes_changed.emit(controller.state.nodes)
    controller.selection_changed.emit(controller.selected_node)
    controller.save()
    QTimer.singleShot(500, controller._start_country_ip_resolution)

    should_auto_connect = (
        controller.state.settings.auto_connect_on_import
        if auto_connect is None
        else auto_connect
    )
    should_reconcile_running = auto_connect is not False and selection_changed and (
        controller.connected or controller._desired_connected
    )
    if added and (should_auto_connect or should_reconcile_running):
        controller._desired_connected = True
        controller._request_transition("new node imported")

    return added, errors


def remove_nodes(controller: AppController, node_ids: set[str]) -> None:
    if not node_ids:
        return
    removed_selected = controller.state.selected_node_id in node_ids
    should_reconcile = removed_selected and (controller.connected or controller._desired_connected)
    controller.state.nodes = [node for node in controller.state.nodes if node.id not in node_ids]
    if removed_selected:
        controller.state.selected_node_id = controller.state.nodes[0].id if controller.state.nodes else None
        controller._reset_auto_switch_state(reset_cooldown=True, reset_cycle=True)
    controller.nodes_changed.emit(controller.state.nodes)
    controller.selection_changed.emit(controller.selected_node)
    controller.save()
    if not should_reconcile:
        return
    if controller.state.selected_node_id is None:
        if controller._can_connect_without_selected_node():
            controller._request_transition("active node removed")
            return
        controller._desired_connected = False
        controller._request_transition("active node removed")
        return
    controller._desired_connected = True
    controller._request_transition("active node removed")


def update_node(controller: AppController, node_id: str, updates: dict) -> bool:
    node = controller._get_node_by_id(node_id)
    if not node:
        return False
    if "name" in updates:
        node.name = updates["name"]
    if "group" in updates:
        node.group = updates["group"]
    if "server" in updates:
        node.server = str(updates["server"])
    if "port" in updates:
        node.port = int(updates["port"] or 0)
    if "outbound" in updates and isinstance(updates["outbound"], dict):
        node.outbound = dict(updates["outbound"])
    if "link" in updates:
        node.link = str(updates["link"] or "")
    controller.nodes_changed.emit(controller.state.nodes)
    controller.save()
    if controller.connected or controller._desired_connected:
        controller._request_transition("node updated")
    return True


def bulk_update_nodes(controller: AppController, node_ids: set[str], operations: dict) -> int:
    group = operations.get("group", "")
    updated = 0
    for node in controller.state.nodes:
        if node.id not in node_ids:
            continue
        if group:
            node.group = group
        updated += 1
    if updated:
        controller.nodes_changed.emit(controller.state.nodes)
        controller.save()
    return updated


def get_all_groups(controller: AppController) -> list[str]:
    groups = {node.group for node in controller.state.nodes if node.group}
    groups.update(
        str(group).strip()
        for group in getattr(controller.state, "manual_groups", [])
        if str(group).strip()
    )
    other_groups = {
        str(group).strip()
        for group in groups
        if str(group).strip() and str(group).strip().casefold() != "default"
    }
    return ["Default", *sorted(other_groups, key=str.lower)]


def delete_group(controller: AppController, group: str) -> bool:
    """Delete a user group together with its nodes and linked subscriptions."""
    name = str(group or "").strip()
    if not name or name.casefold() == "default":
        return False
    key = name.casefold()

    subscriptions = list(getattr(controller.state, "subscriptions", []))
    removed_subscriptions = [
        item
        for item in subscriptions
        if str(item.get("group") or item.get("name") or "").strip().casefold() == key
    ]
    removed_subscription_ids = {
        str(item.get("id") or "").strip()
        for item in removed_subscriptions
        if str(item.get("id") or "").strip()
    }
    if removed_subscriptions:
        controller.state.subscriptions = [item for item in subscriptions if item not in removed_subscriptions]
        controller.subscriptions_changed.emit(list(controller.state.subscriptions))

    manual_groups = list(getattr(controller.state, "manual_groups", []))
    controller.state.manual_groups = [
        item for item in manual_groups if str(item or "").strip().casefold() != key
    ]
    node_ids = {
        node.id
        for node in controller.state.nodes
        if (node.group or "Default").strip().casefold() == key
        or (node.subscription_id and node.subscription_id in removed_subscription_ids)
    }
    changed = bool(removed_subscriptions or node_ids or len(manual_groups) != len(controller.state.manual_groups))
    if node_ids:
        remove_nodes(controller, node_ids)
    elif changed:
        controller.save()
    return changed


def reorder_nodes(controller: AppController, node_id: str, direction: str) -> None:
    ordered = sorted(controller.state.nodes, key=lambda node: node.sort_order)
    idx = next((i for i, node in enumerate(ordered) if node.id == node_id), None)
    if idx is None:
        return
    if direction == "up" and idx > 0:
        ordered[idx], ordered[idx - 1] = ordered[idx - 1], ordered[idx]
    elif direction == "down" and idx < len(ordered) - 1:
        ordered[idx], ordered[idx + 1] = ordered[idx + 1], ordered[idx]
    elif direction == "top" and idx > 0:
        node = ordered.pop(idx)
        ordered.insert(0, node)
    elif direction == "bottom" and idx < len(ordered) - 1:
        node = ordered.pop(idx)
        ordered.append(node)
    else:
        return
    for index, node in enumerate(ordered):
        node.sort_order = index + 1
    controller.nodes_changed.emit(controller.state.nodes)
    controller.save()


def set_selected_node(controller: AppController, node_id: str) -> None:
    if controller.state.selected_node_id == node_id:
        return
    controller.state.selected_node_id = node_id
    controller._reset_auto_switch_state(reset_cooldown=True, reset_cycle=True)
    controller.selection_changed.emit(controller.selected_node)
    controller.schedule_save()
    if controller.connected or controller._desired_connected:
        controller._desired_connected = True
        controller._request_transition("node switched")


# --- Подписки (subscriptions) ---------------------------------------------


def _utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def _maybe_base64_decode(text: str) -> str:
    """Подписки часто отдают base64-блоб со списком ссылок."""
    if "://" in text:
        return ""
    compact = "".join(text.split())
    if not compact:
        return ""
    for decoder in (base64.urlsafe_b64decode, base64.b64decode):
        try:
            padded = compact + "=" * (-len(compact) % 4)
            decoded = decoder(padded).decode("utf-8", errors="strict")
        except (binascii.Error, ValueError, UnicodeDecodeError):
            continue
        if "://" in decoded:
            return decoded
    return ""


def _parse_userinfo_header(value: str) -> dict:
    """Разбирает заголовок subscription-userinfo: upload=..; download=..; total=..; expire=.."""
    info: dict = {}
    if not value:
        return info
    for part in value.split(";"):
        if "=" not in part:
            continue
        key, _, raw = part.partition("=")
        key = key.strip().lower()
        raw = raw.strip()
        if not key:
            continue
        try:
            info[key] = int(raw)
        except (TypeError, ValueError):
            info[key] = raw
    return info


def _extract_userinfo_from_body(text: str) -> tuple[str, dict]:
    """Если тело — JSON вида {"user": {...}, "links": [...]}, достаёт инфо и ссылки."""
    stripped = (text or "").strip()
    if not stripped.startswith("{"):
        return text, {}
    try:
        data = json.loads(stripped)
    except (ValueError, TypeError):
        return text, {}
    if not isinstance(data, dict):
        return text, {}
    info: dict = {}
    user = data.get("user")
    if isinstance(user, dict):
        info = {str(k): v for k, v in user.items()}
    elif isinstance(data.get("userStatus"), str) or "username" in data:
        info = {str(k): v for k, v in data.items() if k != "links"}
    for key in (
        "profileTitle",
        "subscriptionName",
        "supportUrl",
        "profileUrl",
        "telegramUrl",
        "announcement",
        "announcementUrl",
        "providerId",
        "profileUpdateInterval",
    ):
        if key in data and data.get(key) not in (None, ""):
            info[key] = data.get(key)
    premium = data.get("premiumFeatures")
    if isinstance(premium, dict):
        info["premiumFeatures"] = {str(key): str(value) for key, value in premium.items()}
    direct_premium = {
        key: str(data.get(key))
        for key in HAPP_PREMIUM_PARAMETERS
        if data.get(key) not in (None, "")
    }
    if direct_premium:
        info["premiumFeatures"] = {
            **dict(info.get("premiumFeatures") or {}),
            **direct_premium,
        }
    links = data.get("links")
    if isinstance(links, list) and links:
        links_text = "\n".join(str(item) for item in links if item)
        return links_text, info
    return text, info


def _merge_subscription_info(*parts: dict | None) -> dict:
    result: dict = {}
    premium: dict[str, str] = {}
    for part in parts:
        if not isinstance(part, dict):
            continue
        nested = part.get("premiumFeatures")
        if isinstance(nested, dict):
            premium.update({str(key): str(value) for key, value in nested.items()})
        result.update({key: value for key, value in part.items() if key != "premiumFeatures"})
    if premium:
        result["premiumFeatures"] = premium
    return result


def _extract_happ_body_metadata(text: str) -> tuple[str, dict]:
    """Extract Happ directives from ``#key value`` subscription comments."""
    if not text or "#" not in text:
        return text, {}
    kept: list[str] = []
    premium: dict[str, str] = {}
    info: dict = {}
    for line in text.splitlines():
        match = re.match(r"^\s*#\s*([A-Za-z0-9_-]+)\s*:?[ \t]*(.*?)\s*$", line)
        if not match:
            kept.append(line)
            continue
        key = match.group(1).strip().lower().replace("_", "-")
        value = match.group(2).strip()
        if key not in _HAPP_BODY_METADATA_KEYS:
            kept.append(line)
            continue
        if key in HAPP_PREMIUM_PARAMETERS:
            premium[key] = value
        elif key in {"providerid", "provider-id"}:
            info["providerId"] = value
        elif key == "profile-title":
            info["profileTitle"] = _decode_profile_header(value)
        elif key == "support-url":
            info["supportUrl"] = value
        elif key == "profile-web-page-url":
            info["profileUrl"] = value
        elif key == "telegram-url":
            info["telegramUrl"] = value
        elif key == "announce":
            info["announcement"] = _decode_profile_header(value)
        elif key == "announce-url":
            info["announcementUrl"] = value
    if premium:
        info["premiumFeatures"] = premium
    return "\n".join(kept), info


def _metadata_from_subscription_url(url: str) -> dict:
    parsed = urlparse(str(url or "").strip())
    values: dict[str, list[str]] = {}
    for raw in (parsed.query, parsed.fragment.lstrip("?")):
        if raw:
            values.update(parse_qs(raw, keep_blank_values=True))
    lowered = {str(key).lower().replace("_", "-"): items for key, items in values.items()}
    for key in ("providerid", "provider-id"):
        items = lowered.get(key)
        if items and str(items[0]).strip():
            return {"providerId": str(items[0]).strip()}
    return {}


def _subscription_name_from_info(info: dict | None) -> str:
    if not isinstance(info, dict):
        return ""
    for key in ("profileTitle", "subscriptionName", "name", "title"):
        value = str(info.get(key) or "").strip()
        if value:
            return value[:160]
    return ""


def _premium_subscription_url(url: str, info: dict | None) -> str:
    premium = info.get("premiumFeatures") if isinstance(info, dict) else None
    if not isinstance(premium, dict):
        return url
    replacement = str(premium.get("new-url") or "").strip()
    if replacement:
        parsed = urlparse(replacement)
        if parsed.scheme in {"http", "https"} and parsed.netloc:
            return replacement
    new_domain = str(premium.get("new-domain") or "").strip()
    if new_domain:
        domain = urlparse("//" + new_domain).netloc
        source = urlparse(url)
        if domain and source.scheme in {"http", "https"}:
            return source._replace(netloc=domain).geturl()
    return url


def _migrate_subscription_url(controller: AppController, old_url: str, new_url: str) -> bool:
    if not new_url or new_url == old_url:
        return False
    if _find_subscription(controller, new_url) is not None:
        return False
    old_id = _subscription_id(controller, old_url)
    new_id = str(uuid.uuid5(uuid.NAMESPACE_URL, new_url.strip()))
    for node in controller.state.nodes:
        if node.subscription_id == old_id:
            node.subscription_id = new_id
    existing = _find_subscription(controller, old_url)
    if existing is not None:
        existing["url"] = new_url
        existing["id"] = new_id
    return True


_SUBSCRIPTION_CLIENT_PROFILES: tuple[tuple[str, dict[str, str]], ...] = (
    (
        "Happ Windows",
        {
            "User-Agent": HAPP_WINDOWS_USER_AGENT,
            "Accept": "*/*",
            "Accept-Language": "ru-RU,en,*",
            "Profile-Update-Interval": "24",
            "X-App-Version": "2.18.3",
            "X-Device-Locale": "RU",
            "X-Device-Model": "Windows_x86_64",
            "X-Device-Os": "Windows",
            "X-Hwid": DEFAULT_SUBSCRIPTION_HWID,
            "X-Ver-Os": "11_10.0.26200",
        },
    ),
    (
        "SFA",
        {
            "User-Agent": "SFA/1.11.0",
            "Accept": "application/json,*/*",
            "Profile-Update-Interval": "24",
        },
    ),
    (
        "Clash Verge",
        {
            "User-Agent": "ClashVerge/2.0.0",
            "Accept": "text/yaml,application/yaml,*/*",
            "Profile-Update-Interval": "24",
        },
    ),
    (
        "Clash Meta",
        {
            "User-Agent": "clash.meta",
            "Accept": "text/yaml,application/yaml,*/*",
            "Profile-Update-Interval": "24",
        },
    ),
    (
        "FlClashX",
        {
            "User-Agent": "FlClashX/1.0",
            "Accept": "text/yaml,application/yaml,*/*",
            "Profile-Update-Interval": "24",
        },
    ),
    (
        "Happ",
        {
            "User-Agent": HAPP_WINDOWS_USER_AGENT,
            "Accept": "*/*",
            "Profile-Update-Interval": "24",
        },
    ),
    (
        "Lumen",
        {
            "User-Agent": "LumenKVN-Subscription/1.0",
            "Accept": "*/*",
        },
    ),
)


def _decode_profile_header(value: str) -> str:
    text = str(value or "").strip()
    if text.lower().startswith("base64:"):
        raw = text.split(":", 1)[1].strip()
        try:
            return base64.b64decode(raw + "=" * (-len(raw) % 4)).decode("utf-8", errors="replace").strip()
        except Exception:
            return text
    return text


def _extract_subscription_metadata(headers: object, profile_name: str) -> dict:
    info: dict = {"clientProfile": profile_name}
    try:
        profile_title = _decode_profile_header(headers.get("profile-title", ""))
        support_url = _first_header(headers, "support-url", "support_url", "support-link", "support")
        profile_url = _first_header(
            headers,
            "profile-web-page-url",
            "profile-url",
            "profile_url",
            "panel-url",
            "panel_url",
            "sub-web-page-url",
            "subscription-url",
        )
        telegram_url = _first_header(headers, "telegram-url", "telegram_url", "telegram-link", "telegram")
        announcement = _decode_profile_header(_first_header(headers, "announce", "announcement"))
        announcement_url = _first_header(headers, "announce-url", "announcement-url")
        provider_id = _first_header(headers, "providerid", "provider-id", "provider_id")
        update_interval = _first_header(headers, "profile-update-interval")
        content_disposition = _first_header(headers, "content-disposition")
    except Exception:
        return info
    if not profile_title and content_disposition:
        filename_match = re.search(
            r"filename\*?=(?:UTF-8''|\")?([^\";]+)",
            content_disposition,
            flags=re.IGNORECASE,
        )
        if filename_match:
            candidate = unquote(filename_match.group(1)).strip().strip('"')
            candidate = re.sub(r"\.(?:ya?ml|json|txt|conf)$", "", candidate, flags=re.IGNORECASE)
            if candidate.lower() not in {"config", "subscription", "download"} and not candidate.isdigit():
                profile_title = candidate[:160]
    if profile_title:
        info["profileTitle"] = profile_title
    if support_url:
        info["supportUrl"] = support_url
    if profile_url:
        info["profileUrl"] = profile_url
    if telegram_url:
        info["telegramUrl"] = telegram_url
    if announcement:
        info["announcement"] = announcement
    if announcement_url:
        info["announcementUrl"] = announcement_url
    if provider_id:
        info["providerId"] = provider_id
    if update_interval:
        info["profileUpdateInterval"] = update_interval
    premium = {
        key: _first_header(headers, key, key.replace("-", "_"))
        for key in HAPP_PREMIUM_PARAMETERS
    }
    premium = {key: value for key, value in premium.items() if value != ""}
    if premium:
        info["premiumFeatures"] = premium
    return info


def _first_header(headers: object, *names: str) -> str:
    for name in names:
        try:
            value = str(headers.get(name, "") or "").strip()
        except Exception:
            value = ""
        if value:
            return value
    return ""


def _fetch_subscription_with_headers(
    url: str,
    profile_name: str,
    headers: dict[str, str],
    *,
    direct: bool = True,
    proxy_url: str = "",
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> tuple[str, dict]:
    """Загружает подписку и возвращает (текст_со_ссылками, userinfo).

    userinfo берётся из HTTP-заголовка subscription-userinfo и/или из JSON-тела.
    При ``direct=True`` системный/ENV-прокси отключён, а сборка использует
    одноразовый helper-процесс и физический маршрут. При ``direct=False``
    запрос идёт через переданный локальный proxy либо через обычный системный
    сетевой стек, который перехватывается активным TUN.
    """
    _raise_if_subscription_cancelled(cancelled)
    try:
        response = fetch_subscription_http(
            url,
            dict(headers),
            timeout=20,
            max_bytes=MAX_SUBSCRIPTION_BYTES,
            use_proxy_tun=not direct,
            proxy_url=proxy_url,
            cancelled=cancelled,
            response_opened=response_opened,
            response_closed=response_closed,
        )
    except SubscriptionFetcherCancelled as exc:
        raise SubscriptionFetchCancelled(str(exc)) from exc
    raw = response.body
    header_value = response.headers.get("subscription-userinfo", "")
    metadata = _extract_subscription_metadata(response.headers, profile_name)
    userinfo = _merge_subscription_info(_parse_userinfo_header(header_value), metadata)
    text = raw.decode("utf-8", errors="replace").strip()
    text, directive_info = _extract_happ_body_metadata(text)
    # JSON-тело (например, формат с {"user": {...}, "links": [...]}).
    text, body_info = _extract_userinfo_from_body(text)
    # Данные из тела приоритетнее заголовка.
    userinfo = _merge_subscription_info(userinfo, directive_info, body_info)
    decoded = _maybe_base64_decode(text)
    return (decoded or text), userinfo


def _fetch_subscription(url: str, *, user_agent: str = "LumenKVN-Subscription/1.0") -> tuple[str, dict]:
    return _fetch_subscription_with_headers(url, "Lumen", {"User-Agent": user_agent, "Accept": "*/*"})


def _fetch_subscription_happ(url: str) -> tuple[str, dict]:
    return _fetch_subscription_with_headers(
        url,
        "Happ Windows",
        {"User-Agent": HAPP_WINDOWS_USER_AGENT, "Accept": "*/*"},
    )


def _is_tls_eof_error(exc: BaseException) -> bool:
    text = str(exc).lower()
    return (
        "tls/ssl connection has been closed" in text
        or "_ssl.c:1010" in text
        or "unexpected eof" in text
        or "eof occurred in violation of protocol" in text
    )


def _subscription_proxy_tun_hint(use_proxy_tun: bool) -> str:
    if use_proxy_tun:
        return (
            "Загрузка через прокси/TUN уже включена — проверьте подключение Lumen "
            "или временно отключите эту настройку."
        )
    return (
        "Попробуйте включить «Загружать подписки через прокси/TUN» "
        "в Настройки → Подписки."
    )


def _friendly_subscription_fetch_error(exc: BaseException, *, use_proxy_tun: bool) -> str | None:
    raw = str(exc or "").strip()
    low = raw.casefold()
    hint = _subscription_proxy_tun_hint(use_proxy_tun)

    if any(
        token in low
        for token in (
            "getaddrinfo failed",
            "errno 11001",
            "name or service not known",
            "nodename nor servname provided",
            "no address associated with hostname",
            "temporary failure in name resolution",
            "eai_again",
            "eai_noname",
        )
    ):
        return f"Не удалось определить адрес сервера подписки: ошибка DNS. Проверьте системный DNS. {hint}"

    if "certificate verify failed" in low or "cert_verify_failed" in low:
        return (
            "Не удалось проверить TLS-сертификат сервера подписки. "
            "Проверьте дату и время Windows, а также правильность ссылки подписки."
        )

    http_match = re.search(r"http error\s+(\d{3})", low)
    if http_match:
        status = int(http_match.group(1))
        if status == 401:
            return "Сервер подписки отклонил авторизацию (HTTP 401). Проверьте ссылку, HWID и срок подписки."
        if status == 403:
            return f"Сервер подписки запретил доступ (HTTP 403). Проверьте ссылку и HWID. {hint}"
        if status == 404:
            return "Подписка не найдена (HTTP 404). Возможно, ссылка устарела или была удалена."
        if status == 429:
            return "Сервер подписки временно ограничил частоту запросов (HTTP 429). Повторите позже."
        if 500 <= status <= 599:
            return f"Сервер подписки временно недоступен (HTTP {status}). Повторите попытку позже."

    if any(token in low for token in ("timed out", "timeout", "winerror 10060")):
        return f"Сервер подписки не ответил вовремя: превышено время ожидания. {hint}"
    if any(
        token in low
        for token in (
            "winerror 10054",
            "connection reset",
            "forcibly closed",
            "принудительно разорвал",
            "connection aborted",
        )
    ):
        return f"Соединение с сервером подписки было принудительно разорвано. {hint}"
    if any(token in low for token in ("winerror 10061", "connection refused")):
        return f"Сервер подписки отклонил соединение. {hint}"
    if any(
        token in low
        for token in (
            "winerror 10051",
            "winerror 10065",
            "network is unreachable",
            "no route to host",
            "physical internet interface",
            "физический интернет-интерфейс",
            "direct network",
        )
    ):
        return f"Не удалось построить прямой маршрут к серверу подписки. {hint}"
    if _is_tls_eof_error(exc) or any(
        token in low for token in ("ssl eof", "tls handshake", "wrong version number")
    ):
        return f"Защищённое соединение с сервером подписки было прервано во время TLS-обмена. {hint}"
    if "tunnel connection failed" in low or "proxy error" in low:
        return f"Прокси/TUN не смог подключиться к серверу подписки. {hint}"
    return None


def _append_subscription_fetch_error(
    attempts: list[str],
    profile_name: str,
    exc: BaseException,
    *,
    use_proxy_tun: bool,
) -> None:
    friendly = _friendly_subscription_fetch_error(exc, use_proxy_tun=use_proxy_tun)
    message = friendly or f"{profile_name}: {exc}"
    if message not in attempts:
        attempts.append(message)


def _parsed_nodes_are_usable(nodes: list) -> bool:
    for node in nodes:
        normalize_node_outbound(node)
        if validate_node_outbound(node) is None:
            return True
    return False


def _node_validation_errors(nodes: list) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    for node in nodes:
        normalize_node_outbound(node)
        problem = validate_node_outbound(node)
        if problem and problem not in seen:
            errors.append(problem)
            seen.add(problem)
    return errors


def _derive_subscription_name(url: str) -> str:
    if is_happ_link(url):
        return "Подписка Happ"
    host = urlparse(url).hostname or ""
    host = host.strip()
    if host:
        return f"Подписка {host}"
    return "Подписка"


def _happ_direct_payload(decrypted: str) -> tuple[str, dict, list[str]]:
    """Оформляет расшифрованное тело happ-подписки (список ссылок / base64 / JSON)."""
    text, directive_info = _extract_happ_body_metadata(decrypted)
    text, body_info = _extract_userinfo_from_body(text)
    merged_info = _merge_subscription_info({"clientProfile": "Happ"}, directive_info, body_info)
    text = _maybe_base64_decode(text) or text
    nodes, errors = parse_links_text(text)
    if nodes and _parsed_nodes_are_usable(nodes):
        return text, merged_info, errors
    detail = "; ".join((_node_validation_errors(nodes) or errors)[:2]) or "нет подходящих серверов"
    return "", merged_info, [f"Happ: {detail}"]


def _find_subscription(controller: AppController, url: str) -> dict | None:
    for sub in getattr(controller.state, "subscriptions", []):
        if sub.get("url") == url:
            return sub
    return None


def _subscription_id(controller: AppController, url: str) -> str:
    subscription = _find_subscription(controller, url)
    if subscription is not None:
        value = str(subscription.get("id") or "").strip()
        if value:
            return value
    return str(uuid.uuid5(uuid.NAMESPACE_URL, url.strip()))


def fetch_subscription_payload(
    url: str,
    *,
    user_agent: str = "",
    hwid: str = DEFAULT_SUBSCRIPTION_HWID,
    use_real_hwid: bool = True,
    use_proxy_tun: bool = False,
    proxy_url: str = "",
    converter_url: str = "",
    cancelled=None,
    response_opened=None,
    response_closed=None,
) -> tuple[str, dict, list[str]]:
    """Загружает подписку по сети и возвращает (текст_ссылок, userinfo, errors).

    Только сеть, не трогает controller/state — безопасно вызывать в фоновом потоке.
    При неудаче текст пустой, а ошибки лежат в errors.
    """
    url = (url or "").strip()
    _raise_if_subscription_cancelled(cancelled)
    if not url:
        return "", {}, ["Пустой URL подписки"]
    # Закрытые ссылки Happ (happ://crypt*): сначала расшифровываем. Результат —
    # либо реальный URL подписки (грузим ниже штатно), либо готовый текст ссылок.
    if is_happ_crypt_link(url):
        try:
            decrypted = decrypt_happ_link(url).strip()
        except HappDecryptError as exc:
            return "", {}, [f"Happ: {exc}"]
        if not decrypted:
            return "", {}, ["Happ: пустой результат расшифровки"]
        if decrypted.lower().startswith(("http://", "https://")):
            url = decrypted
        else:
            return _happ_direct_payload(decrypted)
    if converter_url:
        try:
            url = _subscription_converter_target(converter_url, url)
        except ValueError as exc:
            return "", {}, [str(exc)]
    attempts: list[str] = []
    first_userinfo: dict = {}
    fetch_options = {"direct": not use_proxy_tun}
    if proxy_url:
        fetch_options["proxy_url"] = str(proxy_url).strip()
    if cancelled is not None:
        fetch_options["cancelled"] = cancelled
    if response_opened is not None:
        fetch_options["response_opened"] = response_opened
    if response_closed is not None:
        fetch_options["response_closed"] = response_closed
    request_hwid = _resolve_subscription_hwid(hwid, use_real_hwid=use_real_hwid)
    if "\r" in request_hwid or "\n" in request_hwid:
        return "", {}, ["HWID не должен содержать переносы строк"]
    if len(request_hwid) > 256:
        return "", {}, ["HWID не должен быть длиннее 256 символов"]
    profiles = [
        (profile_name, {**headers, "X-Hwid": request_hwid})
        for profile_name, headers in _SUBSCRIPTION_CLIENT_PROFILES
    ]
    custom_user_agent = str(user_agent or "").strip()
    if custom_user_agent:
        profiles.insert(
            0,
            (
                "Custom",
                {
                    "User-Agent": custom_user_agent,
                    "Accept": "text/yaml,application/yaml,application/json,*/*",
                    "Profile-Update-Interval": "24",
                    "X-Hwid": request_hwid,
                },
            ),
        )
    for profile_name, headers in profiles:
        _raise_if_subscription_cancelled(cancelled)
        try:
            text, userinfo = _fetch_subscription_with_headers(
                url,
                profile_name,
                headers,
                **fetch_options,
            )
            userinfo = _merge_subscription_info(_metadata_from_subscription_url(url), userinfo)
            network_path = "proxy-tun" if use_proxy_tun else "direct"
            userinfo = {**userinfo, "networkPath": network_path}
            if userinfo and not first_userinfo:
                first_userinfo = dict(userinfo)
            nodes, errors = parse_links_text(text)
            if nodes and _parsed_nodes_are_usable(nodes):
                return text, userinfo, errors
            validation_errors = _node_validation_errors(nodes)
            detail = "; ".join((validation_errors or errors or [])[:2]) or "нет подходящих серверов"
            attempts.append(f"{profile_name}: {detail}")
        except SubscriptionFetchCancelled:
            raise
        except Exception as exc:  # noqa: BLE001 - пробуем следующий профиль клиента
            if _is_tls_eof_error(exc):
                try:
                    text, userinfo = _fetch_subscription_with_headers(
                        url,
                        profile_name,
                        headers,
                        **fetch_options,
                    )
                    userinfo = _merge_subscription_info(_metadata_from_subscription_url(url), userinfo)
                    network_path = "proxy-tun" if use_proxy_tun else "direct"
                    userinfo = {**userinfo, "networkPath": network_path}
                    if userinfo and not first_userinfo:
                        first_userinfo = dict(userinfo)
                    nodes, errors = parse_links_text(text)
                    if nodes and _parsed_nodes_are_usable(nodes):
                        return text, {**userinfo, "networkPath": network_path}, errors
                    validation_errors = _node_validation_errors(nodes)
                    detail = "; ".join((validation_errors or errors or [])[:2]) or "нет подходящих серверов"
                    attempts.append(f"{profile_name} direct: {detail}")
                    continue
                except SubscriptionFetchCancelled:
                    raise
                except Exception as direct_exc:
                    _append_subscription_fetch_error(
                        attempts,
                        f"{profile_name} direct",
                        direct_exc,
                        use_proxy_tun=use_proxy_tun,
                    )
                    continue
            _append_subscription_fetch_error(
                attempts,
                profile_name,
                exc,
                use_proxy_tun=use_proxy_tun,
            )
    return "", first_userinfo, attempts or ["Не удалось загрузить подписку"]


def _subscription_converter_target(template: str, source_url: str) -> str:
    value = str(template or "").strip()
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ValueError("URL конвертера должен начинаться с http:// или https://")
    if "{url}" not in value and "{raw_url}" not in value:
        raise ValueError("URL конвертера должен содержать {url} или {raw_url}")
    return value.replace("{url}", quote(source_url, safe="")).replace("{raw_url}", source_url)


def _compile_subscription_patterns(value: str, label: str) -> tuple[list[re.Pattern[str]], list[str]]:
    patterns: list[re.Pattern[str]] = []
    errors: list[str] = []
    for line_number, raw in enumerate(str(value or "").splitlines(), start=1):
        expression = raw.strip()
        if not expression:
            continue
        try:
            patterns.append(re.compile(expression, re.IGNORECASE))
        except re.error as exc:
            errors.append(f"{label}, строка {line_number}: {exc}")
    return patterns, errors


def _filter_subscription_nodes(controller: AppController, nodes: list) -> tuple[list, list[str]]:
    settings = controller.state.settings
    includes, include_errors = _compile_subscription_patterns(
        getattr(settings, "subscription_include_regex", ""),
        "Include regex",
    )
    excludes, exclude_errors = _compile_subscription_patterns(
        getattr(settings, "subscription_exclude_regex", ""),
        "Exclude regex",
    )
    errors = [*include_errors, *exclude_errors]
    if errors:
        return [], errors
    if not includes and not excludes:
        return list(nodes), []
    filtered = []
    for node in nodes:
        haystack = "\n".join(
            (
                str(getattr(node, "name", "") or ""),
                str(getattr(node, "server", "") or ""),
                str(getattr(node, "scheme", "") or ""),
                str(getattr(node, "link", "") or ""),
            )
        )
        if includes and not any(pattern.search(haystack) for pattern in includes):
            continue
        if excludes and any(pattern.search(haystack) for pattern in excludes):
            continue
        filtered.append(node)
    return filtered, []


def _happ_enabled(value: object) -> bool:
    return str(value or "").strip().lower() in {"1", "true", "yes", "on"}


def _apply_happ_premium_settings(controller: AppController, info: dict) -> list[str]:
    """Apply only Happ commands that have a direct, safe Windows equivalent."""
    premium = info.get("premiumFeatures") if isinstance(info, dict) else None
    if not isinstance(premium, dict) or not premium:
        return []
    settings = getattr(getattr(controller, "state", None), "settings", None)
    if settings is None:
        return []
    applied: list[str] = []

    def assign(key: str, attribute: str, value: object) -> None:
        if key not in premium:
            return
        if getattr(settings, attribute, None) != value:
            setattr(settings, attribute, value)
        applied.append(key)

    if "subscription-always-hwid-enable" in premium and _happ_enabled(
        premium["subscription-always-hwid-enable"]
    ):
        assign("subscription-always-hwid-enable", "subscription_use_real_hwid", True)
    if "subscription-autoconnect" in premium:
        assign("subscription-autoconnect", "auto_connect_last", _happ_enabled(premium["subscription-autoconnect"]))
    if "subscription-auto-update-enable" in premium:
        enabled = _happ_enabled(premium["subscription-auto-update-enable"])
        current = int(getattr(settings, "subscription_auto_update_minutes", 240) or 0)
        assign("subscription-auto-update-enable", "subscription_auto_update_minutes", max(1, current or 240) if enabled else 0)
    if "fragmentation-enable" in premium:
        enabled = _happ_enabled(premium["fragmentation-enable"])
        assign("fragmentation-enable", "enable_xray_fragment", enabled)
        settings.enable_final_fragment = enabled
    assign("fragmentation-packets", "fragment_packets", str(premium.get("fragmentation-packets") or "tlshello").strip())
    assign("fragmentation-length", "fragment_length", str(premium.get("fragmentation-length") or "50-100").strip())
    assign("fragmentation-interval", "fragment_delay", str(premium.get("fragmentation-interval") or "10-20").strip())
    if "ping-type" in premium:
        ping_method = {
            "proxy": "real",
            "tcp": "tcping",
            "icmp": "icmp",
        }.get(str(premium["ping-type"]).strip().lower())
        if ping_method:
            assign("ping-type", "ping_method", ping_method)
    if "change-user-agent" in premium:
        assign("change-user-agent", "subscription_user_agent", str(premium["change-user-agent"]).strip())
    if "mux-enable" in premium:
        assign("mux-enable", "multiplex_enabled", _happ_enabled(premium["mux-enable"]))
    if "mux-tcp-connections" in premium:
        try:
            concurrency = max(-1, min(1024, int(str(premium["mux-tcp-connections"]).strip())))
        except (TypeError, ValueError):
            concurrency = None
        if concurrency is not None:
            assign("mux-tcp-connections", "multiplex_concurrency", concurrency)
    if "exclude-routes" in premium:
        routing = getattr(getattr(controller, "state", None), "routing", None)
        if routing is not None:
            values = [
                value
                for value in re.split(r"[\s,;]+", str(premium["exclude-routes"] or "").strip())
                if value
            ]
            routing.tun_route_exclude_address = values
            applied.append("exclude-routes")
    # Sniffing is part of both generated Xray and sing-box runtime configs.
    if "sniffing-enable" in premium and _happ_enabled(premium["sniffing-enable"]):
        applied.append("sniffing-enable")
    return list(dict.fromkeys(applied))


def _apply_subscription_payload(
    controller: AppController,
    url: str,
    group: str,
    fetched: tuple[str, dict, list[str]],
    *,
    replace_existing_group: bool = False,
) -> tuple[int, list[str], dict]:
    """Apply a fetched subscription as one in-memory transaction."""
    chosen_text, chosen_userinfo, chosen_errors = fetched
    result_info = dict(chosen_userinfo or {})
    result_info["_lumen_applied"] = False
    if not chosen_text:
        return 0, list(chosen_errors), result_info

    subscription_id = _subscription_id(controller, url)
    parsed_nodes, parse_errors = parse_links_text(chosen_text)
    parsed_nodes, filter_errors = _filter_subscription_nodes(controller, parsed_nodes)
    if filter_errors:
        return 0, [*chosen_errors, *parse_errors, *filter_errors], result_info
    prepared = []
    validation_errors: list[str] = []
    seen_links: set[str] = set()
    for node in parsed_nodes:
        normalize_node_outbound(node)
        problem = validate_node_outbound(node)
        if problem:
            validation_errors.append(problem)
            continue
        if not node.link or node.link in seen_links:
            continue
        seen_links.add(node.link)
        node.group = group
        node.subscription_id = subscription_id
        if not node.country_code:
            node.country_code = detect_country(node.name, node.server)
        prepared.append(node)

    if not prepared:
        errors = [*chosen_errors, *parse_errors, *validation_errors]
        if not errors:
            errors.append("Подписка не содержит серверов, подходящих под regex-фильтры")
        return 0, errors, result_info

    old_nodes = [
        node
        for node in controller.state.nodes
        if node.subscription_id == subscription_id
        or (
            replace_existing_group
            and not node.subscription_id
            and (node.group or "Default") == group
        )
    ]
    old_ids = {node.id for node in old_nodes}
    old_by_link = {node.link: node for node in old_nodes if node.link}
    occupied_nodes = {
        (node.link, (node.group or "Default").strip().casefold())
        for node in controller.state.nodes
        if node.id not in old_ids and node.link
    }
    prepared = [
        node
        for node in prepared
        if (node.link, (node.group or "Default").strip().casefold()) not in occupied_nodes
    ]
    if not prepared:
        return 0, [*chosen_errors, "Все серверы подписки уже есть в этой группе"], result_info

    selected_id = controller.state.selected_node_id
    selected_old = next((node for node in old_nodes if node.id == selected_id), None)
    max_order = max(
        (node.sort_order for node in controller.state.nodes if node.id not in old_ids),
        default=0,
    )
    for node in prepared:
        previous = old_by_link.get(node.link)
        if previous is not None:
            node.id = previous.id
            node.sort_order = previous.sort_order
            node.ping_ms = previous.ping_ms
            node.speed_mbps = previous.speed_mbps
            node.is_alive = previous.is_alive
            node.ping_history = list(previous.ping_history)
            node.speed_history = list(previous.speed_history)
        else:
            max_order += 1
            node.sort_order = max_order

    remaining = [node for node in controller.state.nodes if node.id not in old_ids]
    controller.state.nodes = [*remaining, *prepared]

    effective_url = _premium_subscription_url(url, result_info)
    if effective_url != url and _migrate_subscription_url(controller, url, effective_url):
        result_info["_lumen_effective_url"] = effective_url

    premium_applied = _apply_happ_premium_settings(controller, result_info)
    if premium_applied:
        result_info["premiumApplied"] = premium_applied

    reconnect_needed = False
    if selected_old is not None:
        replacement = next((node for node in prepared if node.link == selected_old.link), None)
        if replacement is None:
            replacement = prepared[0]
            reconnect_needed = True
        controller.state.selected_node_id = replacement.id

    controller.nodes_changed.emit(controller.state.nodes)
    controller.selection_changed.emit(controller.selected_node)
    result_info["_lumen_applied"] = True
    if reconnect_needed and (controller.connected or controller._desired_connected):
        controller._desired_connected = True
        controller._request_transition("active subscription updated")
    errors = [*chosen_errors, *parse_errors, *validation_errors]
    return len(prepared), errors, result_info


def _import_subscription_payload(
    controller: AppController,
    url: str,
    group: str,
    *,
    replace_existing_group: bool = False,
    prefer_metadata_name: bool = False,
) -> tuple[int, list[str], dict]:
    # Синхронный путь (блокирует поток). Оставлен для обратной совместимости.
    settings = controller.state.settings
    fetched = fetch_subscription_payload(
        url,
        user_agent=getattr(settings, "subscription_user_agent", ""),
        hwid=getattr(settings, "subscription_hwid", DEFAULT_SUBSCRIPTION_HWID),
        use_real_hwid=bool(getattr(settings, "subscription_use_real_hwid", True)),
        converter_url=(
            getattr(settings, "subscription_converter_url", "")
            if getattr(settings, "subscription_converter_enabled", False)
            else ""
        ),
    )
    effective_group = group
    if prefer_metadata_name:
        effective_group = _subscription_name_from_info(fetched[1]) or group
    added, errors, info = _apply_subscription_payload(
        controller, url, effective_group, fetched, replace_existing_group=replace_existing_group
    )
    info["_lumen_group"] = effective_group
    return added, errors, info


def _record_subscription(
    controller: AppController,
    url: str,
    group: str,
    node_count: int,
    userinfo: dict | None = None,
) -> None:
    now = _utc_now_iso()
    info = dict(userinfo) if isinstance(userinfo, dict) else {}
    info.pop("_lumen_applied", None)
    subscription_id = _subscription_id(controller, url)
    existing = _find_subscription(controller, url)
    if existing is not None:
        existing["id"] = subscription_id
        existing["name"] = group
        existing["group"] = group
        existing["updated_at"] = now
        existing["node_count"] = node_count
        # Сохраняем старую инфо, если новая не пришла.
        if info:
            existing["userinfo"] = info
        elif "userinfo" not in existing:
            existing["userinfo"] = {}
    else:
        controller.state.subscriptions.append(
            {
                "id": subscription_id,
                "url": url,
                "name": group,
                "group": group,
                "updated_at": now,
                "node_count": node_count,
                "userinfo": info,
            }
        )
    controller.subscriptions_changed.emit(list(controller.state.subscriptions))
    controller.save()


def import_subscription(
    controller: AppController, url: str, name: str | None = None
) -> tuple[int, list[str]]:
    url = (url or "").strip()
    if not url:
        return 0, ["Пустой URL подписки"]
    existing = _find_subscription(controller, url)
    if existing is not None:
        # Подписка с таким URL уже есть. Если пользователь задал новое имя —
        # переименовываем группу (у самой записи и у её узлов), иначе сохраняем
        # прежнее. Затем обновляем содержимое.
        new_name = (name or "").strip()
        old_group = (existing.get("group") or "").strip()
        if new_name and new_name != old_group:
            subscription_id = _subscription_id(controller, url)
            if old_group:
                for node in controller.state.nodes:
                    if node.subscription_id == subscription_id or (
                        not node.subscription_id
                        and (node.group or "Default") == old_group
                    ):
                        node.group = new_name
                        node.subscription_id = subscription_id
            existing["name"] = new_name
            existing["group"] = new_name
            controller.nodes_changed.emit(controller.state.nodes)
            controller.subscriptions_changed.emit(list(controller.state.subscriptions))
            controller.save()
        return update_subscription(controller, url)
    explicit_group = (name or "").strip()
    group = explicit_group or _derive_subscription_name(url)
    added, errors, userinfo = _import_subscription_payload(
        controller,
        url,
        group,
        prefer_metadata_name=not bool(explicit_group),
    )
    group = str(userinfo.pop("_lumen_group", group) or group)
    effective_url = str(userinfo.pop("_lumen_effective_url", url) or url)
    if userinfo.pop("_lumen_applied", False):
        _record_subscription(controller, effective_url, group, added, userinfo)
    return added, errors


def update_subscription(controller: AppController, url: str) -> tuple[int, list[str]]:
    url = (url or "").strip()
    sub = _find_subscription(controller, url)
    if sub is None:
        return 0, ["Подписка не найдена"]
    group = (sub.get("group") or "").strip() or _derive_subscription_name(url)
    added, errors, userinfo = _import_subscription_payload(
        controller, url, group, replace_existing_group=True
    )
    userinfo.pop("_lumen_group", None)
    effective_url = str(userinfo.pop("_lumen_effective_url", url) or url)
    if userinfo.pop("_lumen_applied", False):
        _record_subscription(controller, effective_url, group, added, userinfo)
    return added, errors


def update_all_subscriptions(controller: AppController) -> tuple[int, list[str]]:
    total_added = 0
    all_errors: list[str] = []
    for sub in list(controller.state.subscriptions):
        url = sub.get("url") or ""
        if not url:
            continue
        added, errors = update_subscription(controller, url)
        total_added += added
        all_errors.extend(errors)
    return total_added, all_errors


def apply_fetched_subscription(
    controller: AppController,
    url: str,
    name: str | None,
    kind: str,
    text: str,
    userinfo: dict | None,
    errors: list[str] | None,
) -> tuple[int, list[str]]:
    """Применяет подписку, загруженную в фоне (вызывать в GUI-потоке).

    kind: "import" — добавить/обновить по ссылке; "update" — обновить существующую.
    """
    url = (url or "").strip()
    if not url:
        return 0, ["Пустой URL подписки"]
    existing = _find_subscription(controller, url)

    if kind == "import" and existing is None:
        group = (
            (name or "").strip()
            or _subscription_name_from_info(userinfo)
            or _derive_subscription_name(url)
        )
        replace = False
    else:
        if existing is None:
            return 0, ["Подписка не найдена"]
        if kind == "import":
            # Подписка уже есть: при новом имени переименовываем группу и узлы.
            new_name = (name or "").strip()
            old_group = (existing.get("group") or "").strip()
            if new_name and new_name != old_group:
                subscription_id = _subscription_id(controller, url)
                if old_group:
                    for node in controller.state.nodes:
                        if node.subscription_id == subscription_id or (
                            not node.subscription_id
                            and (node.group or "Default") == old_group
                        ):
                            node.group = new_name
                            node.subscription_id = subscription_id
                existing["name"] = new_name
                existing["group"] = new_name
                controller.nodes_changed.emit(controller.state.nodes)
                controller.subscriptions_changed.emit(list(controller.state.subscriptions))
                controller.save()
        group = (existing.get("group") or "").strip() or _derive_subscription_name(url)
        replace = True

    fetched = (text or "", dict(userinfo or {}), list(errors or []))
    added, errs, info = _apply_subscription_payload(
        controller, url, group, fetched, replace_existing_group=replace
    )
    applied = bool(info.pop("_lumen_applied", False))
    if not applied:
        return added, errs
    effective_url = str(info.pop("_lumen_effective_url", url) or url)
    _record_subscription(controller, effective_url, group, added, info)
    return added, errs


def remove_subscription(controller: AppController, url: str, *, delete_nodes: bool = True) -> None:
    url = (url or "").strip()
    sub = _find_subscription(controller, url)
    if sub is None:
        return
    group = (sub.get("group") or "").strip()
    subscription_id = str(sub.get("id") or "").strip()
    controller.state.subscriptions = [
        item for item in controller.state.subscriptions if item.get("url") != url
    ]
    controller.subscriptions_changed.emit(list(controller.state.subscriptions))
    if delete_nodes:
        ids = {
            node.id
            for node in controller.state.nodes
            if (subscription_id and node.subscription_id == subscription_id)
            or (
                not subscription_id
                and group
                and not node.subscription_id
                and (node.group or "Default") == group
            )
        }
        if ids:
            remove_nodes(controller, ids)
            return
    controller.save()
