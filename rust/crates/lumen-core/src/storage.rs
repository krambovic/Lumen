//! State storage (`storage.py`): plain/DPAPI/passphrase encodings, backup
//! rotation, atomic writes, corrupt-file quarantine and path normalization.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};

use crate::constants::{
    configs_dir, log_dir, runtime_dir, singbox_configs_dir, singbox_templates_dir,
    xray_configs_dir, xray_templates_dir, singbox_path_default, xray_path_default,
};
use crate::models::{AppState, AppSettings};
use crate::path_utils::normalize_configured_path;
use crate::routing_presets::{build_routing_preset, ROUTING_PRESET_BLOCKED};
use crate::security::{
    decode_encrypted, decrypt_with_passphrase, encode_encrypted, encrypt_with_passphrase,
    is_passphrase_encrypted,
};

const BACKUP_COUNT: usize = 3;
const PATH_MIGRATION_MARKER: &str = ".state_path_migration_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateEncoding {
    Plain,
    Dpapi,
    Passphrase,
}

#[derive(Debug, thiserror::Error)]
pub enum StateLoadError {
    #[error("passphrase required")]
    PassphraseRequired,
    #[error("wrong passphrase or corrupted encrypted profile")]
    WrongPassphrase,
    #[error("{0}")]
    Corrupt(String),
}

pub struct StateStorage {
    pub state_file: PathBuf,
    inner: Mutex<StorageInner>,
}

