use std::fs::{self, File, FileTimes};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use lumen_core::models::AppState;

use super::*;
use crate::config_profiles::{normalize_relative_json_path, resolve_profile_path};

struct Fixture {
    root: tempfile::TempDir,
    service: ProfileService,
    state: AppState,
    connected: bool,
    desired: bool,
    active_core: Option<ProfileEngine>,
    events: Vec<ProfileEvent>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let service = ProfileService::new(root.path().into());
        let fixture = Self {
            root,
            service,
            state: AppState::default(),
            connected: false,
            desired: false,
            active_core: None,
            events: Vec::new(),
        };
        for engine in ["xray", "sing-box"] {
            fixture.put(&format!("templates/{engine}/default.json"), "{}")?;
        }
        Ok(fixture)
    }

    fn put(&self, relative: &str, text: &str) -> Result<PathBuf> {
        let path = self.root.path().join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, text)?;
        Ok(path)
    }

    fn call<T>(&mut self, f: impl FnOnce(&mut ProfileService, &mut ProfileSession<'_>) -> T) -> T {
        let mut session = ProfileSession {
            state: &mut self.state,
            connected: self.connected,
            desired_connected: &mut self.desired,
            active_core: self.active_core,
            emit: &mut |event| self.events.push(event),
        };
        f(&mut self.service, &mut session)
    }
}

#[test]
fn imports_named_template_over_stale_copy_and_resets_edits() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.put(
        "templates/xray/group/fast.json",
        "{\"from\":\"template\"}\r\n",
    )?;
    fixture.put("configs/xray/group/fast.json", "{\"stale\":true}")?;
    let (path, text) = fixture.call(|service, session| {
        service.import_template(session, ProfileEngine::Xray, Path::new("group/fast"))
    })?;
    assert_eq!(text, "{\"from\":\"template\"}\n");
    assert_eq!(fixture.state.settings.xray_config_file, "group/fast.json");
    assert_eq!(fixture.state.settings.xray_template_file, "group/fast.json");
    assert_eq!(
        fixture
            .service
            .get_active_config_name(&fixture.state.settings, ProfileEngine::Xray)?,
        "fast.json"
    );

    fixture.call(|service, session| {
        service.save_config_text(session, ProfileEngine::Xray, "{}", None, None, false)
    })?;
    assert_eq!(fs::read_to_string(&path)?, "{}");
    let reset = fixture.call(|service, session| {
        service.reset_active_config_to_template(session, ProfileEngine::Xray)
    })?;
    assert_eq!(reset.path.as_deref(), Some(path.as_path()));
    assert!(reset.message.contains("fast.json"));
    Ok(())
}

#[test]
fn syncs_a_newer_template_with_a_backup_and_reconnect_event() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let template = fixture.put(
        "templates/sing-box/default.json",
        "{\"outbounds\":[{\"tag\":\"proxy\"}]}",
    )?;
    let config = fixture.put("configs/sing-box/default.json", "{\"old\":true}")?;
    File::options()
        .write(true)
        .open(&config)?
        .set_times(FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(60)))?;
    assert!(template.metadata()?.modified()? > config.metadata()?.modified()?);
    fixture.connected = true;
    let document =
        fixture.call(|service, session| service.get_singbox_document_state(session).cloned())?;
    assert!(document.has_proxy_outbound);
    assert_eq!(document.text, fs::read_to_string(template)?);
    assert_eq!(
        fs::read_to_string(config.with_extension("json.bak"))?,
        "{\"old\":true}"
    );
    assert!(fixture.desired);
    assert!(fixture.events.contains(&ProfileEvent::TransitionRequested(
        "singbox template synced".into()
    )));
    Ok(())
}

#[test]
fn singbox_save_keeps_newer_routing_choice_until_the_user_applies_json() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.state.routing.tun_default_outbound = "direct".into();
    fixture.active_core = Some(ProfileEngine::Singbox);
    let old = r#"{"log":{"level":"info"},"route":{"final":"proxy"}}"#;
    let text = r#"{"log":{"level":"warning"},"route":{"final":"proxy"}}"#;
    let path = fixture.call(|service, session| {
        service.save_config_text(
            session,
            ProfileEngine::Singbox,
            text,
            None,
            Some(old),
            false,
        )
    })?;
    assert_eq!(fs::read_to_string(&path)?, text);
    assert_eq!(fixture.state.routing.tun_default_outbound, "direct");
    assert!(!fixture.events.contains(&ProfileEvent::RoutingChanged));

    let applied = fixture
        .call(|service, session| service.apply_singbox_config_text(session, text, Some(text)))?;
    assert_eq!(applied.path.as_deref(), Some(path.as_path()));
    assert_eq!(fixture.state.routing.tun_default_outbound, "proxy");
    assert!(fixture.state.routing.tun_default_outbound_user_selected);
    assert!(fixture.events.contains(&ProfileEvent::RoutingChanged));
    assert!(fixture.events.contains(&ProfileEvent::TransitionRequested(
        "sing-box config applied".into()
    )));
    Ok(())
}

#[test]
fn validates_before_writing_and_builds_runtime_from_active_profile() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.active_core = Some(ProfileEngine::Xray);
    fixture.connected = true;
    let invalid = fixture.call(|service, session| service.apply_xray_config_text(session, "{"))?;
    assert!(invalid.path.is_none());
    assert!(invalid.message.starts_with("Ошибка синтаксиса JSON:"));
    assert!(fixture.events.is_empty());
    let applied = fixture.call(|service, session| {
        service.apply_xray_config_text(
            session,
            r#"{"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#,
        )
    })?;
    let path = applied.path.unwrap();
    assert!(fixture.events.contains(&ProfileEvent::TransitionRequested(
        "xray config applied".into()
    )));
    let (loaded_path, text) = fixture
        .call(|service, session| service.load_active_config_text(session, ProfileEngine::Xray))?;
    let runtime = crate::xray_runtime_service::build_runtime_xray_config(
        &loaded_path,
        &text,
        &mut fixture.state,
        None,
        |_| {},
    )?;
    assert_eq!(runtime.source_path, path);
    assert_eq!(runtime.config["outbounds"][0]["tag"], "direct");
    Ok(())
}

#[test]
fn rejects_absolute_paths_outside_the_profile_directory() -> Result<()> {
    let fixture = Fixture::new()?;
    let outside = fixture.root.path().join("outside.json");
    let base = fixture.root.path().join("configs/xray");
    assert!(resolve_profile_path(&base, Some(&outside), "default.json", "xray").is_err());
    assert_eq!(
        normalize_relative_json_path(Some(Path::new(r"group\..\fast")), "default.json"),
        Path::new("group/fast.json")
    );
    Ok(())
}

#[test]
fn loading_a_named_config_prefers_its_matching_template() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.state.settings.xray_template_file = "default.json".into();
    let config = fixture.put("configs/xray/custom.json", r#"{"old":true}"#)?;
    let expected = r#"{"matching":true}"#;
    fixture.put("templates/xray/custom.json", expected)?;
    File::options()
        .write(true)
        .open(&config)?
        .set_times(FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(60)))?;

    let (path, text) = fixture.call(|service, session| {
        service.load_config_text(session, ProfileEngine::Xray, Path::new("custom"))
    })?;
    assert_eq!(path, config.canonicalize()?);
    assert_eq!(text, expected);
    assert_eq!(fixture.state.settings.xray_config_file, "custom.json");
    assert_eq!(fixture.state.settings.xray_template_file, "default.json");
    Ok(())
}
