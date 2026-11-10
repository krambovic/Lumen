use std::fs;
use std::io::{Cursor, Write};
#[cfg(windows)]
use std::path::Path;

use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use zip::write::SimpleFileOptions;

use super::*;

#[test]
fn selects_stable_beta_and_nightly_and_compares_versions() {
    let releases = json!([
        {"tag_name":"v99.0.0", "draft":true},
        {"tag_name":"v26.8.0-dev", "prerelease":true},
        {"tag_name":"v26.7.11-rc.2", "prerelease":true},
        {"tag_name":"v26.3.27"},
    ]);
    for (channel, expected) in [
        ("stable", "v26.3.27"),
        ("beta", "v26.7.11-rc.2"),
        ("nightly", "v26.8.0-dev"),
    ] {
        assert_eq!(
            release::pick_release(releases.as_array().unwrap(), channel).unwrap()["tag_name"],
            expected
        );
    }
    assert!(release::is_newer("26.7.11-rc.10", "26.7.11-rc.2"));
    assert!(release::is_newer("26.7.11", "26.7.11-rc.10"));
    assert!(!release::is_newer("26.3.27", "26.7.11"));
    assert_eq!(
        release::extract_version("Xray 26.7.11-rc.2+build"),
        "26.7.11-rc.2"
    );
}

fn zip_bytes(files: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(*name, SimpleFileOptions::default())?;
        zip.write_all(data)?;
    }
    Ok(zip.finish()?.into_inner())
}

#[test]
fn installs_only_xray_and_wintun_and_backs_up_the_previous_executable() -> Result<()> {
    let payload = zip_bytes(&[
        ("nested/xray.exe", b"new xray"),
        ("nested/wintun.dll", b"new wintun"),
        ("geoip.dat", b"upstream geodata"),
    ])?;
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("custom.exe");
    let dll = dir.path().join("wintun.dll");
    let geoip = dir.path().join("geoip.dat");
    fs::write(&target, b"old xray")?;
    fs::write(&dll, b"old wintun")?;
    fs::write(&geoip, b"lumen geodata")?;
    let archive = archive::ExtractedArchive::open(Cursor::new(payload))?;
    archive.install(&target)?;

    assert_eq!(fs::read(&target)?, b"new xray");
    assert_eq!(fs::read(target.with_extension("exe.bak"))?, b"old xray");
    assert_eq!(fs::read(dll)?, b"new wintun");
    assert_eq!(fs::read(geoip)?, b"lumen geodata");
    Ok(())
}

#[test]
fn rolls_back_xray_if_the_wintun_replacement_fails() -> Result<()> {
    let payload = zip_bytes(&[("xray.exe", b"new xray"), ("wintun.dll", b"new wintun")])?;
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("xray.exe");
    fs::write(&target, b"old xray")?;
    let archive = archive::ExtractedArchive::open(Cursor::new(payload))?;
    let stage = archive.stage(dir.path(), &target)?;
    // A directory at the second destination makes the second rename fail
    // after the first file was already replaced.
    fs::create_dir(dir.path().join("wintun.dll"))?;
    assert!(stage.commit().is_err());
    assert_eq!(fs::read(&target)?, b"old xray");
    assert!(!target.with_extension("exe.bak").exists());
    Ok(())
}

const TEST_RELEASES: &str = "http://updates.test/releases";

async fn request_path(stream: &mut TcpStream) -> Result<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let url = line.split_whitespace().nth(1).expect("HTTP request target");
    let path = reqwest::Url::parse(url)
        .map(|url| url.path().to_owned())
        .unwrap_or_else(|_| url.into());
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 || line == "\r\n" {
            return Ok(path);
        }
    }
}

async fn respond(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .as_bytes(),
        )
        .await?;
    stream.write_all(bytes).await?;
    Ok(())
}

async fn serve_release(zip: Vec<u8>, digest: String) -> Result<(String, JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let proxy = format!("http://{}", listener.local_addr()?);
    let metadata = json!([{"tag_name":"v26.7.11", "assets": [
        {"name": ARCHIVE_NAME, "browser_download_url":"http://updates.test/archive"},
        {"name": "Xray-windows-64.zip.dgst", "browser_download_url":"http://updates.test/digest"}
    ]}])
    .to_string();
    let digest = format!("SHA2-256(Xray-windows-64.zip)= {digest}");
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let path = request_path(&mut stream).await?;
            let bytes = match path.as_str() {
                "/releases" => metadata.as_bytes(),
                "/digest" => digest.as_bytes(),
                "/archive" => zip.as_slice(),
                _ => bail!("unexpected request path {path}"),
            };
            respond(&mut stream, bytes).await?;
        }
    });
    Ok((proxy, task))
}

