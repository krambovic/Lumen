#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--startup-probe-file")) {
        let success = args.next().is_some_and(|path| {
            std::fs::write(path, lumen_core::constants::APP_VERSION).is_ok()
        });
        std::process::exit(i32::from(!success));
    }
    std::process::exit(lumen_ui::run());
}
