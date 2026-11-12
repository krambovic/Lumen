//! Self-update download and install flow: fetch the release asset (segmented
//! when the server supports ranges), verify its SHA-256, then stage and launch
//! the elevated PowerShell install/restart script.

mod script;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use lumen_core::constants::base_dir;
use lumen_core::http_utils::{redirect_blocked, user_agent, WindowsDirectRoute};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinError, JoinHandle};
use url::Url;

use crate::app_update_check::AppUpdate;

pub use script::{build_update_script, UpdateScriptContext};

pub const GITHUB_REPO: &str = "krambovic/Lumen";
/// Inno Setup uninstall key of the current installer package.
pub const APP_ID: &str = "{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1";
pub const LEGACY_APP_DIR_NAMES: [&str; 4] = ["lumen kvn", "lumenkvn", "lumen-kvn", "lumen_kvn"];
pub const DEFAULT_SETUP_NAME: &str = "Lumen-Setup-windows-x64.exe";
pub const DEFAULT_PORTABLE_NAME: &str = "Lumen-portable-windows-x64.zip";

const DOWNLOAD_TIMEOUT: f64 = 30.0; // seconds — per socket operation (connect + each read)
const NUM_SEGMENTS: usize = 4; // parallel download segments
const CHUNK_SIZE: usize = 1024 * 1024; // 1 MB
const REDIRECT_LIMIT: usize = 10;
const REDIRECT_ALLOWED_HEADERS: [&str; 3] = ["accept", "accept-encoding", "user-agent"];

/// `UpdateOperationCancelled(RuntimeError)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("update operation cancelled")]
pub struct UpdateOperationCancelled;

/// Failure that was already reported through the event stream (Python emits
/// the message and returns instead of raising).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("update staging failed")]
struct ReportedFailure;

/// Flow outcome: `Failed` means the user-facing error was already emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UpdateFlowError {
    #[error("update operation cancelled")]
    Cancelled,
    #[error("update flow failed")]
    Failed,
}

/// Cancellation flag shared between the UI thread and the download worker,
/// modelled on the xray core updater (`threading.Event` in Python).
#[derive(Debug, Clone)]
pub struct AppUpdateCancellation(watch::Sender<bool>);

impl Default for AppUpdateCancellation {
    fn default() -> Self {
        Self(watch::Sender::new(false))
    }
}

impl AppUpdateCancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn check(&self) -> Result<(), UpdateOperationCancelled> {
        if *self.0.borrow() {
            Err(UpdateOperationCancelled)
        } else {
            Ok(())
        }
    }

    async fn run<F: Future>(&self, future: F) -> std::result::Result<F::Output, UpdateOperationCancelled> {
        let mut cancelled = self.0.subscribe();
        tokio::select! {
            biased;
            _ = cancelled.wait_for(|cancelled| *cancelled) => Err(UpdateOperationCancelled),
            result = future => Ok(result),
        }
    }
}

/// Events replacing the PyQt6 signals of `UpdateDownloader`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppUpdateDownloadEvent {
    /// percent 0-100
    Progress(u32),
    /// human-readable status message
    Status(String),
    FinishedOk,
    Error(String),
}

// ── module helpers ──────────────────────────────────────────────────────────

pub fn default_asset_name(portable: bool) -> &'static str {
    if portable {
        DEFAULT_PORTABLE_NAME
    } else {
        DEFAULT_SETUP_NAME
    }
}

pub fn is_portable() -> bool {
    is_portable_at(base_dir())
}

pub fn is_portable_at(dir: &Path) -> bool {
    ["portable.txt", "portable"]
        .into_iter()
        .any(|name| dir.join(name).is_file())
}

pub fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn write_utf8_bom_text(path: &Path, text: &str) -> std::io::Result<()> {
    let mut payload = vec![0xef, 0xbb, 0xbf];
    payload.extend_from_slice(text.as_bytes());
    std::fs::write(path, payload)
}

