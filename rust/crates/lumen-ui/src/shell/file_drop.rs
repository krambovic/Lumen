//! Win32 file drops for an elevated Lumen window.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::GetDriveTypeW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::WindowsProgramming::{
    DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE,
};
use windows::Win32::UI::Shell::{
    DROPFILES, DefSubclassProc, DragAcceptFiles, DragFinish, DragQueryFileW, HDROP,
    RemoveWindowSubclass, SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, MSGFLT_ALLOW, MSGFLT_RESET, WM_DROPFILES, WM_NCDESTROY,
};
use windows::core::PCWSTR;

// Windows uses this companion message to marshal the HDROP across processes.
const WM_COPYGLOBALDATA: u32 = 0x0049;
const SUBCLASS_ID: usize = 1;
const MAX_DROP_FILES: u32 = 256;
const MAX_PATH_UNITS: u32 = 32_767;
const MAX_DROP_BYTES: usize = 32 * 1024 * 1024;

/// Must be called on the thread that owns the window.
pub(super) fn install(hwnd: HWND) -> anyhow::Result<Receiver<Vec<PathBuf>>> {
    let (sender, receiver) = sync_channel::<Vec<PathBuf>>(8);
    let sender = Box::into_raw(Box::new(sender));
    unsafe {
        if !SetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, sender as usize).as_bool() {
            drop(Box::from_raw(sender));
            anyhow::bail!("could not subclass the main window for file drops");
        }
        for message in [WM_DROPFILES, WM_COPYGLOBALDATA] {
            if let Err(error) = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None) {
                let _ = ChangeWindowMessageFilterEx(hwnd, WM_DROPFILES, MSGFLT_RESET, None);
                if RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID).as_bool() {
                    drop(Box::from_raw(sender));
                }
                return Err(error.into());
            }
        }
        DragAcceptFiles(hwnd, true);
    }
    Ok(receiver)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        if message == WM_DROPFILES {
            let memory = HGLOBAL(wparam.0 as _);
            let size = GlobalSize(memory);
            if size != 0 {
                let drop = HDROP(memory.0);
                let paths = if valid_payload(memory, size) {
                    read_paths(drop)
                } else {
                    Vec::new()
                };
                DragFinish(drop);
                if !paths.is_empty() {
                    let sender = &*(data as *const SyncSender<Vec<PathBuf>>);
                    let _ = sender.try_send(paths);
                }
            }
            return LRESULT(0);
        }
        if message == WM_NCDESTROY && RemoveWindowSubclass(hwnd, Some(window_proc), id).as_bool() {
            drop(Box::from_raw(data as *mut SyncSender<Vec<PathBuf>>));
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

fn valid_payload(memory: HGLOBAL, size: usize) -> bool {
    let header_size = std::mem::size_of::<DROPFILES>();
    if !(header_size..=MAX_DROP_BYTES).contains(&size) {
        return false;
    }
    let data = unsafe { GlobalLock(memory).cast::<u8>() };
    if data.is_null() {
        return false;
    }
    // The size comes from the same movable allocation that is locked here.
    let header = unsafe { data.cast::<DROPFILES>().read_unaligned() };
    let offset = header.pFiles as usize;
    let valid = if (header_size..size).contains(&offset) {
        let names = unsafe { std::slice::from_raw_parts(data.add(offset), size - offset) };
        if header.fWide.as_bool() {
            offset.is_multiple_of(2) && names.windows(4).step_by(2).any(|end| end == [0; 4])
        } else {
            names.windows(2).any(|pair| pair == [0, 0])
        }
    } else {
        false
    };
    let _ = unsafe { GlobalUnlock(memory) };
    valid
}

fn read_paths(drop: HDROP) -> Vec<PathBuf> {
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    (0..count.min(MAX_DROP_FILES))
        .filter_map(|index| {
            let length = unsafe { DragQueryFileW(drop, index, None) };
            if length == 0 || length > MAX_PATH_UNITS {
                return None;
            }
            let mut buffer = vec![0; length as usize + 1];
            let copied = unsafe { DragQueryFileW(drop, index, Some(&mut buffer)) };
            if copied != length {
                return None;
            }
            let path = PathBuf::from(OsString::from_wide(&buffer[..copied as usize]));
            is_local_path(&path).then_some(path)
        })
        .collect()
}

fn is_local_path(path: &Path) -> bool {
    // Check before the import does any file I/O, including metadata reads.
    if !path.is_absolute() {
        return false;
    }
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return false;
    };
    let Prefix::Disk(letter) = prefix.kind() else {
        return false;
    };
    let root = [u16::from(letter), u16::from(b':'), u16::from(b'\\'), 0];
    matches!(
        unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) },
        DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_RAMDISK
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW, WINDOW_EX_STYLE, WS_POPUP,
    };
    use windows::core::w;

    #[test]
    fn accepts_a_native_drop_with_multiple_unicode_paths() {
        // A genuine shell-format message through the registered window subclass.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Lumen file drop test"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        let receiver = install(hwnd).unwrap();
        let drive = std::env::temp_dir();
        let files = [drive.join("сервер #1.JSON"), drive.join("second.ovpn")];
        let mut filenames = Vec::new();
        for path in &files {
            filenames.extend(path.as_os_str().encode_wide());
            filenames.push(0);
        }
        filenames.push(0);
        let header_size = std::mem::size_of::<DROPFILES>();
        let size = header_size + std::mem::size_of_val(filenames.as_slice());
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, size).unwrap();
            let data = GlobalLock(memory).cast::<u8>();
            assert!(!data.is_null());
            data.cast::<DROPFILES>().write(DROPFILES {
                pFiles: header_size as u32,
                fWide: true.into(),
                ..Default::default()
            });
            std::ptr::copy_nonoverlapping(
                filenames.as_ptr(),
                data.add(header_size).cast(),
                filenames.len(),
            );
            let _ = GlobalUnlock(memory);
            SendMessageW(hwnd, WM_DROPFILES, Some(WPARAM(memory.0 as usize)), None);
            DestroyWindow(hwnd).unwrap();
        }
        assert_eq!(receiver.try_recv().unwrap(), files);
    }
}
