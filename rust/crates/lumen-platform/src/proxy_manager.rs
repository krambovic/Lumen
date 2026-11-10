//! WinINet LAN proxy and optional Firefox-family profile integration.

mod firefox;
mod wininet;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use windows::Win32::Networking::WinInet::{PROXY_TYPE_DIRECT, PROXY_TYPE_PROXY};

use firefox::FirefoxProxyManager;
use wininet::WinInet;

const LOOPBACK: &str = "127.0.0.1";
const DIRECT_AND_PROXY: u32 = PROXY_TYPE_DIRECT | PROXY_TYPE_PROXY;
const DEFAULT_PROXY_BYPASS: &str = concat!(
    "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;",
    "172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;",
    "172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;",
    "*.lan;*.local;::1",
);
const UNPROVEN_OWNER: &str = "Сохранённые настройки прокси требуют проверки: другой экземпляр или неподтверждённая резервная копия";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProxySettings {
    proxy_enable: u32,
    proxy_server: String,
    proxy_override: String,
    #[serde(rename = "AutoConfigURL")]
    auto_config_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Snapshot {
    #[serde(flatten)]
    settings: ProxySettings,
    #[serde(rename = "WinInetFlags")]
    flags: u32,
}

impl Snapshot {
    fn direct() -> Self {
        Self {
            settings: ProxySettings::default(),
            flags: PROXY_TYPE_DIRECT,
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.settings.proxy_enable <= 1 && self.flags <= 15,
            "invalid proxy snapshot"
        );
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Receipt {
    version: u32,
    original: Snapshot,
    applied: Snapshot,
    owner: (u32, u64),
}

#[derive(Debug)]
struct NativeError {
    code: i32,
    option: u32,
}

impl NativeError {
    fn detail(&self) -> String {
        if self.code == 0 && self.option == 0 {
            String::new()
        } else {
            format!(" (Win32={}, option={})", self.code, self.option)
        }
    }
}

trait SystemProxy {
    fn read(&self) -> Result<ProxySettings>;
    fn write(&self, settings: &ProxySettings) -> Result<()>;
    fn flags(&self) -> Result<u32>;
    fn apply(&self, snapshot: &Snapshot) -> std::result::Result<(), NativeError>;
    fn refresh(&self);
    fn process_created(&self, pid: u32) -> Result<Option<u64>>;
}

/// Keeps a version-2 recovery receipt alongside the Python client's runtime
/// files. Automatic recovery requires both the receipt and matching settings.
pub struct ProxyManager {
    backup_path: PathBuf,
    lock_path: PathBuf,
    firefox: FirefoxProxyManager,
}

impl ProxyManager {
    pub fn new(runtime_dir: PathBuf) -> Self {
        Self {
            backup_path: runtime_dir.join("system_proxy_backup.json"),
            lock_path: runtime_dir.join("system_proxy.lock"),
            firefox: FirefoxProxyManager::new(runtime_dir.join("firefox_proxy_backup.json")),
        }
    }

    pub fn is_enabled(&self) -> Result<bool> {
        Ok(WinInet.read()?.proxy_enable == 1)
    }

    /// The system and Firefox both use the mixed inbound on `socks_port`.
    pub fn enable(&self, socks_port: u16, bypass_lan: bool, configure_firefox: bool) -> Result<()> {
        let _lock = self.lock()?;
        self.enable_system(&WinInet, socks_port, bypass_lan)?;
        if configure_firefox {
            let _ = self.firefox.enable(socks_port, bypass_lan);
        } else {
            let _ = self.firefox.disable();
        }
        Ok(())
    }

    pub fn disable(&self, restore_previous: bool) -> Result<bool> {
        let _lock = self.lock()?;
        let restored = self.disable_system(&WinInet, restore_previous)?;
        if restored {
            let _ = self.firefox.disable();
        }
        Ok(restored)
    }

    pub fn reconcile_stale_state(&self) -> Result<bool> {
        let _lock = self.lock()?;
        let restored = match self.load_receipt() {
            Ok(Some(receipt)) => self.restore_owned(&WinInet, receipt, true)?,
            Ok(None) => {
                // Finish a browser restore deferred until Firefox exited.
                let _ = self.firefox.disable();
                false
            }
            Err(_) => false,
        };
        if restored {
            let _ = self.firefox.disable();
        }
        Ok(restored)
    }

    /// Explicit user reset. A verified reset archives any previous receipt.
    pub fn reset_conflicted_state(&self) -> Result<()> {
        let _lock = self.lock()?;
        self.reset_system(&WinInet)
    }

    pub fn disable_necko_overrides(&self) -> Result<()> {
        let _lock = self.lock()?;
        self.firefox.disable()
    }

    fn lock(&self) -> Result<File> {
        fs::create_dir_all(
            self.lock_path
                .parent()
                .context("missing proxy runtime directory")?,
        )?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)?;
        file.lock()?;
        Ok(file)
    }

    fn load_receipt(&self) -> Result<Option<Receipt>> {
        let bytes = match fs::read(&self.backup_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let receipt: Receipt = serde_json::from_slice(&bytes)?;
        ensure!(
            receipt.version == 2 && receipt.owner.0 != 0 && receipt.owner.1 != 0,
            "unproven proxy ownership"
        );
        receipt.original.validate()?;
        receipt.applied.validate()?;
        Ok(Some(receipt))
    }

    fn store_receipt(&self, receipt: &Receipt) -> Result<()> {
        write_backup(&self.backup_path, ".proxy-", receipt)
    }

    fn enable_system(
        &self,
        system: &impl SystemProxy,
        socks_port: u16,
        bypass_lan: bool,
    ) -> Result<()> {
        ensure!(socks_port != 0, "Локальный порт прокси не настроен");
        let original = match self.load_receipt().context(UNPROVEN_OWNER)? {
            Some(receipt) => {
                ensure!(
                    !owner_is_live_elsewhere(system, receipt.owner),
                    UNPROVEN_OWNER
                );
                ensure!(
                    matches_owned(system, &receipt.applied)?,
                    "Системный прокси изменён вне Lumen; автоматическая перезапись отменена"
                );
                receipt.original
            }
            None => {
                let snapshot = Snapshot {
                    settings: system.read()?,
                    flags: system
                        .flags()
                        .context("Не удалось сохранить исходные параметры WinINET")?,
                };
                snapshot.validate()?;
                snapshot
            }
        };
        let applied = Snapshot {
            settings: ProxySettings {
                proxy_enable: 1,
                proxy_server: format!("{LOOPBACK}:{socks_port}"),
                proxy_override: if bypass_lan {
                    DEFAULT_PROXY_BYPASS
                } else {
                    "<local>;localhost;127.*"
                }
                .into(),
                auto_config_url: String::new(),
            },
            flags: DIRECT_AND_PROXY,
        };
        let pid = std::process::id();
        let created = system
            .process_created(pid)?
            .filter(|created| *created != 0)
            .context("Не удалось определить владельца системного прокси")?;
        let mut receipt = Receipt {
            version: 2,
            original,
            applied,
            owner: (pid, created),
        };
        self.store_receipt(&receipt)?;

        let apply = (|| -> Result<()> {
            system.write(&receipt.applied.settings)?;
            let native = system.apply(&receipt.applied);
            system.refresh();
            if let Err(error) = native {
                let verified_flags = system
                    .flags()
                    .ok()
                    .filter(|flags| *flags <= 15 && *flags & PROXY_TYPE_PROXY != 0);
                if system.read()? != receipt.applied.settings || verified_flags.is_none() {
                    bail!("Не удалось применить параметры WinINET{}", error.detail());
                }
                receipt.applied.flags = verified_flags.expect("checked above");
                self.store_receipt(&receipt)?;
            }
            Ok(())
        })();
        if let Err(error) = apply {
            // Only roll back a complete registry write still owned by this operation.
            if system
                .read()
                .is_ok_and(|settings| settings == receipt.applied.settings)
            {
                restore_snapshot(system, &receipt.original)
                    .with_context(|| format!("Не удалось откатить изменение прокси: {error:#}"))?;
                remove_if_exists(&self.backup_path)?;
            }
            return Err(error);
        }
        Ok(())
    }

    fn disable_system(&self, system: &impl SystemProxy, restore_previous: bool) -> Result<bool> {
        match self.load_receipt() {
            Ok(Some(receipt)) => self.restore_owned(system, receipt, restore_previous),
            _ => Ok(false),
        }
    }

    fn restore_owned(
        &self,
        system: &impl SystemProxy,
        receipt: Receipt,
        restore_previous: bool,
    ) -> Result<bool> {
        if owner_is_live_elsewhere(system, receipt.owner)
            || !matches_owned(system, &receipt.applied)?
        {
            return Ok(false);
        }
        let desired = if restore_previous {
            receipt.original
        } else {
            Snapshot::direct()
        };
        restore_snapshot(system, &desired)?;
        remove_if_exists(&self.backup_path)?;
        Ok(true)
    }

    fn reset_system(&self, system: &impl SystemProxy) -> Result<()> {
        const OTHER_OWNER: &str =
            "Сохранённые настройки прокси принадлежат другому экземпляру Lumen";
        let receipt = self.load_receipt().context(OTHER_OWNER)?;
        ensure!(
            receipt
                .as_ref()
                .is_none_or(|receipt| !owner_is_live_elsewhere(system, receipt.owner)),
            OTHER_OWNER
        );
        restore_snapshot(system, &Snapshot::direct())?;
        if receipt.is_some() {
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let recovery = self
                .backup_path
                .with_file_name(format!("system_proxy_backup.recovery-{timestamp}.json"));
            fs::rename(&self.backup_path, recovery)?;
        }
        Ok(())
    }
}

fn owner_is_live_elsewhere(system: &impl SystemProxy, (pid, generation): (u32, u64)) -> bool {
    pid != std::process::id()
        && !system
            .process_created(pid)
            .is_ok_and(|created| created != Some(generation))
}

fn matches_owned(system: &impl SystemProxy, applied: &Snapshot) -> Result<bool> {
    let current = system.read()?;
    let Ok(flags) = system.flags() else {
        return Ok(false);
    };
    if current == applied.settings && flags == applied.flags {
        return Ok(true);
    }
    // Windows can clear just the bypass or the effective PROXY flag.
    Ok(current.proxy_enable == 1
        && current.proxy_server == applied.settings.proxy_server
        && current.auto_config_url == applied.settings.auto_config_url
        && (current.proxy_override.is_empty()
            || current.proxy_override == applied.settings.proxy_override)
        && is_loopback_proxy(&applied.settings.proxy_server)
        && matches!(flags, PROXY_TYPE_DIRECT | DIRECT_AND_PROXY))
}

fn is_loopback_proxy(server: &str) -> bool {
    server.split([',', ';']).any(|entry| {
        let entry = entry.trim();
        let address = entry.rsplit_once('=').map_or(entry, |(_, address)| address);
        let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
        host.trim().trim_matches(['[', ']']) == LOOPBACK
    })
}

fn restore_snapshot(system: &impl SystemProxy, desired: &Snapshot) -> Result<()> {
    system.write(&desired.settings)?;
    let result = system.apply(desired);
    system.refresh();
    if system.read()? != desired.settings || system.flags().ok() != Some(desired.flags) {
        let detail = result
            .err()
            .map_or_else(String::new, |error| error.detail());
        bail!(
            "Не удалось полностью восстановить параметры WinINET{detail}; резервная копия сохранена"
        );
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn write_backup(path: &Path, prefix: &str, contents: &impl Serialize) -> Result<()> {
    let directory = path.parent().context("missing proxy backup directory")?;
    fs::create_dir_all(directory)?;
    let mut staged = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".tmp")
        .tempfile_in(directory)?;
    serde_json::to_writer(&mut staged, contents)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests;
