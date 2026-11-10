//! Manage active JSON copies and bundled templates for both Windows engines.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use lumen_core::constants::{SINGBOX_DEFAULT_CONFIG_NAME, XRAY_DEFAULT_CONFIG_NAME, data_dir};
use lumen_core::models::{AppSettings, AppState, RoutingSettings};
use lumen_engines::singbox::document::SingboxDocumentState;
use serde_json::Value;

use crate::config_documents::SingboxDocumentCache;
use crate::config_profiles::{
    default_template_text, normalize_relative_json_path, read_profile_text, relative_display,
    resolve_profile_path, validate_json_text, write_profile_text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileEngine {
    Xray,
    Singbox,
}

impl ProfileEngine {
    fn directory(self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::Singbox => "sing-box",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::Singbox => "singbox",
        }
    }

    fn default_name(self) -> &'static str {
        match self {
            Self::Xray => XRAY_DEFAULT_CONFIG_NAME,
            Self::Singbox => SINGBOX_DEFAULT_CONFIG_NAME,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileEvent {
    SettingsChanged,
    RoutingChanged,
    SaveRequested,
    TransitionRequested(String),
}

/// The loaded state and connection context owned by the application.
pub struct ProfileSession<'a> {
    pub state: &'a mut AppState,
    pub connected: bool,
    pub desired_connected: &'a mut bool,
    pub active_core: Option<ProfileEngine>,
    pub emit: &'a mut dyn FnMut(ProfileEvent),
}

impl ProfileSession<'_> {
    fn connection_requested(&self) -> bool {
        self.connected || *self.desired_connected
    }

    fn request_transition(&mut self, reason: impl Into<String>) {
        *self.desired_connected = true;
        (self.emit)(ProfileEvent::TransitionRequested(reason.into()));
    }

    fn singbox_editor(&self) -> bool {
        self.state.settings.tun_mode && self.state.settings.core_preference != "xray"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEditResult {
    /// A file was written when this is Some.
    pub path: Option<PathBuf>,
    pub message: String,
}

pub struct ProfileService {
    root: PathBuf,
    singbox_documents: SingboxDocumentCache,
}

impl Default for ProfileService {
    fn default() -> Self {
        Self::new(data_dir().clone())
    }
}

#[derive(Clone, Copy)]
enum Location {
    Config,
    Template,
}

impl Location {
    fn directory(self) -> &'static str {
        match self {
            Self::Config => "configs",
            Self::Template => "templates",
        }
    }

    fn label(self, engine: ProfileEngine) -> String {
        match self {
            Self::Config => engine.directory().into(),
            Self::Template => format!("{} template", engine.directory()),
        }
    }

    fn reference(self, settings: &mut AppSettings, engine: ProfileEngine) -> &mut String {
        match (engine, self) {
            (ProfileEngine::Xray, Self::Config) => &mut settings.xray_config_file,
            (ProfileEngine::Xray, Self::Template) => &mut settings.xray_template_file,
            (ProfileEngine::Singbox, Self::Config) => &mut settings.singbox_config_file,
            (ProfileEngine::Singbox, Self::Template) => &mut settings.singbox_template_file,
        }
    }

    fn selected(self, settings: &AppSettings, engine: ProfileEngine) -> &str {
        match (engine, self) {
            (ProfileEngine::Xray, Self::Config) => &settings.xray_config_file,
            (ProfileEngine::Xray, Self::Template) => &settings.xray_template_file,
            (ProfileEngine::Singbox, Self::Config) => &settings.singbox_config_file,
            (ProfileEngine::Singbox, Self::Template) => &settings.singbox_template_file,
        }
    }
}

impl ProfileService {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            singbox_documents: SingboxDocumentCache::default(),
        }
    }

    fn directory(&self, engine: ProfileEngine, location: Location) -> PathBuf {
        self.root
            .join(location.directory())
            .join(engine.directory())
    }

    fn resolve(
        &self,
        engine: ProfileEngine,
        location: Location,
        path: Option<&Path>,
    ) -> Result<PathBuf> {
        let directory = self.directory(engine, location);
        fs::create_dir_all(&directory)?;
        resolve_profile_path(
            &directory,
            path,
            engine.default_name(),
            &location.label(engine),
        )
    }

    fn resolve_selected(
        &self,
        settings: &AppSettings,
        engine: ProfileEngine,
        location: Location,
        path: Option<&Path>,
    ) -> Result<PathBuf> {
        let path = path
            .filter(|path| !path.to_string_lossy().trim().is_empty())
            .unwrap_or_else(|| Path::new(location.selected(settings, engine)));
        self.resolve(engine, location, Some(path))
    }

    fn set_active(
        &self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        location: Location,
        path: &Path,
        emit_changed: bool,
    ) -> Result<()> {
        let directory = self.directory(engine, location).canonicalize()?;
        let relative = relative_display(path.strip_prefix(&directory)?);
        let field = location.reference(&mut session.state.settings, engine);
        if *field != relative {
            *field = relative;
            if emit_changed {
                (session.emit)(ProfileEvent::SettingsChanged);
            }
            (session.emit)(ProfileEvent::SaveRequested);
        }
        Ok(())
    }

    pub fn get_active_config_path(
        &self,
        settings: &AppSettings,
        engine: ProfileEngine,
    ) -> Result<PathBuf> {
        self.resolve_selected(settings, engine, Location::Config, None)
    }

    pub fn get_active_config_name(
        &self,
        settings: &AppSettings,
        engine: ProfileEngine,
    ) -> Result<String> {
        Ok(file_name(&self.get_active_config_path(settings, engine)?))
    }

    pub fn get_active_template_path(
        &self,
        settings: &AppSettings,
        engine: ProfileEngine,
    ) -> Option<PathBuf> {
        let configured = Location::Template.selected(settings, engine);
        let configured = if configured.is_empty() {
            Location::Config.selected(settings, engine)
        } else {
            configured
        };
        let relative =
            normalize_relative_json_path(Some(Path::new(configured)), engine.default_name());
        self.resolve(engine, Location::Template, Some(&relative))
            .ok()
            .filter(|path| path.exists())
    }

    fn matching_template(
        &self,
        engine: ProfileEngine,
        active_path: &Path,
    ) -> Result<Option<PathBuf>> {
        let directory = self.directory(engine, Location::Config).canonicalize()?;
        let relative = active_path.strip_prefix(&directory)?;
        let template = self.resolve(engine, Location::Template, Some(relative))?;
        Ok(template.exists().then_some(template))
    }

    fn cache_text(&mut self, engine: ProfileEngine, path: &Path, text: &str) {
        if engine == ProfileEngine::Singbox {
            self.singbox_documents.cache_state(path, text);
        }
    }

    fn sync_template(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        active: &Path,
        template: Option<&Path>,
    ) -> Result<bool> {
        let Some(template) = template else {
            return Ok(false);
        };
        let modified = |path: &Path| fs::metadata(path).and_then(|metadata| metadata.modified());
        let (Ok(template_time), Ok(active_time)) = (modified(template), modified(active)) else {
            return Ok(false);
        };
        if template_time <= active_time {
            return Ok(false);
        }
        let template_text = read_profile_text(template)?;
        let active_text = read_profile_text(active)?;
        if template_text == active_text {
            return Ok(false);
        }
        let mut backup_name = active
            .file_name()
            .context("missing profile name")?
            .to_os_string();
        backup_name.push(".bak");
        let _ = write_profile_text(&active.with_file_name(backup_name), &active_text);
        write_profile_text(active, &template_text)?;
        self.cache_text(engine, active, &template_text);
        if session.connection_requested() {
            session.request_transition(format!("{} template synced", engine.key()));
        }
        Ok(true)
    }

    pub fn ensure_active_config(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        path: Option<&Path>,
        sync_template: bool,
    ) -> Result<PathBuf> {
        let resolved =
            self.resolve_selected(&session.state.settings, engine, Location::Config, path)?;
        let mut template = self.get_active_template_path(&session.state.settings, engine);
        if template.is_none() && path.is_some() {
            template = self.matching_template(engine, &resolved)?;
        }
        let default = default_template_text(
            &self.directory(engine, Location::Template),
            engine.default_name(),
            engine.directory(),
        )?;
        fs::create_dir_all(resolved.parent().context("missing profile directory")?)?;
        if !resolved.try_exists()? {
            if let Some(template) = &template {
                write_profile_text(&resolved, &read_profile_text(template)?)?;
                self.set_active(session, engine, Location::Template, template, false)?;
            } else {
                write_profile_text(&resolved, &default)?;
            }
        } else if sync_template {
            self.sync_template(session, engine, &resolved, template.as_deref())?;
        }
        self.set_active(session, engine, Location::Config, &resolved, true)?;
        Ok(resolved)
    }

    pub fn load_active_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
    ) -> Result<(PathBuf, String)> {
        let path = self.ensure_active_config(session, engine, None, true)?;
        let text = read_profile_text(&path)?;
        self.cache_text(engine, &path, &text);
        Ok((path, text))
    }

    pub fn load_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        path: &Path,
    ) -> Result<(PathBuf, String)> {
        let resolved = self.resolve_selected(
            &session.state.settings,
            engine,
            Location::Config,
            Some(path),
        )?;
        ensure!(
            resolved.exists(),
            "Файл не найден: {}",
            file_name(&resolved)
        );
        self.set_active(session, engine, Location::Config, &resolved, true)?;
        let template = self
            .matching_template(engine, &resolved)?
            .or_else(|| self.get_active_template_path(&session.state.settings, engine));
        self.sync_template(session, engine, &resolved, template.as_deref())?;
        let text = read_profile_text(&resolved)?;
        self.cache_text(engine, &resolved, &text);
        Ok((resolved, text))
    }

    pub fn import_template(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        path: &Path,
    ) -> Result<(PathBuf, String)> {
        let template = self.resolve_selected(
            &session.state.settings,
            engine,
            Location::Template,
            Some(path),
        )?;
        ensure!(
            template.exists(),
            "Файл не найден: {}",
            file_name(&template)
        );
        let template_root = self.directory(engine, Location::Template).canonicalize()?;
        let relative = template.strip_prefix(&template_root)?;
        let active = self.resolve(engine, Location::Config, Some(relative))?;
        let text = read_profile_text(&template)?;
        write_profile_text(&active, &text)?;
        self.set_active(session, engine, Location::Template, &template, true)?;
        self.set_active(session, engine, Location::Config, &active, true)?;
        self.cache_text(engine, &active, &text);
        Ok((active, text))
    }

    pub fn reset_active_config_to_template(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
    ) -> Result<ProfileEditResult> {
        let Some(template) = self.get_active_template_path(&session.state.settings, engine) else {
            return Ok(ProfileEditResult {
                path: None,
                message: format!(
                    "Для текущего {} конфига не привязан template.",
                    engine.key()
                ),
            });
        };
        let active = self.ensure_active_config(session, engine, None, true)?;
        let text = read_profile_text(&template)?;
        write_profile_text(&active, &text)?;
        self.cache_text(engine, &active, &text);
        Ok(ProfileEditResult {
            path: Some(active),
            message: format!(
                "Активная копия сброшена к шаблону: {}",
                file_name(&template)
            ),
        })
    }

    pub fn save_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        text: &str,
        path: Option<&Path>,
        previous_text: Option<&str>,
        force_route_final: bool,
    ) -> Result<PathBuf> {
        let resolved = self.ensure_active_config(session, engine, path, false)?;
        write_profile_text(&resolved, text)?;
        self.cache_text(engine, &resolved, text);
        if engine == ProfileEngine::Singbox
            && sync_singbox_routing_from_config(
                &mut session.state.routing,
                text,
                previous_text,
                force_route_final,
            )
        {
            (session.emit)(ProfileEvent::RoutingChanged);
            (session.emit)(ProfileEvent::SaveRequested);
        }
        Ok(resolved)
    }

    pub fn apply_singbox_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        text: &str,
        previous_text: Option<&str>,
    ) -> Result<ProfileEditResult> {
        self.apply_config_text(session, ProfileEngine::Singbox, text, previous_text)
    }

    pub fn apply_xray_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        text: &str,
    ) -> Result<ProfileEditResult> {
        self.apply_config_text(session, ProfileEngine::Xray, text, None)
    }

    fn apply_config_text(
        &mut self,
        session: &mut ProfileSession<'_>,
        engine: ProfileEngine,
        text: &str,
        previous_text: Option<&str>,
    ) -> Result<ProfileEditResult> {
        let (valid, message) = validate_json_text(text);
        if !valid {
            return Ok(ProfileEditResult {
                path: None,
                message,
            });
        }
        let path = self.save_config_text(session, engine, text, None, previous_text, true)?;
        let active_core = session.active_core == Some(engine);
        let apply = match engine {
            ProfileEngine::Singbox => {
                active_core || (session.singbox_editor() && session.connection_requested())
            }
            ProfileEngine::Xray => {
                active_core && !session.singbox_editor() && session.connection_requested()
            }
        };
        let engine_name = engine.directory();
        let message = if apply {
            session.request_transition(format!("{engine_name} config applied"));
            format!("Конфиг сохранён. Применяю изменения {engine_name}...")
        } else {
            format!("Конфиг сохранён. Он будет использован при следующем запуске {engine_name}.")
        };
        Ok(ProfileEditResult {
            path: Some(path),
            message,
        })
    }

    pub fn get_singbox_document_state(
        &mut self,
        session: &mut ProfileSession<'_>,
    ) -> Result<&SingboxDocumentState> {
        let path = self.ensure_active_config(session, ProfileEngine::Singbox, None, true)?;
        Ok(self.singbox_documents.get_state(&path)?)
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Honor an explicit supported route.final without overwriting a later
/// routing choice when an unrelated editor change is saved.
pub fn sync_singbox_routing_from_config(
    routing: &mut RoutingSettings,
    text: &str,
    previous_text: Option<&str>,
    force: bool,
) -> bool {
    fn final_outbound(text: &str) -> Option<String> {
        let payload = serde_json::from_str::<Value>(text).ok()?;
        Some(
            payload["route"]["final"]
                .as_str()?
                .trim()
                .to_ascii_lowercase(),
        )
    }
    let Some(requested) =
        final_outbound(text).filter(|tag| matches!(tag.as_str(), "proxy" | "direct"))
    else {
        return false;
    };
    if !force && previous_text.and_then(final_outbound).as_ref() == Some(&requested) {
        return false;
    }
    let effective = match routing.mode.trim().to_ascii_lowercase().as_str() {
        "global" => "proxy",
        "direct" => "direct",
        _ if routing.tun_default_outbound.is_empty() => "proxy",
        _ => routing.tun_default_outbound.trim(),
    };
    if requested.eq_ignore_ascii_case(effective) {
        return false;
    }
    routing.mode = "rule".into();
    routing.tun_default_outbound = requested;
    routing.tun_default_outbound_user_selected = true;
    true
}

#[cfg(test)]
mod tests;
