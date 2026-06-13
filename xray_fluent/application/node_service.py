from __future__ import annotations

import base64
import binascii
import json
import re
from datetime import datetime, timezone
from typing import TYPE_CHECKING
from urllib.parse import urlparse
from urllib.request import Request

from PyQt6.QtCore import QTimer

from ..country_flags import detect_country
from ..http_utils import urlopen
from ..link_parser import normalize_node_outbound, parse_links_text, validate_node_outbound

if TYPE_CHECKING:
    from ..app_controller import AppController


def import_nodes_from_text(
    controller: AppController,
    text: str,
    *,
    group: str | None = None,
    auto_connect: bool | None = None,
    select_imported: bool = True,
) -> tuple[int, list[str]]:
    nodes, errors = parse_links_text(text)
    if not nodes:
        return 0, errors

    existing_links = {node.link for node in controller.state.nodes}
    max_order = max((node.sort_order for node in controller.state.nodes), default=0)
    first_new_id: str | None = None
    added = 0
    for node in nodes:
        normalize_node_outbound(node)
        problem = validate_node_outbound(node)
        if problem:
            errors.append(problem)
            continue
        if node.link in existing_links:
            continue
        if group:
            node.group = group
        if not node.country_code:
            node.country_code = detect_country(node.name, node.server)
        max_order += 1
        node.sort_order = max_order
        controller.state.nodes.append(node)
        existing_links.add(node.link)
        if first_new_id is None:
            first_new_id = node.id
        added += 1

    if first_new_id and (select_imported or not controller.state.selected_node_id):
        controller.state.selected_node_id = first_new_id
    elif not controller.state.selected_node_id and controller.state.nodes:
        controller.state.selected_node_id = controller.state.nodes[0].id

    controller.nodes_changed.emit(controller.state.nodes)
    controller.selection_changed.emit(controller.selected_node)
    controller.save()
    QTimer.singleShot(500, controller._start_country_ip_resolution)

    should_auto_connect = (
        controller.state.settings.auto_connect_on_import
        if auto_connect is None
        else auto_connect
    )
    if added and should_auto_connect:
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
    if "tags" in updates:
        node.tags = list(updates["tags"])
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
    add_tags = operations.get("add_tags", [])
    remove_tags = set(operations.get("remove_tags", []))
    updated = 0
    for node in controller.state.nodes:
        if node.id not in node_ids:
            continue
        if group:
            node.group = group
        if add_tags:
            existing = set(node.tags)
            for tag in add_tags:
                if tag not in existing:
                    node.tags.append(tag)
        if remove_tags:
            node.tags = [tag for tag in node.tags if tag not in remove_tags]
        updated += 1
    if updated:
        controller.nodes_changed.emit(controller.state.nodes)
        controller.save()
    return updated


def get_all_groups(controller: AppController) -> list[str]:
    return sorted({node.group for node in controller.state.nodes if node.group})


def get_all_tags(controller: AppController) -> list[str]:
    tags: set[str] = set()
    for node in controller.state.nodes:
        tags.update(node.tags)
    return sorted(tags)


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
    links = data.get("links")
    if isinstance(links, list) and links:
        links_text = "\n".join(str(item) for item in links if item)
        return links_text, info
    return text, info


def _fetch_subscription_with_user_agent(url: str, user_agent: str) -> tuple[str, dict]:
    """Загружает подписку и возвращает (текст_со_ссылками, userinfo).

    userinfo берётся из HTTP-заголовка subscription-userinfo и/или из JSON-тела.
    """
    request = Request(url, headers={"User-Agent": user_agent})
    with urlopen(request, timeout=20) as response:
        raw = response.read()
        try:
            header_value = response.headers.get("subscription-userinfo", "")
        except Exception:  # noqa: BLE001 - защита от нестандартных ответов
            header_value = ""
    userinfo = _parse_userinfo_header(header_value)
    text = raw.decode("utf-8", errors="replace").strip()
    # JSON-тело (например, формат с {"user": {...}, "links": [...]}).
    text, body_info = _extract_userinfo_from_body(text)
    if body_info:
        # Данные из тела приоритетнее заголовка.
        userinfo = {**userinfo, **body_info}
    decoded = _maybe_base64_decode(text)
    return (decoded or text), userinfo


def _fetch_subscription(url: str, *, user_agent: str = "LumenKVN-Subscription/1.0") -> tuple[str, dict]:
    return _fetch_subscription_with_user_agent(url, user_agent)


def _fetch_subscription_happ(url: str) -> tuple[str, dict]:
    return _fetch_subscription_with_user_agent(url, "Happ/1.0")


def _derive_subscription_name(url: str) -> str:
    host = urlparse(url).hostname or ""
    host = host.strip()
    if host:
        return f"Подписка {host}"
    return "Подписка"


def _find_subscription(controller: AppController, url: str) -> dict | None:
    for sub in controller.state.subscriptions:
        if sub.get("url") == url:
            return sub
    return None


