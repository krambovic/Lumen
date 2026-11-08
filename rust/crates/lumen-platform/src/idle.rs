//! User idle seconds via GetLastInputInfo (auto-lock).

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

pub fn idle_seconds() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        let tick = GetTickCount();
        let elapsed = tick.saturating_sub(info.dwTime);
        elapsed / 1000
    }
}