/// `Path.resolve(strict=False)` without the `\\?\` verbatim prefix.
pub fn resolve_noverbatim(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(resolved) => {
            let text = resolved.to_string_lossy().into_owned();
            if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
                PathBuf::from(format!(r"\\{rest}"))
            } else if let Some(rest) = text.strip_prefix(r"\\?\") {
                PathBuf::from(rest)
            } else {
                resolved
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

pub fn program_files_app_dir() -> PathBuf {
    let app_name = "Lumen";
    let root = ["ProgramW6432", "ProgramFiles"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    match root {
        Some(root) => resolve_noverbatim(&PathBuf::from(root).join(app_name)),
        None => PathBuf::from(r"C:\Program Files").join(app_name),
    }
}

pub fn is_root_program_dir(path: &Path) -> bool {
    let resolved = resolve_noverbatim(path);
    let parent = match resolved.parent() {
        Some(parent) => parent,
        None => return false,
    };
    if !resolved
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("program"))
    {
        return false;
    }
    let mut components = resolved.components();
    let mut anchor = PathBuf::new();
    match components.next() {
        Some(std::path::Component::Prefix(prefix)) => anchor.push(prefix.as_os_str()),
        _ => return false,
    }
    if components.next() == Some(std::path::Component::RootDir) {
        anchor.push(std::path::MAIN_SEPARATOR_STR);
    }
    if anchor.as_os_str().is_empty() {
        return false;
    }
    parent == anchor.as_path()
}

pub fn is_legacy_app_dir(path: &Path) -> bool {
    let name = resolve_noverbatim(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    LEGACY_APP_DIR_NAMES.contains(&name.as_str())
}

pub fn renamed_app_dir(path: &Path) -> PathBuf {
    resolve_noverbatim(path).with_file_name("Lumen")
}

/// Install directory recorded by the Inno Setup uninstall entries.
#[cfg(windows)]
pub fn registered_install_dir() -> Option<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let subkeys = [
        format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID}"),
        format!(r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID}"),
    ];
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for subkey in &subkeys {
            let Ok(key) = winreg::RegKey::predef(root).open_subkey(subkey) else {
                continue;
            };
            let Ok(value) = key.get_value::<String, _>("InstallLocation") else {
                continue;
            };
            let text = value.trim().trim_matches('"').to_string();
            if !text.is_empty() {
                return Some(resolve_noverbatim(Path::new(&text)));
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn registered_install_dir() -> Option<PathBuf> {
    None
}

/// Where the update must land: portable builds stay (or move out of a legacy
/// name), installed builds follow the registry, else `C:\Program Files\Lumen`.
pub fn target_app_dir(current_app_dir: &Path) -> PathBuf {
    target_app_dir_in(current_app_dir, is_portable())
}

pub fn target_app_dir_in(current_app_dir: &Path, portable: bool) -> PathBuf {
    if portable {
        let renamed = renamed_app_dir(current_app_dir);
        if is_legacy_app_dir(current_app_dir) && !renamed.exists() {
            return renamed;
        }
        return current_app_dir.to_path_buf();
    }
    if is_root_program_dir(current_app_dir) {
        return program_files_app_dir();
    }
    if is_legacy_app_dir(current_app_dir) {
        return renamed_app_dir(current_app_dir);
    }
    if let Some(registered) = registered_install_dir() {
        if is_root_program_dir(&registered) {
            return program_files_app_dir();
        }
        if is_legacy_app_dir(&registered) {
            return renamed_app_dir(&registered);
        }
        return registered;
    }
    #[cfg(windows)]
    {
        program_files_app_dir()
    }
    #[cfg(not(windows))]
    {
        resolve_noverbatim(current_app_dir)
    }
}

/// Absolute interpreter path so PATH/current-directory cannot be hijacked.
pub fn powershell_path() -> String {
    #[cfg(windows)]
    {
        let system_root = std::env::var("SystemRoot")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| r"C:\Windows".to_string());
        PathBuf::from(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe")
            .to_string_lossy()
            .into_owned()
    }
    #[cfg(not(windows))]
    {
        "powershell".to_string()
    }
}

fn update_script_arguments(script: &Path) -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-WindowStyle".to_string(),
        "Hidden".to_string(),
        "-File".to_string(),
        script.to_string_lossy().into_owned(),
    ]
}

#[cfg(windows)]
fn shell_execute_elevated(file: &str, arguments: &str, directory: &str) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;

    let verb = windows::core::w!("runas");
    let file = HSTRING::from(file);
    let params = HSTRING::from(arguments);
    let dir = HSTRING::from(directory);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            // SW_HIDE, matching the Python call.
            SHOW_WINDOW_CMD(0),
        )
    };
    result.0 as isize > 32
}

