//! HTTP subscription fetcher with the SafeRedirectHandler redirect policy.
//!
//! Port of the in-process part of `windows/xray_fluent/subscription_fetcher.py`.
//! The packaged helper-EXE IPC path is intentionally not ported: the Rust core
//! always fetches in-process, so the transports are (1) the explicit proxy
//! ("lumen-proxy") and (2) the direct in-process connection ("direct").

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use url::{Position, Url};

/// urllib's HTTPRedirectHandler default, kept for parity.
const MAX_REDIRECTS: usize = 10;
/// Headers that may cross origins on a redirect (SafeRedirectHandler).
const REDIRECT_ALLOWED_HEADERS: [&str; 3] = ["accept", "accept-encoding", "user-agent"];

/// Payload returned by a successful subscription download.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubscriptionHttpPayload {
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
    pub status: u16,
    pub transport: String,
}

/// Error names mirror the Python exceptions of `subscription_fetcher.py`.
#[derive(Debug, thiserror::Error)]
pub enum SubscriptionFetcherError {
    /// Python `SubscriptionFetcherCancelled`.
    #[error("загрузка подписки отменена")]
    Cancelled,
    /// Python `SubscriptionFetcherUnavailable` (transport/client failures).
    #[error("{0}")]
    Unavailable(String),
    /// Python `RuntimeError` (size limits and other protocol failures).
    #[error("{0}")]
    Runtime(String),
    /// Python `ValueError`.
    #[error("{0}")]
    Value(String),
    /// Python `TimeoutError`.
    #[error("{0}")]
    Timeout(String),
    /// Python `urllib.error.HTTPError` ("HTTP Error <code>: <reason>").
    #[error("{0}")]
    Http(String),
}

/// Python `(scheme, hostname, effective port)` origin triple.
fn origin_of(url: &Url) -> (String, String, Option<u16>) {
    (
        url.scheme().to_string(),
        url.host_str().unwrap_or("").to_lowercase(),
        url.port_or_known_default(),
    )
}

/// True when the authority carries userinfo (`user[:pass]@`). Hosts cannot
/// contain '@', so a '@' inside the authority is exactly userinfo.
fn has_userinfo(url: &Url) -> bool {
    url[Position::AfterScheme..Position::BeforePath].contains('@')
}

/// The url crate normalizes the empty-username spelling (`https://@host`) away,
/// while Python's urlsplit keeps `username == ''` and blocks it. Check the raw
/// absolute-location authority for a '@' to keep that fail-closed behaviour.
fn raw_absolute_has_userinfo(location: &str) -> bool {
    let Some(index) = location.find("://") else {
        return false;
    };
    let scheme = &location[..index];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        return false;
    }
    let rest = &location[index + 3..];
    let end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    rest[..end].contains('@')
}

/// SafeRedirectHandler.redirect_request: the blocking reasons for a redirect.
fn redirect_blocked(source: &Url, dest: &Url) -> bool {
    if !matches!(dest.scheme(), "http" | "https") {
        return true;
    }
    if source.scheme() == "https" && dest.scheme() != "https" {
        return true;
    }
    if has_userinfo(dest) {
        return true;
    }
    false
}

/// Only non-identifying negotiation headers cross origins; custom HWID
/// headers, validators and credentials are stripped.
fn strip_headers_cross_origin(headers: &mut BTreeMap<String, String>, source: &Url, dest: &Url) {
    if origin_of(source) == origin_of(dest) {
        return;
    }
    headers.retain(|name, _| REDIRECT_ALLOWED_HEADERS.iter().any(|allowed| allowed == name));
}

fn http_error(status: u16, reason: &str) -> SubscriptionFetcherError {
    SubscriptionFetcherError::Http(format!("HTTP Error {status}: {reason}"))
}

fn collect_headers(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, value) in headers.iter() {
        // reqwest names are already lowercase; keep the last duplicate like a dict.
        let normalized = name.as_str().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        // Preserve non-ASCII bytes mapped to Unicode, e.g. UTF-8 Cyrillic.
        let raw = String::from_utf8_lossy(value.as_bytes());
        let cleaned = raw
            .trim_matches(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n')
            .to_string();
        out.insert(normalized, cleaned);
    }
    out
}

