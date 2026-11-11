//! Discover the Windows application update for the selected release channel.

use std::path::Path;

use anyhow::{Result, anyhow, ensure};
use lumen_core::constants::{APP_VERSION, base_dir};
use lumen_core::http_utils::{get_update_response, read_body_limited};
use lumen_core::models::{bool_field, int_field, str_field};
use lumen_core::update_checker::is_https_url;
use lumen_engines::xray::core_updater::is_newer;
use serde_json::Value;

const RELEASES: &str = "https://api.github.com/repos/krambovic/Lumen/releases";
const METADATA_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppUpdate {
    pub version: String,
    pub tag: String,
    pub download_url: String,
    pub size: u64,
    pub notes: String,
    pub digest_sha256: String,
    pub asset_name: String,
    pub channel: String,
    pub is_downgrade: bool,
}

pub fn should_auto_install(update: Option<&AppUpdate>, enabled: bool) -> bool {
    enabled && update.is_some_and(|update| !update.is_downgrade)
}

/// Dropping this future cancels the active metadata request.
pub async fn check_app_update(
    channel: &str,
    prefer_qml: bool,
    proxy_url: Option<&str>,
) -> Result<Option<AppUpdate>> {
    let channel = if channel.is_empty() {
        "stable"
    } else {
        channel
    }
    .trim()
    .to_lowercase();
    let preview = matches!(
        channel.as_str(),
        "beta" | "nightly" | "prerelease" | "pre-release" | "pre"
    );
    let portable = portable_at(base_dir());
    let latest = fetch_json(&format!("{RELEASES}/latest"), proxy_url)
        .await
        .ok()
        .filter(|release| {
            desktop_release(release, portable) && !bool_field(release, "prerelease", false)
        });
    let target = if !preview && latest.is_some() {
        latest
    } else {
        let mut stable = latest;
        let search_stable = stable.is_none();
        let mut prerelease = None;
        for page in 1.. {
            let url = format!("{RELEASES}?per_page=100&page={page}");
            let response = fetch_json(&url, proxy_url).await?;
            let releases = response
                .as_array()
                .ok_or_else(|| anyhow!("Invalid GitHub releases response"))?;
            for release in releases
                .iter()
                .filter(|release| desktop_release(release, portable))
            {
                if bool_field(release, "prerelease", false) {
                    if preview {
                        consider(&mut prerelease, release);
                    }
                } else if search_stable {
                    consider(&mut stable, release);
                }
            }
            if releases.len() < 100 {
                break;
            }
        }
        match (stable, prerelease) {
            (Some(stable), Some(pre)) if !is_newer(&tag(&stable), &tag(&pre)) => Some(pre),
            (Some(stable), _) => Some(stable),
            (_, pre) => pre,
        }
    };
    let Some(release) = target else {
        return Ok(None);
    };
    let Some(is_downgrade) = update_direction(&tag(&release), APP_VERSION, preview) else {
        return Ok(None);
    };
    let asset = assets(&release)
        .filter(|asset| compatible(asset, portable))
        .map(|asset| {
            (
                score(asset, prefer_qml || channel == "nightly", portable),
                asset,
            )
        })
        .reduce(|best, candidate| {
            if candidate.0 > best.0 {
                candidate
            } else {
                best
            }
        })
        .map(|(_, asset)| asset)
        .ok_or_else(|| anyhow!("Релиз {} не содержит Windows-пакет", tag(&release)))?;
    let name = str_field(asset, "name");
    let mut digest = extract_digest(&str_field(asset, "digest"));
    if digest.is_empty()
        && let Some(sidecar) = digest_sidecar(&release, &name)
    {
        let url = str_field(sidecar, "browser_download_url");
        digest = fetch_metadata(&url, proxy_url, |body| {
            Ok(extract_digest(&String::from_utf8_lossy(body)))
        })
        .await?;
    }
    let tag = tag(&release);
    let kind = if portable {
        "zip-архив"
    } else {
        "установщик"
    };
    ensure!(
        !digest.is_empty(),
        "Релиз {tag} найден, но {kind} не содержит SHA-256"
    );
    let download_url = str_field(asset, "browser_download_url");
    ensure!(
        is_https_url(&download_url),
        "update download URL must use https"
    );
    Ok(Some(AppUpdate {
        version: tag.trim_start_matches('v').into(),
        tag,
        download_url,
        size: u64::try_from(int_field(asset, "size", 0)).unwrap_or_default(),
        notes: str_field(&release, "body"),
        digest_sha256: digest,
        asset_name: name,
        channel,
        is_downgrade,
    }))
}

fn portable_at(dir: &Path) -> bool {
    ["portable.txt", "portable"]
        .into_iter()
        .any(|name| dir.join(name).is_file())
}

async fn fetch_json(url: &str, proxy_url: Option<&str>) -> Result<Value> {
    fetch_metadata(url, proxy_url, |body| Ok(serde_json::from_slice(body)?)).await
}

