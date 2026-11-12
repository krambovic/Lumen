use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::app_update_check::should_auto_install;
use tokio::sync::mpsc;

fn update(download_url: &str) -> AppUpdate {
    AppUpdate {
        version: "2.0.0".into(),
        tag: "v2.0.0".into(),
        download_url: download_url.into(),
        size: 1,
        notes: String::new(),
        digest_sha256: String::new(),
        asset_name: "setup.exe".into(),
        channel: "stable".into(),
        is_downgrade: false,
    }
}

fn drain(receiver: &mut mpsc::UnboundedReceiver<AppUpdateDownloadEvent>) -> Vec<AppUpdateDownloadEvent> {
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    events
}

struct TestServer {
    port: u16,
    shutdown: std_mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<Vec<std::thread::JoinHandle<()>>>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(handle) = self.handle.take() {
            if let Ok(connections) = handle.join() {
                for connection in connections {
                    let _ = connection.join();
                }
            }
        }
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

fn spawn_test_server(handler: impl Fn(&str, TcpStream) + Send + Sync + 'static) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("local addr").port();
    let (shutdown_tx, shutdown_rx) = std_mpsc::channel::<()>();
    let handler = Arc::new(handler);
    let handle = std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let mut connection_threads = Vec::new();
        loop {
            if shutdown_rx.try_recv().is_ok() {
                return connection_threads;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Handle every connection on its own thread: a stalled or
                    // abandoned body write must not block later connections.
                    stream.set_nonblocking(false).expect("blocking fixture connection");
                    let handler = Arc::clone(&handler);
                    connection_threads.push(std::thread::spawn(move || {
                        let request = read_request(&mut stream);
                        handler(&request, stream);
                    }));
                }
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return connection_threads,
            }
        }
    });
    TestServer {
        port,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

fn segment_payload() -> Vec<u8> {
    let total = NUM_SEGMENTS * CHUNK_SIZE;
    (0..total).map(|index| (index % 251) as u8).collect()
}

// ── module helpers ──────────────────────────────────────────────────────────

#[test]
fn auto_install_requires_permission_and_never_downgrades() {
    let update = update("https://example.test/setup.exe");
    assert!(should_auto_install(Some(&update), true));
    assert!(!should_auto_install(Some(&update), false));
    let mut downgrade = update.clone();
    downgrade.is_downgrade = true;
    assert!(!should_auto_install(Some(&downgrade), true));
    assert!(!should_auto_install(None, true));
}

#[test]
fn update_interpreter_is_launched_from_an_absolute_system_path() {
    let path = powershell_path();
    #[cfg(windows)]
    {
        let system_root = std::env::var("SystemRoot")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| r"C:\Windows".to_string());
        let expected = PathBuf::from(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        assert_eq!(PathBuf::from(&path), expected);
        assert!(Path::new(&path).is_absolute());
    }
    #[cfg(not(windows))]
    assert_eq!(path, "powershell");
}

#[test]
fn setup_asset_name_cannot_escape_the_download_directory() {
    for candidate in [
        "..\\..\\evil.exe",
        "../../evil.exe",
        "C:\\Windows\\System32\\evil.exe",
        "",
        "..",
        "Lumen Setup.exe",
    ] {
        let name = safe_asset_name(candidate, "Lumen-Setup-windows-x64.exe");
        assert_eq!(
            Path::new("/staging").join(name).parent(),
            Some(Path::new("/staging"))
        );
        assert!(!name.is_empty());
        assert_ne!(name, "..");
    }
    assert_eq!(
        safe_asset_name("Lumen-Setup-windows-x64.exe", "default.exe"),
        "Lumen-Setup-windows-x64.exe"
    );
}

#[test]
fn powershell_literal_escapes_single_quotes() {
    assert_eq!(
        powershell_literal(r"C:\Temp\Lumen's Setup"),
        r"'C:\Temp\Lumen''s Setup'"
    );
    assert_eq!(powershell_literal("plain"), "'plain'");
}

#[test]
fn bom_text_writer_prefixes_the_byte_order_mark() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("script.ps1");
    write_utf8_bom_text(&path, "готово")?;
    let raw = std::fs::read(&path)?;
    assert!(raw.starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(String::from_utf8(raw[3..].to_vec())?, "готово");
    Ok(())
}

#[test]
fn digest_extraction_keeps_only_a_full_64_char_hash() {
    let hash = "a".repeat(64);
    assert_eq!(extract_digest(&format!("sha256: {hash}")), hash);
    assert_eq!(extract_digest(&format!("SHA256:{}", hash.to_uppercase())), hash);
    assert_eq!(extract_digest(&format!("{hash}abcd")), "");
    assert_eq!(extract_digest(""), "");
}

#[test]
fn sha256_file_matches_known_digests() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("payload.bin");
    std::fs::write(&path, b"")?;
    assert_eq!(
        sha256_file(&path)?,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    std::fs::write(&path, b"installer-payload")?;
    let digest = Sha256::digest(b"installer-payload");
    assert_eq!(sha256_file(&path)?, format!("{digest:x}"));
    Ok(())
}

#[test]
fn portable_marker_detection() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(!is_portable_at(directory.path()));
    std::fs::write(directory.path().join("portable.txt"), b"")?;
    assert!(is_portable_at(directory.path()));
    std::fs::remove_file(directory.path().join("portable.txt"))?;
    std::fs::write(directory.path().join("portable"), b"")?;
    assert!(is_portable_at(directory.path()));
    Ok(())
}

