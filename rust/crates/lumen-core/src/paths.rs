//! Data directory resolution and legacy brand-name migration (`data_paths.py`).

use std::fs;
use std::path::{Path, PathBuf};

pub const PORTABLE_MARKERS: [&str; 2] = ["portable.txt", "portable"];
pub const LEGACY_APP_NAMES: [&str; 4] = ["Lumen KVN", "lumen-kvn", "LumenKVN", "Lumen_KVN"];
pub const NAME_MIGRATION_MARKER: &str = ".lumen_name_migration_v1";
pub const MIGRATION_BACKUP_SUFFIX: &str = ".pre_lumen_migration";
pub const MIGRATION_CONFLICTS_DIR: &str = ".legacy_migration_conflicts";
pub const MIGRATION_ROOT_BACKUPS_DIR: &str = ".legacy_root_files";
pub const LEGACY_EPHEMERAL_ROOT_NAMES: [&str; 5] = ["cache", "logs", "runtime", "temp", "tmp"];

pub fn is_writable(path: &Path) -> bool {
    if let Err(_) = fs::create_dir_all(path) {
        return false;
    }
    let probe = path.join(".write_test");
    match fs::write(&probe, "ok") {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn backup_conflicting_state(source: &Path, target: &Path) {
    for name in ["state.enc", "traffic_history.json", "install_id"] {
        let source_file = source.join(name);
        let target_file = target.join(name);
        let backup_file = target.join(format!("{name}{MIGRATION_BACKUP_SUFFIX}"));
        if !source_file.is_file() || !target_file.is_file() || backup_file.exists() {
            continue;
        }
        let _ = fs::copy(&target_file, &backup_file);
    }
}

fn migration_source_id(source: &Path) -> String {
    let normalized = dunce_normalize(source).replace('\\', "/").to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    hex(&digest)
}

use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn dunce_normalize(path: &Path) -> String {
    // Absolute, lexical normalization (no symlink resolution needed for parity).
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    abs.to_string_lossy().to_string()
}

fn is_migration_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn read_migration_ids(marker: &Path) -> Vec<String> {
    match fs::read_to_string(marker) {
        Ok(text) => text
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| is_migration_id(l))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn write_migration_ids(marker: &Path, source_ids: &mut Vec<String>) {
    source_ids.sort();
    let _ = fs::write(marker, source_ids.join("\n"));
}

fn files_equal(a: &Path, b: &Path) -> bool {
    match (fs::read(a), fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn copied_tree_matches(source: &Path, target: &Path) -> bool {
    for entry in walk_files(source) {
        let relative = match entry.strip_prefix(source) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        let target_file = target.join(&relative);
        if !target_file.is_file() || !files_equal(&entry, &target_file) {
            return false;
        }
    }
    true
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn merge_additional_legacy_data(source: &Path, target: &Path, source_id: &str) -> bool {
    let conflict_root = target.join(MIGRATION_CONFLICTS_DIR).join(source_id);
    for entry in walk_files(source) {
        let relative = match entry.strip_prefix(source) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        let target_file = target.join(&relative);
        if !target_file.exists() {
            if let Some(parent) = target_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::copy(&entry, &target_file);
            continue;
        }
        if target_file.is_file() && files_equal(&entry, &target_file) {
            continue;
        }
        let conflict_file = conflict_root.join(&relative);
        if let Some(parent) = conflict_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&entry, &conflict_file);
    }
    for entry in walk_files(source) {
        let relative = match entry.strip_prefix(source) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        let target_file = target.join(&relative);
        let conflict_file = conflict_root.join(&relative);
        let target_matches = target_file.is_file() && files_equal(&entry, &target_file);
        let conflict_matches = conflict_file.is_file() && files_equal(&entry, &conflict_file);
        if !target_matches && !conflict_matches {
            return false;
        }
    }
    true
}

fn remove_migrated_source(source: &Path) {
    let _ = fs::remove_dir_all(source);
    if let Some(parent) = source.parent() {
        let _ = fs::remove_dir(parent);
    }
}

fn cleanup_legacy_app_root(legacy_root: &Path, target_data: &Path) -> bool {
    let result: std::io::Result<()> = (|| {
        if !legacy_root.is_dir() {
            return Ok(());
        }
        // Never remove a root whose data migration did not finish.
        if legacy_root.join("data").exists() {
            return Ok(());
        }
        let children: Vec<PathBuf> = fs::read_dir(legacy_root)?
            .flatten()
            .map(|e| e.path())
            .collect();
        for child in &children {
            let name = child
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !LEGACY_EPHEMERAL_ROOT_NAMES.contains(&name.as_str()) {
                continue;
            }
            if child.is_dir() {
                let _ = fs::remove_dir_all(child);
            } else {
                let _ = fs::remove_file(child);
            }
        }
        let remaining: Vec<PathBuf> = fs::read_dir(legacy_root)?
            .flatten()
            .map(|e| e.path())
            .collect();
        if !remaining.is_empty() {
            let backup_root = target_data
                .join(MIGRATION_ROOT_BACKUPS_DIR)
                .join(migration_source_id(legacy_root));
            fs::create_dir_all(&backup_root)?;
            for child in &remaining {
                let destination = backup_root.join(
                    child
                        .file_name()
                        .map(|n| PathBuf::from(n.to_os_string()))
                        .unwrap_or_else(|| PathBuf::from("unnamed")),
                );
                if destination.exists() {
                    return Err(std::io::Error::other("backup destination exists"));
                }
                fs::rename(child, &destination)?;
            }
        }
        fs::remove_dir(legacy_root)?;
        Ok(())
    })();
    result.is_ok()
}

fn migrate_legacy_data(source: &Path, target: &Path) -> bool {
    let source_canonical = dunce_normalize(source);
    let target_canonical = dunce_normalize(target);
    if source_canonical == target_canonical || !source.is_dir() {
        return false;
    }
    let marker = target.join(NAME_MIGRATION_MARKER);
    let source_id = migration_source_id(source);
    let mut migrated_ids = read_migration_ids(&marker);
    if migrated_ids.iter().any(|id| *id == source_id) {
        remove_migrated_source(source);
        return true;
    }
    if let Err(_) = fs::create_dir_all(target) {
        return false;
    }
    if !migrated_ids.is_empty() {
        if !merge_additional_legacy_data(source, target, &source_id) {
            return false;
        }
    } else {
        backup_conflicting_state(source, target);
        if copy_tree(source, target).is_err() {
            return false;
        }
        if !copied_tree_matches(source, target) {
            return false;
        }
    }
    migrated_ids.push(source_id);
    write_migration_ids(&marker, &mut migrated_ids);
    remove_migrated_source(source);
    true
}

fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
    for entry in walk_files(source) {
        let relative = entry.strip_prefix(source).map_err(std::io::Error::other)?;
        let target_file = target.join(relative);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&entry, &target_file)?;
    }
    // Preserve empty directories.
    for dir in walk_dirs(source) {
        let relative = dir.strip_prefix(source).map_err(std::io::Error::other)?;
        let target_dir = target.join(relative);
        let _ = fs::create_dir_all(target_dir);
    }
    Ok(())
}

fn walk_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path.clone());
                stack.push(path);
            }
        }
    }
    out
}