def fetch_subscription_payload(url: str) -> tuple[str, dict, list[str]]:
    """Загружает подписку по сети и возвращает (текст_ссылок, userinfo, errors).

    Только сеть, не трогает controller/state — безопасно вызывать в фоновом потоке.
    При неудаче текст пустой, а ошибки лежат в errors.
    """
    url = (url or "").strip()
    if not url:
        return "", {}, ["Пустой URL подписки"]
    first_error = ""
    try:
        text, userinfo = _fetch_subscription(url)
        nodes, errors = parse_links_text(text)
        if nodes:
            return text, userinfo, errors
        first_error = "; ".join(errors[:3]) or "no nodes parsed"
    except Exception as exc:  # noqa: BLE001 - fallback to Happ-compatible panels
        first_error = str(exc)

    try:
        text, userinfo = _fetch_subscription_happ(url)
        nodes, errors = parse_links_text(text)
        if not nodes:
            if first_error:
                errors = [*errors, f"Default UA: {first_error}"]
            return "", userinfo, errors
        return text, userinfo, errors
    except Exception as exc:  # noqa: BLE001
        if first_error:
            return "", {}, [f"Happ/1.0: {exc}", f"Default UA: {first_error}"]
        return "", {}, [str(exc)]


def _apply_subscription_payload(
    controller: AppController,
    url: str,
    group: str,
    fetched: tuple[str, dict, list[str]],
    *,
    replace_existing_group: bool = False,
) -> tuple[int, list[str], dict]:
    """Применяет уже загруженную подписку к состоянию. Только GUI-поток."""
    chosen_text, chosen_userinfo, chosen_errors = fetched
    # Загрузка не удалась — не трогаем существующие узлы группы.
    if not chosen_text:
        return 0, list(chosen_errors), chosen_userinfo

    if replace_existing_group:
        keep: list = []
        removed_ids: set[str] = set()
        for node in controller.state.nodes:
            if (node.group or "Default") == group:
                removed_ids.add(node.id)
            else:
                keep.append(node)
        controller.state.nodes = keep
        if controller.state.selected_node_id in removed_ids:
            controller.state.selected_node_id = None

    selected_was_removed = controller.state.selected_node_id is None and bool(replace_existing_group)
    added, errors = import_nodes_from_text(
        controller,
        chosen_text,
        group=group,
        auto_connect=False,
        select_imported=selected_was_removed,
    )
    if selected_was_removed and added and (controller.connected or controller._desired_connected):
        controller._desired_connected = True
        controller._request_transition("active subscription updated")
    return added, [*chosen_errors, *errors], chosen_userinfo


def _import_subscription_payload(
    controller: AppController,
    url: str,
    group: str,
    *,
    replace_existing_group: bool = False,
) -> tuple[int, list[str], dict]:
    # Синхронный путь (блокирует поток). Оставлен для обратной совместимости.
    fetched = fetch_subscription_payload(url)
    return _apply_subscription_payload(
        controller, url, group, fetched, replace_existing_group=replace_existing_group
    )


def _record_subscription(
    controller: AppController,
    url: str,
    group: str,
    node_count: int,
    userinfo: dict | None = None,
) -> None:
    now = _utc_now_iso()
    info = dict(userinfo) if isinstance(userinfo, dict) else {}
    existing = _find_subscription(controller, url)
    if existing is not None:
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
            if old_group:
                for node in controller.state.nodes:
                    if (node.group or "Default") == old_group:
                        node.group = new_name
            existing["name"] = new_name
            existing["group"] = new_name
            controller.nodes_changed.emit(controller.state.nodes)
            controller.subscriptions_changed.emit(list(controller.state.subscriptions))
            controller.save()
        return update_subscription(controller, url)
    group = (name or "").strip() or _derive_subscription_name(url)
    added, errors, userinfo = _import_subscription_payload(controller, url, group)
    _record_subscription(controller, url, group, added, userinfo)
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
    _record_subscription(controller, url, group, added, userinfo)
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
        group = (name or "").strip() or _derive_subscription_name(url)
        replace = False
    else:
        if existing is None:
            return 0, ["Подписка не найдена"]
        if kind == "import":
            # Подписка уже есть: при новом имени переименовываем группу и узлы.
            new_name = (name or "").strip()
            old_group = (existing.get("group") or "").strip()
            if new_name and new_name != old_group:
                if old_group:
                    for node in controller.state.nodes:
                        if (node.group or "Default") == old_group:
                            node.group = new_name
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
    _record_subscription(controller, url, group, added, info)
    return added, errs


def remove_subscription(controller: AppController, url: str, *, delete_nodes: bool = True) -> None:
    url = (url or "").strip()
    sub = _find_subscription(controller, url)
    if sub is None:
        return
    group = (sub.get("group") or "").strip()
    controller.state.subscriptions = [
        item for item in controller.state.subscriptions if item.get("url") != url
    ]
    controller.subscriptions_changed.emit(list(controller.state.subscriptions))
    if delete_nodes and group:
        ids = {node.id for node in controller.state.nodes if (node.group or "Default") == group}
        if ids:
            remove_nodes(controller, ids)
            return
    controller.save()
