//! OS identification without fingerprinting: `platform.machine()` and
//! `sys.getwindowsversion()` equivalents.

/// Python `platform.machine()` on Windows (`PROCESSOR_ARCHITEW6432`, then
/// `PROCESSOR_ARCHITECTURE`); elsewhere the Rust target architecture.
pub fn machine() -> String {
    #[cfg(windows)]
    {
        for name in ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"] {
            if let Ok(value) = std::env::var(name) {
                if !value.is_empty() {
                    return value;
                }
            }
        }
        match std::env::consts::ARCH {
            "x86_64" => "AMD64".into(),
            "aarch64" => "ARM64".into(),
            "x86" => "x86".into(),
            other => other.into(),
        }
    }
    #[cfg(not(windows))]
    {
        std::env::consts::ARCH.to_string()
    }
}

/// `(major, minor, build)` of the running Windows installation.
#[cfg(windows)]
pub fn windows_version() -> Option<(u32, u32, u32)> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok()?;
    let build: u32 = key.get_value::<String, _>("CurrentBuildNumber").ok()?.trim().parse().ok()?;
    if let (Ok(major), Ok(minor)) =
        (key.get_value::<u32, _>("CurrentMajorVersionNumber"), key.get_value::<u32, _>("CurrentMinorVersionNumber"))
    {
        return Some((major, minor, build));
    }
    let legacy = key.get_value::<String, _>("CurrentVersion").ok()?;
    let (major, minor) = legacy.trim().split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?, build))
}

#[cfg(not(windows))]
pub fn windows_version() -> Option<(u32, u32, u32)> {
    None
}

/// Derive the provider HWID without exposing the Windows installation GUID.
#[cfg(windows)]
pub fn subscription_hwid(install_id: &str) -> Option<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use uuid::Uuid;
    use winreg::{
        RegKey,
        enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
    };

    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    for view in [KEY_WOW64_64KEY, 0] {
        let Ok(key) =
            machine.open_subkey_with_flags(r"SOFTWARE\Microsoft\Cryptography", KEY_READ | view)
        else {
            continue;
        };
        let Ok(raw) = key.get_value::<String, _>("MachineGuid") else {
            continue;
        };
        let guid = raw.trim().trim_matches(['{', '}']);
        if guid.is_empty() || guid.contains(['\r', '\n']) {
            continue;
        }
        let guid = Uuid::parse_str(guid)
            .map(|guid| guid.to_string())
            .unwrap_or_else(|_| guid.chars().take(256).collect());
        let mut mac =
            Hmac::<Sha256>::new_from_slice(install_id.as_bytes()).expect("HMAC accepts any key size");
        mac.update(guid.as_bytes());
        let digest = mac.finalize().into_bytes();
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        return Some(Uuid::from_bytes(bytes).to_string());
    }
    None
}

#[cfg(not(windows))]
pub fn subscription_hwid(_install_id: &str) -> Option<String> {
    None
}

/// Python `sys.platform`.
pub fn sys_platform() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    /// Python test_node_selection: the provider HWID is derived from the
    /// MachineGuid and the install id, never the raw GUID itself.
    #[cfg(windows)]
    #[test]
    fn subscription_hwid_never_leaks_the_raw_machine_guid() {
        let Some(first) = super::subscription_hwid("install-a") else { return };
        eprintln!("hwid(install-a) = {first}");
        assert_eq!(first.len(), 36);
        assert_eq!(super::subscription_hwid("install-a").as_deref(), Some(first.as_str()));
        let second = super::subscription_hwid("install-b").unwrap();
        eprintln!("hwid(install-b) = {second}");
        assert_ne!(first, second);
        let raw = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Cryptography")
            .and_then(|key| key.get_value::<String, _>("MachineGuid"))
            .unwrap_or_default();
        assert!(!first.eq_ignore_ascii_case(raw.trim_matches(['{', '}'])));
    }

    #[test]
    fn machine_is_known() {
        assert!(!super::machine().is_empty());
        if cfg!(windows) {
            let (major, _, build) = super::windows_version().expect("windows version");
            assert!(major >= 6 && build > 0);
        }
    }
}
