"""Pure helpers for the QML Configs tab.

These mirror the per-core profile bookkeeping that ui/main_window.py performs
for ui/configs_page.py (config + template directories, active relative paths,
available *.json profiles). No Qt / GUI imports live here so the logic can be
unit-tested and reused by the bridge without dragging in widgets.

The heavy lifting (loading / saving / validating / applying documents) stays on
the AppController; this module only enumerates choices and packs a QVariantMap
ready dict for the editor.
"""
from __future__ import annotations

from pathlib import Path

CORES = ("singbox", "xray")


def _profile_dir(controller, core: str, kind: str) -> Path:
    if core == "singbox":
        return (
            controller.get_singbox_config_dir()
            if kind == "config"
            else controller.get_singbox_template_dir()
        )
    return (
        controller.get_xray_config_dir()
        if kind == "config"
        else controller.get_xray_template_dir()
    )


def _active_path(controller, core: str, kind: str) -> Path | None:
    if core == "singbox":
        return (
            controller.get_active_singbox_config_path()
            if kind == "config"
            else controller.get_active_singbox_template_path()
        )
    return (
        controller.get_active_xray_config_path()
        if kind == "config"
        else controller.get_active_xray_template_path()
    )


def active_relative(controller, core: str, kind: str) -> str:
    """Active config/template path relative to its base dir (or "")."""
    try:
        base = _profile_dir(controller, core, kind).resolve()
    except Exception:
        return ""
    path = _active_path(controller, core, kind)
    if path is None:
        return ""
    try:
        return path.resolve().relative_to(base).as_posix()
    except ValueError:
        return ""


def list_profile_items(controller, core: str, kind: str) -> list[dict]:
    """All *.json profiles under the base dir as [{label, value}] (sorted)."""
    items: list[dict] = []
    try:
        base = _profile_dir(controller, core, kind).resolve()
        for path in sorted(base.rglob("*.json")):
            rel = path.relative_to(base).as_posix()
            items.append({"label": rel, "value": rel})
    except Exception:
        return []
    return items


def sync_template_for_config(controller, core: str, config_path: Path) -> Path | None:
    """Mirror main_window._sync_core_template_for_config (no signal emit)."""
    if core == "singbox":
        template_path = controller._default_singbox_template_path_for_config(config_path)
        if template_path is not None:
            controller._set_active_singbox_template_path(template_path, emit_signal=False)
        return template_path
    template_path = controller._default_xray_template_path_for_config(config_path)
    if template_path is not None:
        controller._set_active_xray_template_path(template_path, emit_signal=False)
    return template_path


def build_state(
    controller,
    core: str,
    *,
    text: str = "",
    file_label: str | None = None,
    status_level: str = "",
    status_message: str = "",
) -> dict:
    """Assemble the full editor state for a core as a QVariantMap-ready dict."""
    cfg_path = _active_path(controller, core, "config")
    tpl_path = _active_path(controller, core, "template")
    if file_label is None:
        file_label = cfg_path.as_posix() if cfg_path is not None else ""
    return {
        "core": core,
        "text": text or "",
        "fileLabel": file_label,
        "templateLabel": tpl_path.as_posix() if tpl_path is not None else "",
        "hasTemplate": tpl_path is not None,
        "configItems": list_profile_items(controller, core, "config"),
        "templateItems": list_profile_items(controller, core, "template"),
        "activeConfig": active_relative(controller, core, "config"),
        "activeTemplate": active_relative(controller, core, "template"),
        "statusLevel": status_level,
        "statusMessage": status_message,
    }
