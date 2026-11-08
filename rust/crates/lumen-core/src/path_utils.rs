//! Configured-path normalization and resolution (`path_utils.py`).

use std::path::{Path, PathBuf};

use crate::constants::base_dir;

pub const LEGACY_APP_DIR_NAMES: [&str; 4] = ["lumen kvn", "lumenkvn", "lumen-kvn", "lumen_kvn"];

fn clean_path_value(value: &str) -> &str {
    value.trim()
}

fn base_relative(path: &Path) -> Option<PathBuf> {
    let base = base_dir();
    path.strip_prefix(base).ok().map(|r| r.to_path_buf())
}

fn looks_like_default_location(path: &Path, default_path: &Path) -> bool {
    let default_relative = base_relative(default_path).unwrap_or_else(|| default_path.to_path_buf());
    let path_parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let default_parts: Vec<String> = default_relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if path_parts.len() < default_parts.len() {
        return false;
    }
    &path_parts[path_parts.len() - default_parts.len()..] == default_parts.as_slice()
}

fn is_legacy_default_location(path: &Path, default_path: &Path) -> bool {
    let default_relative = base_relative(default_path).unwrap_or_else(|| default_path.to_path_buf());
    let relative_parts: Vec<_> = default_relative.components().collect();
    if relative_parts.is_empty() || path.components().count() <= relative_parts.len() {
        return false;
    }
    let install_root = path
        .components()
        .count()
        .checked_sub(relative_parts.len())
        .and_then(|_| path.parent());
    let mut current = path.to_path_buf();
    for _ in 0..relative_parts.len() {
        current = match current.parent() {
            Some(p) => p.to_path_buf(),
            None => return false,
        };
    }
    let name = current
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let _ = install_root;
    if LEGACY_APP_DIR_NAMES.contains(&name.as_str()) {
        return true;
    }
    let text = current.to_string_lossy().replace('/', "\\").to_lowercase();
    text == "c:\\program"
}

pub fn normalize_path_for_storage(path_value: &str) -> String {
    let text = clean_path_value(path_value);
    if text.is_empty() {
        return String::new();
    }
    let path = Path::new(text);
    if !path.is_absolute() {
        return text.to_string();
    }
    match base_relative(path) {
        Some(relative) => relative.to_string_lossy().replace('\\', "/"),
        None => text.to_string(),
    }
}

pub fn normalize_configured_path(
    path_value: &str,
    default_path: Option<&Path>,
    use_default_if_empty: bool,
    migrate_default_location: bool,
) -> String {
    let text = clean_path_value(path_value);
    if text.is_empty() {
        if use_default_if_empty {
            if let Some(default) = default_path {
                return normalize_path_for_storage(&default.to_string_lossy());
            }
        }
        return String::new();
    }
    let path = Path::new(text);
    if let Some(default) = default_path {
        if migrate_default_location
            && path.is_absolute()
            && looks_like_default_location(path, default)
            && (!path.exists() || is_legacy_default_location(path, default))
        {
            return normalize_path_for_storage(&default.to_string_lossy());
        }
    }
    normalize_path_for_storage(text)
}

pub fn resolve_configured_path(
    path_value: &str,
    default_path: Option<&Path>,
    use_default_if_empty: bool,
    migrate_default_location: bool,
) -> Option<PathBuf> {
    let normalized = normalize_configured_path(path_value, default_path, use_default_if_empty, migrate_default_location);
    if normalized.is_empty() {
        return None;
    }
    let path = Path::new(&normalized);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir().join(path)
    };
    // Developer-checkout fallback: a repository-level `core/` directory next
    // to windows/ can still hold downloaded cores; packaged builds never hit it.
    if let Some(default) = default_path {
        if !resolved.is_file()
            && paths_equivalent(&resolved, default)
        {
            if let Some(relative) = base_relative(default) {
                let candidate = base_dir().parent().unwrap_or(base_dir()).join(&relative);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    Some(resolved)
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase().replace('/', "\\")
        == b.to_string_lossy().to_lowercase().replace('/', "\\")
}