async fn fetch_metadata<T>(
    url: &str,
    proxy_url: Option<&str>,
    parse: impl Fn(&[u8]) -> Result<T>,
) -> Result<T> {
    ensure!(is_https_url(url), "update metadata URL must use https");
    let proxy_url = proxy_url.filter(|proxy| !proxy.trim().is_empty());
    let parse = &parse;
    let request = |proxy| async move {
        let (response, _route) = get_update_response(url, 15.0, proxy).await?;
        parse(&read_body_limited(response, METADATA_LIMIT).await?)
    };
    match request(proxy_url).await {
        Err(_) if proxy_url.is_some() => request(None).await,
        result => result,
    }
}

fn tag(release: &Value) -> String {
    str_field(release, "tag_name")
}

fn assets(release: &Value) -> impl Iterator<Item = &Value> {
    release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|asset| asset.is_object())
}

fn compatible(asset: &Value, portable: bool) -> bool {
    let name = str_field(asset, "name").to_lowercase();
    !name.contains("android")
        && if portable {
            name.ends_with(".zip") && (name.contains("portable") || name.contains("windows"))
        } else {
            name.ends_with(".exe")
        }
}

fn desktop_release(release: &Value, portable: bool) -> bool {
    let tag = tag(release);
    !tag.is_empty()
        && !tag.to_lowercase().contains("android")
        && !bool_field(release, "draft", false)
        && assets(release).any(|asset| compatible(asset, portable))
}

fn consider(best: &mut Option<Value>, release: &Value) {
    if best
        .as_ref()
        .is_none_or(|current| is_newer(&tag(release), &tag(current)))
    {
        *best = Some(release.clone());
    }
}

fn update_direction(latest: &str, current: &str, preview: bool) -> Option<bool> {
    if latest.trim().trim_start_matches('v') == current.trim().trim_start_matches('v') {
        return None;
    }
    let downgrade = !is_newer(latest, current);
    (!preview || !downgrade).then_some(downgrade)
}

fn score(asset: &Value, prefer_qml: bool, portable: bool) -> (i32, String) {
    let name = str_field(asset, "name").to_lowercase();
    let mut score =
        1 + 2 * i32::from(name.contains("lumen")) + 2 * i32::from(name.contains("windows"));
    score += 2 * i32::from(
        ["x64", "win64", "amd64"]
            .into_iter()
            .any(|arch| name.contains(arch)),
    );
    score += if portable {
        4 * i32::from(name.contains("portable"))
    } else {
        4 * (i32::from(name.contains("setup")) - i32::from(name.contains("portable")))
    };
    if name.contains("nightly") || name.contains("qml") {
        score += if prefer_qml { 3 } else { -1 };
    }
    (score, name)
}

fn digest_sidecar<'a>(release: &'a Value, name: &str) -> Option<&'a Value> {
    [".sha256", ".dgst"].into_iter().find_map(|suffix| {
        let expected = format!("{name}{suffix}");
        assets(release).find(|asset| str_field(asset, "name").eq_ignore_ascii_case(&expected))
    })
}

fn extract_digest(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    let digest: String = value
        .strip_prefix("sha256:")
        .unwrap_or(&value)
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .collect();
    if digest.len() == 64 {
        digest
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_windows_assets_and_prefers_the_nightly_variant_only_when_requested() {
        let stable = json!({"name":"Lumen-Setup-windows-x64.exe"});
        let nightly = json!({"name":"Lumen-qml-Setup-windows-x64.exe"});
        assert!(score(&stable, false, false) > score(&nightly, false, false));
        assert!(score(&nightly, true, false) > score(&stable, true, false));
        assert!(!compatible(
            &json!({"name":"Lumen-android-windows.zip"}),
            true
        ));
        assert!(compatible(&json!({"name":"Lumen-portable-x64.zip"}), true));
    }

    #[test]
    fn distinguishes_stable_rollback_from_preview_upgrades() {
        assert_eq!(update_direction("v2.2.0", "2.2.0", false), None);
        assert_eq!(update_direction("v2.1.0", "2.2.0", false), Some(true));
        assert_eq!(update_direction("v2.1.0", "2.2.0", true), None);
        assert_eq!(
            update_direction("v2.2.0-beta.10", "2.2.0-beta.2", true),
            Some(false)
        );
    }

    #[test]
    fn prefers_sha256_sidecars_and_requires_the_whole_digest() {
        let release = json!({"assets":[{"name":"setup.exe.dgst"},{"name":"SETUP.EXE.SHA256"}]});
        assert_eq!(
            digest_sidecar(&release, "setup.exe").unwrap()["name"],
            "SETUP.EXE.SHA256"
        );
        let hash = "AB".repeat(32);
        assert_eq!(
            extract_digest(&format!("sha256: {hash}")),
            hash.to_lowercase()
        );
        assert!(extract_digest(&format!("{hash}abcd")).is_empty());
    }
}