fn spawn_hidden_powershell(script: &Path) -> std::io::Result<()> {
    let mut command = std::process::Command::new(powershell_path());
    command.args(update_script_arguments(script));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(lumen_platform::subprocess::CREATE_NO_WINDOW);
    }
    command.spawn().map(|_| ())
}

/// `_launch_update_script`: elevated launches go through ShellExecuteW
/// ("runas"), everything else spawns a hidden PowerShell child. `Ok(false)`
/// means the elevated launch was refused; spawn failures propagate like the
/// Python `Popen` exception.
pub fn launch_update_script(script: &Path, elevated: bool) -> Result<bool> {
    #[cfg(windows)]
    if elevated {
        let arguments = lumen_platform::startup::list2cmdline(&update_script_arguments(script));
        let directory = script
            .parent()
            .unwrap_or(Path::new("."))
            .to_string_lossy()
            .into_owned();
        // Any failure here reports false, like the Python try/except.
        return Ok(shell_execute_elevated(
            &powershell_path(),
            &arguments,
            &directory,
        ));
    }
    spawn_hidden_powershell(script)?;
    Ok(true)
}

/// `_extract_digest`: keep only hex characters, require a full 64-char digest.
pub fn extract_digest(value: &str) -> String {
    let text = value.trim().to_lowercase();
    let text = match text.strip_prefix("sha256:") {
        Some(rest) => rest.trim(),
        None => text.as_str(),
    };
    let parts: String = text
        .chars()
        .filter(|ch| matches!(ch, '0'..='9' | 'a'..='f'))
        .collect();
    if parts.len() == 64 {
        parts
    } else {
        String::new()
    }
}

/// `_sha256_file`: hash in 1 MiB chunks, no cancellation checks (Python).
pub fn sha256_file(file_path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(file_path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..read]);
    }
}

/// `_safe_asset_name`: reduce release metadata to a plain file name inside the
/// download dir.
pub fn safe_asset_name<'a>(name: &'a str, default: &'a str) -> &'a str {
    lumen_engines::xray::core_updater::safe_asset_name(name, default)
}

fn percent(value: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }
    let value = u128::from(value) * 100 / u128::from(total);
    value.min(u128::from(u32::MAX)) as u32
}

fn is_cancelled_error(error: &anyhow::Error, cancellation: &AppUpdateCancellation) -> bool {
    error.is::<UpdateOperationCancelled>() || cancellation.check().is_err()
}

// ── HTTP plumbing ───────────────────────────────────────────────────────────

struct DirectResolver(Arc<WindowsDirectRoute>);

