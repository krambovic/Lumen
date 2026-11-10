//! HTTP health of the selected outbound, independent of TCP/UDP transport.

use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

use crate::http_utils::{redirect_blocked, WarmHttpProbe};
use crate::metrics_api::{monotonic_seconds, ClashApiClient};
use crate::traffic_route_classifier::{RouteClass, RouteClassifier};

pub const HEALTH_MAX_AGE_SEC: f64 = 10.0;
pub const DEFAULT_HEALTH_URL: &str = "https://www.gstatic.com/generate_204";

#[derive(Debug, Clone, PartialEq)]
pub struct HealthSample {
    pub status: String,
    /// `monotonic_seconds()`, never wall-clock or TCP ping time.
    pub checked_at: f64,
    pub profile_id: String,
    pub latency_ms: Option<i64>,
    pub source: String,
    pub reason: String,
}

impl Default for HealthSample {
    fn default() -> Self {
        Self {
            status: "UNKNOWN".to_string(),
            checked_at: 0.0,
            profile_id: String::new(),
            latency_ms: None,
            source: String::new(),
            reason: "not sampled".to_string(),
        }
    }
}

impl HealthSample {
    pub fn at_time(&self, now: f64) -> Self {
        let mut sample = self.clone();
        let fresh = self.checked_at > 0.0 && (0.0..=HEALTH_MAX_AGE_SEC).contains(&(now - self.checked_at));
        if !fresh {
            sample.status = "UNKNOWN".into();
            sample.reason = "health sample stale or unavailable".into();
        }
        if sample.status != "HEALTHY" {
            sample.latency_ms = None;
        }
        sample
    }

    pub fn payload(&self, now: Option<f64>) -> Value {
        let sample = self.at_time(now.unwrap_or_else(monotonic_seconds));
        json!({
            "health_status": sample.status,
            "health_checked_at": sample.checked_at,
            "health_profile_id": sample.profile_id,
            "health_source": sample.source,
            "health_reason": sample.reason,
            "latency_ms": sample.latency_ms,
        })
    }
}

