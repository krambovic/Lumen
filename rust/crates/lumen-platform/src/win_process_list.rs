//! Enumerate executable names through a Windows Toolhelp snapshot.

pub fn list_running_executables() -> Vec<String> {
    #[cfg(windows)]
    return try_list_running_executables().unwrap_or_default();

    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(windows)]
pub(crate) fn try_list_running_executables() -> windows::core::Result<Vec<String>> {
    use std::collections::BTreeSet;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use windows::Win32::Foundation::ERROR_NO_MORE_FILES;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    let _handle = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut names = BTreeSet::new();
    let mut next = unsafe { Process32FirstW(snapshot, &mut entry) };
    loop {
        match next {
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        let len = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
        let exe = exe.trim();
        if exe.to_ascii_lowercase().ends_with(".exe") {
            names.insert(exe.to_owned());
        }
        next = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort_by_cached_key(|name| name.to_lowercase());
    Ok(names)
}

#[cfg(all(test, windows))]
#[test]
fn snapshot_contains_the_current_executable() {
    let exe = std::env::current_exe().unwrap();
    let exe = exe.file_name().unwrap().to_string_lossy();
    assert!(
        list_running_executables()
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&exe))
    );
}