fn migrate_first_legacy_data(candidates: &[PathBuf], target: &Path) {
    let marker = target.join(NAME_MIGRATION_MARKER);
    if marker.is_file() {
        if let Ok(marker_value) = fs::read_to_string(&marker) {
            let value = marker_value.trim();
            if read_migration_ids(&marker).is_empty() && !value.is_empty() {
                let marker_path = PathBuf::from(value);
                if let Some(matched) = candidates
                    .iter()
                    .find(|c| dunce_normalize(c) == dunce_normalize(&marker_path))
                {
                    write_migration_ids(&marker, &mut vec![migration_source_id(matched)]);
                }
            }
        }
    }
    for legacy_dir in candidates {
        migrate_legacy_data(legacy_dir, target);
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

pub fn user_data_dir(app_name: &str) -> PathBuf {
    let mut bases: Vec<PathBuf> = Vec::new();
    for name in ["LOCALAPPDATA", "APPDATA"] {
        if let Some(p) = env_path(name) {
            if !bases.contains(&p) {
                bases.push(p);
            }
        }
    }
    if !bases.is_empty() {
        let target_dir = bases[0].join(app_name).join("data");
        let legacy_roots: Vec<PathBuf> = bases
            .iter()
            .flat_map(|base| {
                LEGACY_APP_NAMES
                    .iter()
                    .map(move |name| base.join(name))
                    .collect::<Vec<_>>()
            })
            .collect();
        let data_roots: Vec<PathBuf> = legacy_roots.iter().map(|r| r.join("data")).collect();
        migrate_first_legacy_data(&data_roots, &target_dir);
        for legacy_root in &legacy_roots {
            cleanup_legacy_app_root(legacy_root, &target_dir);
        }
        return target_dir;
    }
    let home = home_dir();
    let target_dir = home.join(format!(".{app_name}")).join("data");
    let data_roots: Vec<PathBuf> = LEGACY_APP_NAMES
        .iter()
        .map(|name| home.join(format!(".{name}")).join("data"))
        .collect();
    migrate_first_legacy_data(&data_roots, &target_dir);
    target_dir
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn portable_marker_present(base_dir: &Path) -> bool {
    PORTABLE_MARKERS
        .iter()
        .any(|name| base_dir.join(name).is_file())
}

/// Portable-mode migration: legacy roots sitting next to the install base.
pub fn migrate_legacy_roots_near(base_dir: &Path, install_data: &Path) {
    let parent = base_dir.parent().unwrap_or(base_dir);
    let legacy_roots: Vec<PathBuf> = LEGACY_APP_NAMES
        .iter()
        .map(|name| parent.join(name))
        .collect();
    let data_roots: Vec<PathBuf> = legacy_roots.iter().map(|r| r.join("data")).collect();
    migrate_first_legacy_data(&data_roots, install_data);
    for legacy_root in &legacy_roots {
        cleanup_legacy_app_root(legacy_root, install_data);
    }
}

pub fn resolve_data_dir(base_dir: &Path, install_data_dir: &Path, app_name: &str) -> PathBuf {
    if portable_marker_present(base_dir) && is_writable(install_data_dir) {
        let legacy_roots: Vec<PathBuf> = LEGACY_APP_NAMES
            .iter()
            .map(|name| {
                base_dir
                    .parent()
                    .unwrap_or(base_dir)
                    .join(name)
            })
            .collect();
        let data_roots: Vec<PathBuf> = legacy_roots.iter().map(|r| r.join("data")).collect();
        migrate_first_legacy_data(&data_roots, install_data_dir);
        for legacy_root in &legacy_roots {
            cleanup_legacy_app_root(legacy_root, install_data_dir);
        }
        return install_data_dir.to_path_buf();
    }
    user_data_dir(app_name)
}

/// Seed a fresh user data dir from the packaged templates (`seed_user_data`).
pub fn seed_user_data(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if src == dst || !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for sub in ["templates", "configs"] {
        let source = src.join(sub);
        let target = dst.join(sub);
        if source.is_dir() && !target.exists() {
            copy_tree(&source, &target)?;
        }
    }
    refresh_pristine_singbox_default(src, dst);
    for name in ["state.enc", "traffic_history.json"] {
        let source = src.join(name);
        let target = dst.join(name);
        if !source.is_file() {
            continue;
        }
        if !target.exists() {
            let _ = fs::copy(&source, &target);
        }
    }
    Ok(())
}

fn refresh_pristine_singbox_default(src: &Path, dst: &Path) {
    let source = src.join("templates").join("sing-box").join("default.json");
    let target = dst.join("templates").join("sing-box").join("default.json");
    if !source.is_file() || !target.is_file() {
        return;
    }
    let (Ok(source_payload), Ok(target_payload)) = (
        fs::read_to_string(&source),
        fs::read_to_string(&target),
    ) else {
        return;
    };
    let (Ok(source_json), Ok(target_json)) = (
        serde_json::from_str::<serde_json::Value>(&source_payload),
        serde_json::from_str::<serde_json::Value>(&target_payload),
    ) else {
        return;
    };
    let source_tun = match source_json
        .get("inbounds")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|item| {
                item.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t.eq_ignore_ascii_case("tun"))
                    .unwrap_or(false)
            })
        }) {
        Some(v) => v.clone(),
        None => return,
    };
    if source_tun.get("interface_name").and_then(|v| v.as_str()) != Some("singbox_tun") {
        return;
    }
    // Compare the target against a legacy-shaped copy of the packaged file.
    let mut legacy_json = source_json.clone();
    if let Some(inbounds) = legacy_json.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
        if let Some(legacy_tun) = inbounds.iter_mut().find(|item| {
            item.get("type")
                .and_then(|t| t.as_str())
                .map(|t| t.eq_ignore_ascii_case("tun"))
                .unwrap_or(false)
        }) {
            legacy_tun["interface_name"] = serde_json::Value::String("tun0".into());
        }
    }
    if target_json == legacy_json {
        let _ = fs::copy(&source, &target);
    }
}

static INSTALL_ID: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// Stable anonymous client id used to correlate diagnostics.
pub fn get_install_id() -> String {
    let mut guard = INSTALL_ID.lock();
    if let Some(value) = guard.as_ref() {
        return value.clone();
    }
    let path = crate::constants::data_dir().join("install_id");
    let mut value = fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if value.is_empty() {
        value = uuid::Uuid::new_v4().simple().to_string();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, &value);
    }
    *guard = Some(value.clone());
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_id_is_hex64() {
        let id = migration_source_id(Path::new(r"C:\Program Files\Lumen"));
        assert_eq!(id.len(), 64);
        assert!(is_migration_id(&id));
    }
}
