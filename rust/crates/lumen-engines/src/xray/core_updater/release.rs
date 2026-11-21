use std::cmp::Ordering;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, ensure};
use lumen_core::http_utils::get_update_response;
use lumen_core::models::{bool_field, str_field, str_or};
use lumen_core::update_checker::{check_update, is_https_url};
use regex::Regex;
use serde_json::Value;

use super::{UpdateCancellation, XrayCoreRelease};

pub(super) const ARCHIVE_NAME: &str = "Xray-windows-64.zip";

static SEMVER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]+)\.([0-9]+)\.([0-9]+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?")
        .expect("valid release version regex")
});
static DIGEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-f0-9]{64}").expect("valid SHA-256 regex"));
static ASSET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").expect("valid asset name regex"));

pub fn extract_version(text: &str) -> String {
    let value = text.trim().trim_start_matches('v');
    let Some(parts) = SEMVER.captures(value) else {
        return value.into();
    };
    let mut version = format!("{}.{}.{}", &parts[1], &parts[2], &parts[3]);
    if let Some(suffix) = parts.get(4) {
        version.push('-');
        version.push_str(suffix.as_str());
    }
    version
}

fn parse_version(value: &str) -> Option<([u64; 3], Vec<&str>)> {
    let parts = SEMVER.captures(value.trim().trim_start_matches('v'))?;
    let base = [
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
        parts[3].parse().ok()?,
    ];
    let suffix = parts
        .get(4)
        .map(|suffix| suffix.as_str().split('.').collect())
        .unwrap_or_default();
    Some((base, suffix))
}

fn compare_prerelease(left: &[&str], right: &[&str]) -> Ordering {
    if left.is_empty() || right.is_empty() {
        return left.is_empty().cmp(&right.is_empty());
    }
    for (left, right) in left.iter().zip(right) {
        let numeric =
            |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
        let order = match (numeric(left), numeric(right)) {
            (true, true) => {
                let left = left.trim_start_matches('0');
                let right = right.trim_start_matches('0');
                left.len().cmp(&right.len()).then_with(|| left.cmp(right))
            }
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.cmp(right),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

pub(super) fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let (left_base, left_suffix) = parse_version(left)?;
    let (right_base, right_suffix) = parse_version(right)?;
    Some(
        left_base
            .cmp(&right_base)
            .then_with(|| compare_prerelease(&left_suffix, &right_suffix)),
    )
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    compare_versions(latest, current).map_or_else(
        || extract_version(latest) != extract_version(current),
        |ordering| ordering == Ordering::Greater,
    )
}

pub(super) fn normalize_channel(channel: &str) -> &'static str {
    match channel.trim().to_ascii_lowercase().as_str() {
        "beta" => "beta",
        "nightly" => "nightly",
        _ => "stable",
    }
}

fn tag(release: &Value) -> &str {
    release["tag_name"]
        .as_str()
        .filter(|tag| !tag.is_empty())
        .or_else(|| release["name"].as_str())
        .unwrap_or_default()
}

pub(super) fn pick_release<'a>(releases: &'a [Value], channel: &str) -> Option<&'a Value> {
    let mut candidates: Vec<_> = releases
        .iter()
        .filter(|item| item.is_object() && !bool_field(item, "draft", false))
        .collect();
    candidates.sort_by_cached_key(|release| {
        std::cmp::Reverse(parse_version(tag(release)).unwrap_or_default())
    });
    if channel == "stable" {
        return candidates
            .into_iter()
            .find(|release| !bool_field(release, "prerelease", false));
    }
    let mut previews = candidates
        .into_iter()
        .filter(|release| bool_field(release, "prerelease", false));
    let matches = |release: &&Value, needles: &[&str]| {
        let text = format!(
            "{} {}",
            str_field(release, "tag_name"),
            str_field(release, "name")
        )
        .to_lowercase();
        needles.iter().any(|needle| text.contains(needle))
    };
    if channel == "beta" {
        let first = previews.next()?;
        if matches(&first, &["beta", "rc"]) {
            return Some(first);
        }
        previews
            .find(|release| matches(release, &["beta", "rc"]))
            .or(Some(first))
    } else {
        previews.find(|release| matches(release, &["nightly", "dev"]))
    }
}

fn find_asset<'a>(release: &'a Value, name: &str) -> Option<&'a Value> {
    release["assets"].as_array()?.iter().find(|asset| {
        asset["name"]
            .as_str()
            .is_some_and(|asset| asset.eq_ignore_ascii_case(name))
    })
}

pub fn safe_asset_name<'a>(name: &'a str, default: &'a str) -> &'a str {
    let candidate = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if candidate != "." && candidate != ".." && ASSET.is_match(candidate) {
        candidate
    } else {
        default
    }
}

pub fn extract_digest(value: &str) -> String {
    let text = value.trim().to_ascii_lowercase();
    let text = text.strip_prefix("sha256:").unwrap_or(&text).trim();
    DIGEST
        .find(text)
        .map(|hash| hash.as_str().to_owned())
        .unwrap_or_default()
}

pub(super) fn transports(proxy: Option<&str>) -> impl Iterator<Item = Option<&str>> {
    proxy
        .filter(|proxy| !proxy.trim().is_empty())
        .map(Some)
        .into_iter()
        .chain(std::iter::once(None))
}

