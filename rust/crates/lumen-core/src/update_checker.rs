//! HTTPS release feeds shared by the app and component updaters.

use anyhow::{Result, ensure};
use serde_json::Value;
use url::Url;

use crate::http_utils::get_update_response;
use crate::models::{str_field, str_or};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub channel: String,
    pub digest_sha256: String,
}

pub fn is_https_url(url: &str) -> bool {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    scheme.eq_ignore_ascii_case("https")
        && !authority.is_empty()
        && !authority.contains('\\')
        && Url::parse(url).is_ok_and(|url| url.scheme() == "https" && url.has_host())
}

pub async fn check_update(
    feed_url: &str,
    channel: &str,
    timeout: f64,
    proxy_url: Option<&str>,
) -> Result<Option<UpdateInfo>> {
    if feed_url.is_empty() {
        return Ok(None);
    }
    ensure!(is_https_url(feed_url), "update feed URL must use https");
    let proxy_url = proxy_url.filter(|proxy| !proxy.trim().is_empty());
    let payload = match request_feed(feed_url, timeout, proxy_url).await {
        Err(_) if proxy_url.is_some() => request_feed(feed_url, timeout, None).await?,
        result => result?,
    };
    parse_update(&payload, channel)
}

async fn request_feed(feed_url: &str, timeout: f64, proxy_url: Option<&str>) -> Result<Value> {
    let (response, _route) = get_update_response(feed_url, timeout, proxy_url).await?;
    Ok(response.json().await?)
}

fn parse_update(payload: &Value, channel: &str) -> Result<Option<UpdateInfo>> {
    let release = if let Some(channels) = payload.get("channels") {
        channels.get(channel).unwrap_or(&Value::Null)
    } else {
        payload
    };
    let version = str_field(release, "version");
    let url = str_field(release, "url");
    if version.is_empty() || url.is_empty() {
        return Ok(None);
    }
    ensure!(is_https_url(&url), "update download URL must use https");
    let digest = str_or(release, "digest", &str_field(release, "sha256"));
    let digest_sha256 = match digest.split_once(':') {
        Some((prefix, digest)) if prefix.eq_ignore_ascii_case("sha256") => digest.trim().into(),
        _ => digest,
    };
    Ok(Some(UpdateInfo {
        version,
        url,
        notes: str_field(release, "notes"),
        channel: str_or(release, "channel", channel),
        digest_sha256,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn refuses_non_https_feeds_before_opening_a_connection() {
        for url in [
            "http://example.test/xray.json",
            "https:example.test/xray.json",
            "https:///example.test/xray.json",
            "file:///C:/temp/xray.json",
            "ftp://example.test/xray.json",
        ] {
            let error = check_update(url, "stable", 5.0, None).await.unwrap_err();
            assert_eq!(error.to_string(), "update feed URL must use https");
        }
    }

    #[test]
    fn selects_channels_and_requires_an_https_download_url() -> anyhow::Result<()> {
        let feed = json!({"channels": {
            "stable": {"version":"26.3.27", "url":"http://example.test/x.zip"},
            "beta": {"version":"26.7.11", "url":"https://example.test/x.zip", "digest":"SHA256: abc",
                     "channel":"preview", "notes":"details"}
        }});
        assert!(parse_update(&feed, "stable").is_err());
        assert!(parse_update(&feed, "nightly")?.is_none());
        let beta = parse_update(&feed, "beta")?.unwrap();
        assert_eq!(
            (beta.version.as_str(), beta.url.as_str()),
            ("26.7.11", "https://example.test/x.zip")
        );
        assert_eq!(
            (
                beta.channel.as_str(),
                beta.notes.as_str(),
                beta.digest_sha256.as_str()
            ),
            ("preview", "details", "abc")
        );
        Ok(())
    }
}