/// A `proxy_url` may be supplied ONLY for a guaranteed active-outbound listener.
///
/// A listening server/TCP handshake is not health. Missing credentials, a
/// failed local API, unsupported protocols, stale data or unverified routing
/// are UNKNOWN, not failed. Local API errors are not remote-profile failures.
#[allow(clippy::too_many_arguments)]
pub async fn probe_active_profile(
    profile_id: &str,
    clash_client: Option<&ClashApiClient>,
    outbound_tag: &str,
    outbound_graph: Option<&Value>,
    proxy_url: &str,
    health_url: &str,
    timeout: f64,
    mut warm_probe: Option<&mut WarmHttpProbe>,
) -> HealthSample {
    let started = monotonic_seconds();
    let mut source = if clash_client.is_some() {
        "clash-active-outbound"
    } else {
        "active-http-proxy"
    };

    let sample = |status: &str, reason: &str, latency: Option<i64>, source: &str| HealthSample {
        status: status.to_string(),
        checked_at: monotonic_seconds(),
        profile_id: profile_id.to_string(),
        latency_ms: latency,
        source: source.to_string(),
        reason: reason.to_string(),
    };

    if profile_id.is_empty() {
        return sample("UNKNOWN", "active profile identity unavailable", None, source);
    }
    // Python urlsplit almost never raises; malformed targets land in the
    // unsupported-URL branch below.
    let Ok(target) = Url::parse(health_url) else {
        return sample("UNKNOWN", "unsupported HTTP health URL", None, source);
    };
    let parsed = Url::parse(proxy_url).unwrap_or_else(|_| Url::parse("http://invalid.invalid").unwrap());
    if !matches!(target.scheme(), "https" | "http") || target.host_str().unwrap_or("").is_empty() {
        return sample("UNKNOWN", "unsupported HTTP health URL", None, source);
    }
    if warm_probe.is_some() && !proxy_url.is_empty() && target.scheme() == "https" {
        source = "active-http-proxy-warm";
        if parsed.scheme() != "http" || !matches!(parsed.host_str(), Some("127.0.0.1") | Some("::1")) {
            return sample("UNKNOWN", "guaranteed active local HTTP proxy unavailable", None, source);
        }
        let Some(probe) = warm_probe.as_deref_mut() else {
            return sample("UNKNOWN", "warm HTTP probe unavailable", None, source);
        };
        return match probe.measure(proxy_url, health_url, timeout.max(3.0)).await {
            Ok(latency) => sample("HEALTHY", "", Some(latency as i64), source),
            Err(err) => {
                let transport = err.downcast_ref::<reqwest::Error>().is_some()
                    || err.downcast_ref::<std::io::Error>().is_some();
                if transport {
                    sample("FAILED", "active profile HTTP request failed", None, source)
                } else {
                    sample("UNKNOWN", "warm HTTP probe unavailable", None, source)
                }
            }
        };
    }
    if let Some(client) = clash_client {
        let classifier = match outbound_graph {
            Some(graph) => RouteClassifier::new(graph),
            None => RouteClassifier::default(),
        };
        if classifier.classify(&[outbound_tag]) != RouteClass::Proxy {
            return sample("UNKNOWN", "active outbound type/selection unverified", None, source);
        }
        let delay_timeout = ((timeout * 1000.0) as i64 - 200).max(100);
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={delay_timeout}",
            quote_strict(outbound_tag),
            quote_plus(health_url),
        );
        return match client.get(&path, timeout) {
            Ok(result) => {
                let delay = result.get("delay").and_then(Value::as_i64);
                match delay {
                    Some(delay) if delay > 0 => {
                        let latency = if warm_probe.is_none() { Some(delay) } else { None };
                        sample("HEALTHY", "", latency, source)
                    }
                    _ => sample("UNKNOWN", "active outbound delay unsupported or invalid", None, source),
                }
            }
            Err(exc) => {
                if exc.unsupported {
                    return sample("UNKNOWN", "active outbound HTTP probe unsupported", None, source);
                }
                // sing-box getProxyDelay uses 503 for an explicit test error and
                // 504 for its context deadline, not generic local API failures.
                if matches!(exc.status, 408 | 503 | 504) {
                    sample("FAILED", "active outbound HTTP probe failed", None, source)
                } else {
                    sample("UNKNOWN", &exc.reason, None, source)
                }
            }
        };
    }

    if parsed.scheme() != "http" || !matches!(parsed.host_str(), Some("127.0.0.1") | Some("::1")) {
        return sample("UNKNOWN", "guaranteed active local HTTP proxy unavailable", None, source);
    }
    let proxy_url = proxy_url.to_string();
    let health_url = health_url.to_string();
    match tokio::task::spawn_blocking(move || strict_proxy_get_sync(&proxy_url, &health_url, timeout))
        .await
    {
        Ok(Ok(status)) => {
            if (200..300).contains(&status) {
                let latency = ((monotonic_seconds() - started) * 1000.0).round().max(0.0) as i64;
                sample("HEALTHY", "", Some(latency), source)
            } else if matches!(status, 405 | 407 | 501) {
                sample("UNKNOWN", &format!("active proxy probe unsupported (HTTP {status})"), None, source)
            } else {
                sample("FAILED", &format!("active profile HTTP {status}"), None, source)
            }
        }
        Ok(Err(StrictGetError::Transport)) => sample("FAILED", "active profile HTTP request failed", None, source),
        Ok(Err(StrictGetError::Build)) | Err(_) => sample("UNKNOWN", "active profile probe unavailable", None, source),
    }
}

enum StrictGetError {
    /// Opener construction failed (Python `ValueError` from the handler).
    Build,
    /// Socket-level failure (Python `TimeoutError` / `URLError` / `OSError`).
    Transport,
}