impl reqwest::dns::Resolve for DirectResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let route = Arc::clone(&self.0);
        let host = name.as_str().to_string();
        Box::pin(async move {
            let bypass = route.bypass_required();
            let target = tokio::task::spawn_blocking(move || route.resolve(&host))
                .await
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?;
            let addrs: Vec<std::net::SocketAddr> = if bypass {
                vec![std::net::SocketAddr::new(
                    target
                        .parse()
                        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?,
                    0,
                )]
            } else {
                tokio::net::lookup_host((target.as_str(), 0)).await?.collect()
            };
            let addrs: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

/// urllib opener equivalent: proxy client, plain no-proxy client, or the
/// physical-route direct client (DirectUrlOpener).
struct DownloadOpener {
    client: reqwest::Client,
    /// Keeps the temporary direct routes alive for the whole session.
    _route: Option<Arc<WindowsDirectRoute>>,
}

fn download_client_builder() -> reqwest::ClientBuilder {
    let timeout = Duration::from_secs_f64(DOWNLOAD_TIMEOUT);
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(user_agent())
        .connect_timeout(timeout)
        .read_timeout(timeout)
}

fn build_download_client(proxy: Option<&str>) -> Result<reqwest::Client> {
    let mut builder = download_client_builder();
    builder = match proxy.map(str::trim).filter(|proxy| !proxy.is_empty()) {
        Some(proxy) => builder.proxy(reqwest::Proxy::all(proxy)?),
        None => builder.no_proxy(),
    };
    Ok(builder.build()?)
}

async fn open_download_opener(update_url: &str, proxy_url: Option<&str>) -> Result<DownloadOpener> {
    match proxy_url.map(str::trim).filter(|proxy| !proxy.is_empty()) {
        Some(proxy) => Ok(DownloadOpener {
            client: build_download_client(Some(proxy))?,
            _route: None,
        }),
        None => {
            let host = Url::parse(update_url)?
                .host_str()
                .unwrap_or_default()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_lowercase();
            if matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") {
                return Ok(DownloadOpener {
                    client: build_download_client(None)?,
                    _route: None,
                });
            }
            // DirectUrlOpener: resolve and route outside an active TUN.
            let route = Arc::new(tokio::task::spawn_blocking(WindowsDirectRoute::open).await??);
            let client = download_client_builder()
                .no_proxy()
                .dns_resolver(Arc::new(DirectResolver(Arc::clone(&route))))
                .build()?;
            Ok(DownloadOpener {
                client,
                _route: Some(route),
            })
        }
    }
}

fn origin(url: &Url) -> (String, String, Option<u16>) {
    (
        url.scheme().into(),
        url.host_str().unwrap_or("").to_lowercase(),
        url.port_or_known_default(),
    )
}

/// GET/HEAD following redirects with the `SafeRedirectHandler` policy: unsafe
/// targets are rejected and identifying headers never cross origins.
async fn send_with_safe_redirects(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    headers: &BTreeMap<String, String>,
    cancellation: &AppUpdateCancellation,
) -> Result<reqwest::Response> {
    let mut current = Url::parse(url.trim())?;
    let mut current_headers: BTreeMap<String, String> = headers
        .iter()
        .map(|(name, value)| (name.to_lowercase(), value.clone()))
        .collect();
    for _ in 0..=REDIRECT_LIMIT {
        let mut request = client.request(method.clone(), current.clone());
        for (name, value) in &current_headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = cancellation.run(request.send()).await??;
        let status = response.status().as_u16();
        if !matches!(status, 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .trim()
            .to_string();
        if location.is_empty() {
            return Ok(response);
        }
        let dest = current.join(&location)?;
        if redirect_blocked(&current, &dest, &location) {
            bail!("HTTP Error {status}: Unsafe subscription redirect blocked");
        }
        if origin(&current) != origin(&dest) {
            current_headers
                .retain(|name, _| REDIRECT_ALLOWED_HEADERS.contains(&name.as_str()));
        }
        current = dest;
    }
    bail!("HTTP Error 310: redirect limit reached")
}

fn content_length(response: &reqwest::Response) -> Result<u64> {
    match response.headers().get(reqwest::header::CONTENT_LENGTH) {
        Some(value) => Ok(value.to_str()?.trim().parse::<u64>()?),
        None => Ok(0),
    }
}

/// HEAD request to check Range support and get Content-Length.
async fn probe_range_support(
    client: &reqwest::Client,
    url: &str,
    cancellation: &AppUpdateCancellation,
) -> Result<(bool, u64)> {
    cancellation.check()?;
    let headers = BTreeMap::new();
    let response = send_with_safe_redirects(client, reqwest::Method::HEAD, url, &headers, cancellation).await?;
    let accepts = response
        .headers()
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let length = content_length(&response)?;
    Ok((accepts == "bytes" && length > 0, length))
}

/// Read one chunk; a pending cancellation interrupts a blocked read instead of
/// waiting behind it (`_read_chunk`).
async fn read_chunk(
    response: &mut reqwest::Response,
    cancellation: &AppUpdateCancellation,
) -> Result<Option<impl AsRef<[u8]>>> {
    match cancellation.run(response.chunk()).await {
        Ok(chunk) => Ok(chunk?),
        Err(_) => bail!(UpdateOperationCancelled),
    }
}

/// Download one segment with a Range header.
#[allow(clippy::too_many_arguments)]
async fn download_segment(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    end: u64,
    seg_path: &Path,
    seg_index: usize,
    progress: &Mutex<Vec<u64>>,
    total: u64,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<()> {
    cancellation.check()?;
    let expected_length = end - start + 1;
    let mut headers = BTreeMap::new();
    headers.insert("Range".to_string(), format!("bytes={start}-{end}"));
    let mut response =
        send_with_safe_redirects(client, reqwest::Method::GET, url, &headers, cancellation).await?;
    cancellation.check()?;
    let status_code = response.status().as_u16();
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if status_code != 206 || !content_range.starts_with(&format!("bytes {start}-{end}/")) {
        bail!("Сервер некорректно ответил на Range-запрос");
    }

    let mut file = tokio::fs::File::create(seg_path).await?;
    let mut downloaded: u64 = 0;
    while let Some(chunk) = read_chunk(&mut response, cancellation).await? {
        let chunk = chunk.as_ref();
        file.write_all(chunk).await?;
        downloaded += chunk.len() as u64;
        let done = {
            let mut progress = progress.lock();
            progress[seg_index] += chunk.len() as u64;
            progress.iter().sum()
        };
        let _ = events.send(AppUpdateDownloadEvent::Progress(percent(done, total)));
    }
    if downloaded != expected_length {
        bail!("Сервер вернул неполный фрагмент установщика");
    }
    file.flush().await?;
    Ok(())
}

/// Single-connection fallback download.
async fn download_single(
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<()> {
    cancellation.check()?;
    let headers = BTreeMap::new();
    let mut response =
        send_with_safe_redirects(client, reqwest::Method::GET, url, &headers, cancellation).await?;
    cancellation.check()?;
    let total = content_length(&response)?;

    let mut file = tokio::fs::File::create(target_path).await?;
    let mut downloaded: u64 = 0;
    loop {
        cancellation.check()?;
        let chunk = read_chunk(&mut response, cancellation).await?;
        let Some(chunk) = chunk else {
            if downloaded == 0 {
                bail!("Сервер не отдаёт данные");
            }
            break;
        };
        let chunk = chunk.as_ref();
        file.write_all(chunk).await?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let _ = events.send(AppUpdateDownloadEvent::Progress(percent(downloaded, total)));
        }
    }
    file.flush().await?;
    Ok(())
}

/// Download with multi-segment acceleration: parallel Range-based download
/// first, single connection when the server refuses ranges, and the same
/// fallback when a segmented attempt fails.
async fn download_with_opener(
    update_url: &str,
    target_path: &Path,
    opener: &DownloadOpener,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<()> {
    let client = &opener.client;
    let (supports_range, total) = match probe_range_support(client, update_url, cancellation).await {
        Ok(result) => result,
        Err(error) => {
            if is_cancelled_error(&error, cancellation) {
                bail!(UpdateOperationCancelled);
            }
            (false, 0)
        }
    };

    if !supports_range || total == 0 || total < (NUM_SEGMENTS * CHUNK_SIZE) as u64 {
        return download_single(client, update_url, target_path, cancellation, events).await;
    }

    // Split into segments.
    let seg_size = total / NUM_SEGMENTS as u64;
    let segments: Vec<(u64, u64)> = (0..NUM_SEGMENTS)
        .map(|index| {
            let start = index as u64 * seg_size;
            let end = if index == NUM_SEGMENTS - 1 {
                total - 1
            } else {
                (index as u64 + 1) * seg_size - 1
            };
            (start, end)
        })
        .collect();

    let seg_dir = target_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("_segments");
    tokio::fs::create_dir_all(&seg_dir).await?;
    let seg_paths: Vec<PathBuf> = (0..NUM_SEGMENTS)
        .map(|index| seg_dir.join(format!("seg_{index}")))
        .collect();
    let progress: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(vec![0; NUM_SEGMENTS]));

    let outcome = async {
        // Download segments in parallel.
        let mut handles = Vec::new();
        for (index, &(start, end)) in segments.iter().enumerate() {
            let client = client.clone();
            let cancellation = cancellation.clone();
            let events = events.clone();
            let seg_path = seg_paths[index].clone();
            let progress = Arc::clone(&progress);
            let url = update_url.to_string();
            handles.push(tokio::spawn(async move {
                download_segment(
                    &client,
                    &url,
                    start,
                    end,
                    &seg_path,
                    index,
                    &progress,
                    total,
                    &cancellation,
                    &events,
                )
                .await
            }));
        }

        // Re-raise any segment exception, in submission order.
        let mut first_error: Option<anyhow::Error> = None;
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    first_error.get_or_insert(error);
                }
                Err(error) => {
                    first_error.get_or_insert(anyhow::Error::new(error));
                }
            }
        }

        let staged: Result<()> = match first_error {
            Some(error) => Err(error),
            None => {
                // Concatenate segments into the final file.
                let mut output = tokio::fs::File::create(target_path).await?;
                for seg_path in &seg_paths {
                    let mut segment = tokio::fs::File::open(seg_path).await?;
                    tokio::io::copy(&mut segment, &mut output).await?;
                }
                output.flush().await?;
                Ok(())
            }
        };

        match staged {
            Ok(()) => Ok(()),
            Err(error) => {
                if is_cancelled_error(&error, cancellation) {
                    bail!(UpdateOperationCancelled);
                }
                // Segmented download failed — fall back to a single download.
                if tokio::fs::try_exists(target_path).await.unwrap_or(false) {
                    tokio::fs::remove_file(target_path).await?;
                }
                let _ = events.send(AppUpdateDownloadEvent::Progress(0));
                download_single(client, update_url, target_path, cancellation, events).await
            }
        }
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&seg_dir).await;
    outcome
}

