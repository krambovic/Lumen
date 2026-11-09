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
    #[test]
    fn machine_is_known() {
        assert!(!super::machine().is_empty());
        if cfg!(windows) {
            let (major, _, build) = super::windows_version().expect("windows version");
            assert!(major >= 6 && build > 0);
        }
    }
}
