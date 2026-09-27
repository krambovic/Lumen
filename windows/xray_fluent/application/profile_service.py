from __future__ import annotations

from copy import deepcopy
import json
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ..app_controller import AppController


def get_active_config_path(controller: AppController, engine: str) -> Path:
    if engine == "singbox":
        return controller._resolve_singbox_config_path()
    return controller._resolve_xray_config_path()


def get_active_config_name(controller: AppController, engine: str) -> str:
    return get_active_config_path(controller, engine).name


def get_active_template_path(controller: AppController, engine: str) -> Path | None:
    if engine == "singbox":
        relative = controller._normalize_singbox_template_relative_path(
            controller.state.settings.singbox_template_file or controller.state.settings.singbox_config_file
        )
        resolver = controller._resolve_singbox_template_path
    else:
        relative = controller._normalize_xray_template_relative_path(
            controller.state.settings.xray_template_file or controller.state.settings.xray_config_file
        )
        resolver = controller._resolve_xray_template_path
    try:
        resolved = resolver(relative)
    except ValueError:
        return None
    return resolved if resolved.exists() else None


def _backup_path_for(path: Path) -> Path:
    return path.with_name(f"{path.name}.bak")


def _sync_active_config_from_template(
    controller: AppController,
    engine: str,
    active_path: Path,
    template_path: Path | None,
) -> bool:
    if template_path is None or not active_path.exists() or not template_path.exists():
        return False
    try:
        active_stat = active_path.stat()
        template_stat = template_path.stat()
    except OSError:
        return False
    if int(getattr(template_stat, "st_mtime_ns", 0)) <= int(getattr(active_stat, "st_mtime_ns", 0)):
        return False

    template_text = template_path.read_text(encoding="utf-8")
    active_text = active_path.read_text(encoding="utf-8")
    if active_text == template_text:
        return False

    try:
        _backup_path_for(active_path).write_text(active_text, encoding="utf-8")
    except OSError:
        pass
    active_path.write_text(template_text, encoding="utf-8")
    if engine == "singbox":
        controller._cache_singbox_document_state(active_path, template_text)
    if controller.connected or controller._desired_connected:
        controller._desired_connected = True
        controller._request_transition(f"{engine} template synced")
    return True


def ensure_active_config(
    controller: AppController,
    engine: str,
    path: str | Path | None = None,
    *,
    sync_template: bool = True,
) -> Path:
    if engine == "singbox":
        resolved = controller._resolve_singbox_config_path(path)
        template_path = get_active_template_path(controller, "singbox")
        if template_path is None and path is not None:
            template_path = controller._default_singbox_template_path_for_config(resolved)
        setter = controller._set_active_singbox_config_path
        template_setter = controller._set_active_singbox_template_path
        default_text = controller._default_singbox_config_text()
    else:
        resolved = controller._resolve_xray_config_path(path)
        template_path = get_active_template_path(controller, "xray")
        if template_path is None and path is not None:
            template_path = controller._default_xray_template_path_for_config(resolved)
        setter = controller._set_active_xray_config_path
        template_setter = controller._set_active_xray_template_path
        default_text = controller._default_xray_config_text()

    resolved.parent.mkdir(parents=True, exist_ok=True)
    if not resolved.exists():
        if template_path is not None:
            resolved.write_text(template_path.read_text(encoding="utf-8"), encoding="utf-8")
            template_setter(template_path, emit_signal=False)
        else:
            resolved.write_text(default_text, encoding="utf-8")
    elif sync_template:
        _sync_active_config_from_template(controller, engine, resolved, template_path)
    setter(resolved)
    return resolved


def load_active_config_text(controller: AppController, engine: str) -> tuple[Path, str]:
    path = ensure_active_config(controller, engine)
    text = path.read_text(encoding="utf-8")
    if engine == "singbox":
        controller._cache_singbox_document_state(path, text)
    return path, text


def load_config_text(controller: AppController, engine: str, path: str | Path) -> tuple[Path, str]:
    if engine == "singbox":
        resolved = controller._resolve_singbox_config_path(path)
        setter = controller._set_active_singbox_config_path
    else:
        resolved = controller._resolve_xray_config_path(path)
        setter = controller._set_active_xray_config_path
    if not resolved.exists():
        raise FileNotFoundError(f"Файл не найден: {resolved.name}")
    setter(resolved)
    if engine == "singbox":
        template_path = controller._default_singbox_template_path_for_config(resolved)
    else:
        template_path = controller._default_xray_template_path_for_config(resolved)
    if template_path is None:
        template_path = get_active_template_path(controller, engine)
    _sync_active_config_from_template(controller, engine, resolved, template_path)
    text = resolved.read_text(encoding="utf-8")
    if engine == "singbox":
        controller._cache_singbox_document_state(resolved, text)
    return resolved, text