#[test]
fn target_dir_rules() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;

    // A portable build inside a legacy-named directory moves next to it while
    // the renamed target is free.
    let legacy = root.path().join("Lumen KVN");
    std::fs::create_dir(&legacy)?;
    assert_eq!(
        target_app_dir_in(&legacy, true),
        root.path().join("Lumen")
    );
    // Once the renamed directory exists, the portable build stays in place.
    std::fs::create_dir(root.path().join("Lumen"))?;
    assert_eq!(target_app_dir_in(&legacy, true), legacy);

    // Non-legacy portable directories stay as they are.
    let normal = root.path().join("Tools");
    std::fs::create_dir(&normal)?;
    assert_eq!(target_app_dir_in(&normal, true), normal);

    // An installed build resolved to the bare Program root moves to the
    // Program Files application directory.
    assert_eq!(
        target_app_dir_in(Path::new(r"C:\Program"), false),
        program_files_app_dir()
    );

    assert!(is_legacy_app_dir(Path::new(r"C:\Apps\lumen-kvn")));
    assert!(is_legacy_app_dir(Path::new(r"C:\Apps\Lumen_KVN")));
    assert!(!is_legacy_app_dir(Path::new(r"C:\Apps\Lumen")));
    assert!(is_root_program_dir(Path::new(r"C:\Program")));
    assert!(!is_root_program_dir(Path::new(r"C:\Program Files")));
    assert_eq!(
        renamed_app_dir(Path::new(r"C:\Apps\Lumen KVN")),
        PathBuf::from(r"C:\Apps\Lumen")
    );
    Ok(())
}

// ── script building ─────────────────────────────────────────────────────────

fn script_context(portable: bool) -> UpdateScriptContext {
    UpdateScriptContext {
        pid: 4242,
        setup_path: PathBuf::from(r"C:\Temp\lumen_update_1\Lumen-Setup-windows-x64.exe"),
        current_app_dir: PathBuf::from(r"C:\Program Files\Lumen"),
        app_dir: PathBuf::from(r"C:\Program Files\Lumen"),
        temp_dir: PathBuf::from(r"C:\Temp\lumen_update_1"),
        expected_version: "2.2.0".into(),
        expected_hash: "a".repeat(64),
        portable,
    }
}