async fn request_metadata<T>(
    url: &str,
    proxy_url: Option<&str>,
    attempts: u64,
    decode: impl Fn(&[u8]) -> Result<T>,
) -> Result<T> {
    retry_metadata(proxy_url, attempts, decode, |active_proxy| async move {
        let (response, _route) = get_update_response(url, 12.0, active_proxy).await?;
        Ok(response.bytes().await?.to_vec())
    })
    .await
}

/// Python `_request_json` retry policy: `attempts` tries through the proxy
/// (when set), then the same number direct, with a short back-off.
async fn retry_metadata<'p, T, F, Fut>(
    proxy_url: Option<&'p str>,
    attempts: u64,
    decode: impl Fn(&[u8]) -> Result<T>,
    fetch: F,
) -> Result<T>
where
    F: Fn(Option<&'p str>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>>>,
{
    let mut transports = transports(proxy_url).peekable();
    while let Some(active_proxy) = transports.next() {
        for attempt in 1..=attempts {
            let result = async { decode(&fetch(active_proxy).await?) }.await;
            match result {
                Ok(data) => return Ok(data),
                Err(error) if attempt == attempts && transports.peek().is_none() => {
                    return Err(error);
                }
                Err(_) if attempt < attempts => {
                    tokio::time::sleep(Duration::from_millis(150 * attempt)).await
                }
                Err(_) => {}
            }
        }
    }
    unreachable!("at least one transport and attempt")
}

pub async fn request_json(
    url: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<Value> {
    cancellation
        .run(request_metadata(url, proxy_url, 3, |data| {
            Ok(serde_json::from_slice(data)?)
        }))
        .await?
}

pub async fn fetch_dgst_hash(
    url: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<String> {
    cancellation
        .run(request_metadata(url, proxy_url, 1, |data| {
            Ok(extract_digest(&String::from_utf8_lossy(data)))
        }))
        .await?
}

pub(super) async fn resolve(
    channel: &str,
    feed_url: &str,
    github_api: &str,
    proxy_url: Option<&str>,
    cancellation: &UpdateCancellation,
) -> Result<Option<XrayCoreRelease>> {
    let channel = normalize_channel(channel);
    if !feed_url.trim().is_empty() {
        return Ok(cancellation
            .run(check_update(feed_url.trim(), channel, 5.0, proxy_url))
            .await??
            .map(|info| XrayCoreRelease {
                version: info.version,
                channel: channel.into(),
                url: info.url,
                digest_sha256: info.digest_sha256,
                notes: info.notes,
            }));
    }

    let payload = request_json(github_api, proxy_url, cancellation).await?;
    let Some(release) = payload
        .as_array()
        .and_then(|releases| pick_release(releases, channel))
    else {
        return Ok(None);
    };
    let Some(asset) = find_asset(release, ARCHIVE_NAME) else {
        return Ok(None);
    };
    let url = str_field(asset, "browser_download_url");
    let require_https = is_https_url(github_api);
    ensure!(
        url.is_empty() || !require_https || is_https_url(&url),
        "update download URL must use https"
    );
    let mut digest = extract_digest(&str_field(asset, "digest"));
    if digest.is_empty()
        && let Some(dgst) = find_asset(release, "Xray-windows-64.zip.dgst")
    {
        let digest_url = str_field(dgst, "browser_download_url");
        ensure!(
            !require_https || is_https_url(&digest_url),
            "update digest URL must use https"
        );
        digest = fetch_dgst_hash(&digest_url, proxy_url, cancellation).await?;
    }
    Ok(Some(XrayCoreRelease {
        version: extract_version(tag(release)),
        channel: channel.into(),
        url,
        digest_sha256: digest,
        notes: str_or(release, "body", ""),
    }))
}

#[cfg(test)]
mod retry_tests {
    //! Python test_resource_updater: `_request_json` retries.
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn transient_connection_reset_is_retried() {
        let calls = Mutex::new(0);
        let value: Value = retry_metadata(None, 3, |data| Ok(serde_json::from_slice(data)?), |_| {
            let call = {
                let mut calls = calls.lock().unwrap();
                *calls += 1;
                *calls
            };
            async move {
                if call == 1 {
                    Err(anyhow::anyhow!("connection reset (os error 10054)"))
                } else {
                    Ok(br#"{"tag_name":"v1.2.3"}"#.to_vec())
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(value, serde_json::json!({"tag_name": "v1.2.3"}));
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn every_attempt_uses_the_proxy_then_direct() {
        let proxies = Mutex::new(Vec::new());
        let error = retry_metadata(
            Some("http://127.0.0.1:2080"),
            3,
            |data| Ok(serde_json::from_slice::<Value>(data)?),
            |proxy| {
                proxies.lock().unwrap().push(proxy.map(str::to_owned));
                async { Err::<Vec<u8>, _>(anyhow::anyhow!("connection refused (os error 10061)")) }
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("10061"));
        let proxies = proxies.into_inner().unwrap();
        assert_eq!(proxies.len(), 6);
        assert!(proxies[..3].iter().all(|proxy| proxy.as_deref() == Some("http://127.0.0.1:2080")));
        assert!(proxies[3..].iter().all(Option::is_none));
    }
}