def import_template(controller: AppController, engine: str, path: str | Path) -> tuple[Path, str]:
    if engine == "singbox":
        template_path = controller._resolve_singbox_template_path(path)
        template_dir = controller.get_singbox_template_dir().resolve()
        resolve_config = controller._resolve_singbox_config_path
        set_template = controller._set_active_singbox_template_path
        set_config = controller._set_active_singbox_config_path
    else:
        template_path = controller._resolve_xray_template_path(path)
        template_dir = controller.get_xray_template_dir().resolve()
        resolve_config = controller._resolve_xray_config_path
        set_template = controller._set_active_xray_template_path
        set_config = controller._set_active_xray_config_path

    if not template_path.exists():
        raise FileNotFoundError(f"Файл не найден: {template_path.name}")
    relative = template_path.relative_to(template_dir).as_posix()
    active_path = resolve_config(relative)
    active_path.parent.mkdir(parents=True, exist_ok=True)
    template_text = template_path.read_text(encoding="utf-8")
    # Keep the selected template and the active runtime copy in sync. Otherwise
    # the UI can appear to switch templates while launch still uses an older
    # config file from data/configs/.
    active_path.write_text(template_text, encoding="utf-8")
    set_template(template_path)
    set_config(active_path)
    text = template_text
    if engine == "singbox":
        controller._cache_singbox_document_state(active_path, text)
    return active_path, text


def reset_active_config_to_template(controller: AppController, engine: str) -> tuple[bool, Path | None, str]:
    template_path = get_active_template_path(controller, engine)
    if template_path is None:
        return False, None, f"Для текущего {engine} конфига не привязан template."
    active_path = ensure_active_config(controller, engine)
    text = template_path.read_text(encoding="utf-8")
    active_path.write_text(text, encoding="utf-8")
    if engine == "singbox":
        controller._cache_singbox_document_state(active_path, text)
    return True, active_path, f"Активная копия сброшена к шаблону: {template_path.name}"


def save_config_text(
    controller: AppController,
    engine: str,
    text: str,
    path: str | Path | None = None,
    *,
    previous_text: str | None = None,
    force_route_final: bool = False,
) -> Path:
    resolved = ensure_active_config(controller, engine, path, sync_template=False)
    resolved.write_text(text, encoding="utf-8")
    if engine == "singbox":
        controller._set_active_singbox_config_path(resolved)
        controller._cache_singbox_document_state(resolved, text)
        sync_singbox_routing_from_config(
            controller,
            text,
            previous_text=previous_text,
            force=force_route_final,
        )
    else:
        controller._set_active_xray_config_path(resolved)
    return resolved


def sync_singbox_routing_from_config(
    controller: AppController,
    text: str,
    *,
    previous_text: str | None = None,
    force: bool = False,
) -> bool:
    """Honor an explicit route.final saved in the GUI config editor.

    The runtime config is rebuilt for every connection, so changing route.final
    only in the source JSON used to be silently undone by GUI routing. Mirror a
    supported explicit value into RoutingSettings, which remains the canonical
    input for the generated runtime while preserving all other source fields.
    """
    try:
        payload = json.loads(text)
    except (TypeError, json.JSONDecodeError):
        return False
    if not isinstance(payload, dict):
        return False
    route = payload.get("route")
    if not isinstance(route, dict):
        return False
    requested = str(route.get("final") or "").strip().lower()
    if requested not in {"proxy", "direct"}:
        return False
    if not force and previous_text is not None:
        try:
            previous_payload = json.loads(previous_text)
        except (TypeError, json.JSONDecodeError):
            previous_payload = None
        previous_route = previous_payload.get("route") if isinstance(previous_payload, dict) else None
        previous_final = (
            str(previous_route.get("final") or "").strip().lower()
            if isinstance(previous_route, dict)
            else ""
        )
        if previous_final == requested:
            return False

    current = controller.state.routing
    mode = str(current.mode or "").strip().lower()
    effective = (
        "proxy"
        if mode == "global"
        else "direct"
        if mode == "direct"
        else str(current.tun_default_outbound or "proxy").strip().lower()
    )
    if requested == effective:
        return False

    routing = deepcopy(current)
    routing.mode = "rule"
    routing.tun_default_outbound = requested
    routing.tun_default_outbound_user_selected = True
    controller.update_routing(routing, restart_runtime=False)
    return True


def apply_singbox_config_text(
    controller: AppController,
    text: str,
    previous_text: str | None = None,
) -> tuple[bool, Path | None, str]:
    ok, message = controller.validate_json_text(text)
    if not ok:
        return False, None, message
    path = save_config_text(
        controller,
        "singbox",
        text,
        previous_text=previous_text,
        force_route_final=True,
    )
    if controller._active_core == "singbox" or (
        controller.is_singbox_editor_mode() and (controller.connected or controller._desired_connected)
    ):
        controller._desired_connected = True
        controller._request_transition("sing-box config applied")
        return True, path, "Конфиг сохранён. Применяю изменения sing-box..."
    return True, path, "Конфиг сохранён. Он будет использован при следующем запуске sing-box."


def apply_xray_config_text(controller: AppController, text: str) -> tuple[bool, Path | None, str]:
    ok, message = controller.validate_json_text(text)
    if not ok:
        return False, None, message
    path = save_config_text(controller, "xray", text)
    if controller._active_core == "xray" and (controller.connected or controller._desired_connected) and controller.uses_xray_raw_config():
        controller._desired_connected = True
        controller._request_transition("xray config applied")
        return True, path, "Конфиг сохранён. Применяю изменения xray..."
    return True, path, "Конфиг сохранён. Он будет использован при следующем запуске xray."
