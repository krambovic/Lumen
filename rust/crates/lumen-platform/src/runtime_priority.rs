//! Keep the UI and owned network cores schedulable under CPU load.

#[cfg(windows)]
use windows::Win32::System::Threading::{
    ABOVE_NORMAL_PRIORITY_CLASS, GetCurrentProcess, GetPriorityClass, NORMAL_PRIORITY_CLASS,
    SetPriorityClass,
};

#[cfg(windows)]
pub const CORE_RUN_PRIORITY_FLAG: u32 = ABOVE_NORMAL_PRIORITY_CLASS.0;
#[cfg(not(windows))]
pub const CORE_RUN_PRIORITY_FLAG: u32 = 0;

/// Raise a normal-priority Lumen UI process to above normal.
#[cfg(windows)]
pub fn prioritize_lumen_ui() -> bool {
    // The pseudo-handle refers to the current process and needs no close.
    unsafe {
        let process = GetCurrentProcess();
        GetPriorityClass(process) == NORMAL_PRIORITY_CLASS.0
            && SetPriorityClass(process, ABOVE_NORMAL_PRIORITY_CLASS).is_ok()
    }
}

#[cfg(not(windows))]
pub fn prioritize_lumen_ui() -> bool {
    false
}

#[cfg(all(test, windows))]
mod tests {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use super::*;

    #[test]
    fn raises_normal_priority_once_in_an_isolated_process() -> std::io::Result<()> {
        const CHILD: &str = "LUMEN_TEST_PRIORITY_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert!(prioritize_lumen_ui());
            assert!(!prioritize_lumen_ui());
            return Ok(());
        }
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "runtime_priority::tests::raises_normal_priority_once_in_an_isolated_process",
            ])
            .env(CHILD, "1")
            .creation_flags(NORMAL_PRIORITY_CLASS.0 | crate::subprocess::CREATE_NO_WINDOW)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        Ok(())
    }
}
