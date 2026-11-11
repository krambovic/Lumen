//! Profile path resolution and the JSON config editor API
//! (`AppController._resolve_*_config_path`, `load/save/apply_*_config_text`,
//! `export_*_json` and friends) on top of the ported `ProfileService`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use lumen_core::constants::{SINGBOX_DEFAULT_CONFIG_NAME, XRAY_DEFAULT_CONFIG_NAME, data_dir};
use serde_json::Value;

use crate::app_controller::{AppController, AppEvent};
use crate::config_profiles::{default_template_text, resolve_profile_path, validate_json_text};
use crate::profile_service::{ProfileEngine, ProfileEvent, ProfileService, ProfileSession};
use crate::xray_runtime_service::XrayConfigInspection;

impl AppController {
    /// Run a `ProfileService` mutation against a session view of the live
    /// state, forwarding emitted events to the controller signals.
    pub(crate) fn profile_edit<T>(
        &mut self,
        f: impl FnOnce(&mut ProfileService, &mut ProfileSession<'_>) -> T,
    ) -> T {
        let mut events = Vec::new();
        let active_core = match self.active_core.as_str() {
            "singbox" => Some(ProfileEngine::Singbox),
            "xray" => Some(ProfileEngine::Xray),
            _ => None,
        };
        let mut desired = self.runtime.desired_connected;
        let result = {
            let mut session = ProfileSession {
                state: &mut self.state,
                connected: self.connected,
                desired_connected: &mut desired,
                active_core,
                emit: &mut |event| events.push(event),
            };
            f(&mut self.profiles.borrow_mut(), &mut session)
        };
        self.runtime.desired_connected = desired;
        for event in events {
            match event {
                ProfileEvent::SettingsChanged => {
                    self.emit(AppEvent::SettingsChanged(self.state.settings.clone()))
                }
                ProfileEvent::RoutingChanged => self.emit(AppEvent::RoutingChanged),
                ProfileEvent::SaveRequested => self.schedule_save(),
                ProfileEvent::TransitionRequested(reason) => self.request_transition(&reason),
            }
        }
        result
    }

    // ── directories ────────────────────────────────────────────────────────

    /// Python `get_singbox_config_dir`.
    pub fn get_singbox_config_dir(&self) -> PathBuf {
        let directory = data_dir().join("configs").join("sing-box");
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    /// Python `get_xray_config_dir`.
    pub fn get_xray_config_dir(&self) -> PathBuf {
        let directory = data_dir().join("configs").join("xray");
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    /// Python `get_singbox_template_dir`.
    pub fn get_singbox_template_dir(&self) -> PathBuf {
        let directory = data_dir().join("templates").join("sing-box");
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    /// Python `get_xray_template_dir`.
    pub fn get_xray_template_dir(&self) -> PathBuf {
        let directory = data_dir().join("templates").join("xray");
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    // ── path normalization and resolution ──────────────────────────────────

    /// Python `_normalize_singbox_config_relative_path`.
    pub fn normalize_singbox_config_relative_path(&self, value: Option<&Path>) -> PathBuf {
        crate::config_profiles::normalize_relative_json_path(value, SINGBOX_DEFAULT_CONFIG_NAME)
    }

    /// Python `_normalize_singbox_template_relative_path`.
    pub fn normalize_singbox_template_relative_path(&self, value: Option<&Path>) -> PathBuf {
        self.normalize_singbox_config_relative_path(value)
    }

    /// Python `_resolve_singbox_config_path`.
    pub fn resolve_singbox_config_path(&self, path: Option<&Path>) -> Result<PathBuf> {
        let value = path.filter(|path| !path.to_string_lossy().trim().is_empty());
        let value = match value {
            Some(path) => Some(path.to_path_buf()),
            None => {
                let configured = self.state.settings.singbox_config_file.trim();
                (!configured.is_empty()).then(|| PathBuf::from(configured))
            }
        };
        resolve_profile_path(
            &self.get_singbox_config_dir(),
            value.as_deref(),
            SINGBOX_DEFAULT_CONFIG_NAME,
            "sing-box",
        )
    }

    /// Python `_resolve_singbox_template_path`.
    pub fn resolve_singbox_template_path(&self, path: Option<&Path>) -> Result<PathBuf> {
        let value = path.filter(|path| !path.to_string_lossy().trim().is_empty());
        let value = match value {
            Some(path) => Some(path.to_path_buf()),
            None => {
                let configured = self.state.settings.singbox_template_file.trim();
                (!configured.is_empty()).then(|| PathBuf::from(configured))
            }
        };
        resolve_profile_path(
            &self.get_singbox_template_dir(),
            value.as_deref(),
            SINGBOX_DEFAULT_CONFIG_NAME,
            "sing-box template",
        )
    }

    /// Python `_normalize_xray_config_relative_path`.
    pub fn normalize_xray_config_relative_path(&self, value: Option<&Path>) -> PathBuf {
        crate::config_profiles::normalize_relative_json_path(value, XRAY_DEFAULT_CONFIG_NAME)
    }

    /// Python `_normalize_xray_template_relative_path`.
    pub fn normalize_xray_template_relative_path(&self, value: Option<&Path>) -> PathBuf {
        self.normalize_xray_config_relative_path(value)
    }

    /// Python `_resolve_xray_config_path`.
    pub fn resolve_xray_config_path(&self, path: Option<&Path>) -> Result<PathBuf> {
        let value = path.filter(|path| !path.to_string_lossy().trim().is_empty());
        let value = match value {
            Some(path) => Some(path.to_path_buf()),
            None => {
                let configured = self.state.settings.xray_config_file.trim();
                (!configured.is_empty()).then(|| PathBuf::from(configured))
            }
        };
        resolve_profile_path(
            &self.get_xray_config_dir(),
            value.as_deref(),
            XRAY_DEFAULT_CONFIG_NAME,
            "xray",
        )
    }

    /// Python `_resolve_xray_template_path`.
    pub fn resolve_xray_template_path(&self, path: Option<&Path>) -> Result<PathBuf> {
        let value = path.filter(|path| !path.to_string_lossy().trim().is_empty());
        let value = match value {
            Some(path) => Some(path.to_path_buf()),
            None => {
                let configured = self.state.settings.xray_template_file.trim();
                (!configured.is_empty()).then(|| PathBuf::from(configured))
            }
        };
        resolve_profile_path(
            &self.get_xray_template_dir(),
            value.as_deref(),
            XRAY_DEFAULT_CONFIG_NAME,
            "xray template",
        )
    }

    fn relative_to(&self, path: &Path, directory: &Path) -> Result<String> {
        let directory = directory.canonicalize()?;
        let relative = path.strip_prefix(&directory)?;
        Ok(crate::config_profiles::relative_display(relative))
    }

    /// Python `_set_active_singbox_config_path`.
    pub fn set_active_singbox_config_path(
        &mut self,
        path: &Path,
        emit_signal: bool,
    ) -> Result<PathBuf> {
        let resolved = self.resolve_singbox_config_path(Some(path))?;
        let relative = self.relative_to(&resolved, &self.get_singbox_config_dir())?;
        if self.state.settings.singbox_config_file == relative {
            return Ok(resolved);
        }
        self.state.settings.singbox_config_file = relative;
        if emit_signal {
            self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        }
        self.schedule_save();
        Ok(resolved)
    }

    /// Python `_set_active_singbox_template_path`.
    pub fn set_active_singbox_template_path(
        &mut self,
        path: &Path,
        emit_signal: bool,
    ) -> Result<PathBuf> {
        let resolved = self.resolve_singbox_template_path(Some(path))?;
        let relative = self.relative_to(&resolved, &self.get_singbox_template_dir())?;
        if self.state.settings.singbox_template_file == relative {
            return Ok(resolved);
        }
        self.state.settings.singbox_template_file = relative;
        if emit_signal {
            self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        }
        self.schedule_save();
        Ok(resolved)
    }

    /// Python `_set_active_xray_config_path`.
    pub fn set_active_xray_config_path(
        &mut self,
        path: &Path,
        emit_signal: bool,
    ) -> Result<PathBuf> {
        let resolved = self.resolve_xray_config_path(Some(path))?;
        let relative = self.relative_to(&resolved, &self.get_xray_config_dir())?;
        if self.state.settings.xray_config_file == relative {
            return Ok(resolved);
        }
        self.state.settings.xray_config_file = relative;
        if emit_signal {
            self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        }
        self.schedule_save();
        Ok(resolved)
    }

    /// Python `_set_active_xray_template_path`.
    pub fn set_active_xray_template_path(
        &mut self,
        path: &Path,
        emit_signal: bool,
    ) -> Result<PathBuf> {
        let resolved = self.resolve_xray_template_path(Some(path))?;
        let relative = self.relative_to(&resolved, &self.get_xray_template_dir())?;
        if self.state.settings.xray_template_file == relative {
            return Ok(resolved);
        }
        self.state.settings.xray_template_file = relative;
        if emit_signal {
            self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        }
        self.schedule_save();
        Ok(resolved)
    }

    // ── default templates ──────────────────────────────────────────────────

    /// Python `_default_singbox_config_text`.
    pub fn default_singbox_config_text(&self) -> Result<String> {
        default_template_text(
            &self.get_singbox_template_dir(),
            SINGBOX_DEFAULT_CONFIG_NAME,
            "sing-box",
        )
    }

    /// Python `_default_xray_config_text` (the bundled template already
    /// carries PROXY_HOST/ports defaults).
    pub fn default_xray_config_text(&self) -> Result<String> {
        default_template_text(
            &self.get_xray_template_dir(),
            XRAY_DEFAULT_CONFIG_NAME,
            "xray",
        )
    }

    // ── active profile queries and mutations ───────────────────────────────

    /// Python `get_active_singbox_config_path`.
    pub fn get_active_singbox_config_path(&mut self) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.get_active_config_path(&session.state.settings, ProfileEngine::Singbox)
        })
    }

    /// Python `get_active_singbox_config_name`.
    pub fn get_active_singbox_config_name(&mut self) -> String {
        self.profile_edit(|profiles, session| {
            profiles
                .get_active_config_name(&session.state.settings, ProfileEngine::Singbox)
                .unwrap_or_default()
        })
    }

    /// Python `get_active_singbox_template_path`.
    pub fn get_active_singbox_template_path(&mut self) -> Option<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.get_active_template_path(&session.state.settings, ProfileEngine::Singbox)
        })
    }

    /// Python `get_active_xray_config_path`.
    pub fn get_active_xray_config_path(&mut self) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.get_active_config_path(&session.state.settings, ProfileEngine::Xray)
        })
    }

    /// Python `get_active_xray_config_name`.
    pub fn get_active_xray_config_name(&mut self) -> String {
        self.profile_edit(|profiles, session| {
            profiles
                .get_active_config_name(&session.state.settings, ProfileEngine::Xray)
                .unwrap_or_default()
        })
    }

    /// Python `get_active_xray_template_path`.
    pub fn get_active_xray_template_path(&mut self) -> Option<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.get_active_template_path(&session.state.settings, ProfileEngine::Xray)
        })
    }

    /// Python `_ensure_active_singbox_config`.
    pub fn ensure_active_singbox_config(&mut self, path: Option<&Path>) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.ensure_active_config(session, ProfileEngine::Singbox, path, true)
        })
    }

    /// Python `_ensure_active_xray_config`.
    pub fn ensure_active_xray_config(&mut self, path: Option<&Path>) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.ensure_active_config(session, ProfileEngine::Xray, path, true)
        })
    }

    /// Python `load_active_singbox_config_text`.
    pub fn load_active_singbox_config_text(&mut self) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.load_active_config_text(session, ProfileEngine::Singbox)
        })
    }

    /// Python `load_active_xray_config_text`.
    pub fn load_active_xray_config_text(&mut self) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.load_active_config_text(session, ProfileEngine::Xray)
        })
    }

    /// Python `load_singbox_config_text`.
    pub fn load_singbox_config_text(&mut self, path: &Path) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.load_config_text(session, ProfileEngine::Singbox, path)
        })
    }

    /// Python `load_xray_config_text`.
    pub fn load_xray_config_text(&mut self, path: &Path) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.load_config_text(session, ProfileEngine::Xray, path)
        })
    }

    /// Python `import_singbox_template`.
    pub fn import_singbox_template(&mut self, path: &Path) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.import_template(session, ProfileEngine::Singbox, path)
        })
    }

    /// Python `import_xray_template`.
    pub fn import_xray_template(&mut self, path: &Path) -> Result<(PathBuf, String)> {
        self.profile_edit(|profiles, session| {
            profiles.import_template(session, ProfileEngine::Xray, path)
        })
    }

    /// Python `reset_active_singbox_config_to_template`.
    pub fn reset_active_singbox_config_to_template(
        &mut self,
    ) -> Result<crate::profile_service::ProfileEditResult> {
        self.profile_edit(|profiles, session| {
            profiles.reset_active_config_to_template(session, ProfileEngine::Singbox)
        })
    }

    /// Python `reset_active_xray_config_to_template`.
    pub fn reset_active_xray_config_to_template(
        &mut self,
    ) -> Result<crate::profile_service::ProfileEditResult> {
        self.profile_edit(|profiles, session| {
            profiles.reset_active_config_to_template(session, ProfileEngine::Xray)
        })
    }

    /// Python `save_singbox_config_text`.
    pub fn save_singbox_config_text(
        &mut self,
        text: &str,
        path: Option<&Path>,
        previous_text: Option<&str>,
    ) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.save_config_text(session, ProfileEngine::Singbox, text, path, previous_text, false)
        })
    }

    /// Python `save_xray_config_text`.
    pub fn save_xray_config_text(&mut self, text: &str, path: Option<&Path>) -> Result<PathBuf> {
        self.profile_edit(|profiles, session| {
            profiles.save_config_text(session, ProfileEngine::Xray, text, path, None, false)
        })
    }

    /// Python `apply_singbox_config_text`.
    pub fn apply_singbox_config_text(
        &mut self,
        text: &str,
        previous_text: Option<&str>,
    ) -> Result<crate::profile_service::ProfileEditResult> {
        self.profile_edit(|profiles, session| {
            profiles.apply_singbox_config_text(session, text, previous_text)
        })
    }

    /// Python `apply_xray_config_text`.
    pub fn apply_xray_config_text(&mut self, text: &str) -> Result<crate::profile_service::ProfileEditResult> {
        self.profile_edit(|profiles, session| {
            profiles.apply_xray_config_text(session, text)
        })
    }

    // ── read-only profile inspection (usable through `&self`) ─────────────

    /// Resolve the active config path and materialize the file from the
    /// template/default when missing, without touching stored settings.
    fn readonly_config_path(
        &self,
        engine_directory: &str,
        default_name: &str,
        label: &str,
        configured: &str,
    ) -> Result<PathBuf> {
        let directory = data_dir().join("configs").join(engine_directory);
        std::fs::create_dir_all(&directory)?;
        let value = if configured.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(configured.trim()))
        };
        let path = resolve_profile_path(&directory, value.as_deref(), default_name, label)?;
        if !path.exists() {
            let template_directory = data_dir().join("templates").join(engine_directory);
            let text = default_template_text(&template_directory, default_name, engine_directory)
                .unwrap_or_else(|_| String::new());
            crate::config_profiles::write_profile_text(&path, &text)?;
        }
        Ok(path)
    }

    /// Read-only sing-box inspection for `&self` flows.
    pub(crate) fn readonly_singbox_has_proxy_outbound(&self) -> Result<bool> {
        let configured = self.state.settings.singbox_config_file.clone();
        let path = self.readonly_config_path("sing-box", SINGBOX_DEFAULT_CONFIG_NAME, "sing-box", &configured)?;
        let text = crate::config_profiles::read_profile_text(&path)?;
        let state = lumen_engines::singbox::document::inspect_singbox_document_text(&path, &text);
        Ok(state.has_proxy_outbound)
    }

    /// Read-only active config name for `&self` flows (no ensure IO beyond
    /// directory creation; the resolved name matches the ensured one).
    pub(crate) fn readonly_config_name(
        &self,
        engine_directory: &str,
        default_name: &str,
        label: &str,
        configured: &str,
    ) -> String {
        let directory = data_dir().join("configs").join(engine_directory);
        let _ = std::fs::create_dir_all(&directory);
        let value = if configured.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(configured.trim()))
        };
        resolve_profile_path(&directory, value.as_deref(), default_name, label)
            .ok()
            .map(|path| {
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default()
    }

    /// Read-only Xray inspection for `&self` flows.
    pub(crate) fn readonly_xray_has_proxy_outbound(&self) -> Result<bool> {
        let configured = self.state.settings.xray_config_file.clone();
        let path = self.readonly_config_path("xray", XRAY_DEFAULT_CONFIG_NAME, "xray", &configured)?;
        let text = crate::config_profiles::read_profile_text(&path)?;
        let settings = self.state.settings.clone();
        let inspection = crate::xray_runtime_service::inspect_active_xray_config(
            &path,
            &text,
            &settings,
            |_| {},
        );
        Ok(inspection.has_proxy_outbound)
    }

    // ── sing-box document cache ────────────────────────────────────────────

    /// Python `_cache_singbox_document_state`.
    pub fn cache_singbox_document_state(&mut self, path: &Path, text: &str) {
        let state = self.singbox_cache.cache_state(path, text).clone();
        if let Some(parsed) = self.parsed_singbox_document.as_ref()
            && (parsed.source_path != state.source_path || parsed.text_hash != state.text_hash)
        {
            self.parsed_singbox_document = None;
        }
    }

    /// Python `_get_singbox_document_state`.
    pub fn singbox_document_state(
        &mut self,
    ) -> Result<lumen_engines::singbox::document::SingboxDocumentState> {
        let path = self.ensure_active_singbox_config(None)?;
        Ok(self.singbox_cache.get_state(&path)?.clone())
    }

    /// Python `_inspect_active_singbox_config` -> `(path, text_hash,
    /// has_proxy_outbound)`.
    pub fn inspect_active_singbox_config(&mut self) -> (PathBuf, String, bool) {
        match self.singbox_document_state() {
            Ok(state) => (state.source_path, state.text_hash, state.has_proxy_outbound),
            Err(_) => (PathBuf::new(), String::new(), false),
        }
    }

    /// Python `_inspect_active_xray_config` -> inspection of the active Xray
    /// profile (may create or sync the file, exactly like the Python flow).
    pub fn inspect_active_xray_config(&mut self) -> Result<XrayConfigInspection> {
        let (path, text) = self.load_active_xray_config_text()?;
        let settings = self.state.settings.clone();
        let mut logs = Vec::new();
        let inspection = crate::xray_runtime_service::inspect_active_xray_config(
            &path,
            &text,
            &settings,
            |line| logs.push(line),
        );
        for line in logs {
            self.log(&line);
        }
        Ok(inspection)
    }

    /// Profile inspection for the signature service (`None` on IO failure).
    pub(crate) fn xray_profile_inspection(&mut self) -> Option<XrayConfigInspection> {
        self.inspect_active_xray_config().ok()
    }

    // ── validation ─────────────────────────────────────────────────────────

    /// Python `validate_json_text`.
    pub fn validate_json_text(&self, text: &str) -> (bool, String) {
        validate_json_text(text)
    }

    /// Python `validate_singbox_json_text`.
    pub fn validate_singbox_json_text(&self, text: &str) -> (bool, String) {
        self.validate_json_text(text)
    }

    /// Python `validate_xray_json_text`.
    pub fn validate_xray_json_text(&self, text: &str) -> (bool, String) {
        let (ok, message) = self.validate_json_text(text);
        if !ok {
            return (false, message);
        }
        if text.to_lowercase().contains("fakedns") {
            return (
                true,
                "JSON корректен. Внимание: в конфиге есть FakeDNS; некоторые версии Xray-core могут падать на старте. Если запуск завершается с panic, отключите FakeDNS или обновите Xray core.".into(),
            );
        }
        (true, message)
    }

    // ── runtime JSON export for the UI ─────────────────────────────────────

    /// Python `export_node_outbound_json`.
    pub fn export_node_outbound_json(&self, node_id: Option<&str>) -> Option<String> {
        let node = match node_id {
            Some(node_id) => self.get_node_by_id(Some(node_id)),
            None => self.selected_node(),
        }?;
        Some(dumps_ascii_pretty(&node.outbound))
    }

    /// Python `export_runtime_config_json`.
    pub fn export_runtime_config_json(&mut self, node_id: Option<&str>) -> Option<String> {
        let node = match node_id {
            Some(node_id) => self.get_node_by_id(Some(node_id)),
            None => self.selected_node(),
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String, String> {
            let settings = self.state.settings.clone();
            let use_singbox = self.is_singbox_editor_mode_with(&settings)
                || (!settings.tun_mode
                    && crate::node_runtime_service::proxy_core_for_node(
                        node.as_ref(),
                        crate::node_runtime_service::core_preference_of(Some(&settings)),
                    ) == "singbox");
            if use_singbox {
                let plan = self
                    .plan_runtime_singbox(node.as_ref(), settings.tun_mode, true)
                    .map_err(|error| error)?;
                return Ok(dumps_ascii_pretty(&plan.singbox_config));
            }
            let node_protocol = node
                .as_ref()
                .and_then(|node| node.outbound.get("protocol"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .map(str::to_lowercase)
                .or_else(|| {
                    node.as_ref()
                        .map(|node| node.scheme.trim().to_lowercase())
                })
                .unwrap_or_default();
            if self.uses_xray_raw_config() || node_protocol == "xray_config" {
                let runtime = self.build_runtime_xray_config(node.as_ref())?;
                return Ok(dumps_ascii_pretty(&runtime.config));
            }
            let node = node.ok_or_else(|| String::new())?;
            // Python repairs the stored node through the controller reference;
            // mirror that so the repair persists.
            let node_id = node.id.clone();
            if let Some(stored) = self.state.nodes.iter_mut().find(|node| node.id == node_id)
                && let Some(problem) =
                    crate::node_runtime_service::prepare_node_for_runtime(Some(stored), |_| {})
            {
                return Err(problem);
            }
            let node = self
                .get_node_by_id(Some(&node_id))
                .ok_or_else(|| String::new())?;
            let routing = self.runtime_routing();
            let cfg = lumen_core::config::xray_builder::build_xray_config(
                &node,
                &routing.to_dict(),
                &settings.to_dict(),
                0,
                u16::try_from(settings.local_socks_port).unwrap_or(0),
                u16::try_from(settings.local_http_port).unwrap_or(0),
            );
            Ok(dumps_ascii_pretty(&cfg))
        }));
        match outcome {
            Ok(Ok(text)) => Some(text),
            // RuntimeError comes from a missing rule-set binary or port
            // exhaustion; ValueError from a malformed profile.
            Ok(Err(_)) | Err(_) => None,
        }
    }
}

/// Python `json.dumps(value, ensure_ascii=True, indent=2)`: every non-ASCII
/// character of a valid JSON rendering lives inside a string literal, so a
/// post-pass escape is byte-identical to the Python encoder.
pub fn dumps_ascii_pretty(value: &Value) -> String {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
    let mut out = String::with_capacity(pretty.len());
    for ch in pretty.chars() {
        match ch {
            '\u{7F}'..='\u{10FFFF}' => {
                let scalar = ch as u32;
                if scalar > 0xFFFF {
                    // Python emits surrogate pairs for astral characters.
                    let scalar = scalar - 0x1_0000;
                    let high = 0xD800 + (scalar >> 10);
                    let low = 0xDC00 + (scalar & 0x3FF);
                    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                } else {
                    out.push_str(&format!("\\u{scalar:04x}"));
                }
            }
            ch => out.push(ch),
        }
    }
    out
}
