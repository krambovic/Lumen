// Fixture sing-box replacement compiled per-test run (see fake_xray.rs).
// Supports: version, check -c <path> -D <dir>, run -c <path> -D <dir>.
use std::fs;
use std::io::Write;

fn assert_cwd_is_exe_dir() {
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        exe_dir.canonicalize().unwrap()
    );
}

fn extract_port(config: &str) -> Option<u16> {
    config.lines().find_map(|line| {
        line.trim()
            .strip_prefix(r#""listen_port": "#)?
            .trim_end_matches(',')
            .parse()
            .ok()
    })
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let command = args.next().unwrap().to_string_lossy().into_owned();
    if command == "version" {
        let marker = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("lumen_marker");
        if marker.exists() {
            println!("sing-box fixture 1.13.14-extended-2.5.1-lumen.1");
        } else {
            println!("sing-box fixture 1.13.14-extended-2.5.1");
        }
        return;
    }
    if command == "check" {
        assert_eq!(args.next().unwrap().to_string_lossy(), "-c");
        let path = std::path::PathBuf::from(args.next().unwrap());
        assert_eq!(args.next().unwrap().to_string_lossy(), "-D");
        assert_cwd_is_exe_dir();
        let config = fs::read_to_string(&path).unwrap();
        if config.contains(r#""reject": true"#) {
            eprintln!("FATAL start service: initialize outbound[proxy] failed: rejected");
            std::process::exit(3);
        }
        println!("Configuration OK");
        return;
    }
    if command == "run" {
        assert_eq!(args.next().unwrap().to_string_lossy(), "-c");
        let path = std::path::PathBuf::from(args.next().unwrap());
        assert_eq!(args.next().unwrap().to_string_lossy(), "-D");
        assert_cwd_is_exe_dir();
        let config = fs::read_to_string(&path).unwrap();
        if config.contains(r#""exit": 1"#) {
            std::process::exit(1);
        }
        if let Some(port) = extract_port(&config) {
            let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
            let _ = listener;
            loop {
                std::thread::park();
            }
        }
        loop {
            std::thread::park();
        }
    }
    panic!("unexpected fixture command: {command}");
}