async fn download_to_path(
    update_url: &str,
    target_path: &Path,
    proxy_url: Option<&str>,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<()> {
    let opener = open_download_opener(update_url, proxy_url).await?;
    download_with_opener(update_url, target_path, &opener, cancellation, events).await
}

// ── staging ─────────────────────────────────────────────────────────────────

/// Verify the downloaded payload and write the elevated install script next to
/// it. Reported failures (missing/mismatched digest) surface as
/// `ReportedFailure` errors after emitting the user-facing message.
pub async fn verify_and_stage_script(
    temp_dir: &Path,
    setup_path: &Path,
    update: &AppUpdate,
    portable: bool,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<(PathBuf, String)> {
    let _ = events.send(AppUpdateDownloadEvent::Status(
        if portable {
            "Проверка архива..."
        } else {
            "Проверка установщика..."
        }
        .to_string(),
    ));
    let expected_hash = extract_digest(&update.digest_sha256);
    if expected_hash.is_empty() {
        let _ = events.send(AppUpdateDownloadEvent::Error(
            if portable {
                "У релизного архива отсутствует SHA-256"
            } else {
                "У релизного установщика отсутствует SHA-256"
            }
            .to_string(),
        ));
        bail!(ReportedFailure);
    }

    let real_hash = sha256_file(setup_path)?;
    if real_hash.to_lowercase() != expected_hash.to_lowercase() {
        let _ = events.send(AppUpdateDownloadEvent::Error(
            if portable {
                "Контрольная сумма архива не совпадает"
            } else {
                "Контрольная сумма установщика не совпадает"
            }
            .to_string(),
        ));
        bail!(ReportedFailure);
    }

    let _ = events.send(AppUpdateDownloadEvent::Progress(100));
    let _ = events.send(AppUpdateDownloadEvent::Status("Подготовка установки...".into()));

    let current_app_dir = resolve_noverbatim(base_dir());
    let app_dir = target_app_dir(&current_app_dir);
    let script = temp_dir.join("_update.ps1");
    let script_text = build_update_script(&UpdateScriptContext {
        pid: std::process::id(),
        setup_path: setup_path.to_path_buf(),
        current_app_dir: current_app_dir.clone(),
        app_dir,
        temp_dir: temp_dir.to_path_buf(),
        expected_version: update.version.clone(),
        expected_hash: expected_hash.clone(),
        portable,
    });
    write_utf8_bom_text(&script, &script_text)?;
    Ok((script, expected_hash))
}

enum StageResult {
    Staged(StagedUpdate),
    Reported,
}

/// The download/install payload held between staging and the (possibly slow)
/// elevated install. Dropping it removes the temp directory, exactly like the
/// Python cleanup paths.
pub struct StagedUpdate {
    temp_dir: tempfile::TempDir,
    pub setup_path: PathBuf,
    pub script_path: PathBuf,
    pub expected_hash: String,
}

impl StagedUpdate {
    /// Python leaves the temp directory in place on success; the elevated
    /// script removes it after the install finishes.
    pub fn keep_temp_dir(self) -> PathBuf {
        self.temp_dir.keep()
    }
}

async fn stage_inner(
    update: &AppUpdate,
    proxy_url: Option<&str>,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> Result<StageResult> {
    cancellation.check()?;
    let temp_dir = tempfile::Builder::new()
        .prefix("lumen_update_")
        .tempdir()?;
    let temp_path = temp_dir.path().to_path_buf();
    let portable = is_portable();
    let default_setup = default_asset_name(portable);
    let setup_name = safe_asset_name(&update.asset_name, default_setup);
    let setup_path = temp_path.join(setup_name);

    let mut downloaded_ok = false;
    let proxy_active = proxy_url.map(|proxy| !proxy.trim().is_empty()).unwrap_or(false);

    // Attempt 1: through proxy (if available)
    if proxy_active {
        let _ = events.send(AppUpdateDownloadEvent::Status("Загрузка через прокси...".into()));
        match download_to_path(
            &update.download_url,
            &setup_path,
            proxy_url,
            cancellation,
            events,
        )
        .await
        {
            Ok(()) => downloaded_ok = true,
            Err(error) => {
                if is_cancelled_error(&error, cancellation) {
                    bail!(UpdateOperationCancelled);
                }
                let _ = events.send(AppUpdateDownloadEvent::Status(
                    "Прокси-сервер недоступен, пробую напрямую...".into(),
                ));
                let _ = events.send(AppUpdateDownloadEvent::Progress(0));
                // clean partial file
                if tokio::fs::try_exists(&setup_path).await.unwrap_or(false) {
                    tokio::fs::remove_file(&setup_path).await?;
                }
            }
        }
    }

    // Attempt 2: direct (no proxy)
    if !downloaded_ok {
        let _ = events.send(AppUpdateDownloadEvent::Status("Загрузка напрямую...".into()));
        match download_to_path(&update.download_url, &setup_path, None, cancellation, events).await
        {
            Ok(()) => downloaded_ok = true,
            Err(error) => {
                if is_cancelled_error(&error, cancellation) {
                    bail!(UpdateOperationCancelled);
                }
            }
        }
    }

    if !downloaded_ok {
        let message = if proxy_active {
            "Не удалось скачать обновление ни через прокси, ни напрямую.\nПереключитесь на рабочий сервер и попробуйте снова."
        } else {
            "Не удалось скачать обновление.\nПереключитесь на рабочий сервер и попробуйте снова."
        };
        let _ = events.send(AppUpdateDownloadEvent::Error(message.to_string()));
        return Ok(StageResult::Reported);
    }

    cancellation.check()?;

    let (script, expected_hash) =
        verify_and_stage_script(temp_dir.path(), &setup_path, update, portable, events).await?;

    // The installer requires elevation and owns Windows registration.
    match launch_update_script(&script, true) {
        Ok(true) => {}
        Ok(false) => {
            let _ = events.send(AppUpdateDownloadEvent::Error(
                "Не удалось запустить установку обновления с правами администратора".into(),
            ));
            return Ok(StageResult::Reported);
        }
        Err(error) => return Err(error),
    }

    Ok(StageResult::Staged(StagedUpdate {
        temp_dir,
        setup_path,
        script_path: script,
        expected_hash,
    }))
}

async fn stage(
    update: &AppUpdate,
    proxy_url: Option<&str>,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> std::result::Result<StageResult, UpdateFlowError> {
    match stage_inner(update, proxy_url, cancellation, events).await {
        Ok(result) => Ok(result),
        Err(error) => {
            if error.is::<ReportedFailure>() {
                return Err(UpdateFlowError::Failed);
            }
            if is_cancelled_error(&error, cancellation) {
                return Err(UpdateFlowError::Cancelled);
            }
            // Uncaught failure: surface it like the Python outer except block.
            let _ = events.send(AppUpdateDownloadEvent::Error(format!("{error}")));
            Err(UpdateFlowError::Failed)
        }
    }
}

/// Download the update, verify its digest and launch the elevated install
/// script (`UpdateDownloader.run`). `Err` only for cancellation; every other
/// failure is reported through the event stream.
pub async fn run_update_flow(
    update: &AppUpdate,
    proxy_url: Option<&str>,
    cancellation: &AppUpdateCancellation,
    events: &mpsc::UnboundedSender<AppUpdateDownloadEvent>,
) -> std::result::Result<(), UpdateOperationCancelled> {
    match stage(update, proxy_url, cancellation, events).await {
        Ok(StageResult::Staged(staged)) => {
            let _kept = staged.keep_temp_dir();
            let _ = events.send(AppUpdateDownloadEvent::FinishedOk);
            Ok(())
        }
        Ok(StageResult::Reported) => Ok(()),
        Err(UpdateFlowError::Cancelled) => Err(UpdateOperationCancelled),
        Err(UpdateFlowError::Failed) => Ok(()),
    }
}

/// `UpdateDownloader` worker: starts the flow on the async executor and
/// reports the PyQt6 signals as channel events. Dropping the worker cancels
/// the active download.
pub struct AppUpdateDownloader {
    pub events: mpsc::UnboundedReceiver<AppUpdateDownloadEvent>,
    cancellation: AppUpdateCancellation,
    task: Option<JoinHandle<()>>,
}

impl AppUpdateDownloader {
    /// `UpdateDownloader(update, proxy_url=..., restart_in_tray=...)`. The
    /// restart-in-tray argument was kept only for older callers; an update is
    /// always an interactive relaunch and must reopen visibly.
    pub fn start(update: AppUpdate, proxy_url: Option<String>) -> Self {
        let cancellation = AppUpdateCancellation::default();
        let (events, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                let _ =
                    run_update_flow(&update, proxy_url.as_deref(), &cancellation, &events).await;
            }
        });
        Self {
            events: receiver,
            cancellation,
            task: Some(task),
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub async fn join(mut self) -> Result<(), JoinError> {
        self.task.take().expect("app update worker task").await
    }
}

impl Drop for AppUpdateDownloader {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