/// Fetch a subscription over HTTP with a manual redirect-follow loop.
///
/// * `timeout_secs` applies per request (each redirect hop).
/// * `max_bytes` bounds the body: a larger Content-Length is rejected up front,
///   streamed chunks abort as soon as the total crosses the limit.
/// * `use_proxy_tun` selects the Lumen proxy/TUN path: with `proxy_url` the
///   request goes through that proxy ("lumen-proxy"), without one it stays
///   direct on the system route ("system-proxy-tun"). Otherwise the fetch
///   bypasses TUN on physical /32 routes ("direct-route", Python's
///   `DirectUrlOpener`) and, when `allow_process_direct_fallback` is set,
///   falls back to a plain in-process connection ("process-direct") while
///   the TUN-bypass route cannot be prepared.
/// * `cancelled` is polled before the download starts.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_subscription_http(
    url: &str,
    headers: &BTreeMap<String, String>,
    timeout_secs: f64,
    max_bytes: usize,
    use_proxy_tun: bool,
    proxy_url: &str,
    allow_process_direct_fallback: bool,
    cancelled: Option<&(dyn Fn() -> bool + Send + Sync)>,
) -> Result<SubscriptionHttpPayload, SubscriptionFetcherError> {
    if let Some(check) = cancelled {
        if check() {
            return Err(SubscriptionFetcherError::Cancelled);
        }
    }

    let trimmed_proxy = proxy_url.trim();
    let proxy_requested = use_proxy_tun && !trimmed_proxy.is_empty();
    let mut transport = if proxy_requested {
        "lumen-proxy"
    } else if use_proxy_tun {
        // Python's ProxyHandler({}) path: direct connection on the TUN route.
        "system-proxy-tun"
    } else {
        "direct-route"
    };

    // The bypass route must outlive the download: routes are released on drop.
    let direct_route: Option<Arc<crate::http_utils::WindowsDirectRoute>> =
        if proxy_requested || use_proxy_tun {
            None
        } else {
            // DirectUrlOpener: GET outside an active TUN via physical /32 routes.
            match crate::http_utils::direct_client(timeout_secs) {
                Ok((client, route)) => {
                    let client_ref = client;
                    return download_with_client(
                        &client_ref,
                        url,
                        headers,
                        max_bytes,
                        transport,
                        Some(route),
                    )
                    .await;
                }
                Err(exc) => {
                    if !allow_process_direct_fallback {
                        return Err(SubscriptionFetcherError::Unavailable(exc.to_string()));
                    }
                    // In the packaged helper, sing-box permanently routes this
                    // process to the direct outbound. This fallback also remains
                    // direct when TUN is disabled and avoids failing just because
                    // a host route could not be prepared at that exact moment.
                    transport = "process-direct";
                    None
                }
            }
        };

    // reqwest::redirect::Policy cannot inspect headers, so redirects are
    // followed manually below.
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs_f64(timeout_secs.max(0.001)));
    if proxy_requested {
        let proxy = reqwest::Proxy::all(trimmed_proxy)
            .map_err(|exc| SubscriptionFetcherError::Unavailable(exc.to_string()))?;
        builder = builder.proxy(proxy);
    } else {
        // Python's ProxyHandler({}) also disables environment proxy detection.
        builder = builder.no_proxy();
    }
    let client = builder
        .build()
        .map_err(|exc| SubscriptionFetcherError::Unavailable(exc.to_string()))?;

    download_with_client(&client, url, headers, max_bytes, transport, direct_route).await
}

