//! Discover, verify and install updates for the upstream Windows Xray core.

mod archive;
mod release;

use std::cmp::Ordering;
use std::fs::File;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use lumen_core::constants::{XRAY_GITHUB_RELEASES_API, xray_path_default};
use lumen_core::http_utils::get_update_response;
use lumen_core::path_utils::resolve_configured_path;
use parking_lot::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use super::manager::get_xray_version;
use archive::ExtractedArchive;
pub use archive::sha256_file;
use release::{ARCHIVE_NAME, compare_versions, normalize_channel, transports};
pub use release::{
    extract_digest, extract_version, fetch_dgst_hash, is_newer, request_json, safe_asset_name,
};

static INSTALL_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrayCoreRelease {
    pub version: String,
    pub channel: String,
    pub url: String,
    pub digest_sha256: String,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrayCoreUpdateStatus {
    UpToDate,
    Available,
    Updated,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct XrayCoreUpdateResult {
    pub status: XrayCoreUpdateStatus,
    pub message: String,
    pub channel: String,
    pub current_version: String,
    pub latest_version: String,
    pub updated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct XrayCoreUpdateOptions {
    pub xray_path: String,
    pub channel: String,
    pub feed_url: String,
    pub apply_update: bool,
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("update cancelled")]
pub struct UpdateCancelled;

#[derive(Debug, Clone)]
pub struct UpdateCancellation(watch::Sender<bool>);

impl Default for UpdateCancellation {
    fn default() -> Self {
        Self(watch::Sender::new(false))
    }
}

impl UpdateCancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn check(&self) -> Result<(), UpdateCancelled> {
        if *self.0.borrow() {
            Err(UpdateCancelled)
        } else {
            Ok(())
        }
    }

    pub(crate) async fn run<F: Future>(&self, future: F) -> Result<F::Output, UpdateCancelled> {
        let mut cancelled = self.0.subscribe();
        tokio::select! {
            biased;
            _ = cancelled.wait_for(|cancelled| *cancelled) => Err(UpdateCancelled),
            result = future => Ok(result),
        }
    }
}

pub async fn resolve_xray_release(
    channel: &str,
    feed_url: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<Option<XrayCoreRelease>> {
    release::resolve(
        channel,
        feed_url,
        XRAY_GITHUB_RELEASES_API,
        proxy_url,
        cancellation,
    )
    .await
}

pub async fn download_file(
    url: &str,
    destination: &Path,
    proxy_url: Option<&str>,
    mut on_progress: impl FnMut(u64, u64),
    cancellation: &UpdateCancellation,
) -> Result<()> {
    cancellation.check()?;
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut transports = transports(proxy_url).peekable();
    while let Some(proxy) = transports.next() {
        let download = async {
            let (mut response, _route) = get_update_response(url, 120.0, proxy).await?;
            let total = response.content_length().unwrap_or_default();
            let mut file = tokio::fs::File::create(destination).await?;
            let mut downloaded = 0;
            let mut last_report = 0;
            while let Some(chunk) = response.chunk().await? {
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                if total > 0 && (downloaded - last_report >= 1024 * 1024 || downloaded == total) {
                    on_progress(downloaded, total);
                    last_report = downloaded;
                }
            }
            file.flush().await?;
            if total > 0 && downloaded != last_report {
                on_progress(downloaded, total);
            }
            Ok(())
        };
        match cancellation.run(download).await? {
            Ok(()) => return Ok(()),
            Err(error) if transports.peek().is_none() => return Err(error),
            Err(_) => match tokio::fs::remove_file(destination).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
        }
    }
    unreachable!("at least one download transport")
}

/// Check the configured channel, or download and install its selected release.
/// The installation callback must stop services that use the executable.
pub async fn check_and_update_xray_core<F: Future<Output = Result<()>>>(
    options: &XrayCoreUpdateOptions,
    on_progress: impl FnMut(u64, u64),
    on_install_start: impl FnOnce() -> F,
    cancellation: &UpdateCancellation,
) -> Result<XrayCoreUpdateResult, UpdateCancelled> {
    check_and_update(
        options,
        XRAY_GITHUB_RELEASES_API,
        on_progress,
        on_install_start,
        cancellation,
    )
    .await
}

async fn check_and_update<F: Future<Output = Result<()>>>(
    options: &XrayCoreUpdateOptions,
    github_api: &str,
    on_progress: impl FnMut(u64, u64),
    on_install_start: impl FnOnce() -> F,
    cancellation: &UpdateCancellation,
) -> Result<XrayCoreUpdateResult, UpdateCancelled> {
    cancellation.check()?;
    let mut result = XrayCoreUpdateResult {
        status: XrayCoreUpdateStatus::Error,
        message: String::new(),
        channel: normalize_channel(&options.channel).into(),
        current_version: String::new(),
        latest_version: String::new(),
        updated: false,
    };
    let outcome = update(
        options,
        github_api,
        on_progress,
        on_install_start,
        cancellation,
        &mut result,
    )
    .await;
    match outcome {
        Ok(()) => Ok(result),
        Err(error) if error.is::<UpdateCancelled>() => Err(UpdateCancelled),
        Err(error) => {
            result.message = format!("{error:#}");
            Ok(result)
        }
    }
}

async fn update<F: Future<Output = Result<()>>>(
    options: &XrayCoreUpdateOptions,
    github_api: &str,
    on_progress: impl FnMut(u64, u64),
    on_install_start: impl FnOnce() -> F,
    cancellation: &UpdateCancellation,
    result: &mut XrayCoreUpdateResult,
) -> Result<()> {
    let default_exe = xray_path_default();
    let exe = resolve_configured_path(&options.xray_path, Some(&default_exe), true, true)
        .unwrap_or(default_exe);
    let core_missing = !exe.is_file();
    if !core_missing {
        result.current_version = cancellation
            .run(tokio::task::spawn_blocking({
                let exe = exe.clone();
                move || installed_version(&exe)
            }))
            .await??;
    }

    let proxy = options.proxy_url.as_deref();
    let release = release::resolve(
        &result.channel,
        &options.feed_url,
        github_api,
        proxy,
        cancellation,
    )
    .await
    .context("Не удалось получить информацию о релизе")?
    .filter(|release| !release.url.is_empty())
    .context("Информация о релизе не найдена")?;
    result.channel = release.channel;
    result.latest_version = extract_version(&release.version);
    let latest = &result.latest_version;
    let current = &result.current_version;
    if !current.is_empty() && compare_versions(latest, current) == Some(Ordering::Equal) {
        result.status = XrayCoreUpdateStatus::UpToDate;
        result.message = format!("Xray core актуален ({current})");
        return Ok(());
    }
    if !options.apply_update {
        result.status = XrayCoreUpdateStatus::Available;
        result.message = if core_missing {
            format!("Xray core не установлен. Доступна установка версии {latest}")
        } else if is_newer(latest, current) {
            format!("Доступно обновление Xray до {latest}")
        } else {
            format!("Доступен переход Xray на {latest}")
        };
        return Ok(());
    }

    let temp = tempfile::Builder::new()
        .prefix("xray_core_update_")
        .tempdir()
        .context("Ошибка загрузки")?;
    let path = temp.path().join(ARCHIVE_NAME);
    download_file(&release.url, &path, proxy, on_progress, cancellation)
        .await
        .context("Ошибка загрузки")?;
    let expected_hash = extract_digest(&release.digest_sha256);
    ensure!(
        !expected_hash.is_empty(),
        "Для релиза Xray отсутствует контрольная сумма SHA-256"
    );
    let archive = verify_archive(
        temp,
        path,
        expected_hash,
        latest.clone(),
        cancellation.clone(),
    )
    .await?;

    cancellation
        .run(async { on_install_start().await })
        .await?
        .context("Ошибка остановки служб")?;
    let refreshed = tokio::task::spawn_blocking({
        let cancellation = cancellation.clone();
        move || -> Result<String> {
            let _lock = INSTALL_LOCK.lock();
            cancellation.check()?;
            archive.install(&exe).context("Ошибка установки")?;
            Ok(installed_version(&exe))
        }
    })
    .await??;
    if !refreshed.is_empty() {
        result.latest_version = refreshed;
    }
    result.status = XrayCoreUpdateStatus::Updated;
    result.message = format!("Xray core обновлён до {}", result.latest_version);
    result.updated = true;
    Ok(())
}

fn installed_version(exe: &Path) -> String {
    extract_version(&get_xray_version(&exe.to_string_lossy()).unwrap_or_default())
}

async fn verify_archive(
    temp: tempfile::TempDir,
    path: PathBuf,
    expected_hash: String,
    version: String,
    cancellation: UpdateCancellation,
) -> Result<ExtractedArchive> {
    tokio::task::spawn_blocking(move || {
        let _temp = temp;
        ensure!(
            sha256_file(&path, &cancellation)? == expected_hash,
            "Контрольная сумма архива не совпадает"
        );
        let validate = || -> Result<_> {
            let archive = ExtractedArchive::open(File::open(path)?)?;
            cancellation.check()?;
            archive.validate(&version)?;
            Ok(archive)
        };
        validate().context("Архив Xray не прошел проверку совместимости")
    })
    .await?
}

#[derive(Debug)]
pub enum XrayCoreUpdateEvent {
    Progress(u32),
    /// Reply with whether the services using the current core stopped.
    RequestDisconnect(oneshot::Sender<bool>),
    Done(XrayCoreUpdateResult),
}

pub struct XrayCoreUpdateWorker {
    pub events: mpsc::UnboundedReceiver<XrayCoreUpdateEvent>,
    cancellation: UpdateCancellation,
    task: Option<JoinHandle<()>>,
}

impl XrayCoreUpdateWorker {
    pub fn start(options: XrayCoreUpdateOptions) -> Self {
        Self::start_with_source(options, XRAY_GITHUB_RELEASES_API)
    }

    fn start_with_source(options: XrayCoreUpdateOptions, github_api: &'static str) -> Self {
        let cancellation = UpdateCancellation::default();
        let (events, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                let result = check_and_update(
                    &options,
                    github_api,
                    |downloaded, total| {
                        let percent =
                            ((u128::from(downloaded) * 100) / u128::from(total)).min(100) as u32;
                        let _ = events.send(XrayCoreUpdateEvent::Progress(percent));
                    },
                    || request_disconnect(&events),
                    &cancellation,
                )
                .await;
                if let Ok(result) = result
                    && cancellation.check().is_ok()
                {
                    let _ = events.send(XrayCoreUpdateEvent::Done(result));
                }
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

    pub async fn join(mut self) -> Result<(), tokio::task::JoinError> {
        self.task.take().expect("worker task").await
    }
}

impl Drop for XrayCoreUpdateWorker {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn request_disconnect(events: &mpsc::UnboundedSender<XrayCoreUpdateEvent>) -> Result<()> {
    let (confirm, ack) = oneshot::channel();
    events
        .send(XrayCoreUpdateEvent::RequestDisconnect(confirm))
        .map_err(|_| anyhow::anyhow!("failed to stop the active core before update"))?;
    match tokio::time::timeout(Duration::from_secs(60), ack).await {
        Ok(Ok(true)) => Ok(()),
        Ok(_) => bail!("failed to stop the active core before update"),
        Err(_) => bail!("timed out waiting for the active core to stop"),
    }
}

#[cfg(test)]
mod tests;
