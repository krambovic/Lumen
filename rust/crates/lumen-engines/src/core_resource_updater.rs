//! Resource updates for the managed cores: droute, sing-box extended and the
//! regional geodata bundles (port of `windows/xray_fluent/core_resource_updater.py`).
//!
//! GitHub release feeds are fetched through [`crate::xray::core_updater`]
//! helpers (`_download_file`, `_request_json`, ...); progress events,
//! cancellation and the staged install/rollback flow mirror the Python
//! worker. `StartupResourceCheckWorker` becomes [`startup_resource_check`];
//! `ResourceUpdateWorker` becomes [`ResourceUpdateWorker`] with an event
//! channel (`Progress` / `RequestDisconnect` / `Done`).

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use parking_lot::Mutex as ParkingMutex;
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use lumen_core::component_compatibility::ensure_component_compatible;
use lumen_core::constants::{singbox_path_default, xray_path_default};
pub use lumen_core::geodata_resources::GEODATA_METADATA_NAME;
use lumen_core::geodata_resources::{
    SINGBOX_BINARY_RULE_SETS, active_user_geodata_dir, regional_singbox_rule_sets,
    singbox_rule_set_dir, user_geodata_dir,
};
use lumen_core::http_utils::get_update_response;
use lumen_core::models::{int_field, str_field};
use lumen_core::path_utils::resolve_configured_path;
use lumen_core::pyjson;
use lumen_core::zip_utils::safe_extract_zip;
use lumen_platform::subprocess::{hidden_command, run_merged};

use crate::discord_proxy_manager::{
    DROUTE_LEGACY_VERSION, droute_dir, get_droute_bundle_version, verify_bundled_droute,
};
use crate::singbox::manager::get_singbox_version;
// Python imports these names from `.engines.xray.core_updater`; re-exporting
// them here keeps the same import path for the rest of the crate.
pub use crate::xray::core_updater::{
    UpdateCancellation, UpdateCancelled, download_file, extract_digest, extract_version,
    fetch_dgst_hash, is_newer, request_json, safe_asset_name, sha256_file,
};

pub const DROUTE_LATEST_API: &str = "https://api.github.com/repos/snowluwu/droute/releases/latest";
pub const SINGBOX_EXTENDED_LATEST_API: &str =
    "https://api.github.com/repos/shtorm-7/sing-box-extended/releases/latest";
pub const RUNETFREEDOM_LATEST_API: &str =
    "https://api.github.com/repos/runetfreedom/russia-v2ray-rules-dat/releases/latest";
pub const LOYALSOLDIER_LATEST_API: &str =
    "https://api.github.com/repos/Loyalsoldier/v2ray-rules-dat/releases/latest";
pub const IRAN_RULES_LATEST_API: &str =
    "https://api.github.com/repos/Chocolate4U/Iran-v2ray-rules/releases/latest";
const CHINA_GEODATA_CDN: &str = "https://cdn.jsdelivr.net/gh/Loyalsoldier/v2ray-rules-dat@release";
const CHINA_GEODATA_RELEASE: &str =
    "https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download";
const MAX_RESOURCE_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RULE_SET_MEMBER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RULE_SET_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
// sing-box binary rule-sets start with the "SRS" magic followed by a version byte.
const SRS_MAGIC: &[u8] = b"SRS";

static SINGBOX_VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d+\.\d+\.\d+(?:-extended-\d+\.\d+\.\d+)?)").expect("valid version regex")
});
static LUMEN_BUILD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)-lumen(?:\.|\b)").expect("valid build regex"));
static CORE_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\d+\.\d+").expect("valid core version regex"));
static DROUTE_VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+\.\d+$").expect("valid droute version regex"));

// ── Result type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceUpdateStatus {
    /// `available`
    Available,
    /// `up_to_date`
    UpToDate,
    /// `updated`
    Updated,
    /// `error`
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceUpdateResult {
    pub kind: String,
    /// `available | up_to_date | updated | error`
    pub status: ResourceUpdateStatus,
    pub message: String,
    pub current_version: String,
    pub latest_version: String,
}

impl ResourceUpdateResult {
    pub fn new(kind: &str, status: ResourceUpdateStatus, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            status,
            message: message.into(),
            current_version: String::new(),
            latest_version: String::new(),
        }
    }

    pub fn with_versions(
        kind: &str,
        status: ResourceUpdateStatus,
        message: impl Into<String>,
        current_version: impl Into<String>,
        latest_version: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.to_string(),
            status,
            message: message.into(),
            current_version: current_version.into(),
            latest_version: latest_version.into(),
        }
    }

    fn error(kind: &str, message: impl Into<String>) -> Self {
        Self::new(kind, ResourceUpdateStatus::Error, message)
    }
}

/// `_core_dir`: the directory holding the active Xray core and geodata.
pub fn core_dir() -> PathBuf {
    let mut dir = xray_path_default();
    dir.pop();
    dir
}

fn active_geodata_dir() -> PathBuf {
    active_user_geodata_dir().unwrap_or_else(core_dir)
}

// ── Download helpers ────────────────────────────────────────────────────────

/// `_download_direct`: proxy attempt first, then the physical route, with the
/// resource size cap enforced both from Content-Length and while streaming.
async fn download_direct(
    url: &str,
    destination: &Path,
    mut on_progress: Option<&mut (dyn FnMut(u64, u64) + Send)>,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<()> {
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let proxy = proxy_url.filter(|proxy| !proxy.trim().is_empty());
    let attempts: Vec<Option<&str>> = match proxy {
        Some(proxy) => vec![Some(proxy), None],
        None => vec![None],
    };
    for (index, active_proxy) in attempts.iter().enumerate() {
        cancellation.check()?;
        let attempt = async {
            let (mut response, _route) = get_update_response(url, 120.0, *active_proxy).await?;
            let total = response.content_length().unwrap_or_default();
            if total > MAX_RESOURCE_DOWNLOAD_BYTES {
                bail!("ресурс слишком большой: {total} байт");
            }
            let mut file = tokio::fs::File::create(destination).await?;
            let mut downloaded: u64 = 0;
            while let Some(chunk) = response.chunk().await? {
                // Python checks cancellation before every 1 MiB read.
                cancellation.check()?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                if downloaded > MAX_RESOURCE_DOWNLOAD_BYTES {
                    bail!("ресурс превышает лимит {MAX_RESOURCE_DOWNLOAD_BYTES} байт");
                }
                if total > 0
                    && let Some(callback) = on_progress.as_deref_mut()
                {
                    callback(downloaded, total);
                }
            }
            file.flush().await?;
            if total > 0 && downloaded != total {
                bail!("ресурс загружен не полностью: {downloaded} из {total} байт");
            }
            Ok(())
        };
        match cancellation.run(attempt).await? {
            Ok(()) => {
                cancellation.check()?;
                return Ok(());
            }
            Err(error) => {
                if index == attempts.len() - 1 {
                    return Err(error);
                }
                let _ = tokio::fs::remove_file(destination).await;
            }
        }
    }
    unreachable!("at least one download attempt")
}

/// `_scaled_progress(offset, span)` progress sink factory for update_geodata.
fn scaled_sink<'a>(
    offset: i64,
    span: i64,
    on_progress: &'a mut Option<&mut (dyn FnMut(u64, u64) + Send)>,
) -> impl FnMut(u64, u64) + Send + 'a {
    move |done: u64, total: u64| {
        if total > 0
            && let Some(callback) = on_progress.as_deref_mut()
        {
            let percent = offset + (done as i64 * span / total as i64);
            callback(percent.clamp(0, 100) as u64, 100);
        }
    }
}

// ── Version helpers ─────────────────────────────────────────────────────────

/// `_extract_singbox_version`: prefer the extended-build token inside the text.
pub fn extract_singbox_version(text: &str) -> String {
    let value = extract_version(text);
    match SINGBOX_VERSION_RE.captures(text) {
        Some(found) => found[1].to_string(),
        None => value,
    }
}

/// `_is_lumen_singbox_build`: the current binary carries a Lumen build tag.
pub fn is_lumen_singbox_build(text: &str) -> bool {
    LUMEN_BUILD_RE.is_match(text)
}

fn assets_of(release: &Value) -> Vec<&Value> {
    release["assets"]
        .as_array()
        .map(|items| items.iter().filter(|asset| asset.is_object()).collect())
        .unwrap_or_default()
}