async fn download_with_client(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    max_bytes: usize,
    transport: &str,
    // Keeps the TUN-bypass routes alive for the whole download (released on drop).
    _direct_route: Option<Arc<crate::http_utils::WindowsDirectRoute>>,
) -> Result<SubscriptionHttpPayload, SubscriptionFetcherError> {
    let mut current = Url::parse(url.trim()).map_err(|_| {
        SubscriptionFetcherError::Unavailable(format!("unknown url type: {url}"))
    })?;
    if !matches!(current.scheme(), "http" | "https") {
        return Err(SubscriptionFetcherError::Unavailable(format!(
            "unknown url type: {url}"
        )));
    }
    // Header names are normalized once so the cross-origin strip can compare
    // lowercase names the way SafeRedirectHandler does.
    let mut current_headers: BTreeMap<String, String> = headers
        .iter()
        .map(|(name, value)| (name.to_lowercase(), value.clone()))
        .collect();

    let mut redirects = 0usize;
    loop {
        let mut request = client.get(current.clone());
        for (name, value) in &current_headers {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::from_bytes(name.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                request = request.header(name, value);
            }
        }
        let response = request.send().await.map_err(|exc| {
            if exc.is_timeout() {
                SubscriptionFetcherError::Timeout(exc.to_string())
            } else {
                SubscriptionFetcherError::Unavailable(exc.to_string())
            }
        })?;
        let status = response.status().as_u16();

        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            redirects += 1;
            if redirects > MAX_REDIRECTS {
                return Err(http_error(status, "redirect limit reached"));
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            if location.is_empty() {
                return Err(http_error(status, response.status().canonical_reason().unwrap_or("")));
            }
            let dest = current
                .join(&location)
                .map_err(|_| http_error(status, "Unsafe subscription redirect blocked"))?;
            if redirect_blocked(&current, &dest) || raw_absolute_has_userinfo(&location) {
                return Err(http_error(status, "Unsafe subscription redirect blocked"));
            }
            strip_headers_cross_origin(&mut current_headers, &current, &dest);
            current = dest;
            continue;
        }

        // urllib's optional HTTPError(304) path becomes a normal payload.
        if status == 304 {
            let mut payload_headers = collect_headers(response.headers());
            payload_headers.insert(
                "x-lumen-effective-url".into(),
                current.as_str().trim().to_string(),
            );
            return Ok(SubscriptionHttpPayload {
                body: Vec::new(),
                headers: payload_headers,
                status,
                transport: transport.into(),
            });
        }

        if !(200..300).contains(&status) {
            return Err(http_error(
                status,
                response.status().canonical_reason().unwrap_or(""),
            ));
        }

        let declared_size = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(0);
        if declared_size > max_bytes as i64 {
            return Err(SubscriptionFetcherError::Runtime(format!(
                "подписка слишком большая: {declared_size} байт, максимум {max_bytes}"
            )));
        }

        let mut body: Vec<u8> = Vec::new();
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    body.extend_from_slice(&chunk);
                    if body.len() > max_bytes {
                        return Err(SubscriptionFetcherError::Runtime(format!(
                            "подписка превышает допустимый размер {max_bytes} байт"
                        )));
                    }
                }
                Ok(None) => break,
                Err(exc) => {
                    return Err(if exc.is_timeout() {
                        SubscriptionFetcherError::Timeout(exc.to_string())
                    } else {
                        SubscriptionFetcherError::Unavailable(exc.to_string())
                    })
                }
            }
        }

        let mut payload_headers = collect_headers(response.headers());
        // Internal-only metadata used to reject HTTPS -> HTTP redirect downgrades.
        payload_headers.insert(
            "x-lumen-effective-url".into(),
            current.as_str().trim().to_string(),
        );
        return Ok(SubscriptionHttpPayload {
            body,
            headers: payload_headers,
            status,
            transport: transport.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_redirects_are_blocked() {
        let source = Url::parse("https://panel.example.com/sub").unwrap();
        // HTTPS -> HTTP downgrade.
        assert!(redirect_blocked(
            &source,
            &Url::parse("http://panel.example.com/sub").unwrap()
        ));
        // Non-http(s) scheme.
        assert!(redirect_blocked(&source, &Url::parse("ftp://elsewhere.example.com/x").unwrap()));
        // Userinfo in the destination.
        assert!(redirect_blocked(
            &source,
            &Url::parse("https://user:pass@elsewhere.example.com/x").unwrap()
        ));
        // The empty-username spelling is normalized away by the url crate, so
        // it is caught by the raw-location check used in the redirect loop.
        assert!(raw_absolute_has_userinfo("https://@elsewhere.example.com/x"));
        assert!(!raw_absolute_has_userinfo("sub?email=user@example.com"));
        assert!(!raw_absolute_has_userinfo("https://host/x?next=a@b"));
        // Plain same-scheme redirects stay allowed.
        assert!(!redirect_blocked(
            &source,
            &Url::parse("https://panel.example.com/sub?v=2").unwrap()
        ));
        assert!(!redirect_blocked(
            &Url::parse("http://panel.example.com/sub").unwrap(),
            &Url::parse("http://mirror.example.com/sub").unwrap()
        ));
    }

    #[test]
    fn cross_origin_redirect_strips_headers() {
        let source = Url::parse("https://a.example.com/sub").unwrap();
        let dest = Url::parse("https://b.example.com/sub").unwrap();
        let mut headers = BTreeMap::from([
            ("accept".to_string(), "*/*".to_string()),
            ("accept-encoding".to_string(), "gzip".to_string()),
            ("user-agent".to_string(), "Lumen".to_string()),
            ("x-lumen-hwid".to_string(), "secret-hwid".to_string()),
            ("authorization".to_string(), "Bearer token".to_string()),
        ]);
        strip_headers_cross_origin(&mut headers, &source, &dest);
        assert!(!headers.contains_key("x-lumen-hwid"));
        assert!(!headers.contains_key("authorization"));
        assert_eq!(headers.get("user-agent").map(String::as_str), Some("Lumen"));
        assert_eq!(headers.get("accept").map(String::as_str), Some("*/*"));
        assert_eq!(
            headers.get("accept-encoding").map(String::as_str),
            Some("gzip")
        );
    }

    #[test]
    fn same_origin_redirect_keeps_headers() {
        let source = Url::parse("https://a.example.com/sub").unwrap();
        let dest = Url::parse("https://a.example.com/sub2").unwrap();
        let mut headers = BTreeMap::from([(
            "x-lumen-hwid".to_string(),
            "secret-hwid".to_string(),
        )]);
        strip_headers_cross_origin(&mut headers, &source, &dest);
        assert_eq!(
            headers.get("x-lumen-hwid").map(String::as_str),
            Some("secret-hwid")
        );
    }
}