/// StrictProxyHandler deliberately ignores NO_PROXY, including '*'. Do not
/// replace this with a client that consults ambient proxy configuration.
fn strict_proxy_get_sync(proxy_url: &str, health_url: &str, timeout: f64) -> Result<u16, StrictGetError> {
    let proxy = Url::parse(proxy_url).map_err(|_| StrictGetError::Build)?;
    if proxy.scheme() != "http" || proxy.host_str().is_none() {
        return Err(StrictGetError::Build);
    }
    let mut current = Url::parse(health_url).map_err(|_| StrictGetError::Build)?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::all(proxy_url).map_err(|_| StrictGetError::Build)?)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs_f64(timeout.max(0.001)))
        .build()
        .map_err(|_| StrictGetError::Build)?;
    // Python's opener keeps its own redirect budget (urllib max_redirections).
    let mut redirects = 0;
    // Cross-origin redirects strip everything but negotiation headers, so the
    // Cache-Control hint is dropped exactly like SafeRedirectHandler does.
    let mut strip_cache_control = false;
    loop {
        let mut request = client.get(current.clone());
        if !strip_cache_control {
            request = request.header("Cache-Control", "no-cache");
        }
        let response = request.send().map_err(|_| StrictGetError::Transport)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) || !(300..400).contains(&status) {
            return Ok(status);
        }
        let followed = matches!(status, 301 | 302 | 303 | 307 | 308);
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        drop(response);
        if !followed {
            return Ok(status); // urllib raises HTTPError(code)
        }
        let Some(location) = location else { return Ok(status) };
        let dest = current.join(&location).map_err(|_| StrictGetError::Build)?;
        if redirect_blocked(&current, &dest, &location) {
            return Ok(status); // SafeRedirectHandler raises HTTPError(code)
        }
        if previous_origin(&current) != previous_origin(&dest) {
            strip_cache_control = true;
        }
        redirects += 1;
        if redirects > 10 {
            return Ok(status); // Redirection limit reached -> HTTPError(code)
        }
        current = dest;
    }
}

fn previous_origin(url: &Url) -> (String, String, u16) {
    let default_port = if url.scheme() == "https" { 443 } else { 80 };
    (url.scheme().to_string(), url.host_str().unwrap_or("").to_string(), url.port().unwrap_or(default_port))
}

/// Python `quote(value, safe="")`: everything outside the RFC 3986 unreserved
/// set is percent-encoded.
fn quote_strict(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Python `urlencode` value encoding (`quote_plus`).
fn quote_plus(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_samples_report_unknown() {
        let fresh = HealthSample {
            status: "HEALTHY".to_string(),
            checked_at: 100.0,
            profile_id: "p1".to_string(),
            latency_ms: Some(42),
            source: "clash-active-outbound".to_string(),
            reason: String::new(),
        };
        let payload = fresh.payload(Some(105.0));
        assert_eq!(payload["health_status"], "HEALTHY");
        assert_eq!(payload["latency_ms"], 42);

        let payload = fresh.payload(Some(200.0));
        assert_eq!(payload["health_status"], "UNKNOWN");
        assert_eq!(payload["health_reason"], "health sample stale or unavailable");
        assert_eq!(payload["latency_ms"], Value::Null);
    }

    #[test]
    fn default_sample_is_unknown() {
        let payload = HealthSample::default().payload(Some(1.0));
        assert_eq!(payload["health_status"], "UNKNOWN");
        assert_eq!(payload["health_reason"], "health sample stale or unavailable");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unknown_without_profile_identity() {
        let sample = probe_active_profile(
            "",
            None,
            "proxy",
            None,
            "http://127.0.0.1:10809",
            DEFAULT_HEALTH_URL,
            1.2,
            None,
        )
        .await;
        assert_eq!(sample.status, "UNKNOWN");
        assert_eq!(sample.reason, "active profile identity unavailable");
        assert_eq!(sample.source, "active-http-proxy");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unsupported_health_urls_are_unknown() {
        let sample = probe_active_profile(
            "p1",
            None,
            "proxy",
            None,
            "http://127.0.0.1:10809",
            "ftp://example.com/x",
            1.2,
            None,
        )
        .await;
        assert_eq!(sample.status, "UNKNOWN");
        assert_eq!(sample.reason, "unsupported HTTP health URL");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_local_proxy_is_unknown() {
        let sample = probe_active_profile(
            "p1",
            None,
            "proxy",
            None,
            "http://10.0.0.5:10809",
            DEFAULT_HEALTH_URL,
            1.2,
            None,
        )
        .await;
        assert_eq!(sample.status, "UNKNOWN");
        assert_eq!(sample.reason, "guaranteed active local HTTP proxy unavailable");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unverified_clash_route_is_unknown() {
        let client = ClashApiClient::new(1, "secret");
        let graph = json!({"proxies": {"plain": {"type": "Direct"}}});
        let sample = probe_active_profile(
            "p1",
            Some(&client),
            "plain",
            Some(&graph),
            "",
            DEFAULT_HEALTH_URL,
            1.2,
            None,
        )
        .await;
        assert_eq!(sample.status, "UNKNOWN");
        assert_eq!(sample.reason, "active outbound type/selection unverified");
        assert_eq!(sample.source, "clash-active-outbound");
    }
}