fn asset_name_of(asset: &Value) -> String {
    asset
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn asset_url_of(asset: &Value) -> String {
    asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn asset_digest_of(asset: Option<&&Value>) -> String {
    match asset {
        Some(asset) => extract_digest(&str_field(asset, "digest")),
        None => String::new(),
    }
}

/// `_release_asset_digest`: release `digest` field or a `.sha256`/`.dgst` sidecar.
async fn release_asset_digest(
    release: &Value,
    asset_name: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<String> {
    let assets = assets_of(release);
    let selected = assets.iter().find(|asset| {
        asset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .eq_ignore_ascii_case(asset_name)
    });
    let digest = asset_digest_of(selected);
    if !digest.is_empty() {
        return Ok(digest);
    }
    for suffix in [".sha256", ".sha256sum", ".dgst"] {
        let sidecar_name = format!("{asset_name}{suffix}").to_lowercase();
        let sidecar = assets.iter().find(|asset| {
            asset
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                == sidecar_name
        });
        if let Some(sidecar) = sidecar {
            return fetch_dgst_hash(
                &str_field(sidecar, "browser_download_url"),
                proxy_url,
                cancellation,
            )
            .await;
        }
    }
    Ok(String::new())
}

/// `_singbox_asset_digest`: release `digest` field or `.sha256`/`.dgst`.
async fn singbox_asset_digest(
    release: &Value,
    asset_name: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<String> {
    let assets = assets_of(release);
    let selected = assets.iter().find(|asset| {
        asset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .eq_ignore_ascii_case(asset_name)
    });
    let digest = asset_digest_of(selected);
    if !digest.is_empty() {
        return Ok(digest);
    }
    for suffix in [".sha256", ".dgst"] {
        let sidecar_name = format!("{asset_name}{suffix}").to_lowercase();
        let sidecar = assets.iter().find(|asset| {
            asset
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                == sidecar_name
        });
        if let Some(sidecar) = sidecar {
            return fetch_dgst_hash(
                &str_field(sidecar, "browser_download_url"),
                proxy_url,
                cancellation,
            )
            .await;
        }
    }
    Ok(String::new())
}

// ── Geodata release resolution ──────────────────────────────────────────────

/// One release asset: `(download URL, published SHA-256)`.
type ReleaseAssets = Vec<(String, (String, String))>;

fn release_asset<'a>(
    release_assets: &'a ReleaseAssets,
    name: &str,
) -> Option<&'a (String, String)> {
    release_assets
        .iter()
        .find(|(asset_name, _)| asset_name == name)
        .map(|(_, value)| value)
}

fn geodata_source(region: &str) -> (&'static str, &'static str, &'static [&'static str]) {
    match region {
        "china" => (
            "Loyalsoldier",
            LOYALSOLDIER_LATEST_API,
            &["geoip.dat", "geosite.dat"],
        ),
        "iran" => (
            "Chocolate4U",
            IRAN_RULES_LATEST_API,
            &["geoip.dat", "geosite.dat"],
        ),
        _ => (
            "RuNetFreedom",
            RUNETFREEDOM_LATEST_API,
            &["geoip.dat", "geosite.dat", "sing-box.zip"],
        ),
    }
}

/// `_resolve_geodata_release`: `(version, [(asset name, (url, sha256))])`.
async fn resolve_geodata_release(
    region: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<(String, ReleaseAssets)> {
    let region = if region.trim().is_empty() {
        "russia".to_string()
    } else {
        region.trim().to_lowercase()
    };
    let (source_name, release_api, required_assets) = geodata_source(&region);
    if region == "china" {
        match china_cdn_release(proxy_url, cancellation).await {
            Ok(release) => return Ok(release),
            Err(error) if error.is::<UpdateCancelled>() => return Err(error),
            Err(cdn_error) => {
                return resolve_github_geodata_release(
                    source_name, release_api, required_assets, proxy_url, cancellation,
                )
                .await
                .with_context(|| format!("China geodata CDN unavailable: {cdn_error:#}"));
            }
        }
    }
    resolve_github_geodata_release(source_name, release_api, required_assets, proxy_url, cancellation)
        .await
}

async fn china_cdn_release(
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<(String, ReleaseAssets)> {
    let geoip = format!("{CHINA_GEODATA_CDN}/geoip.dat");
    let geosite = format!("{CHINA_GEODATA_CDN}/geosite.dat");
    let geoip_hash_url = format!("{geoip}.sha256sum");
    let geosite_hash_url = format!("{geosite}.sha256sum");
    let (geoip_hash, geosite_hash) = tokio::time::timeout(
        Duration::from_secs(15),
        async {
            tokio::try_join!(
                fetch_dgst_hash(&geoip_hash_url, proxy_url, cancellation),
                fetch_dgst_hash(&geosite_hash_url, proxy_url, cancellation),
            )
        },
    )
    .await
    .context("timed out fetching China geodata checksums")??;
    if geoip_hash.len() != 64 || geosite_hash.len() != 64 {
        bail!("invalid China geodata checksums");
    }
    let version = format!("cdn-{}-{}", &geoip_hash[..8], &geosite_hash[..8]);
    Ok((
        version,
        vec![
            ("geoip.dat".into(), (geoip, geoip_hash)),
            ("geosite.dat".into(), (geosite, geosite_hash)),
        ],
    ))
}

async fn resolve_github_geodata_release(
    source_name: &str,
    release_api: &str,
    required_assets: &[&str],
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<(String, ReleaseAssets)> {
    let release = request_json(release_api, proxy_url, cancellation).await?;
    if !release.is_object() {
        bail!("invalid {source_name} release response");
    }
    let version = {
        let tag = str_field(&release, "tag_name");
        let raw = if tag.is_empty() {
            str_field(&release, "name")
        } else {
            tag
        };
        raw.trim().to_string()
    };
    let mut assets: ReleaseAssets = Vec::new();
    for name in required_assets {
        let asset = release["assets"]
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item.is_object()
                        && item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .eq_ignore_ascii_case(name)
                })
            })
            .cloned()
            .with_context(|| format!("{source_name} release does not contain {name}"))?;
        let url = str_field(&asset, "browser_download_url").trim().to_string();
        let digest = release_asset_digest(&release, name, proxy_url, cancellation).await?;
        if url.is_empty() || digest.is_empty() {
            bail!("{source_name} release does not provide a published SHA-256 for {name}");
        }
        require_https(&url, name)?;
        assets.push((name.to_string(), (url, digest)));
    }
    Ok((version, assets))
}

fn china_rule_set_cdn_url(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://raw.githubusercontent.com/2dust/sing-box-rules/")?;
    let (branch, file) = path.split_once('/')?;
    Some(format!("https://cdn.jsdelivr.net/gh/2dust/sing-box-rules@{branch}/{file}"))
}

async fn download_china_geodata(
    urls: [&str; 2],
    destination: &Path,
    progress: &mut (dyn FnMut(u64, u64) + Send),
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
    validate: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    let mut errors = Vec::new();
    for url in urls {
        cancellation.check()?;
        let attempt = async {
            download_direct(url, destination, Some(progress), proxy_url, cancellation).await?;
            validate(destination)
        };
        match tokio::time::timeout(Duration::from_secs(45), attempt).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) if error.is::<UpdateCancelled>() => return Err(error),
            Ok(Err(error)) => errors.push(format!("{url}: {error:#}")),
            Err(_) => errors.push(format!("{url}: timed out")),
        }
    }
    bail!("{}", errors.join("; "))
}

/// `_read_geodata_metadata`: `{}` on any read or parse failure.
fn read_geodata_metadata(target_dir: &Path) -> Value {
    let path = target_dir.join(GEODATA_METADATA_NAME);
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(payload) if payload.is_object() => payload,
            _ => json!({}),
        },
        Err(_) => json!({}),
    }
}

fn rule_set_filename(key: &str, source: &str) -> String {
    if let Some(archive_path) = source.strip_prefix("archive:") {
        Path::new(archive_path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        format!("{}.srs", key.replace(':', "-"))
    }
}

/// Whether the locally active geodata fully supports `region`.
///
/// Xray uses one active `geoip.dat`/`geosite.dat` pair, so merely having
/// another region's sing-box `.srs` files is not enough. The metadata must
/// also identify the requested region. The check is local-only and verifies
/// every available recorded hash. Legacy Russian bundles without metadata
/// remain compatible, but their binary rule-sets still have to pass the SRS
/// format check; the next resource update upgrades them to hashed schema 3.
pub fn regional_geodata_installed(
    region: &str,
    target_dir: Option<&Path>,
    rule_set_dir: Option<&Path>,
) -> bool {
    let mut normalized = region.trim().to_lowercase();
    let regional = regional_singbox_rule_sets();
    if !regional.contains_key(&normalized) {
        normalized = "russia".to_string();
    }
    let target = target_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(active_geodata_dir);
    let rules_dir = rule_set_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| target.join("rule-sets"));
    let metadata = read_geodata_metadata(&target);
    let schema = int_field(&metadata, "schema", 0);
    let installed_region = {
        let value = str_field(&metadata, "region");
        if value.is_empty() {
            "russia".to_string()
        } else {
            value.trim().to_lowercase()
        }
    };
    if installed_region != normalized {
        return false;
    }
    let recorded_hashes = metadata.get("sha256").cloned().unwrap_or(Value::Null);
    let recorded_rule_hashes = metadata
        .get("rule_set_sha256")
        .cloned()
        .unwrap_or(Value::Null);
    if schema >= 3 && (!recorded_hashes.is_object() || !recorded_rule_hashes.is_object()) {
        return false;
    }
    let recorded_hashes = recorded_hashes.as_object().cloned().unwrap_or_default();
    let recorded_rule_hashes = recorded_rule_hashes
        .as_object()
        .cloned()
        .unwrap_or_default();
    let cancellation = UpdateCancellation::default();
    for name in ["geoip.dat", "geosite.dat"] {
        let path = target.join(name);
        let expected_hash = recorded_hashes
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let actual = if path.is_file() {
            sha256_file(&path, &cancellation)
                .unwrap_or_default()
                .to_lowercase()
        } else {
            String::new()
        };
        if !path.is_file()
            || size < 1024
            || (!expected_hash.is_empty() && actual != expected_hash)
            || (schema >= 3 && expected_hash.is_empty())
        {
            return false;
        }
    }
    let Some(expected_rules) = regional.get(&normalized) else {
        return false;
    };
    for (key, source) in expected_rules {
        let filename = rule_set_filename(key, source);
        let path = rules_dir.join(&filename);
        if ensure_rule_set_file(&path, key).is_err() {
            return false;
        }
        let expected_hash = recorded_rule_hashes
            .get(&filename)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let actual = sha256_file(&path, &cancellation)
            .unwrap_or_default()
            .to_lowercase();
        if (!expected_hash.is_empty() && actual != expected_hash)
            || (schema >= 3 && expected_hash.is_empty())
        {
            return false;
        }
    }
    true
}

/// `_validate_geodata_with_xray`: run a minimal routing config through the
/// installed Xray to prove the downloaded geodata files actually load.
fn validate_geodata_with_xray(staging: &Path, region: &str) -> Result<()> {
    let default_path = xray_path_default();
    let Some(exe) = resolve_configured_path("", Some(&default_path), true, true) else {
        return Ok(());
    };
    if !exe.is_file() {
        return Ok(());
    }
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let work = tempfile::tempdir()?;
    let config_path = work.path().join("xray-geodata-check.json");
    let (geosite_codes, geoip_codes): (&[&str], &[&str]) = match region {
        "china" => (
            &["cn", "gfw", "greatfire", "google"][..],
            &["cn", "google", "telegram"][..],
        ),
        "iran" => (&["ir"][..], &["ir"][..]),
        _ => (
            &["ru-available-only-inside", "ru-blocked"][..],
            &["ru", "ru-blocked", "ru-blocked-community"][..],
        ),
    };
    let geosite_refs: Vec<Value> = geosite_codes
        .iter()
        .map(|code| Value::String(format!("geosite:{code}")))
        .collect();
    let geoip_refs: Vec<Value> = geoip_codes
        .iter()
        .map(|code| Value::String(format!("geoip:{code}")))
        .collect();
    let config = json!({
        "log": {"loglevel": "none"},
        "outbounds": [{"protocol": "freedom", "tag": "direct"}],
        "routing": {
            "rules": [
                {"type": "field", "domain": geosite_refs, "outboundTag": "direct"},
                {"type": "field", "ip": geoip_refs, "outboundTag": "direct"},
            ]
        },
    });
    fs::write(&config_path, pyjson::dumps(&config, true, None))?;
    let mut command = hidden_command(&exe);
    command
        .args(["run", "-test", "-c"])
        .arg(&config_path)
        .env("XRAY_LOCATION_ASSET", staging)
        .current_dir(&exe_dir);
    let result = run_merged(command, 20.0)?;
    if result.returncode != 0 {
        let detail = result.output_text().trim().to_string();
        bail!("Xray rejected downloaded geodata: {detail}");
    }
    Ok(())
}

