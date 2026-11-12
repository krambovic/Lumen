#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::{ffi::OsStr, path::Path};

use lumen_core::constants::APP_VERSION;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    for flag in ["--version-file", "--startup-probe-file"] {
        if let Some(index) = args.iter().position(|arg| arg == OsStr::new(flag)) {
            let success = args.get(index + 1).map(Path::new).is_some_and(|path| {
                if flag == "--version-file"
                    && (!path.is_absolute() || path.parent().and_then(Path::parent).is_none())
                {
                    return false;
                }
                std::fs::write(path, APP_VERSION).is_ok()
            });
            std::process::exit(if success { 0 } else { 2 });
        }
    }
    if args.iter().any(|arg| arg == OsStr::new("--version")) {
        println!("{APP_VERSION}");
        return;
    }
    std::process::exit(lumen_ui::run());
}