#[cfg(windows)]
fn version_fixture(directory: &Path) -> Result<Vec<u8>> {
    let source = directory.join("fixture.rs");
    let exe = directory.join("fixture.exe");
    fs::write(
        &source,
        "fn main() { println!(\"Xray 26.7.11 test build\"); }",
    )?;
    let result = std::process::Command::new(option_env!("RUSTC").unwrap_or("rustc"))
        .arg(source)
        .arg("-o")
        .arg(&exe)
        .output()?;
    ensure!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(fs::read(exe)?)
}

#[cfg(windows)]
#[tokio::test]
async fn worker_validates_and_waits_for_disconnect_before_installing() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("xray.exe");
    let executable = version_fixture(dir.path())?;
    let zip = zip_bytes(&[
        ("nested/xray.exe", &executable),
        ("nested/wintun.dll", b"new DLL"),
        ("geosite.dat", b"upstream geodata"),
    ])?;
    let digest = format!("{:x}", Sha256::digest(&zip));
    let (proxy, server) = serve_release(zip, digest).await?;
    fs::write(&target, b"old xray")?;
    fs::write(dir.path().join("geosite.dat"), b"lumen geodata")?;
    let options = XrayCoreUpdateOptions {
        xray_path: target.to_string_lossy().into_owned(),
        channel: "stable".into(),
        apply_update: true,
        proxy_url: Some(proxy),
        ..XrayCoreUpdateOptions::default()
    };

    for accept_disconnect in [false, true] {
        let mut worker = XrayCoreUpdateWorker::start_with_source(options.clone(), TEST_RELEASES);
        let (result, progress) = tokio::time::timeout(Duration::from_secs(10), async {
            let mut progress = Vec::new();
            while let Some(event) = worker.events.recv().await {
                match event {
                    XrayCoreUpdateEvent::Progress(percent) => progress.push(percent),
                    XrayCoreUpdateEvent::RequestDisconnect(reply) => {
                        assert_eq!(fs::read(&target)?, b"old xray");
                        reply.send(accept_disconnect).unwrap();
                    }
                    XrayCoreUpdateEvent::Done(result) => return Ok((result, progress)),
                }
            }
            bail!("update worker closed before reporting a result")
        })
        .await??;
        worker.join().await?;
        assert_eq!(progress.last(), Some(&100));
        if accept_disconnect {
            assert_eq!(result.status, XrayCoreUpdateStatus::Updated, "{result:?}");
            assert_eq!(result.latest_version, "26.7.11");
        } else {
            assert_eq!(result.status, XrayCoreUpdateStatus::Error, "{result:?}");
            assert!(result.message.starts_with("Ошибка остановки служб"));
            assert_eq!(fs::read(&target)?, b"old xray");
        }
    }
    assert_eq!(fs::read(&target)?, executable);
    assert_eq!(fs::read(target.with_extension("exe.bak"))?, b"old xray");
    assert_eq!(fs::read(dir.path().join("geosite.dat"))?, b"lumen geodata");
    assert_eq!(fs::read(dir.path().join("wintun.dll"))?, b"new DLL");
    server.abort();
    Ok(())
}

#[tokio::test]
async fn rejects_a_bad_digest_before_disconnect_or_install() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("xray.exe");
    let (proxy, server) =
        serve_release(b"not the expected archive".to_vec(), "a".repeat(64)).await?;
    let options = XrayCoreUpdateOptions {
        xray_path: target.to_string_lossy().into_owned(),
        apply_update: true,
        proxy_url: Some(proxy),
        ..XrayCoreUpdateOptions::default()
    };
    let result = check_and_update(
        &options,
        TEST_RELEASES,
        |_, _| {},
        || async { panic!("must verify the digest first") },
        &UpdateCancellation::default(),
    )
    .await?;
    assert_eq!(result.status, XrayCoreUpdateStatus::Error);
    assert_eq!(result.message, "Контрольная сумма архива не совпадает");
    assert!(!target.exists());
    server.abort();
    Ok(())
}

#[tokio::test]
async fn cancellation_interrupts_a_stalled_archive_download() -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let proxy = format!("http://{}", listener.local_addr()?);
    let (started, ready) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        assert_eq!(request_path(&mut stream).await?, "/archive");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\ninitial bytes")
            .await?;
        started.send(()).unwrap();
        std::future::pending::<Result<()>>().await
    });
    let dir = tempfile::tempdir()?;
    let destination = dir.path().join("xray.zip");
    let cancellation = UpdateCancellation::default();
    let download = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            download_file(
                "http://updates.test/archive",
                &destination,
                Some(&proxy),
                |_, _| {},
                &cancellation,
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(3), ready).await??;
    cancellation.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), download)
        .await??
        .unwrap_err();
    assert!(error.is::<UpdateCancelled>());
    server.abort();
    Ok(())
}