#[test]
fn installed_update_script_matches_the_python_layout() {
    let script = build_update_script(&script_context(false));
    assert!(script.starts_with("$ErrorActionPreference = 'Stop'\r\n$pidToWait = 4242"));
    assert!(script.contains(r"$setupPath = 'C:\Temp\lumen_update_1\Lumen-Setup-windows-x64.exe'"));
    assert!(script.contains("Get-FileHash -LiteralPath $setupPath -Algorithm SHA256"));
    assert!(script.contains("a".repeat(64).as_str()));
    assert!(script.contains("& $exePath '--version-file' $versionFile"));
    assert!(!script.contains("Start-Process -FilePath $exePath -ArgumentList '--version-file',$versionFile"));
    assert!(!script.contains("continuing because installer finished successfully"));
    assert!(script
        .contains(r#"$installDirArg = '/DIR="' + $appDir + '"'"#));
    assert!(script.contains("IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)"));
    assert!(script.contains("Remove-LumenDrivers $ownedRoots"));
    assert!(script.contains(r"foreach ($uk in $uninstallKeys)"));
    assert!(script.ends_with("\r\n"));
}

#[test]
fn portable_update_script_preserves_the_profile_and_refreshes_bundled_data() {
    let script = build_update_script(&script_context(true));
    assert!(script.contains(
        "if (-not (Test-Path -LiteralPath $fallbackExe)) { $fallbackExe = Join-Path $currentAppDir 'LumenKVN.exe' }"
    ));
    assert!(script.contains("Expand-Archive -LiteralPath $setupPath -DestinationPath $tempExtractDir -Force"));
    assert!(script.contains("-ne 'data' -and $name -ne 'portable.txt' -and $name -ne 'portable'"));
    assert!(script.contains("Copy-Item -LiteralPath $bundledDataDir -Destination $appDir -Recurse -Force -ErrorAction Stop"));
    assert!(script.contains("if ($_.Name -ine 'data') {"));
    assert!(!script.contains("$setupLog"));
    assert!(script.ends_with("\r\n"));
}

#[tokio::test]
async fn staged_script_reverifies_the_installer_hash() -> anyhow::Result<()> {
    let payload = b"installer-payload";
    let digest = format!("{:x}", Sha256::digest(payload));
    let directory = tempfile::tempdir()?;
    let setup_path = directory.path().join("Lumen-Setup-windows-x64.exe");
    std::fs::write(&setup_path, payload)?;
    let mut staged_update = update("https://example.test/Lumen-Setup-windows-x64.exe");
    staged_update.digest_sha256 = digest.clone();
    staged_update.asset_name = "Lumen-Setup-windows-x64.exe".into();

    let (events, mut receiver) = mpsc::unbounded_channel();
    let (script, expected_hash) =
        verify_and_stage_script(directory.path(), &setup_path, &staged_update, false, &events).await?;
    assert_eq!(expected_hash, digest);

    let raw = std::fs::read(&script)?;
    assert!(raw.starts_with(&[0xef, 0xbb, 0xbf]));
    let text = String::from_utf8(raw[3..].to_vec())?;
    assert!(text.contains("Get-FileHash -LiteralPath $setupPath -Algorithm SHA256"));
    assert!(text.contains(&digest));
    assert!(text.contains("& $exePath '--version-file' $versionFile"));
    assert!(!text.contains("continuing because installer finished successfully"));
    assert_eq!(
        drain(&mut receiver),
        vec![
            AppUpdateDownloadEvent::Status("Проверка установщика...".into()),
            AppUpdateDownloadEvent::Progress(100),
            AppUpdateDownloadEvent::Status("Подготовка установки...".into()),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn staged_script_rejects_a_checksum_mismatch() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let setup_path = directory.path().join("setup.exe");
    std::fs::write(&setup_path, b"installer-payload")?;
    let mut staged_update = update("https://example.test/setup.exe");
    staged_update.digest_sha256 = "b".repeat(64);

    let (events, mut receiver) = mpsc::unbounded_channel();
    let result =
        verify_and_stage_script(directory.path(), &setup_path, &staged_update, false, &events).await;
    assert!(result.is_err());
    assert_eq!(
        drain(&mut receiver),
        vec![
            AppUpdateDownloadEvent::Status("Проверка установщика...".into()),
            AppUpdateDownloadEvent::Error("Контрольная сумма установщика не совпадает".into()),
        ]
    );
    assert!(!directory.path().join("_update.ps1").exists());
    Ok(())
}

#[tokio::test]
async fn staged_script_requires_a_digest() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let setup_path = directory.path().join("setup.exe");
    std::fs::write(&setup_path, b"installer-payload")?;
    let staged_update = update("https://example.test/setup.exe");

    let (events, mut receiver) = mpsc::unbounded_channel();
    let result =
        verify_and_stage_script(directory.path(), &setup_path, &staged_update, false, &events).await;
    assert!(result.is_err());
    assert_eq!(
        drain(&mut receiver),
        vec![
            AppUpdateDownloadEvent::Status("Проверка установщика...".into()),
            AppUpdateDownloadEvent::Error("У релизного установщика отсутствует SHA-256".into()),
        ]
    );
    Ok(())
}

// ── downloads ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn download_to_path_streams_a_single_connection_file() -> anyhow::Result<()> {
    let payload = b"Lumen setup payload".to_vec();
    let expected = payload.clone();
    let payload_len = payload.len();
    let server = spawn_test_server(move |request, mut stream| {
        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {payload_len}\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(head.as_bytes());
        if !request.starts_with("HEAD") {
            let _ = stream.write_all(&payload);
        }
        let _ = stream.flush();
    });
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("setup.exe");
    let cancellation = AppUpdateCancellation::default();
    let (events, mut receiver) = mpsc::unbounded_channel();

    download_to_path(
        &format!("http://127.0.0.1:{}/setup.exe", server.port),
        &target,
        None,
        &cancellation,
        &events,
    )
    .await?;

    assert_eq!(std::fs::read(&target)?, expected);
    let mut progress = Vec::new();
    for event in drain(&mut receiver) {
        if let AppUpdateDownloadEvent::Progress(percent) = event {
            progress.push(percent);
        }
    }
    assert_eq!(progress.last(), Some(&100));
    Ok(())
}

#[tokio::test]
async fn segmented_download_reassembles_all_ranges() -> anyhow::Result<()> {
    let payload = segment_payload();
    let expected = payload.clone();
    let total = payload.len();
    let server = spawn_test_server(move |request, mut stream| {
        if request.starts_with("HEAD") {
            let head = format!(
                "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.flush();
            return;
        }
        let Some(range) = header_value(&request, "Range") else {
            return;
        };
        let (start, end) = range.trim_start_matches("bytes=").split_once('-').expect("range spec");
        let (start, end) = (
            start.parse::<usize>().expect("range start"),
            end.parse::<usize>().expect("range end"),
        );
        let slice = &payload[start..=end];
        let head = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            slice.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(slice);
        let _ = stream.flush();
    });
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("setup.exe");
    let cancellation = AppUpdateCancellation::default();
    let (events, mut receiver) = mpsc::unbounded_channel();

    download_to_path(
        &format!("http://127.0.0.1:{}/setup.exe", server.port),
        &target,
        None,
        &cancellation,
        &events,
    )
    .await?;

    assert_eq!(std::fs::read(&target)?, expected);
    let mut progress = Vec::new();
    for event in drain(&mut receiver) {
        if let AppUpdateDownloadEvent::Progress(percent) = event {
            progress.push(percent);
        }
    }
    assert_eq!(progress.last(), Some(&100));
    assert!(!directory.path().join("_segments").exists());
    Ok(())
}

#[tokio::test]
async fn segmented_download_falls_back_to_a_single_connection_when_ranges_are_ignored()
-> anyhow::Result<()> {
    // HEAD advertises a 4 MiB file so the client starts a segmented download;
    // GET then ignores Range and answers with the small real payload, like a
    // server that lies about range support.
    let payload = b"Lumen fallback payload".to_vec();
    let expected = payload.clone();
    let payload_len = payload.len();
    let advertised_total = NUM_SEGMENTS * CHUNK_SIZE;
    let server = spawn_test_server(move |request, mut stream| {
        if request.starts_with("HEAD") {
            let head = format!(
                "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nContent-Length: {advertised_total}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
        } else {
            // Ignore the Range header entirely and answer with the whole file.
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {payload_len}\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&payload);
        }
        let _ = stream.flush();
    });
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("setup.exe");
    let cancellation = AppUpdateCancellation::default();
    let (events, mut receiver) = mpsc::unbounded_channel();

    download_to_path(
        &format!("http://127.0.0.1:{}/setup.exe", server.port),
        &target,
        None,
        &cancellation,
        &events,
    )
    .await?;

    assert_eq!(std::fs::read(&target)?, expected);
    let events = drain(&mut receiver);
    assert!(events.contains(&AppUpdateDownloadEvent::Progress(0)));
    assert_eq!(events.last(), Some(&AppUpdateDownloadEvent::Progress(100)));
    Ok(())
}

// The worker task is spawned on the runtime while this test blocks on the
// std channel, so the runtime must have spare worker threads.
#[tokio::test(flavor = "multi_thread")]
async fn downloader_cancel_interrupts_blocking_response() {
    let (started_tx, started_rx) = std_mpsc::channel::<()>();
    let (release_tx, release_rx) = std_mpsc::channel::<()>();
    let release_rx = std::sync::Mutex::new(release_rx);
    let server = spawn_test_server(move |request, mut stream| {
        let head = "HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.flush();
        if request.starts_with("HEAD") {
            return;
        }
        let _ = started_tx.send(());
        // Hold the body back like the Python test handler does.
        let _ = release_rx
            .lock()
            .expect("release lock")
            .recv_timeout(Duration::from_secs(5));
    });
    let mut staged_update = update(&format!("http://127.0.0.1:{}/setup.exe", server.port));
    staged_update.digest_sha256 = "0".repeat(64);
    staged_update.asset_name = "setup.exe".into();

    let worker = AppUpdateDownloader::start(staged_update, None);
    assert!(started_rx.recv_timeout(Duration::from_secs(3)).is_ok());
    let started = std::time::Instant::now();
    worker.cancel();
    tokio::time::timeout(Duration::from_secs(3), worker.join())
        .await
        .expect("worker finished after cancel")
        .expect("worker joined");
    assert!(started.elapsed() < Duration::from_secs(1));
    let _ = release_tx.send(());
}

#[tokio::test]
async fn cancelled_flow_reports_cancellation_without_events() {
    let cancellation = AppUpdateCancellation::default();
    cancellation.cancel();
    let (events, mut receiver) = mpsc::unbounded_channel();
    let result = run_update_flow(
        &update("https://example.test/setup.exe"),
        None,
        &cancellation,
        &events,
    )
    .await;
    assert!(matches!(result, Err(UpdateOperationCancelled)));
    assert!(receiver.try_recv().is_err());
}

// ── launch ──────────────────────────────────────────────────────────────────

#[cfg(windows)]
#[test]
fn non_elevated_launch_spawns_a_hidden_powershell_child() -> anyhow::Result<()> {
    // Leak the directory: the detached PowerShell child reads the script
    // asynchronously, after this test has finished.
    let directory = Box::leak(Box::new(tempfile::tempdir()?));
    let script = directory.path().join("noop.ps1");
    std::fs::write(&script, "exit 0\r\n")?;
    assert!(launch_update_script(&script, false)?);
    Ok(())
}

const PORTABLE_REFERENCE: &str = concat!(
    "$ErrorActionPreference = 'Stop'\r\n",
    "$pidToWait = 4242\r\n",
    "$setupPath = 'C:\\Temp\\lumen_update_1\\Lumen-Setup-windows-x64.exe'\r\n",
    "$currentAppDir = 'C:\\Program Files\\Lumen'\r\n",
    "$appDir = 'C:\\Program Files\\Lumen'\r\n",
    "$exePath = 'C:\\Program Files\\Lumen\\Lumen.exe'\r\n",
    "$fallbackExe = Join-Path $currentAppDir 'Lumen.exe'\r\n",
    "if (-not (Test-Path -LiteralPath $fallbackExe)) { $fallbackExe = Join-Path $currentAppDir 'LumenKVN.exe' }\r\n",
    "$tempDir = 'C:\\Temp\\lumen_update_1'\r\n",
    "$expectedVersion = '2.2.0'\r\n",
    "$expectedHash = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'\r\n",
    "$logDir = Join-Path (Join-Path $appDir 'data') 'logs'\r\n",
    "$runtimeDir = Join-Path (Join-Path $appDir 'data') 'runtime'\r\n",
    "$errorLog = Join-Path $logDir 'update_error.log'\r\n",
    "for ($i = 0; $i -lt 120; $i++) {\r\n",
    "    if (-not (Get-Process -Id $pidToWait -ErrorAction SilentlyContinue)) { break }\r\n",
    "    Start-Sleep -Milliseconds 500\r\n",
    "}\r\n",
    "$proc = Get-Process -Id $pidToWait -ErrorAction SilentlyContinue\r\n",
    "if ($proc) { Stop-Process -Id $pidToWait -Force }\r\n",
    "$ownedRoots = @($appDir, $currentAppDir) | Where-Object { $_ } | ForEach-Object { $_.ToLower() }\r\n",
    "function Stop-LumenProcesses([string[]]$roots) {\r\n",
    "    $names = @('Lumen','LumenKVN','Lumen-qml','LumenKVN-qml','xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')\r\n",
    "    foreach ($candidate in (Get-Process -Name $names -ErrorAction SilentlyContinue)) {\r\n",
    "        if (-not $candidate.Path) { continue }\r\n",
    "        foreach ($root in $roots) {\r\n",
    "            if ($candidate.Path.StartsWith(($root.TrimEnd('\\') + '\\'), [StringComparison]::OrdinalIgnoreCase)) {\r\n",
    "                Stop-Process -InputObject $candidate -Force -ErrorAction SilentlyContinue\r\n",
    "                break\r\n",
    "            }\r\n",
    "        }\r\n",
    "    }\r\n",
    "}\r\n",
    "Stop-LumenProcesses $ownedRoots\r\n",
    "foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {\r\n",
    "    $svcKey = 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\' + $driverName\r\n",
    "    $svcImage = (Get-ItemProperty -Path $svcKey -Name ImagePath -ErrorAction SilentlyContinue).ImagePath\r\n",
    "    if (-not $svcImage) { continue }\r\n",
    "    $svcImage = $svcImage.ToLower()\r\n",
    "    $owned = $false\r\n",
    "    foreach ($root in $ownedRoots) { if ($svcImage.Contains($root)) { $owned = $true } }\r\n",
    "    if (-not $owned) { continue }\r\n",
    "    & sc.exe stop $driverName *> $null\r\n",
    "    & sc.exe delete $driverName *> $null\r\n",
    "}\r\n",
    "Start-Sleep -Milliseconds 1200\r\n",
    "try {\r\n",
    "    if ((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash -ine $expectedHash) { throw 'Update payload failed SHA-256 verification' }\r\n",
    "    $tempExtractDir = Join-Path $tempDir 'extract'\r\n",
    "    New-Item -ItemType Directory -Path $tempExtractDir -Force | Out-Null\r\n",
    "    Expand-Archive -LiteralPath $setupPath -DestinationPath $tempExtractDir -Force\r\n",
    "    $srcDir = $tempExtractDir\r\n",
    "    if (Test-Path -Path (Join-Path $tempExtractDir 'Lumen')) {\r\n",
    "        $srcDir = Join-Path $tempExtractDir 'Lumen'\r\n",
    "    } elseif (Test-Path -Path (Join-Path $tempExtractDir 'LumenKVN')) {\r\n",
    "        $srcDir = Join-Path $tempExtractDir 'LumenKVN'\r\n",
    "    }\r\n",
    "    $currentDataDir = Join-Path $currentAppDir 'data'\r\n",
    "    $targetDataDir = Join-Path $appDir 'data'\r\n",
    "    New-Item -ItemType Directory -Path $appDir -Force | Out-Null\r\n",
    "    if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir) -and -not (Test-Path -LiteralPath $targetDataDir)) {\r\n",
    "        Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force -ErrorAction Stop\r\n",
    "    }\r\n",
    "    $bundledDataDir = Join-Path $srcDir 'data'\r\n",
    "    if (Test-Path -LiteralPath $bundledDataDir -PathType Container) {\r\n",
    "        Copy-Item -LiteralPath $bundledDataDir -Destination $appDir -Recurse -Force -ErrorAction Stop\r\n",
    "    }\r\n",
    "    Get-ChildItem -Path $appDir -Force | ForEach-Object {\r\n",
    "        $name = $_.Name.ToLower()\r\n",
    "        if ($name -ne 'data' -and $name -ne 'portable.txt' -and $name -ne 'portable' -and $_.FullName -ne $tempDir) {\r\n",
    "            Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "        }\r\n",
    "    }\r\n",
    "    Get-ChildItem -Path $srcDir -Force | ForEach-Object {\r\n",
    "        if ($_.Name -ine 'data') {\r\n",
    "            Move-Item -LiteralPath $_.FullName -Destination $appDir -Force -ErrorAction Stop\r\n",
    "        }\r\n",
    "    }\r\n",
    "    if (-not (Test-Path -LiteralPath $exePath)) { throw 'Updated Lumen.exe was not found' }\r\n",
    "    $started = Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -PassThru -ErrorAction Stop\r\n",
    "    Start-Sleep -Seconds 5\r\n",
    "    if ($started.HasExited) {\r\n",
    "        throw ('Updated application exited immediately with code ' + $started.ExitCode)\r\n",
    "    }\r\n",
    "    $legacyPortableNames = @('Lumen KVN','LumenKVN','lumen-kvn','Lumen_KVN')\r\n",
    "    if ($currentAppDir -ine $appDir -and $legacyPortableNames -icontains (Split-Path -Leaf $currentAppDir)) {\r\n",
    "        Remove-Item -LiteralPath $currentAppDir -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "    }\r\n",
    "}\r\n",
    "catch {\r\n",
    "    if (Test-Path -LiteralPath $exePath) { Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -ErrorAction SilentlyContinue | Out-Null } elseif (Test-Path -LiteralPath $fallbackExe) { Start-Process -FilePath $fallbackExe -ArgumentList '--relaunched' -WorkingDirectory $currentAppDir -ErrorAction SilentlyContinue | Out-Null }\r\n",
    "    New-Item -ItemType Directory -Path $logDir -Force | Out-Null\r\n",
    "    ($_ | Out-String) | Set-Content -LiteralPath $errorLog -Encoding UTF8\r\n",
    "    throw\r\n",
    "}\r\n",
    "Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "",
);

const INSTALLED_REFERENCE: &str = concat!(
    "$ErrorActionPreference = 'Stop'\r\n",
    "$pidToWait = 4242\r\n",
    "$setupPath = 'C:\\Temp\\lumen_update_1\\Lumen-Setup-windows-x64.exe'\r\n",
    "$currentAppDir = 'C:\\Program Files\\Lumen'\r\n",
    "$appDir = 'C:\\Program Files\\Lumen'\r\n",
    "$exePath = 'C:\\Program Files\\Lumen\\Lumen.exe'\r\n",
    "$fallbackExe = Join-Path $currentAppDir 'Lumen.exe'\r\n",
    "$tempDir = 'C:\\Temp\\lumen_update_1'\r\n",
    "$expectedVersion = '2.2.0'\r\n",
    "$expectedHash = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'\r\n",
    "$logDir = Join-Path (Join-Path $appDir 'data') 'logs'\r\n",
    "$runtimeDir = Join-Path (Join-Path $appDir 'data') 'runtime'\r\n",
    "$errorLog = Join-Path $logDir 'update_error.log'\r\n",
    "$setupLog = Join-Path $logDir 'setup_update.log'\r\n",
    "if ((Split-Path -Leaf $appDir) -ieq 'Program') { throw 'Install directory resolved to C:\\Program; aborting update.' }\r\n",
    "New-Item -ItemType Directory -Path $appDir -Force | Out-Null\r\n",
    "New-Item -ItemType Directory -Path $logDir -Force | Out-Null\r\n",
    "New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null\r\n",
    "$adminIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()\r\n",
    "$adminPrincipal = New-Object Security.Principal.WindowsPrincipal($adminIdentity)\r\n",
    "if (-not $adminPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Lumen updater must run as administrator to replace zapret drivers.' }\r\n",
    "for ($i = 0; $i -lt 120; $i++) {\r\n",
    "    if (-not (Get-Process -Id $pidToWait -ErrorAction SilentlyContinue)) { break }\r\n",
    "    Start-Sleep -Milliseconds 500\r\n",
    "}\r\n",
    "$proc = Get-Process -Id $pidToWait -ErrorAction SilentlyContinue\r\n",
    "if ($proc) { Stop-Process -Id $pidToWait -Force }\r\n",
    "$ownedRoots = @($appDir, $currentAppDir) | Where-Object { $_ } | ForEach-Object { $_.ToLower() }\r\n",
    "function Stop-LumenProcesses([string[]]$roots) {\r\n",
    "    $names = @('Lumen','LumenKVN','Lumen-qml','LumenKVN-qml','xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')\r\n",
    "    foreach ($candidate in (Get-Process -Name $names -ErrorAction SilentlyContinue)) {\r\n",
    "        if (-not $candidate.Path) { continue }\r\n",
    "        foreach ($root in $roots) {\r\n",
    "            if ($candidate.Path.StartsWith(($root.TrimEnd('\\') + '\\'), [StringComparison]::OrdinalIgnoreCase)) {\r\n",
    "                Stop-Process -InputObject $candidate -Force -ErrorAction SilentlyContinue\r\n",
    "                break\r\n",
    "            }\r\n",
    "        }\r\n",
    "    }\r\n",
    "}\r\n",
    "Stop-LumenProcesses $ownedRoots\r\n",
    "function Remove-LumenDrivers($roots) {\r\n",
    "    foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {\r\n",
    "        $svcKey = 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\' + $driverName\r\n",
    "        $svcImage = (Get-ItemProperty -Path $svcKey -Name ImagePath -ErrorAction SilentlyContinue).ImagePath\r\n",
    "        if (-not $svcImage) { continue }\r\n",
    "        $svcImage = $svcImage.ToLower()\r\n",
    "        $owned = $false\r\n",
    "        foreach ($root in $roots) { if ($svcImage.Contains($root)) { $owned = $true } }\r\n",
    "        if (-not $owned) { continue }\r\n",
    "        & sc.exe stop $driverName *> $null\r\n",
    "        & sc.exe delete $driverName *> $null\r\n",
    "    }\r\n",
    "}\r\n",
    "Remove-LumenDrivers $ownedRoots\r\n",
    "$zapretExeDir = Join-Path (Join-Path $appDir 'zapret') 'exe'\r\n",
    "$currentDataDir = Join-Path $currentAppDir 'data'\r\n",
    "if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir)) {\r\n",
    "    Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force\r\n",
    "}\r\n",
    "foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {\r\n",
    "    Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue\r\n",
    "}\r\n",
    "Start-Sleep -Milliseconds 1200\r\n",
    "try {\r\n",
    "    if ((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash -ine $expectedHash) { throw 'Update payload failed SHA-256 verification' }\r\n",
    "    $installDirArg = '/DIR=\"' + $appDir + '\"'\r\n",
    "    $logArg = '/LOG=\"' + $setupLog + '\"'\r\n",
    "    $installerArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CLOSEAPPLICATIONS','/FORCECLOSEAPPLICATIONS','/RESTARTEXITCODE=3010',$installDirArg,$logArg)\r\n",
    "    $install = $null\r\n",
    "    for ($attempt = 1; $attempt -le 3; $attempt++) {\r\n",
    "        $install = Start-Process -FilePath $setupPath -ArgumentList $installerArgs -Wait -PassThru -ErrorAction Stop\r\n",
    "        if ($install.ExitCode -eq 0 -or $install.ExitCode -eq 3010) { break }\r\n",
    "        Stop-LumenProcesses $ownedRoots\r\n",
    "        Remove-LumenDrivers $ownedRoots\r\n",
    "        foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {\r\n",
    "            Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue\r\n",
    "        }\r\n",
    "        Start-Sleep -Seconds 2\r\n",
    "    }\r\n",
    "    if ($install.ExitCode -ne 0 -and $install.ExitCode -ne 3010) {\r\n",
    "        $setupTail = ''\r\n",
    "        if (Test-Path -LiteralPath $setupLog) { $setupTail = (Get-Content -LiteralPath $setupLog -Tail 30 -ErrorAction SilentlyContinue) -join [Environment]::NewLine }\r\n",
    "        throw ('Installer exited with code ' + $install.ExitCode + [Environment]::NewLine + 'Setup log tail:' + [Environment]::NewLine + $setupTail) }\r\n",
    "    $uninstallKeys = @('HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LumenKVN_is1','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LumenKVN_is1','HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LumenKVN_is1')\r\n",
    "    foreach ($uk in $uninstallKeys) { try { $loc = (Get-ItemProperty -LiteralPath $uk -ErrorAction Stop).InstallLocation; if ($loc -and (Test-Path -LiteralPath (Join-Path $loc 'Lumen.exe'))) { $appDir = $loc.TrimEnd('\\'); $exePath = Join-Path $appDir 'Lumen.exe'; break } elseif ($loc -and (Test-Path -LiteralPath (Join-Path $loc 'LumenKVN.exe'))) { $appDir = $loc.TrimEnd('\\'); $exePath = Join-Path $appDir 'Lumen.exe'; break } } catch {} }\r\n",
    "    $currentDataDir = Join-Path $currentAppDir 'data'\r\n",
    "    if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir)) {\r\n",
    "        Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force\r\n",
    "    }\r\n",
    "    if (-not (Test-Path -LiteralPath $exePath)) { throw 'Updated Lumen.exe was not installed' }\r\n",
    "    $versionFile = Join-Path $runtimeDir 'update_version.txt'\r\n",
    "    Remove-Item -LiteralPath $versionFile -Force -ErrorAction SilentlyContinue\r\n",
    "    & $exePath '--version-file' $versionFile\r\n",
    "    $versionProbeExitCode = $LASTEXITCODE\r\n",
    "    if ($versionProbeExitCode -ne 0) { throw ('Version probe exited with code ' + $versionProbeExitCode) }\r\n",
    "    $versionLine = Get-Content -LiteralPath $versionFile -ErrorAction SilentlyContinue | Select-Object -First 1\r\n",
    "    if ($null -eq $versionLine -or [string]::IsNullOrWhiteSpace([string]$versionLine)) { throw 'Updated executable did not write version probe file' }\r\n",
    "    $installedVersion = ([string]$versionLine).Trim().TrimStart('v')\r\n",
    "    if ($expectedVersion -and $installedVersion -ne $expectedVersion.TrimStart('v')) { throw ('Updated executable reports v' + $installedVersion + ', expected v' + $expectedVersion) }\r\n",
    "    $oldExePath = Join-Path $currentAppDir 'Lumen.exe'\r\n",
    "    $oldLegacyExePath = Join-Path $currentAppDir 'LumenKVN.exe'\r\n",
    "    $currentRoot = [System.IO.Path]::GetPathRoot($currentAppDir)\r\n",
    "    $currentParent = Split-Path -Parent $currentAppDir\r\n",
    "    $isRootProgramDir = ((Split-Path -Leaf $currentAppDir) -ieq 'Program' -and $currentParent.TrimEnd('\\') -ieq $currentRoot.TrimEnd('\\'))\r\n",
    "    $legacyDirNames = @('Lumen KVN','LumenKVN','lumen-kvn','Lumen_KVN')\r\n",
    "    $isLegacyNamedDir = $legacyDirNames -icontains (Split-Path -Leaf $currentAppDir)\r\n",
    "    $hasLegacyApp = (Test-Path -LiteralPath $oldExePath) -or (Test-Path -LiteralPath $oldLegacyExePath)\r\n",
    "    if ($currentAppDir -ine $appDir -and ($isRootProgramDir -or $isLegacyNamedDir) -and $hasLegacyApp) {\r\n",
    "        Remove-Item -LiteralPath $currentAppDir -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "    }\r\n",
    "    $legacyRootProgram = 'C:\\Program'\r\n",
    "    $legacyRootExe = Join-Path $legacyRootProgram 'Lumen.exe'\r\n",
    "    $legacyRootOldExe = Join-Path $legacyRootProgram 'LumenKVN.exe'\r\n",
    "    if ($legacyRootProgram -ine $appDir -and ((Test-Path -LiteralPath $legacyRootExe) -or (Test-Path -LiteralPath $legacyRootOldExe))) {\r\n",
    "        Remove-Item -LiteralPath $legacyRootProgram -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "    }\r\n",
    "    $started = Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -PassThru -ErrorAction Stop\r\n",
    "    Start-Sleep -Seconds 5\r\n",
    "    if ($started.HasExited) {\r\n",
    "        throw ('Updated application exited immediately with code ' + $started.ExitCode)\r\n",
    "    }\r\n",
    "}\r\n",
    "catch {\r\n",
    "    if (Test-Path -LiteralPath $exePath) { Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -ErrorAction SilentlyContinue | Out-Null } elseif (Test-Path -LiteralPath $fallbackExe) { Start-Process -FilePath $fallbackExe -ArgumentList '--relaunched' -WorkingDirectory $currentAppDir -ErrorAction SilentlyContinue | Out-Null }\r\n",
    "    New-Item -ItemType Directory -Path $logDir -Force | Out-Null\r\n",
    "    ($_ | Out-String) | Set-Content -LiteralPath $errorLog -Encoding UTF8\r\n",
    "    throw\r\n",
    "}\r\n",
    "Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue\r\n",
    "",
);

fn reference_context(portable: bool) -> UpdateScriptContext {
    UpdateScriptContext {
        pid: 4242,
        setup_path: PathBuf::from(r"C:\Temp\lumen_update_1\Lumen-Setup-windows-x64.exe"),
        current_app_dir: PathBuf::from(r"C:\Program Files\Lumen"),
        app_dir: PathBuf::from(r"C:\Program Files\Lumen"),
        temp_dir: PathBuf::from(r"C:\Temp\lumen_update_1"),
        expected_version: "2.2.0".into(),
        expected_hash: "a".repeat(64),
        portable,
    }
}

/// Keep the generated Windows scripts byte-identical to the fixtures
/// (CRLF-joined line list with these exact context values).
#[test]
fn update_scripts_match_the_windows_fixtures() {
    assert_eq!(build_update_script(&reference_context(true)), PORTABLE_REFERENCE);
    assert_eq!(
        build_update_script(&reference_context(false)),
        INSTALLED_REFERENCE
    );
}
