use std::fs;
use std::net::TcpListener;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let command = args.next().unwrap();
    if command == "version" {
        println!("Xray fixture 1.0\nbuild details");
        return;
    }
    assert_eq!(command, "run");
    let option = args.next().unwrap();
    let validation = option == "-test";
    if validation {
        assert_eq!(args.next().unwrap(), "-c");
    } else {
        assert_eq!(option, "-c");
    }
    let path = std::path::PathBuf::from(args.next().expect("config path"));
    let config = fs::read_to_string(&path).expect("read config");
    let exe = std::env::current_exe().unwrap();
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        exe.parent().unwrap().canonicalize().unwrap()
    );
    if validation {
        if config.contains(r#""reject": true"#) {
            eprintln!("validation rejected");
            std::process::exit(3);
        }
        println!("Configuration OK");
        return;
    }
    if config.contains(r#""crash": true"#) {
        eprintln!("panic: runtime error: invalid memory address");
        eprintln!("github.com/xtls/xray-core/app/dns.FakeDnsPostProcessingStage");
        std::process::exit(23);
    }
    if config.contains(r#""delay_first": true"#)
        && fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.with_extension("attempted"))
            .is_ok()
    {
        loop {
            std::thread::park();
        }
    }
    let port: u16 = config
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(r#""port": "#)?
                .trim_end_matches(',')
                .parse()
                .ok()
        })
        .unwrap();
    eprintln!("[Warning] fixture stderr");
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    for stream in listener.incoming() {
        drop(stream.unwrap());
    }
}
