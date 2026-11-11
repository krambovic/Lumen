//! Pure helpers for the Configs tab (port of `qml_app/bridge/configs_helpers.py`).
//!
//! These mirror the per-core profile bookkeeping the classic `ui/main_window.py`
//! performed for `ui/configs_page.py` (config + template directories, active
//! relative paths, available `*.json` profiles). The heavy lifting (loading /
//! saving / validating / applying documents) stays on the `AppController`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use lumen_app::app_controller::AppController;

pub const CORES: [&str; 2] = ["singbox", "xray"];

/// Python `_profile_dir`.
pub fn profile_dir(controller: &AppController, core: &str, kind: &str) -> PathBuf {
    if core == "singbox" {
        return if kind == "config" {
            controller.get_singbox_config_dir()
        } else {
            controller.get_singbox_template_dir()
        };
    }
    if kind == "config" {
        controller.get_xray_config_dir()
    } else {
        controller.get_xray_template_dir()
    }
}

/// Python `_active_path`.
pub fn active_path(controller: &mut AppController, core: &str, kind: &str) -> Option<PathBuf> {
    if core == "singbox" {
        if kind == "config" {
            return controller.get_active_singbox_config_path().ok();
        }
        return controller.get_active_singbox_template_path();
    }
    if kind == "config" {
        controller.get_active_xray_config_path().ok()
    } else {
        controller.get_active_xray_template_path()
    }
}

/// Python `Path.relative_to(base).as_posix()` with the Windows path cleanup.
fn relative_posix(path: &Path, base: &Path) -> Option<String> {
    let path = std::fs::canonicalize(path).ok()?;
    let base = std::fs::canonicalize(base).ok()?;
    let relative = path.strip_prefix(&base).ok()?;
    Some(
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Python `active_relative` — the active config/template path relative to its
/// base dir (or "").
pub fn active_relative(controller: &mut AppController, core: &str, kind: &str) -> String {
    let base = match profile_dir(controller, core, kind).canonicalize() {
        Ok(base) => base,
        Err(_) => return String::new(),
    };
    let Some(path) = active_path(controller, core, kind) else {
        return String::new();
    };
    relative_posix(&path, &base).unwrap_or_default()
}

/// Python `list_profile_items` — all `*.json` profiles under the base dir as
/// `[{label, value}]` (sorted; empty on failure).
pub fn list_profile_items(controller: &AppController, core: &str, kind: &str) -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    let base = match profile_dir(controller, core, kind).canonicalize() {
        Ok(base) => base,
        Err(_) => return items,
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_json_files(&base, &mut paths);
    paths.sort();
    for path in paths {
        if let Ok(relative) = path.strip_prefix(&base) {
            let rel = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            items.push(json!({"label": rel, "value": rel}));
        }
    }
    items
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

/// Python `sync_template_for_config` — mirror
/// `main_window._sync_core_template_for_config` (no signal emit).
pub fn sync_template_for_config(
    controller: &mut AppController,
    core: &str,
    config_path: &Path,
) -> Option<PathBuf> {
    let template_path: Option<PathBuf> = if core == "singbox" {
        default_singbox_template_path_for_config(controller, config_path)
    } else {
        default_xray_template_path_for_config(controller, config_path)
    };
    if let Some(template_path) = template_path.as_ref() {
        let _ = if core == "singbox" {
            controller.set_active_singbox_template_path(template_path, false)
        } else {
            controller.set_active_xray_template_path(template_path, false)
        };
    }
    template_path
}

/// Python `_default_singbox_template_path_for_config`.
fn default_singbox_template_path_for_config(
    controller: &mut AppController,
    config_path: &Path,
) -> Option<PathBuf> {
    let config_dir = controller.get_singbox_config_dir().canonicalize().ok()?;
    let relative = config_path
        .canonicalize()
        .ok()?
        .strip_prefix(&config_dir)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    let template = controller
        .resolve_singbox_template_path(Some(Path::new(&relative)))
        .ok()?;
    template.exists().then_some(template)
}

/// Python `_default_xray_template_path_for_config`.
fn default_xray_template_path_for_config(
    controller: &mut AppController,
    config_path: &Path,
) -> Option<PathBuf> {
    let config_dir = controller.get_xray_config_dir().canonicalize().ok()?;
    let relative = config_path
        .canonicalize()
        .ok()?
        .strip_prefix(&config_dir)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    let template = controller
        .resolve_xray_template_path(Some(Path::new(&relative)))
        .ok()?;
    template.exists().then_some(template)
}

/// Python `build_state` — the full editor state for a core as a QVariantMap.
#[allow(clippy::too_many_arguments)]
pub fn build_state(
    controller: &mut AppController,
    core: &str,
    text: &str,
    file_label: Option<&str>,
    status_level: &str,
    status_message: &str,
) -> Value {
    let cfg_path = active_path(controller, core, "config");
    let tpl_path = active_path(controller, core, "template");
    let file_label = file_label
        .map(str::to_string)
        .or_else(|| {
            cfg_path
                .as_ref()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .unwrap_or_default();
    json!({
        "core": core,
        "text": text,
        "fileLabel": file_label,
        "templateLabel": tpl_path.as_ref().map(|path| path.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
        "hasTemplate": tpl_path.is_some(),
        "configItems": list_profile_items(controller, core, "config"),
        "templateItems": list_profile_items(controller, core, "template"),
        "activeConfig": active_relative(controller, core, "config"),
        "activeTemplate": active_relative(controller, core, "template"),
        "statusLevel": status_level,
        "statusMessage": status_message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_json_files_finds_nested_profiles() {
        let base = std::env::temp_dir().join(format!("lumen-configs-test-{}", std::process::id()));
        let nested = base.join("sub");
        let _ = std::fs::create_dir_all(&nested);
        std::fs::write(base.join("a.json"), "{}").unwrap();
        std::fs::write(nested.join("b.json"), "{}").unwrap();
        std::fs::write(base.join("ignored.txt"), "x").unwrap();
        let mut found = Vec::new();
        collect_json_files(&base, &mut found);
        found.sort();
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.json", "b.json"]);
        let _ = std::fs::remove_dir_all(&base);
    }
}
