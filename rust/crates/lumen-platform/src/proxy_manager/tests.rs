use std::cell::{Cell, RefCell};

use super::*;

struct FakeSystem {
    current: RefCell<ProxySettings>,
    flags: Cell<u32>,
    writes: Cell<usize>,
    native_fails: Cell<bool>,
    native_sets_flags: Cell<bool>,
    foreign_owner: Cell<Option<u64>>,
}

impl FakeSystem {
    fn new() -> Self {
        Self {
            current: RefCell::new(ProxySettings {
                proxy_enable: 0,
                proxy_server: "corp:3128".into(),
                proxy_override: "corp.local".into(),
                auto_config_url: "https://corp/pac".into(),
            }),
            flags: Cell::new(13),
            writes: Cell::new(0),
            native_fails: Cell::new(false),
            native_sets_flags: Cell::new(true),
            foreign_owner: Cell::new(None),
        }
    }
}

impl SystemProxy for FakeSystem {
    fn read(&self) -> Result<ProxySettings> {
        Ok(self.current.borrow().clone())
    }
    fn write(&self, settings: &ProxySettings) -> Result<()> {
        self.current.replace(settings.clone());
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
    fn flags(&self) -> Result<u32> {
        Ok(self.flags.get())
    }
    fn apply(&self, snapshot: &Snapshot) -> std::result::Result<(), NativeError> {
        if self.native_sets_flags.get() {
            self.flags.set(snapshot.flags);
        }
        if self.native_fails.get() {
            Err(NativeError {
                code: 87,
                option: 0,
            })
        } else {
            Ok(())
        }
    }
    fn refresh(&self) {}
    fn process_created(&self, pid: u32) -> Result<Option<u64>> {
        Ok(if pid == std::process::id() {
            Some(987654321)
        } else {
            self.foreign_owner.get()
        })
    }
}

#[test]
fn restores_pac_wpad_and_original_static_fields_after_crash() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let system = FakeSystem::new();
    let original = system.read()?;
    let manager = ProxyManager::new(dir.path().into());
    manager.enable_system(&system, 10808, true)?;
    assert_eq!(system.current.borrow().proxy_server, "127.0.0.1:10808");
    assert_eq!(system.flags.get(), 3);
    let mut persisted = manager.load_receipt()?.unwrap();
    persisted.owner = (u32::MAX, 1234);
    manager.store_receipt(&persisted)?;
    system.current.borrow_mut().proxy_override.clear();
    system.flags.set(1);

    let recovered = ProxyManager::new(dir.path().into());
    assert!(recovered.disable_system(&system, true)?);
    assert_eq!(system.read()?, original);
    assert_eq!(system.flags.get(), 13);
    assert!(!manager.backup_path.exists());
    Ok(())
}

#[test]
fn external_changes_and_foreign_live_owner_block_automatic_writes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let system = FakeSystem::new();
    let manager = ProxyManager::new(dir.path().into());
    manager.enable_system(&system, 10808, false)?;
    system.writes.set(0);
    system.current.borrow_mut().auto_config_url = "https://external/proxy.pac".into();
    assert!(!manager.disable_system(&system, true)?);
    assert!(manager.enable_system(&system, 10808, false).is_err());
    assert_eq!(system.writes.get(), 0);
    let mut receipt = manager.load_receipt()?.unwrap();
    receipt.owner = (u32::MAX, 1234);
    manager.store_receipt(&receipt)?;
    system.foreign_owner.set(Some(1234));
    assert!(manager.reset_system(&system).is_err());
    assert_eq!(system.writes.get(), 0);
    assert!(manager.backup_path.is_file());
    Ok(())
}

#[test]
fn unverifiable_or_unwritable_receipt_prevents_any_proxy_change() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let system = FakeSystem::new();
    let manager = ProxyManager::new(dir.path().into());
    fs::write(&manager.backup_path, b"{\"ProxyEnable\":1}")?;
    assert!(manager.enable_system(&system, 10808, false).is_err());
    assert!(!manager.disable_system(&system, true)?);
    assert_eq!(system.writes.get(), 0);
    fs::remove_file(&manager.backup_path)?;

    let file_as_directory = dir.path().join("file");
    fs::write(&file_as_directory, b"not a directory")?;
    let unwritable = ProxyManager::new(file_as_directory);
    assert!(unwritable.enable_system(&system, 10808, false).is_err());
    assert_eq!(system.writes.get(), 0);
    Ok(())
}

#[test]
fn accepts_native_failure_only_when_effective_settings_can_be_verified() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let manager = ProxyManager::new(dir.path().into());
    let system = FakeSystem::new();
    let original = system.read()?;
    system.native_fails.set(true);
    system.native_sets_flags.set(false);
    assert!(manager.enable_system(&system, 10808, true).is_err());
    assert_eq!(system.read()?, original);
    assert_eq!(system.flags.get(), 13);
    assert!(!manager.backup_path.exists());

    system.native_sets_flags.set(true);
    manager.enable_system(&system, 10808, true)?;
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(&manager.backup_path)?)?;
    assert_eq!(receipt["version"], 2);
    assert_eq!(receipt["original"]["AutoConfigURL"], "https://corp/pac");
    assert_eq!(receipt["original"]["WinInetFlags"], 13);
    assert_eq!(receipt["applied"]["ProxyEnable"], 1);
    assert_eq!(receipt["applied"]["ProxyServer"], "127.0.0.1:10808");
    assert_eq!(receipt["applied"]["WinInetFlags"], 3);
    assert_eq!(
        receipt["owner"],
        serde_json::json!([std::process::id(), 987654321])
    );
    Ok(())
}

#[test]
fn explicit_reset_archives_only_after_verified_restore() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let manager = ProxyManager::new(dir.path().into());
    let system = FakeSystem::new();
    manager.enable_system(&system, 10808, true)?;
    system.current.borrow_mut().proxy_server = "foreign:5000".into();
    system.native_fails.set(true);
    system.native_sets_flags.set(false);
    assert!(manager.reset_system(&system).is_err());
    assert!(manager.backup_path.exists());

    system.native_sets_flags.set(true);
    manager.reset_system(&system)?;
    assert_eq!(system.read()?, ProxySettings::default());
    assert_eq!(system.flags.get(), PROXY_TYPE_DIRECT);
    assert!(!manager.backup_path.exists());
    assert!(
        fs::read_dir(dir.path())?
            .filter_map(|entry| entry.ok())
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("system_proxy_backup.recovery-"))
    );
    Ok(())
}
