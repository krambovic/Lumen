//! Primary user locale via GetUserDefaultLocaleName.

use windows::Win32::Globalization::GetUserDefaultLocaleName;

pub fn system_language() -> String {
    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(len - 1) as usize]).to_lowercase()
}