struct StorageInner {
    passphrase: String,
    encoding: Option<StateEncoding>,
    load_state: LoadState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadState {
    Unloaded,
    Loaded,
    Locked,
    Error,
}

pub fn is_legacy_bundled_singbox_path(value: &str) -> bool {
    let raw = value.trim();
    if raw.is_empty() {
        return false;
    }
    let normalized = raw.replace('/', "\\").trim_start_matches(".\\").to_lowercase();
    if normalized == "core\\sing-box.exe"
        || normalized == "core\\sing-box-extended-1.14.1-2.7.2\\sing-box.exe"
    {
        return true;
    }
    let candidate = PathBuf::from(raw);
    if !candidate.is_absolute() {
        return false;
    }
    let legacy_bundles = [
        singbox_path_default(),
        singbox_path_default()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default()
            .parent()
            .map(|p| p.join("core").join("sing-box.exe"))
            .unwrap_or_default(),
    ];
    legacy_bundles
        .iter()
        .any(|b| same_path(&candidate, b))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase().replace('/', "\\")
        == b.to_string_lossy().to_lowercase().replace('/', "\\")
}

fn ensure_dirs() {
    for dir in [
        crate::constants::data_dir().clone(),
        configs_dir(),
        singbox_templates_dir(),
        xray_templates_dir(),
        singbox_configs_dir(),
        xray_configs_dir(),
        runtime_dir(),
        log_dir(),
    ] {
        let _ = fs::create_dir_all(dir);
    }
}

/// Normalized profile reference: relative to the profile dir when possible.
fn normalize_profile_reference(value: &str, base: &Path) -> String {
    let text = value.trim();
    if text.is_empty() {
        return String::new();
    }
    let candidate = PathBuf::from(text);
    if !candidate.is_absolute() {
        return candidate.to_string_lossy().replace('\\', "/");
    }
    if let Ok(relative) = candidate.strip_prefix(base) {
        return relative.to_string_lossy().replace('\\', "/");
    }
    // Older builds stored full paths below data/configs or data/templates;
    // recover the relative suffix after the matching profile directory.
    let base_tail: Vec<String> = base
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let base_tail: Vec<String> = base_tail.into_iter().rev().collect();
    let candidate_parts: Vec<String> = candidate
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let candidate_folded: Vec<String> = candidate_parts
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let legacy_names = ["lumen kvn", "lumenkvn", "lumen-kvn", "lumen_kvn"];
    let contains_legacy_name = candidate_folded.iter().any(|p| legacy_names.contains(&p.as_str()));
    if candidate_folded.len() >= base_tail.len() {
        for index in (0..=candidate_folded.len() - base_tail.len()).rev() {
            if &candidate_folded[index..index + base_tail.len()] != base_tail.as_slice() {
                continue;
            }
            let suffix = &candidate_parts[index + base_tail.len()..];
            if suffix.is_empty() || suffix.iter().any(|p| p.is_empty() || p == "." || p == "..") {
                continue;
            }
            let migrated: PathBuf = base.join(suffix.join("\\"));
            if contains_legacy_name || migrated.is_file() {
                return suffix.join("/").replace('\\', "/");
            }
        }
    }
    let fallback = base.join(
        candidate
            .file_name()
            .map(|n| PathBuf::from(n.to_os_string()))
            .unwrap_or_default(),
    );
    if fallback.is_file() {
        return candidate
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    text.to_string()
}

fn normalize_state_paths(mut state: AppState) -> AppState {
    let xray_default = xray_path_default();
    let singbox_default = singbox_path_default();
    let mut singbox = state.settings.singbox_path.clone();
    if is_legacy_bundled_singbox_path(&singbox) {
        singbox = String::new();
    }
    state.settings.xray_path = normalize_configured_path(
        &state.settings.xray_path,
        Some(&xray_default),
        true,
        true,
    );
    state.settings.singbox_path = normalize_configured_path(&singbox, Some(&singbox_default), true, true);
    state.settings.xray_config_file =
        normalize_profile_reference(&state.settings.xray_config_file, &xray_configs_dir());
    state.settings.xray_template_file =
        normalize_profile_reference(&state.settings.xray_template_file, &xray_templates_dir());
    state.settings.singbox_config_file = normalize_profile_reference(
        &state.settings.singbox_config_file,
        &singbox_configs_dir(),
    );
    state.settings.singbox_template_file = normalize_profile_reference(
        &state.settings.singbox_template_file,
        &singbox_templates_dir(),
    );
    state
}

fn default_state() -> AppState {
    let mut state = AppState::default();
    state.routing = build_routing_preset(&state.routing, ROUTING_PRESET_BLOCKED);
    normalize_state_paths(state)
}

impl StateStorage {
    pub fn new(state_file: PathBuf) -> Self {
        ensure_dirs();
        Self {
            state_file,
            inner: Mutex::new(StorageInner {
                passphrase: String::new(),
                encoding: None,
                load_state: LoadState::Unloaded,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StorageInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_passphrase(&self, value: &str) {
        self.lock().passphrase = value.to_string();
    }

    pub fn passphrase(&self) -> String {
        self.lock().passphrase.clone()
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        let mut name = self.state_file.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".bak{index}"));
        self.state_file.with_file_name(name)
    }

    fn tmp_path(&self) -> PathBuf {
        let mut name = std::ffi::OsString::from(".");
        name.push(self.state_file.file_name().unwrap_or_default());
        name.push(".tmp");
        self.state_file.with_file_name(name)
    }

    fn serialize_state(state: &AppState) -> String {
        let mut payload = state.to_dict();
        if let Some(settings) = payload.get_mut("settings").and_then(|v| v.as_object_mut()) {
            Self::normalize_settings_payload_paths(settings);
        }
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    }

    fn normalize_settings_payload_paths(settings: &mut Map<String, Value>) {
        let xray_default = xray_path_default();
        let singbox_default = singbox_path_default();
        let get = |key: &str| -> String {
            settings
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let xray = normalize_configured_path(&get("xray_path"), Some(&xray_default), true, true);
        let mut singbox = get("singbox_path");
        if is_legacy_bundled_singbox_path(&singbox) {
            singbox = String::new();
        }
        let singbox = normalize_configured_path(&singbox, Some(&singbox_default), true, true);
        let xray_config = normalize_profile_reference(&get("xray_config_file"), &xray_configs_dir());
        let xray_template =
            normalize_profile_reference(&get("xray_template_file"), &xray_templates_dir());
        let singbox_config =
            normalize_profile_reference(&get("singbox_config_file"), &singbox_configs_dir());
        let singbox_template = normalize_profile_reference(
            &get("singbox_template_file"),
            &singbox_templates_dir(),
        );
        settings.insert("xray_path".into(), Value::String(xray));
        settings.insert("singbox_path".into(), Value::String(singbox));
        settings.insert("xray_config_file".into(), Value::String(xray_config));
        settings.insert("xray_template_file".into(), Value::String(xray_template));
        settings.insert("singbox_config_file".into(), Value::String(singbox_config));
        settings.insert("singbox_template_file".into(), Value::String(singbox_template));
    }

    /// Decode raw state text, returning the state plus whether paths were
    /// rewritten (triggering an immediate re-encode) and the detected encoding.
    fn decode_state_details(
        &self,
        raw_text: &str,
        passphrase: &str,
    ) -> Result<(AppState, bool, StateEncoding), StateLoadError> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            return Err(StateLoadError::Corrupt("state file is empty".into()));
        }
        let (payload, encoding) = if is_passphrase_encrypted(raw_text) {
            if passphrase.is_empty() {
                return Err(StateLoadError::PassphraseRequired);
            }
            match decrypt_with_passphrase(raw_text, passphrase) {
                Ok(decrypted) => {
                    let parsed: Value = serde_json::from_slice(&decrypted)
                        .map_err(|e| StateLoadError::Corrupt(e.to_string()))?;
                    (parsed, StateEncoding::Passphrase)
                }
                Err(_) => return Err(StateLoadError::WrongPassphrase),
            }
        } else if raw_text.starts_with('{') {
            let parsed: Value = serde_json::from_str(raw_text)
                .map_err(|e| StateLoadError::Corrupt(e.to_string()))?;
            (parsed, StateEncoding::Plain)
        } else {
            match decode_encrypted(raw_text) {
                Ok(decrypted) => {
                    let parsed: Value = serde_json::from_slice(&decrypted)
                        .map_err(|e| StateLoadError::Corrupt(e.to_string()))?;
                    (parsed, StateEncoding::Dpapi)
                }
                Err(_) => {
                    let parsed: Value = serde_json::from_str(raw_text)
                        .map_err(|e| StateLoadError::Corrupt(e.to_string()))?;
                    (parsed, StateEncoding::Plain)
                }
            }
        };
        let state = AppState::from_dict(&payload);
        let before = state_path_values(&state.settings);
        let normalized = normalize_state_paths(state);
        let changed = state_path_values(&normalized.settings) != before;
        Ok((normalized, changed, encoding))
    }

    fn encode_state(&self, state: &AppState, encoding: StateEncoding, passphrase: &str) -> String {
        let payload = Self::serialize_state(state);
        match encoding {
            StateEncoding::Passphrase => encrypt_with_passphrase(payload.as_bytes(), passphrase)
                .unwrap_or(payload),
            StateEncoding::Dpapi => encode_encrypted(payload.as_bytes()).unwrap_or(payload),
            StateEncoding::Plain => payload,
        }
    }

    fn atomic_write_path(path: &Path, content: &str) -> anyhow::Result<()> {
        let staged = path.with_file_name(format!(
            ".{}.migration.tmp",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        fs::write(&staged, content)?;
        fs::rename(&staged, path)?;
        Ok(())
    }

    fn quarantine_unreadable_state(&self) -> Option<PathBuf> {
        if !self.state_file.exists() {
            return None;
        }
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let quarantine = self.state_file.with_file_name(format!(
            "{}.corrupt-{stamp}",
            self.state_file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        fs::rename(&self.state_file, &quarantine).ok()?;
        Some(quarantine)
    }

    fn normalize_backup_paths_once(&self, passphrase: &str) {
        let marker = self.state_file.with_file_name(PATH_MIGRATION_MARKER);
        if marker.is_file() {
            return;
        }
        let mut complete = true;
        for index in 1..=BACKUP_COUNT {
            let backup = self.backup_path(index);
            if !backup.is_file() {
                continue;
            }
            let Ok(raw_text) = fs::read_to_string(&backup) else {
                complete = false;
                continue;
            };
            match self.decode_state_details(&raw_text, passphrase) {
                Ok((state, changed, encoding)) => {
                    if changed {
                        let content = self.encode_state(&state, encoding, passphrase);
                        let _ = Self::atomic_write_path(&backup, &content);
                    }
                }
                Err(_) => complete = false,
            }
        }
        if complete {
            let _ = Self::atomic_write_path(&marker, "complete\n");
        }
    }

    fn rotate_backups(&self) {
        let Ok(meta) = fs::metadata(&self.state_file) else {
            return;
        };
        if meta.len() == 0 {
            return;
        }
        for index in (2..=BACKUP_COUNT).rev() {
            let previous = self.backup_path(index - 1);
            let current = self.backup_path(index);
            if previous.is_file() {
                let _ = fs::rename(&previous, &current);
            }
        }
        let backup = self.backup_path(1);
        let staged = backup.with_file_name(format!(
            ".{}.tmp",
            backup.file_name().unwrap_or_default().to_string_lossy()
        ));
        if fs::copy(&self.state_file, &staged).is_ok() {
            let _ = fs::rename(&staged, &backup);
        }
    }

    pub fn is_encrypted(&self) -> bool {
        match fs::read_to_string(&self.state_file) {
            Ok(raw) => is_passphrase_encrypted(raw.trim()),
            Err(_) => false,
        }
    }

    /// Load state; returns Err(PassphraseRequired) when locked.
    pub fn load(&self) -> Result<AppState, StateLoadError> {
        ensure_dirs();
        let mut inner = self.lock();
        inner.load_state = LoadState::Unloaded;
        let passphrase = inner.passphrase.clone();
        let candidates = [
            self.state_file.clone(),
            self.tmp_path(),
            self.backup_path(1),
            self.backup_path(2),
            self.backup_path(3),
        ];
        let existing: Vec<PathBuf> = candidates
            .iter()
            .filter(|p| p.is_file())
            .cloned()
            .collect();
        if existing.is_empty() {
            let state = default_state();
            inner.encoding = Some(StateEncoding::Plain);
            inner.load_state = LoadState::Loaded;
            return Ok(state);
        }
        let mut errors: Vec<String> = Vec::new();
        for candidate in &existing {
            let raw_text = match fs::read_to_string(candidate) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("{}: {e}", candidate.display()));
                    continue;
                }
            };
            let (state, paths_migrated, encoding) =
                match self.decode_state_details(&raw_text, &passphrase) {
                    Ok(v) => v,
                    Err(StateLoadError::PassphraseRequired) => {
                        inner.load_state = LoadState::Locked;
                        return Err(StateLoadError::PassphraseRequired);
                    }
                    Err(StateLoadError::WrongPassphrase) => {
                        inner.load_state = LoadState::Locked;
                        return Err(StateLoadError::WrongPassphrase);
                    }
                    Err(e) => {
                        errors.push(format!("{}: {e}", candidate.display()));
                        continue;
                    }
                };
            if candidate != &self.state_file {
                self.quarantine_unreadable_state();
                let restored_content = if paths_migrated {
                    self.encode_state(&state, encoding, &passphrase)
                } else {
                    raw_text.trim().to_string()
                };
                if let Err(e) = Self::atomic_write_path(&self.state_file, &restored_content) {
                    errors.push(format!("restore: {e}"));
                    inner.load_state = LoadState::Error;
                    return Err(StateLoadError::Corrupt(format!("restore failed: {e}")));
                }
            } else if paths_migrated {
                let content = self.encode_state(&state, encoding, &passphrase);
                let _ = Self::atomic_write_path(&self.state_file, &content);
            }
            inner.encoding = Some(encoding);
            inner.load_state = LoadState::Loaded;
            self.normalize_backup_paths_once(&passphrase);
            return Ok(state);
        }
        let quarantine = self.quarantine_unreadable_state();
        inner.load_state = LoadState::Error;
        Err(StateLoadError::Corrupt(format!(
            "Не удалось восстановить {}.{} {}",
            self.state_file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            quarantine
                .map(|q| format!(" Файл сохранён как {}.", q.display()))
                .unwrap_or_default(),
            errors.iter().take(4).cloned().collect::<Vec<_>>().join("; ")
        )))
    }

    fn save_encoding(&self, state: &AppState, encoding: Option<StateEncoding>, passphrase: &str) -> StateEncoding {
        // Keep the loaded encoding and protect newly stored proxy secrets.
        if !passphrase.is_empty() {
            return StateEncoding::Passphrase;
        }
        if encoding == Some(StateEncoding::Dpapi) {
            return StateEncoding::Dpapi;
        }
        if !state.settings.proxy_auth_password.is_empty() {
            return StateEncoding::Dpapi;
        }
        StateEncoding::Plain
    }

    pub fn save(&self, state: &AppState) -> Result<(), StateLoadError> {
        ensure_dirs();
        let mut inner = self.lock();
        if inner.load_state != LoadState::Loaded {
            let candidates = [
                self.state_file.clone(),
                self.tmp_path(),
                self.backup_path(1),
                self.backup_path(2),
                self.backup_path(3),
            ];
            if inner.load_state != LoadState::Unloaded || candidates.iter().any(|p| p.exists()) {
                return Err(StateLoadError::Corrupt(
                    "Профиль не загружен: запись заблокирована".into(),
                ));
            }
            // A genuinely new store has no existing data to overwrite.
            inner.load_state = LoadState::Loaded;
        }
        let passphrase = inner.passphrase.clone();
        let encoding = self.save_encoding(state, inner.encoding, &passphrase);
        let content = self.encode_state(state, encoding, &passphrase);
        self.rotate_backups();
        Self::atomic_write_path(&self.state_file, &content)
            .map_err(|e| StateLoadError::Corrupt(format!("write failed: {e}")))
    }

    pub fn export_backup(&self, path: &Path, passphrase: &str) -> Result<(), StateLoadError> {
        let state = self.load()?;
        let payload = Self::serialize_state(&state);
        let content = if passphrase.is_empty() {
            payload
        } else {
            encrypt_with_passphrase(payload.as_bytes(), passphrase)
                .map_err(|e| StateLoadError::Corrupt(e.to_string()))?
        };
        fs::write(path, content).map_err(|e| StateLoadError::Corrupt(e.to_string()))
    }

    pub fn import_backup(&self, path: &Path, passphrase: &str) -> Result<AppState, StateLoadError> {
        let raw = fs::read_to_string(path)
            .map_err(|e| StateLoadError::Corrupt(e.to_string()))?
            .trim()
            .to_string();
        let payload: Value = if is_passphrase_encrypted(&raw) {
            if passphrase.is_empty() {
                return Err(StateLoadError::PassphraseRequired);
            }
            let decrypted = decrypt_with_passphrase(&raw, passphrase)
                .map_err(|_| StateLoadError::WrongPassphrase)?;
            serde_json::from_slice(&decrypted).map_err(|e| StateLoadError::Corrupt(e.to_string()))?
        } else {
            serde_json::from_str(&raw).map_err(|e| StateLoadError::Corrupt(e.to_string()))?
        };
        Ok(normalize_state_paths(AppState::from_dict(&payload)))
    }

    pub fn has_pending_state(&self) -> bool {
        let candidates = [
            self.state_file.clone(),
            self.tmp_path(),
            self.backup_path(1),
            self.backup_path(2),
            self.backup_path(3),
        ];
        candidates.iter().any(|p| p.exists())
    }
}

fn state_path_values(settings: &AppSettings) -> [String; 6] {
    [
        settings.xray_path.clone(),
        settings.singbox_path.clone(),
        settings.xray_config_file.clone(),
        settings.xray_template_file.clone(),
        settings.singbox_config_file.clone(),
        settings.singbox_template_file.clone(),
    ]
}

#[cfg(test)]
mod quarantine_tests {
    use super::*;

    /// Python test_state_storage_safety: a broken profile is moved aside,
    /// never silently replaced by a fresh one.
    #[test]
    fn invalid_state_is_quarantined_instead_of_reset() {
        let dir = tempfile::tempdir().unwrap();
        let state_file = dir.path().join("state.json");
        fs::write(&state_file, r#"{"nodes": ["#).unwrap();
        assert!(StateStorage::new(state_file.clone()).load().is_err());
        assert!(!state_file.exists());
        let quarantined = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().starts_with("state.json.corrupt-"));
        assert!(quarantined);
    }
}