// ── Staged install ──────────────────────────────────────────────────────────

/// Backup file for `_atomic_replace_files`: `name.ext -> name.ext.bak`.
fn backup_path(destination: &Path) -> PathBuf {
    let extension = destination
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
        .unwrap_or_default();
    let new_extension = if extension.is_empty() {
        "bak".to_string()
    } else {
        format!("{extension}.bak")
    };
    destination.with_extension(new_extension)
}

/// Stage and replace a group of files, rolling every target back on failure.
fn atomic_replace_files(
    replacements: &[(PathBuf, PathBuf)],
    backup_targets: &[PathBuf],
    validator: Option<&mut dyn FnMut() -> Result<()>>,
) -> Result<()> {
    let temp_name = |destination: &Path, suffix: &str| -> String {
        format!(
            ".{}.{suffix}",
            destination
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        )
    };
    let mut staged: Vec<(tempfile::TempPath, PathBuf)> = Vec::new();
    let mut rollback: Vec<(PathBuf, Option<tempfile::TempPath>)> = Vec::new();
    let mut replaced: Vec<PathBuf> = Vec::new();
    let outcome = (|| -> Result<()> {
        for (source, destination) in replacements {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let prefix = temp_name(destination, "");
            let temp = tempfile::Builder::new()
                .prefix(&prefix)
                .suffix(".new")
                .tempfile_in(destination.parent().unwrap_or_else(|| Path::new(".")))?;
            let temp_path = temp.into_temp_path();
            fs::copy(source, &temp_path)?;
            staged.push((temp_path, destination.clone()));

            if destination.exists() {
                let rollback_file = tempfile::Builder::new()
                    .prefix(&prefix)
                    .suffix(".rollback")
                    .tempfile_in(destination.parent().unwrap_or_else(|| Path::new(".")))?;
                let rollback_path = rollback_file.into_temp_path();
                fs::copy(destination, &rollback_path)?;
                rollback.push((destination.clone(), Some(rollback_path)));
            } else {
                rollback.push((destination.clone(), None));
            }
        }

        for (staged_path, destination) in staged.drain(..) {
            staged_path
                .persist(&destination)
                .map_err(|error| anyhow!("{error}"))?;
            replaced.push(destination);
        }

        if let Some(validator) = validator {
            validator()?;
        }

        for destination in backup_targets {
            let Some((_, Some(rollback_path))) =
                rollback.iter().find(|(path, _)| path == destination)
            else {
                continue;
            };
            fs::copy(rollback_path, backup_path(destination))?;
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        for destination in replaced.iter().rev() {
            let restore = || -> Result<()> {
                let Some((_, rollback_path)) =
                    rollback.iter().find(|(path, _)| path == destination)
                else {
                    return Ok(());
                };
                let Some(rollback_path) = rollback_path else {
                    let _ = fs::remove_file(destination);
                    return Ok(());
                };
                let restore_prefix = temp_name(destination, "");
                let restore_file = tempfile::Builder::new()
                    .prefix(&restore_prefix)
                    .suffix(".restore")
                    .tempfile_in(destination.parent().unwrap_or_else(|| Path::new(".")))?;
                let restore_path = restore_file.into_temp_path();
                fs::copy(rollback_path, &restore_path)?;
                restore_path
                    .persist(destination)
                    .map_err(|error| anyhow!("{error}"))?;
                Ok(())
            };
            let _ = restore();
        }
        return Err(error);
    }
    Ok(())
}

/// `_extract_singbox_rule_sets`: staged rule-set replacements from the
/// RuNetFreedom sing-box archive.
fn extract_singbox_rule_sets(
    archive: &Path,
    staging_dir: &Path,
    target_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    fs::create_dir_all(staging_dir)?;
    let mut replacements = Vec::new();
    let mut zip_file = zip::ZipArchive::new(fs::File::open(archive)?)?;
    let names: BTreeSet<String> = (0..zip_file.len())
        .filter_map(|index| {
            zip_file
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .collect();
    let mut missing: Vec<&str> = SINGBOX_BINARY_RULE_SETS
        .iter()
        .map(|(_, member)| *member)
        .filter(|member| !names.contains(*member))
        .collect();
    missing.sort_unstable();
    if !missing.is_empty() {
        bail!(
            "в архиве sing-box отсутствуют rule-set: {}",
            missing.join(", ")
        );
    }
    let mut total: u64 = 0;
    for (_, member) in SINGBOX_BINARY_RULE_SETS {
        let size = zip_file.by_name(member)?.size();
        if size > MAX_RULE_SET_MEMBER_BYTES {
            bail!("rule-set {member} слишком большой");
        }
        total += size;
        if total > MAX_RULE_SET_TOTAL_BYTES {
            bail!("архив rule-set превышает допустимый размер");
        }
        let mut payload = Vec::new();
        zip_file.by_name(member)?.read_to_end(&mut payload)?;
        if payload.len() < 64 {
            bail!("rule-set {member} выглядит повреждённым");
        }
        let file_name = Path::new(member)
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_default();
        let source = staging_dir.join(&file_name);
        fs::write(&source, &payload)?;
        replacements.push((source, target_dir.join(file_name)));
    }
    Ok(replacements)
}

/// `_install_singbox_rule_sets`: replace the active binary rule-sets.
pub fn install_singbox_rule_sets(archive: &Path, target_dir: Option<&Path>) -> Result<()> {
    let target = target_dir
        .map(|path| path.to_path_buf())
        .unwrap_or_else(singbox_rule_set_dir);
    let temp = tempfile::Builder::new()
        .prefix("singbox_rules_stage_")
        .tempdir()?;
    let replacements = extract_singbox_rule_sets(archive, temp.path(), &target)?;
    atomic_replace_files(&replacements, &[], None)
}

// ── sing-box release assets ─────────────────────────────────────────────────

/// `_pick_singbox_asset`: the windows-amd64 extended archive.
fn pick_singbox_asset(release: &Value) -> (String, String) {
    let assets = release.get("assets").and_then(Value::as_array);
    if let Some(assets) = assets {
        for asset in assets {
            let name = asset_name_of(asset);
            let lower = name.to_lowercase();
            if lower.ends_with(".zip")
                && lower.contains("windows-amd64.zip")
                && !lower.contains("purego")
            {
                return (name, asset_url_of(asset));
            }
        }
        for asset in assets {
            let name = asset_name_of(asset);
            let lower = name.to_lowercase();
            if lower.ends_with(".zip")
                && lower.contains("windows")
                && (lower.contains("amd64") || lower.contains("x64"))
            {
                return (name, asset_url_of(asset));
            }
        }
    }
    (String::new(), String::new())
}

/// `_pick_singbox_cronet_asset`: the Windows purego archive that carries
/// libcronet.dll. Upstream's regular windows-amd64 archive contains only
/// sing-box.exe; the Naive outbound loads Cronet dynamically and the companion
/// DLL is shipped in the purego archive of the same extended release.
fn pick_singbox_cronet_asset(release: &Value) -> (String, String) {
    if let Some(assets) = release.get("assets").and_then(Value::as_array) {
        for asset in assets {
            let name = asset_name_of(asset);
            let lower = name.to_lowercase();
            if lower.ends_with(".zip") && lower.contains("windows-amd64-purego.zip") {
                return (name, asset_url_of(asset));
            }
        }
    }
    (String::new(), String::new())
}

fn ensure_zip_file(path: &Path, label: &str) -> Result<()> {
    let is_zip = fs::File::open(path)
        .map(|file| zip::ZipArchive::new(file).is_ok())
        .unwrap_or(false);
    if !is_zip {
        let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        bail!("{label}: скачанный файл не является zip-архивом ({size} байт)");
    }
    Ok(())
}

fn require_https(url: &str, label: &str) -> Result<()> {
    let scheme = url
        .split_once("://")
        .map(|(scheme, _)| scheme.to_lowercase())
        .unwrap_or_default();
    if scheme != "https" {
        bail!("{label}: загрузка допускается только по https");
    }
    Ok(())
}

/// Reject a rule-set that is not a complete sing-box binary rule-set.
///
/// Unlike the russia archive these files carry no published SHA-256, so the
/// format check is the only defence against a truncated or substituted body.
fn ensure_rule_set_file(path: &Path, key: &str) -> Result<()> {
    let probe = (|| -> Result<(u64, Vec<u8>)> {
        let meta = fs::metadata(path)?;
        let mut file = fs::File::open(path)?;
        let mut header = vec![0u8; SRS_MAGIC.len()];
        let mut read = 0;
        while read < header.len() {
            let count = file.read(&mut header[read..])?;
            if count == 0 {
                break;
            }
            read += count;
        }
        header.truncate(read);
        Ok((meta.len(), header))
    })()
    .map_err(|exc| anyhow!("rule-set {key} недоступен: {exc}"))?;
    let (size, header) = probe;
    if size < 64 || !header.starts_with(SRS_MAGIC) {
        bail!("rule-set {key} выглядит повреждённым");
    }
    Ok(())
}

// ── droute ──────────────────────────────────────────────────────────────────

/// `check_or_update_droute`: verify the droute release and optionally install.
pub async fn check_or_update_droute(
    apply_update: bool,
    on_progress: Option<&mut (dyn FnMut(u64, u64) + Send)>,
    proxy_url: Option<&str>,
    bundle_dir: Option<&Path>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled> {
    let mut on_progress = on_progress;
    let flow = droute_flow(
        apply_update,
        &mut on_progress,
        proxy_url,
        bundle_dir,
        cancellation,
    )
    .await;
    match flow {
        Ok(result) => Ok(result),
        Err(error) if error.is::<UpdateCancelled>() => Err(UpdateCancelled),
        Err(error) => Ok(ResourceUpdateResult::error(
            "droute",
            format!("Не удалось обновить droute: {error:#}"),
        )),
    }
}

async fn droute_flow(
    apply_update: bool,
    on_progress: &mut Option<&mut (dyn FnMut(u64, u64) + Send)>,
    proxy_url: Option<&str>,
    bundle_dir: Option<&Path>,
    cancellation: &UpdateCancellation,
) -> Result<ResourceUpdateResult> {
    cancellation.check()?;
    let target: PathBuf = match bundle_dir {
        Some(dir) => dir.to_path_buf(),
        None => droute_dir(),
    };
    let mut current = if bundle_dir.is_none() {
        get_droute_bundle_version()
    } else {
        String::new()
    };
    if bundle_dir.is_some() && target.join("droute.exe").is_file() {
        current = match fs::read_to_string(target.join("version.txt")) {
            Ok(text) => text.trim().trim_start_matches('v').to_string(),
            Err(_) => DROUTE_LEGACY_VERSION.to_string(),
        };
    }
    let mut latest = String::new();
    let flow = async {
        cancellation.check()?;
        let release = request_json(DROUTE_LATEST_API, proxy_url, cancellation).await?;
        latest = str_field(&release, "tag_name")
            .trim_start_matches('v')
            .to_string();
        if !DROUTE_VERSION_RE.is_match(&latest) {
            bail!("GitHub returned an invalid droute version");
        }
        let asset = release["assets"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| str_field(item, "name") == "droute.zip")
            })
            .cloned()
            .context("droute.zip is missing from the release")?;
        let digest = extract_digest(&str_field(&asset, "digest"));
        if digest.is_empty() {
            bail!("droute archive has no published SHA-256");
        }
        let verified = verify_bundled_droute(&target).is_ok();
        if !current.is_empty() && verified && !is_newer(&latest, &current) {
            return Ok(ResourceUpdateResult::with_versions(
                "droute",
                ResourceUpdateStatus::UpToDate,
                format!("droute актуален ({current})"),
                current.clone(),
                latest.clone(),
            ));
        }
        if !apply_update {
            return Ok(ResourceUpdateResult::with_versions(
                "droute",
                ResourceUpdateStatus::Available,
                format!("Доступен droute {latest}"),
                current.clone(),
                latest.clone(),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = tempfile::Builder::new()
            .prefix("droute-update-")
            .tempdir_in(target.parent().unwrap_or_else(|| Path::new(".")))?;
        let work = temporary.path().to_path_buf();
        let archive = work.join("droute.zip");
        let staged = work.join("new");
        {
            let sink = |done: u64, total: u64| {
                if let Some(callback) = on_progress.as_deref_mut() {
                    callback(done, total);
                }
            };
            download_file(
                &str_field(&asset, "browser_download_url"),
                &archive,
                proxy_url,
                sink,
                cancellation,
            )
            .await?;
        }
        if sha256_file(&archive, cancellation)?.to_lowercase() != digest.to_lowercase() {
            bail!("droute archive SHA-256 mismatch");
        }
        let mut package = zip::ZipArchive::new(fs::File::open(&archive)?)?;
        safe_extract_zip(&mut package, &staged)?;
        let executables = find_files_recursive(&staged, "droute.exe");
        let valid = executables.len() == 1
            && fs::metadata(&executables[0])
                .map(|meta| meta.len() >= 1024)
                .unwrap_or(false);
        if !valid {
            bail!("droute archive contains no valid executable");
        }
        let payload = executables[0]
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| staged.clone());
        fs::write(payload.join("version.txt"), format!("{latest}\n"))?;
        let mut payload_files: Vec<PathBuf> = Vec::new();
        collect_all_recursive(&payload, &mut payload_files);
        payload_files.sort_by_key(|path| path.to_string_lossy().to_string());
        let mut sums: Vec<String> = Vec::new();
        for file in payload_files {
            if !file.is_file()
                || file
                    .file_name()
                    .map(|name| name == "SHA256SUMS.txt")
                    .unwrap_or(false)
            {
                continue;
            }
            let relative = file
                .strip_prefix(&payload)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            sums.push(format!("{}  {relative}", sha256_file(&file, cancellation)?));
        }
        fs::write(
            payload.join("SHA256SUMS.txt"),
            format!("{}\n", sums.join("\n")),
        )?;
        cancellation.check()?;
        let backup = work.join("previous");
        if target.exists() {
            fs::rename(&target, &backup)?;
        }
        if let Err(error) = fs::rename(&payload, &target) {
            if backup.exists() {
                let _ = fs::rename(&backup, &target);
            }
            return Err(error.into());
        }
        Ok(ResourceUpdateResult::with_versions(
            "droute",
            ResourceUpdateStatus::Updated,
            format!("droute обновлен до {latest}"),
            current.clone(),
            latest.clone(),
        ))
    };
    match flow.await {
        Ok(result) => Ok(result),
        Err(error) => {
            let message = format!("Не удалось обновить droute: {error:#}");
            Ok(ResourceUpdateResult::with_versions(
                "droute",
                ResourceUpdateStatus::Error,
                message,
                current,
                latest,
            ))
        }
    }
}

// ── sing-box ────────────────────────────────────────────────────────────────

enum SingboxInstallOutcome {
    Refreshed(String),
    Cancelled,
    StopFailed(anyhow::Error),
    Generic(anyhow::Error),
}

/// `check_or_update_singbox`: verify and optionally install the extended core.
pub async fn check_or_update_singbox<F>(
    singbox_path: &str,
    apply_update: bool,
    mut on_progress: Option<&mut (dyn FnMut(u64, u64) + Send)>,
    on_install_start: impl FnOnce() -> F,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled>
where
    F: std::future::Future<Output = Result<()>>,
{
    if cancellation.check().is_err() {
        return Err(UpdateCancelled);
    }
    let default_path = singbox_path_default();
    let exe = resolve_configured_path(singbox_path, Some(&default_path), true, true)
        .unwrap_or_else(|| default_path.clone());
    let current_text = get_singbox_version(&exe.to_string_lossy()).unwrap_or_default();
    let current = extract_singbox_version(&current_text);

    // Stage 1: release discovery; failures become "Не удалось проверить sing-box".
    let (release, asset_name, url, cronet_asset_name, cronet_url, latest) = match async {
        cancellation.check()?;
        let release = request_json(SINGBOX_EXTENDED_LATEST_API, proxy_url, cancellation).await?;
        if !release.is_object() {
            bail!("GitHub вернул неожиданный ответ");
        }
        let (asset_name, url) = pick_singbox_asset(&release);
        let (cronet_asset_name, cronet_url) = pick_singbox_cronet_asset(&release);
        let tag = {
            let tag = str_field(&release, "tag_name");
            if tag.is_empty() {
                str_field(&release, "name")
            } else {
                tag
            }
        };
        let latest = extract_singbox_version(&tag);
        if url.is_empty() {
            bail!("не найден windows-amd64 архив sing-box extended");
        }
        if cronet_url.is_empty() {
            bail!("не найден windows-amd64-purego архив с libcronet.dll");
        }
        Ok((
            release,
            asset_name,
            url,
            cronet_asset_name,
            cronet_url,
            latest,
        ))
    }
    .await
    {
        Ok(values) => values,
        Err(error) if error.is::<UpdateCancelled>() => return Err(UpdateCancelled),
        Err(error) => {
            return Ok(ResourceUpdateResult::with_versions(
                "singbox",
                ResourceUpdateStatus::Error,
                format!("Не удалось проверить sing-box: {error:#}"),
                current,
                "",
            ));
        }
    };

    let cronet_target = exe.with_file_name("libcronet.dll");
    let cronet_size = fs::metadata(&cronet_target)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let cronet_missing = !cronet_target.is_file() || cronet_size < 1024;
    let same_core_version = !current.is_empty()
        && !latest.is_empty()
        && CORE_VERSION_RE.is_match(&current)
        && CORE_VERSION_RE.is_match(&latest)
        && CORE_VERSION_RE.find(&current).map(|found| found.as_str())
            == CORE_VERSION_RE.find(&latest).map(|found| found.as_str());
    let repair_cronet_only = cronet_missing && same_core_version;
    if !current.is_empty()
        && !latest.is_empty()
        && !is_newer(&latest, &current)
        && !repair_cronet_only
    {
        return Ok(ResourceUpdateResult::with_versions(
            "singbox",
            ResourceUpdateStatus::UpToDate,
            format!("sing-box актуален ({current})"),
            current,
            latest,
        ));
    }
    if repair_cronet_only && !apply_update {
        return Ok(ResourceUpdateResult::with_versions(
            "singbox",
            ResourceUpdateStatus::Available,
            "Для поддержки NaiveProxy требуется восстановить libcronet.dll",
            current,
            latest,
        ));
    }
    if is_lumen_singbox_build(&current_text) && !repair_cronet_only {
        return Ok(ResourceUpdateResult::with_versions(
            "singbox",
            ResourceUpdateStatus::Available,
            format!(
                "Доступен sing-box extended {latest}. Совместимое ядро \
                 устанавливается вместе с обновлением Lumen"
            ),
            current,
            latest,
        ));
    }
    if !apply_update {
        return Ok(ResourceUpdateResult::with_versions(
            "singbox",
            ResourceUpdateStatus::Available,
            format!("Доступен sing-box extended {latest}"),
            current,
            latest,
        ));
    }

    // Stage 2: digests; failures become "Не удалось проверить архив sing-box".
    let (expected_hash, cronet_expected_hash) = match async {
        cancellation.check()?;
        let expected_hash =
            singbox_asset_digest(&release, &asset_name, proxy_url, cancellation).await?;
        if expected_hash.is_empty() {
            bail!("для архива sing-box отсутствует SHA-256");
        }
        let cronet_expected_hash =
            singbox_asset_digest(&release, &cronet_asset_name, proxy_url, cancellation).await?;
        if cronet_expected_hash.is_empty() {
            bail!("для архива libcronet отсутствует SHA-256");
        }
        Ok((expected_hash, cronet_expected_hash))
    }
    .await
    {
        Ok(values) => values,
        Err(error) if error.is::<UpdateCancelled>() => return Err(UpdateCancelled),
        Err(error) => {
            return Ok(ResourceUpdateResult::with_versions(
                "singbox",
                ResourceUpdateStatus::Error,
                format!("Не удалось проверить архив sing-box: {error:#}"),
                current,
                latest,
            ));
        }
    };

    // Stage 3: download, verify, stop services, stage and validate the install.
    let mut install_start = Some(on_install_start);
    let outcome: SingboxInstallOutcome = async {
        let temp = match tempfile::Builder::new().prefix("singbox_update_").tempdir() {
            Ok(temp) => temp,
            Err(error) => return SingboxInstallOutcome::Generic(error.into()),
        };
        let temp_dir = temp.path().to_path_buf();
        let archive = temp_dir.join(safe_asset_name(&asset_name, "sing-box-windows-amd64.zip"));
        if !repair_cronet_only {
            let sink = |done: u64, total: u64| {
                if let Some(callback) = on_progress.as_deref_mut() {
                    callback(done, total);
                }
            };
            // download_file races the cancellation internally.
            if let Err(error) = download_file(&url, &archive, proxy_url, sink, cancellation).await {
                return if error.is::<UpdateCancelled>() {
                    SingboxInstallOutcome::Cancelled
                } else {
                    SingboxInstallOutcome::Generic(error)
                };
            }
            if let Err(error) = ensure_zip_file(&archive, "sing-box") {
                return SingboxInstallOutcome::Generic(error);
            }
            let actual = match sha256_file(&archive, cancellation) {
                Ok(actual) => actual.to_lowercase(),
                Err(error) if error.is::<UpdateCancelled>() => {
                    return SingboxInstallOutcome::Cancelled;
                }
                Err(error) => return SingboxInstallOutcome::Generic(error),
            };
            if actual != expected_hash.to_lowercase() {
                return SingboxInstallOutcome::Generic(anyhow!(
                    "контрольная сумма архива sing-box не совпадает"
                ));
            }
        }
        let cronet_archive = temp_dir.join(safe_asset_name(
            &cronet_asset_name,
            "sing-box-windows-amd64-purego.zip",
        ));
        let sink = |done: u64, total: u64| {
            // Python passes on_progress only when repairing cronet only.
            if repair_cronet_only && let Some(callback) = on_progress.as_deref_mut() {
                callback(done, total);
            }
        };
        if let Err(error) =
            download_file(&cronet_url, &cronet_archive, proxy_url, sink, cancellation).await
        {
            return if error.is::<UpdateCancelled>() {
                SingboxInstallOutcome::Cancelled
            } else {
                SingboxInstallOutcome::Generic(error)
            };
        }
        if let Err(error) = ensure_zip_file(&cronet_archive, "libcronet") {
            return SingboxInstallOutcome::Generic(error);
        }
        let actual = match sha256_file(&cronet_archive, cancellation) {
            Ok(actual) => actual.to_lowercase(),
            Err(error) if error.is::<UpdateCancelled>() => return SingboxInstallOutcome::Cancelled,
            Err(error) => return SingboxInstallOutcome::Generic(error),
        };
        if actual != cronet_expected_hash.to_lowercase() {
            return SingboxInstallOutcome::Generic(anyhow!(
                "контрольная сумма архива libcronet не совпадает"
            ));
        }
        let extract_dir = temp_dir.join("extract");
        let cronet_extract_dir = temp_dir.join("cronet");
        let prepare = (|| -> Result<Option<PathBuf>> {
            fs::create_dir_all(&extract_dir)?;
            fs::create_dir_all(&cronet_extract_dir)?;
            let mut new_exe: Option<PathBuf> = None;
            if !repair_cronet_only {
                let mut zip_file = zip::ZipArchive::new(fs::File::open(&archive)?)?;
                safe_extract_zip(&mut zip_file, &extract_dir)?;
                new_exe = find_files_recursive(&extract_dir, "sing-box.exe")
                    .into_iter()
                    .next();
                let Some(new_exe_path) = new_exe.clone() else {
                    bail!("sing-box.exe не найден в архиве");
                };
                let candidate_version = extract_singbox_version(
                    &get_singbox_version(&new_exe_path.to_string_lossy()).unwrap_or_default(),
                );
                ensure_component_compatible("singbox", &candidate_version)?;
                let latest_core = CORE_VERSION_RE.find(&latest).map(|found| found.as_str());
                let candidate_core = CORE_VERSION_RE
                    .find(&candidate_version)
                    .map(|found| found.as_str());
                if let (Some(latest_core), Some(candidate_core)) = (latest_core, candidate_core)
                    && latest_core != candidate_core
                {
                    bail!("архив содержит sing-box {candidate_version}, ожидалась версия {latest}");
                }
            }
            let mut cronet_zip = zip::ZipArchive::new(fs::File::open(&cronet_archive)?)?;
            safe_extract_zip(&mut cronet_zip, &cronet_extract_dir)?;
            let new_cronet = find_files_recursive(&cronet_extract_dir, "libcronet.dll")
                .into_iter()
                .next();
            let cronet_ok = new_cronet
                .as_ref()
                .map(|path| {
                    fs::metadata(path)
                        .map(|meta| meta.len() >= 1024)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if new_cronet.is_none() || !cronet_ok {
                bail!("libcronet.dll не найден или поврежден в архиве");
            }
            fs::create_dir_all(exe.parent().unwrap_or_else(|| Path::new(".")))?;
            Ok(new_exe)
        })();
        let new_exe = match prepare {
            Ok(new_exe) => new_exe,
            Err(error) => return SingboxInstallOutcome::Generic(error),
        };
        let new_cronet = find_files_recursive(&cronet_extract_dir, "libcronet.dll")
            .into_iter()
            .next();

        // Stop the services that hold the core before replacing it.
        if let Some(start) = install_start.take() {
            match start().await {
                Ok(()) => {}
                Err(error) if error.is::<UpdateCancelled>() => {
                    return SingboxInstallOutcome::Cancelled;
                }
                Err(error) => return SingboxInstallOutcome::StopFailed(error),
            }
        }

        let holder = Arc::new(ParkingMutex::new(None::<String>));
        let mut validate = {
            let holder = holder.clone();
            let exe = exe.clone();
            let cronet_target = cronet_target.clone();
            let latest = latest.clone();
            move || -> Result<()> {
                let refreshed = extract_singbox_version(
                    &get_singbox_version(&exe.to_string_lossy()).unwrap_or_default(),
                );
                if refreshed.is_empty() {
                    bail!("установленный sing-box не запускается");
                }
                ensure_component_compatible("singbox", &refreshed)?;
                let latest_core = CORE_VERSION_RE.find(&latest).map(|found| found.as_str());
                let refreshed_core = CORE_VERSION_RE.find(&refreshed).map(|found| found.as_str());
                if let (Some(latest_core), Some(refreshed_core)) = (latest_core, refreshed_core)
                    && latest_core != refreshed_core
                {
                    bail!("установлен sing-box {refreshed}, ожидалась версия {latest}");
                }
                *holder.lock() = Some(refreshed);
                let cronet_size = fs::metadata(&cronet_target)
                    .map(|meta| meta.len())
                    .unwrap_or(0);
                if !cronet_target.is_file() || cronet_size < 1024 {
                    bail!("libcronet.dll не установлен");
                }
                Ok(())
            }
        };
        let mut replacements: Vec<(PathBuf, PathBuf)> = Vec::new();
        if let Some(new_exe) = &new_exe {
            replacements.push((new_exe.clone(), exe.clone()));
        }
        if let Some(new_cronet) = new_cronet {
            replacements.push((new_cronet, cronet_target.clone()));
        }
        let backup_targets = vec![exe.clone()];
        if let Err(error) =
            atomic_replace_files(&replacements, &backup_targets, Some(&mut validate))
        {
            return SingboxInstallOutcome::Generic(error);
        }
        match holder.lock().clone() {
            Some(refreshed) => SingboxInstallOutcome::Refreshed(refreshed),
            None => {
                SingboxInstallOutcome::Generic(anyhow!("установленный sing-box не запускается"))
            }
        }
    }
    .await;
    let refreshed = match outcome {
        SingboxInstallOutcome::Refreshed(refreshed) => refreshed,
        SingboxInstallOutcome::Cancelled => return Err(UpdateCancelled),
        SingboxInstallOutcome::StopFailed(error) => {
            return Ok(ResourceUpdateResult::with_versions(
                "singbox",
                ResourceUpdateStatus::Error,
                format!("Не удалось остановить службы перед обновлением: {error:#}"),
                current,
                latest,
            ));
        }
        SingboxInstallOutcome::Generic(error) => {
            return Ok(ResourceUpdateResult::with_versions(
                "singbox",
                ResourceUpdateStatus::Error,
                format!("Не удалось обновить sing-box: {error:#}"),
                current,
                latest,
            ));
        }
    };
    let message = if repair_cronet_only {
        format!("Компонент NaiveProxy для sing-box {refreshed} восстановлен")
    } else {
        format!("sing-box обновлен до {refreshed}")
    };
    Ok(ResourceUpdateResult::with_versions(
        "singbox",
        ResourceUpdateStatus::Updated,
        message,
        current,
        refreshed,
    ))
}

// ── geodata ─────────────────────────────────────────────────────────────────

/// `update_geodata`: download, verify, validate through Xray and replace the
/// active geodata plus the regional sing-box rule-sets.
pub async fn update_geodata<F>(
    mut on_progress: Option<&mut (dyn FnMut(u64, u64) + Send)>,
    region: &str,
    on_install_start: impl FnOnce() -> F,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled>
where
    F: std::future::Future<Output = Result<()>>,
{
    if cancellation.check().is_err() {
        return Err(UpdateCancelled);
    }
    let regional = regional_singbox_rule_sets();
    let mut region = region.trim().to_lowercase();
    if !regional.contains_key(&region) {
        region = "russia".to_string();
    }
    let target_dir = user_geodata_dir();
    if let Err(error) = fs::create_dir_all(&target_dir) {
        return Ok(ResourceUpdateResult::with_versions(
            "geodata",
            ResourceUpdateStatus::Error,
            format!("Не удалось обновить geoip/geosite: {error}"),
            "",
            "",
        ));
    }
    let active_dir = active_geodata_dir();
    let current_version = str_field(&read_geodata_metadata(&active_dir), "version");
    let rules_dir = target_dir.join("rule-sets");
    let mut latest_version = String::new();
    let mut install_start = Some(on_install_start);
    let flow = async {
        let (version, release_assets) =
            resolve_geodata_release(&region, proxy_url, cancellation).await?;
        latest_version = version;
        if geodata_install_matches(
            &active_dir,
            &latest_version,
            &release_assets,
            &region,
            &active_dir.join("rule-sets"),
        ) {
            return Ok(ResourceUpdateResult::with_versions(
                "geodata",
                ResourceUpdateStatus::UpToDate,
                "geoip/geosite актуальны",
                current_version.clone(),
                latest_version.clone(),
            ));
        }
        let temp = tempfile::Builder::new()
            .prefix("geodata_update_")
            .tempdir()?;
        let temp_dir = temp.path().to_path_buf();
        let geoip = temp_dir.join("geoip.dat");
        let geosite = temp_dir.join("geosite.dat");
        for (offset, name, path) in [(0, "geoip.dat", &geoip), (25, "geosite.dat", &geosite)] {
            let (url, expected) =
                release_asset(&release_assets, name).with_context(|| format!("missing {name}"))?;
            let validate = |file: &Path| {
                if !sha256_file(file, cancellation)?.eq_ignore_ascii_case(expected) {
                    bail!("SHA-256 mismatch for {name}");
                }
                if fs::metadata(file)?.len() < 1024 {
                    bail!("invalid geodata file: {name}");
                }
                Ok(())
            };
            let mut sink = scaled_sink(offset, 25, &mut on_progress);
            if region == "china" {
                let fallback = if url.starts_with(CHINA_GEODATA_CDN) {
                    format!("{CHINA_GEODATA_RELEASE}/{name}")
                } else {
                    format!("{CHINA_GEODATA_CDN}/{name}")
                };
                download_china_geodata(
                    [url, &fallback],
                    path,
                    &mut sink,
                    proxy_url,
                    cancellation,
                    validate,
                )
                .await?;
            } else {
                download_direct(url, path, Some(&mut sink), proxy_url, cancellation).await?;
                validate(path)?;
            }
        }
        let mut rule_replacements: Vec<(PathBuf, PathBuf)> = Vec::new();
        if region == "russia" {
            let singbox_rules = temp_dir.join("sing-box.zip");
            {
                let mut sink = scaled_sink(50, 50, &mut on_progress);
                download_direct(
                    &release_asset(&release_assets, "sing-box.zip")
                        .context("missing sing-box.zip")?
                        .0,
                    &singbox_rules,
                    Some(&mut sink),
                    proxy_url,
                    cancellation,
                )
                .await?;
            }
            let expected = release_asset(&release_assets, "sing-box.zip")
                .context("missing sing-box.zip")?
                .1
                .to_lowercase();
            if sha256_file(&singbox_rules, cancellation)?.to_lowercase() != expected {
                bail!("SHA-256 mismatch for sing-box.zip");
            }
            ensure_zip_file(&singbox_rules, "sing-box rules")?;
            rule_replacements = extract_singbox_rule_sets(
                &singbox_rules,
                &temp_dir.join("rule-sets"),
                &rules_dir,
            )?;
        } else {
            let sources: Vec<(String, String)> = regional
                .get(&region)
                .context("unknown region")?
                .iter()
                .map(|(key, url)| (key.clone(), url.clone()))
                .collect();
            let staging = temp_dir.join("rule-sets");
            fs::create_dir_all(&staging)?;
            let count = sources.len().max(1) as i64;
            for (index, (key, url)) in sources.iter().enumerate() {
                let filename = format!("{}.srs", key.replace(':', "-"));
                let source = staging.join(&filename);
                let offset = 50 + (index as i64 * 50 / count);
                let span = (50 / count).max(1);
                require_https(url, &format!("rule-set {key}"))?;
                let mut sink = scaled_sink(offset, span, &mut on_progress);
                if let Some(cdn) = china_rule_set_cdn_url(url) {
                    download_china_geodata(
                        [&cdn, url],
                        &source,
                        &mut sink,
                        proxy_url,
                        cancellation,
                        |path| ensure_rule_set_file(path, key),
                    )
                    .await?;
                } else {
                    download_direct(url, &source, Some(&mut sink), proxy_url, cancellation).await?;
                    ensure_rule_set_file(&source, key)?;
                }
                rule_replacements.push((source, rules_dir.join(&filename)));
            }
        }
        let mut replacements: Vec<(PathBuf, PathBuf)> = vec![
            (geoip.clone(), target_dir.join("geoip.dat")),
            (geosite.clone(), target_dir.join("geosite.dat")),
        ];
        replacements.extend(rule_replacements.iter().cloned());
        validate_geodata_with_xray(&temp_dir, &region)?;
        let mut sha_map = serde_json::Map::new();
        for (name, (_url, digest)) in &release_assets {
            sha_map.insert(name.clone(), Value::String(digest.clone()));
        }
        let mut rule_sha_map = serde_json::Map::new();
        for (source, destination) in &rule_replacements {
            let name = destination
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            rule_sha_map.insert(name, Value::String(sha256_file(source, cancellation)?));
        }
        let rule_sets_value = regional.get(&region).cloned().unwrap_or_default();
        let rule_sets_object: serde_json::Map<String, Value> = rule_sets_value
            .into_iter()
            .map(|(key, source)| (key, Value::String(source)))
            .collect();
        let metadata = json!({
            "schema": 3,
            "region": region,
            "version": latest_version,
            "rule_sets": Value::Object(rule_sets_object),
            "sha256": Value::Object(sha_map),
            "rule_set_sha256": Value::Object(rule_sha_map),
        });
        let metadata_path = temp_dir.join(GEODATA_METADATA_NAME);
        fs::write(&metadata_path, pyjson::dumps(&metadata, true, Some(2)))?;
        replacements.push((metadata_path, target_dir.join(GEODATA_METADATA_NAME)));
        if let Some(start) = install_start.take() {
            match start().await {
                Ok(()) => {}
                Err(error) if error.is::<UpdateCancelled>() => bail!(UpdateCancelled),
                Err(error) => {
                    bail!("Не удалось остановить службы перед обновлением geoip/geosite: {error:#}")
                }
            }
        }
        cancellation.check()?;
        let backup_targets = vec![target_dir.join("geoip.dat"), target_dir.join("geosite.dat")];
        atomic_replace_files(&replacements, &backup_targets, None)?;
        Ok(ResourceUpdateResult::with_versions(
            "geodata",
            ResourceUpdateStatus::Updated,
            format!("geoip.dat и geosite.dat обновлены до {latest_version}"),
            current_version.clone(),
            latest_version.clone(),
        ))
    };
    match flow.await {
        Ok(result) => Ok(result),
        Err(error) if error.is::<UpdateCancelled>() => Err(UpdateCancelled),
        Err(error) => Ok(ResourceUpdateResult::with_versions(
            "geodata",
            ResourceUpdateStatus::Error,
            format!("Не удалось обновить geoip/geosite: {error:#}"),
            current_version,
            latest_version,
        )),
    }
}

fn bundled_geodata(region: &str) -> Option<PathBuf> {
    let bundle = match region {
        "china" => core_dir().join("geodata").join("china"),
        "russia" => core_dir(),
        _ => return None,
    };
    (int_field(&read_geodata_metadata(&bundle), "schema", 0) >= 3
        && regional_geodata_installed(region, Some(&bundle), None))
    .then_some(bundle)
}

async fn install_bundled_geodata(
    bundle: &Path,
    region: &str,
    on_install_start: impl std::future::Future<Output = Result<()>>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled> {
    let target = user_geodata_dir();
    let version = str_field(&read_geodata_metadata(bundle), "version");
    let current = str_field(&read_geodata_metadata(&active_geodata_dir()), "version");
    let result: Result<()> = async {
        cancellation.check()?;
        let rules = regional_singbox_rule_sets();
        let mut files: Vec<PathBuf> = rules[region]
            .iter()
            .map(|(key, source)| Path::new("rule-sets").join(rule_set_filename(key, source)))
            .collect();
        files.extend(["geoip.dat", "geosite.dat", GEODATA_METADATA_NAME].map(PathBuf::from));
        let replacements: Vec<_> = files
            .iter()
            .map(|file| (bundle.join(file), target.join(file)))
            .collect();
        validate_geodata_with_xray(bundle, region)?;
        on_install_start.await?;
        cancellation.check()?;
        let backups = [target.join("geoip.dat"), target.join("geosite.dat")];
        atomic_replace_files(&replacements, &backups, None)
    }
    .await;
    match result {
        Ok(()) => Ok(ResourceUpdateResult::with_versions(
            "geodata",
            ResourceUpdateStatus::Updated,
            format!("geoip.dat и geosite.dat обновлены до {version}"),
            current,
            version,
        )),
        Err(error) if error.is::<UpdateCancelled>() => Err(UpdateCancelled),
        Err(error) => Ok(ResourceUpdateResult::error(
            "geodata",
            format!("Не удалось обновить geoip/geosite: {error:#}"),
        )),
    }
}

/// `check_geodata_update`: check the verified release identity without
/// downloading payloads.
pub async fn check_geodata_update(
    region: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled> {
    check_geodata_release_identity(&active_geodata_dir(), region, proxy_url, cancellation).await
}

async fn check_geodata_release_identity(
    target_dir: &Path,
    region: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> std::result::Result<ResourceUpdateResult, UpdateCancelled> {
    let outcome: Result<(String, String, bool)> = async {
        cancellation.check()?;
        let (latest_version, release_assets) =
            resolve_geodata_release(region, proxy_url, cancellation).await?;
        let metadata = read_geodata_metadata(target_dir);
        let current_version = str_field(&metadata, "version");
        let changed = !geodata_install_matches(
            target_dir,
            &latest_version,
            &release_assets,
            region,
            &target_dir.join("rule-sets"),
        );
        Ok((current_version, latest_version, changed))
    }
    .await;
    match outcome {
        Ok((current_version, latest_version, changed)) => Ok(if changed {
            ResourceUpdateResult::with_versions(
                "geodata",
                ResourceUpdateStatus::Available,
                "Доступно обновление geoip/geosite",
                current_version,
                latest_version,
            )
        } else {
            ResourceUpdateResult::with_versions(
                "geodata",
                ResourceUpdateStatus::UpToDate,
                "geoip/geosite актуальны",
                current_version,
                latest_version,
            )
        }),
        Err(error) if error.is::<UpdateCancelled>() => Err(UpdateCancelled),
        Err(error) => Ok(ResourceUpdateResult::error(
            "geodata",
            format!("Не удалось проверить geoip/geosite: {error:#}"),
        )),
    }
}

/// `_geodata_install_matches` against the real sing-box rule-set directory.
fn geodata_install_matches(
    target_dir: &Path,
    latest_version: &str,
    release_assets: &ReleaseAssets,
    region: &str,
    rule_set_dir: &Path,
) -> bool {
    let metadata = read_geodata_metadata(target_dir);
    if int_field(&metadata, "schema", 0) != 3 {
        return false;
    }
    let installed_region = {
        let value = str_field(&metadata, "region");
        if value.is_empty() {
            "russia".to_string()
        } else {
            value.trim().to_lowercase()
        }
    };
    let expected_region = if region.trim().is_empty() {
        "russia".to_string()
    } else {
        region.trim().to_lowercase()
    };
    if installed_region != expected_region {
        return false;
    }
    // GitHub tags and CDN checksums identify the same China data independently.
    if expected_region != "china" && str_field(&metadata, "version") != latest_version {
        return false;
    }
    let Some(recorded_hashes) = metadata.get("sha256").filter(|value| value.is_object()) else {
        return false;
    };
    for (name, (_url, expected_hash)) in release_assets {
        if recorded_hashes
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
            != expected_hash.to_lowercase()
        {
            return false;
        }
        if name == "geoip.dat" || name == "geosite.dat" {
            let path = target_dir.join(name);
            let cancellation = UpdateCancellation::default();
            if !path.is_file()
                || sha256_file(&path, &cancellation)
                    .unwrap_or_default()
                    .to_lowercase()
                    != expected_hash.to_lowercase()
            {
                return false;
            }
        }
    }
    let regional = regional_singbox_rule_sets();
    let expected_rules = regional
        .get(expected_region.as_str())
        .cloned()
        .unwrap_or_else(|| regional["russia"].clone());
    let expected_rules_object: serde_json::Map<String, Value> = expected_rules
        .iter()
        .map(|(key, source)| (key.clone(), Value::String(source.clone())))
        .collect();
    if metadata.get("rule_sets").cloned().unwrap_or(Value::Null)
        != Value::Object(expected_rules_object)
    {
        return false;
    }
    let Some(recorded_rule_hashes) = metadata
        .get("rule_set_sha256")
        .filter(|value| value.is_object())
    else {
        return false;
    };
    let cancellation = UpdateCancellation::default();
    for (key, source) in &expected_rules {
        let filename = rule_set_filename(key, source);
        let path = rule_set_dir.join(&filename);
        if ensure_rule_set_file(&path, key).is_err() {
            return false;
        }
        let expected_hash = recorded_rule_hashes
            .get(&filename)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        if expected_hash.is_empty()
            || sha256_file(&path, &cancellation)
                .unwrap_or_default()
                .to_lowercase()
                != expected_hash
        {
            return false;
        }
    }
    true
}

// ── Workers ─────────────────────────────────────────────────────────────────

/// `StartupResourceCheckWorker.run`: check sing-box and geodata updates in the
/// background without applying anything.
pub async fn startup_resource_check(
    singbox_path: &str,
    proxy_url: Option<&str>,
    region: &str,
    cancellation: &UpdateCancellation,
) -> std::result::Result<Vec<ResourceUpdateResult>, UpdateCancelled> {
    let mut results = Vec::new();
    match check_or_update_singbox(
        singbox_path,
        false,
        None,
        || async { Ok(()) },
        proxy_url,
        cancellation,
    )
    .await
    {
        Ok(result) => results.push(result),
        Err(UpdateCancelled) => return Err(UpdateCancelled),
    }
    match check_geodata_update(region, proxy_url, cancellation).await {
        Ok(result) => results.push(result),
        Err(UpdateCancelled) => return Err(UpdateCancelled),
    }
    if cancellation.check().is_ok() {
        Ok(results)
    } else {
        Err(UpdateCancelled)
    }
}

#[derive(Debug, Clone)]
pub struct ResourceUpdateOptions {
    /// `singbox` | `geodata` | `droute`
    pub kind: String,
    pub singbox_path: String,
    pub apply_update: bool,
    pub proxy_url: Option<String>,
    pub region: String,
    pub prefer_bundled_geodata: bool,
}

impl Default for ResourceUpdateOptions {
    fn default() -> Self {
        Self {
            kind: String::new(),
            singbox_path: String::new(),
            apply_update: true,
            proxy_url: None,
            region: "russia".to_string(),
            prefer_bundled_geodata: false,
        }
    }
}

#[derive(Debug)]
pub enum ResourceUpdateEvent {
    Progress(u32),
    /// Reply with whether the services using the core stopped.
    RequestDisconnect(oneshot::Sender<bool>),
    Done(ResourceUpdateResult),
}

pub struct ResourceUpdateWorker {
    pub events: mpsc::UnboundedReceiver<ResourceUpdateEvent>,
    cancellation: UpdateCancellation,
    task: Option<JoinHandle<()>>,
}

impl ResourceUpdateWorker {
    pub fn start(options: ResourceUpdateOptions) -> Self {
        let cancellation = UpdateCancellation::default();
        let (events, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                let proxy = options.proxy_url.as_deref();
                let progress_sender = events.clone();
                let mut progress = move |done: u64, total: u64| {
                    if total > 0 {
                        let percent =
                            ((u128::from(done) * 100) / u128::from(total)).min(100) as u32;
                        let _ = progress_sender.send(ResourceUpdateEvent::Progress(percent));
                    }
                };
                let result = match options.kind.as_str() {
                    "singbox" => {
                        check_or_update_singbox(
                            &options.singbox_path,
                            options.apply_update,
                            Some(&mut progress),
                            || request_disconnect(&events, &cancellation),
                            proxy,
                            &cancellation,
                        )
                        .await
                    }
                    "geodata" if options.apply_update => {
                        if let Some(bundle) = options
                            .prefer_bundled_geodata
                            .then(|| bundled_geodata(&options.region))
                            .flatten()
                        {
                            install_bundled_geodata(
                                &bundle,
                                &options.region,
                                request_disconnect(&events, &cancellation),
                                &cancellation,
                            )
                            .await
                        } else {
                            update_geodata(
                                Some(&mut progress),
                                &options.region,
                                || request_disconnect(&events, &cancellation),
                                proxy,
                                &cancellation,
                            )
                            .await
                        }
                    }
                    "geodata" => check_geodata_update(&options.region, proxy, &cancellation).await,
                    "droute" => {
                        check_or_update_droute(
                            options.apply_update,
                            Some(&mut progress),
                            proxy,
                            None,
                            &cancellation,
                        )
                        .await
                    }
                    other => Ok(ResourceUpdateResult::error(
                        other,
                        format!("Неизвестный тип обновления: {other}"),
                    )),
                };
                if let Ok(result) = result
                    && cancellation.check().is_ok()
                {
                    let _ = events.send(ResourceUpdateEvent::Done(result));
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

    pub async fn join(mut self) -> std::result::Result<(), tokio::task::JoinError> {
        self.task.take().expect("worker task").await
    }
}

impl Drop for ResourceUpdateWorker {
    fn drop(&mut self) {
        self.cancel();
    }
}

async fn request_disconnect(
    events: &mpsc::UnboundedSender<ResourceUpdateEvent>,
    cancellation: &UpdateCancellation,
) -> Result<()> {
    let (confirm, ack) = oneshot::channel();
    events
        .send(ResourceUpdateEvent::RequestDisconnect(confirm))
        .map_err(|_| anyhow!("failed to stop the active core before update"))?;
    // Python waits in 100 ms slices so cancel() can break the ack wait.
    let mut ack = ack;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        match tokio::time::timeout(Duration::from_millis(100), &mut ack).await {
            Ok(Ok(true)) => return Ok(()),
            Ok(_) => bail!("failed to stop the active core before update"),
            Err(_) => {
                cancellation.check()?;
                if std::time::Instant::now() >= deadline {
                    bail!("timed out waiting for the active core to stop");
                }
            }
        }
    }
}

// ── Filesystem helpers ──────────────────────────────────────────────────────

/// `list(path.rglob(name))` for a concrete file name (case-insensitive).
fn find_files_recursive(root: &Path, file_name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file()
                && path
                    .file_name()
                    .map(|name| name.eq_ignore_ascii_case(file_name))
                    .unwrap_or(false)
            {
                found.push(path);
            }
        }
    }
    found.sort_by_key(|path| path.to_string_lossy().to_string());
    found
}

/// Every path below `root`, depth-first (dirs included), like `Path.rglob("*")`.
fn collect_all_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    out.push(root.to_path_buf());
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_all_recursive(&path, out);
            } else {
                out.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha_of(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn extracts_extended_versions_and_lumen_builds() {
        assert_eq!(extract_singbox_version("sing-box version 1.12.6"), "1.12.6");
        assert_eq!(
            extract_singbox_version("1.12.6-extended-1.3.0"),
            "1.12.6-extended-1.3.0"
        );
        assert_eq!(
            extract_singbox_version("v1.12.8-extended-2.0.1 x"),
            "1.12.8-extended-2.0.1"
        );
        assert_eq!(
            extract_singbox_version("no version here"),
            "no version here"
        );
        assert!(is_lumen_singbox_build("sing-box version 1.12.6-lumen.9"));
        assert!(is_lumen_singbox_build("1.12.6-LUMEN.9"));
        assert!(!is_lumen_singbox_build("sing-box version 1.12.6"));
    }

    #[test]
    fn picks_windows_amd64_and_purego_assets() {
        let release = json!({
            "tag_name": "v1.12.6-extended-1.3.0",
            "assets": [
                {"name": "sing-box-1.12.6-linux-amd64.zip", "browser_download_url": "https://example/linux.zip"},
                {"name": "sing-box-1.12.6-windows-amd64-purego.zip", "browser_download_url": "https://example/purego.zip"},
                {"name": "sing-box-1.12.6-windows-amd64.zip", "browser_download_url": "https://example/win.zip"},
            ]
        });
        let (name, url) = pick_singbox_asset(&release);
        assert_eq!(name, "sing-box-1.12.6-windows-amd64.zip");
        assert_eq!(url, "https://example/win.zip");
        let (cronet_name, cronet_url) = pick_singbox_cronet_asset(&release);
        assert_eq!(cronet_name, "sing-box-1.12.6-windows-amd64-purego.zip");
        assert_eq!(cronet_url, "https://example/purego.zip");
        let empty = json!({"assets": []});
        assert_eq!(pick_singbox_asset(&empty), (String::new(), String::new()));
        assert_eq!(
            pick_singbox_cronet_asset(&empty),
            (String::new(), String::new())
        );
    }

    #[test]
    fn only_https_downloads_are_accepted() {
        assert!(require_https("https://example/x.zip", "geoip.dat").is_ok());
        let error = require_https("http://example/x.zip", "geoip.dat").unwrap_err();
        assert_eq!(
            error.to_string(),
            "geoip.dat: загрузка допускается только по https"
        );
        let error = require_https("ftp://example/x.zip", "rule-set geoip:ru").unwrap_err();
        assert!(error.to_string().contains("только по https"));
    }

    #[test]
    fn rule_set_files_need_the_srs_magic_and_min_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("geosite-ru-blocked.srs");
        let damaged = dir.path().join("damaged.srs");
        let mut payload = b"SRS".to_vec();
        payload.extend_from_slice(&[0u8; 80]);
        fs::write(&path, &payload).unwrap();
        fs::write(&damaged, [0u8; 80]).unwrap();
        assert!(ensure_rule_set_file(&path, "geosite:ru-blocked").is_ok());
        assert!(ensure_rule_set_file(&damaged, "geosite:ru").is_err());
        assert!(ensure_rule_set_file(&dir.path().join("missing.srs"), "geoip:ru").is_err());
    }

    #[test]
    fn geodata_install_matches_verifies_metadata_and_payloads() {
        let target = tempfile::tempdir().unwrap();
        let rules = tempfile::tempdir().unwrap();
        let geoip_bytes: Vec<u8> = (0..2048u32).map(|i| (i % 251) as u8).collect();
        let geosite_bytes: Vec<u8> = (0..2048u32).map(|i| (i % 241) as u8).collect();
        write_file(&target.path().join("geoip.dat"), &geoip_bytes);
        write_file(&target.path().join("geosite.dat"), &geosite_bytes);
        let mut rule_hashes = serde_json::Map::new();
        for (_, member) in SINGBOX_BINARY_RULE_SETS {
            let name = Path::new(member)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap();
            let mut payload = b"SRS".to_vec();
            payload.extend_from_slice(&[0u8; 80]);
            write_file(&rules.path().join(&name), &payload);
            rule_hashes.insert(name, Value::String(sha_of(&payload)));
        }
        let regional = regional_singbox_rule_sets();
        let metadata = json!({
            "schema": 3,
            "region": "russia",
            "version": "v1.0.0",
            "rule_sets": regional["russia"],
            "sha256": {
                "geoip.dat": sha_of(&geoip_bytes),
                "geosite.dat": sha_of(&geosite_bytes),
                "sing-box.zip": sha_of(b"sing-box archive"),
            },
            "rule_set_sha256": rule_hashes,
        });
        write_file(
            &target.path().join(GEODATA_METADATA_NAME),
            metadata.to_string().as_bytes(),
        );
        let assets: ReleaseAssets = vec![
            (
                "geoip.dat".into(),
                ("https://example/geoip.dat".into(), sha_of(&geoip_bytes)),
            ),
            (
                "geosite.dat".into(),
                ("https://example/geosite.dat".into(), sha_of(&geosite_bytes)),
            ),
            (
                "sing-box.zip".into(),
                (
                    "https://example/sing-box.zip".into(),
                    sha_of(b"sing-box archive"),
                ),
            ),
        ];
        assert!(geodata_install_matches(
            target.path(),
            "v1.0.0",
            &assets,
            "russia",
            rules.path()
        ));
        // Wrong version, region or digest all break the match.
        assert!(!geodata_install_matches(
            target.path(),
            "v1.0.1",
            &assets,
            "russia",
            rules.path()
        ));
        assert!(!geodata_install_matches(
            target.path(),
            "v1.0.0",
            &assets,
            "china",
            rules.path()
        ));
        let mut wrong_digest = assets.clone();
        wrong_digest[0].1.1 = sha_of(b"other");
        assert!(!geodata_install_matches(
            target.path(),
            "v1.0.0",
            &wrong_digest,
            "russia",
            rules.path()
        ));
        // A missing rule-set file fails the payload check.
        let first_rule = rules.path().join("geosite-category-ru.srs");
        let backup = fs::read(&first_rule).unwrap();
        fs::remove_file(&first_rule).unwrap();
        assert!(!geodata_install_matches(
            target.path(),
            "v1.0.0",
            &assets,
            "russia",
            rules.path()
        ));
        fs::write(&first_rule, backup).unwrap();
    }

    #[test]
    fn regional_geodata_installed_requires_metadata_and_region() {
        let target = tempfile::tempdir().unwrap();
        let rules = tempfile::tempdir().unwrap();
        // No metadata at all: legacy bundles only pass with hashes absent, but
        // the schema check needs region identity first.
        assert!(!regional_geodata_installed(
            "russia",
            Some(target.path()),
            Some(rules.path())
        ));
        write_file(&target.path().join("geoip.dat"), vec![0u8; 2048].as_slice());
        write_file(
            &target.path().join("geosite.dat"),
            vec![1u8; 2048].as_slice(),
        );
        let metadata = json!({"schema": 2, "region": "china"});
        write_file(
            &target.path().join(GEODATA_METADATA_NAME),
            metadata.to_string().as_bytes(),
        );
        assert!(!regional_geodata_installed(
            "china",
            Some(target.path()),
            Some(rules.path())
        ));
        let mut payload = b"SRS".to_vec();
        payload.extend_from_slice(&[0; 80]);
        for (key, source) in &regional_singbox_rule_sets()["china"] {
            write_file(&rules.path().join(rule_set_filename(key, source)), &payload);
        }
        assert!(regional_geodata_installed(
            "china",
            Some(target.path()),
            Some(rules.path())
        ));
        assert!(!regional_geodata_installed(
            "russia",
            Some(target.path()),
            Some(rules.path())
        ));
    }

    #[test]
    fn geodata_release_regions_map_to_feeds() {
        assert_eq!(geodata_source("russia").0, "RuNetFreedom");
        assert_eq!(geodata_source("").0, "RuNetFreedom");
        assert_eq!(geodata_source("china").0, "Loyalsoldier");
        assert_eq!(geodata_source("iran").0, "Chocolate4U");
        assert_eq!(geodata_source("china").2.len(), 2);
        assert_eq!(geodata_source("russia").2.len(), 3);
    }

    #[tokio::test]
    async fn china_geodata_retries_other_source_after_invalid_download() -> Result<()> {
        use tokio::io::AsyncBufReadExt;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let proxy = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            for body in ["wrong file", "correct file"] {
                let (mut stream, _) = listener.accept().await?;
                let mut reader = tokio::io::BufReader::new(&mut stream);
                let mut line = String::new();
                while reader.read_line(&mut line).await? != 0 && line != "\r\n" {
                    line.clear();
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await?;
            }
            Ok::<_, anyhow::Error>(())
        });
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join("geoip.dat");
        let cancellation = UpdateCancellation::default();
        tokio::time::timeout(
            Duration::from_secs(5),
            download_china_geodata(
                ["http://cdn.test/geoip.dat", "http://origin.test/geoip.dat"],
                &destination,
                &mut |_, _| {},
                Some(&proxy),
                &cancellation,
                |path| {
                    anyhow::ensure!(
                        sha256_file(path, &cancellation)? == sha_of(b"correct file"),
                        "incorrect checksum"
                    );
                    Ok(())
                },
            ),
        )
        .await??;
        server.await??;
        assert_eq!(fs::read(destination)?, b"correct file");
        Ok(())
    }

    #[tokio::test]
    async fn update_worker_reports_unknown_kinds() {
        let mut worker = ResourceUpdateWorker::start(ResourceUpdateOptions {
            kind: "nonsense".into(),
            ..Default::default()
        });
        let event = worker.events.recv().await.expect("done event");
        match event {
            ResourceUpdateEvent::Done(result) => {
                assert_eq!(result.kind, "nonsense");
                assert_eq!(result.status, ResourceUpdateStatus::Error);
                assert_eq!(result.message, "Неизвестный тип обновления: nonsense");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn resource_download_cancels_while_waiting_for_body() -> Result<()> {
        use tokio::io::AsyncBufReadExt;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let proxy = format!("http://{}", listener.local_addr()?);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut reader = tokio::io::BufReader::new(&mut stream);
            let mut line = String::new();
            while reader.read_line(&mut line).await? != 0 && line != "\r\n" {
                line.clear();
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\ninitial bytes")
                .await?;
            std::future::pending::<Result<()>>().await
        });
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join("resource.zip");
        let cancellation = UpdateCancellation::default();
        let (started, ready) = tokio::sync::oneshot::channel();
        let mut started = Some(started);
        let mut progress = |_, _| {
            if let Some(started) = started.take() {
                let _ = started.send(());
            }
        };
        let download = download_direct(
            "http://updates.test/resource",
            &destination,
            Some(&mut progress),
            Some(&proxy),
            &cancellation,
        );
        let cancel = async {
            tokio::time::timeout(Duration::from_secs(3), ready)
                .await
                .context("waiting for initial download progress")??;
            cancellation.cancel();
            Ok::<_, anyhow::Error>(())
        };
        let (result, cancelled) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(download, cancel)
        })
        .await?;
        server.abort();
        cancelled?;
        assert!(result.unwrap_err().is::<UpdateCancelled>());
        Ok(())
    }

    #[test]
    fn cancellation_flag_flips_check_results() {
        // The primitive behind `_raise_if_cancelled`/`abort_http_response`:
        // every updater step checks it before and during work.
        let cancellation = UpdateCancellation::default();
        assert!(cancellation.check().is_ok());
        cancellation.cancel();
        assert!(cancellation.check().is_err());
    }

    #[test]
    fn backup_names_keep_the_full_extension() {
        assert_eq!(
            backup_path(Path::new("/core/geoip.dat")),
            Path::new("/core/geoip.dat.bak")
        );
        assert_eq!(
            backup_path(Path::new("/core/sing-box")),
            Path::new("/core/sing-box.bak")
        );
    }
}

#[cfg(test)]
mod atomic_replace_tests {
    //! Python test_resource_updater: `_atomic_replace_files` and
    //! `_safe_asset_name`.
    use super::*;

    #[test]
    fn rolls_every_file_back_when_validation_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = |name: &str| dir.path().join(name);
        fs::write(path("first.dat"), b"old-first").unwrap();
        fs::write(path("second.dat"), b"old-second").unwrap();
        fs::write(path("new-first.dat"), b"new-first").unwrap();
        fs::write(path("new-second.dat"), b"new-second").unwrap();
        let mut reject = || -> Result<()> { Err(anyhow!("invalid install")) };
        let error = atomic_replace_files(
            &[
                (path("new-first.dat"), path("first.dat")),
                (path("new-second.dat"), path("second.dat")),
            ],
            &[],
            Some(&mut reject),
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid install"));
        assert_eq!(fs::read(path("first.dat")).unwrap(), b"old-first");
        assert_eq!(fs::read(path("second.dat")).unwrap(), b"old-second");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.ends_with(".rollback") || name.ends_with(".new"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn writes_the_backup_only_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sing-box.exe");
        let source = dir.path().join("downloaded.exe");
        fs::write(&target, b"old").unwrap();
        fs::write(&source, b"new").unwrap();
        atomic_replace_files(&[(source, target.clone())], &[target.clone()], None).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read(dir.path().join("sing-box.exe.bak")).unwrap(), b"old");
    }

    #[test]
    fn release_asset_name_cannot_escape_the_download_directory() {
        use crate::xray::core_updater::safe_asset_name;
        let dir = std::path::Path::new(r"C:\downloads");
        for candidate in [r"..\..\evil.exe", "../../evil.exe", r"C:\Windows\System32\evil.exe", "", "..", "a b.zip"] {
            let name = safe_asset_name(candidate, "default.zip");
            assert_eq!(dir.join(name).parent(), Some(dir), "{candidate}");
        }
        assert_eq!(
            safe_asset_name("sing-box-1.13.14-windows-amd64.zip", "default.zip"),
            "sing-box-1.13.14-windows-amd64.zip"
        );
    }
}
