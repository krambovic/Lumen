//! Port of `engines/singbox/clash_api.py`: authenticated client for the
//! local sing-box Clash-compatible API (selector switching with rollback).

use std::io::Read;
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Value, json};

use lumen_core::constants::{PROXY_HOST, SINGBOX_CLASH_API_PORT};
use lumen_core::pyjson;

/// A selector lists every stored node; thousands of tags exceed 64 KiB.
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// `urllib.parse.quote(leaf, safe='')`.
const QUOTE_SET: &AsciiSet = &NON_ALPHANUMERIC.remove(b'_').remove(b'.').remove(b'-').remove(b'~');

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ClashApiError(pub String);

impl ClashApiError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorSwitchResult {
    pub selector: String,
    pub previous: String,
    pub current: String,
    pub changed: bool,
}

pub struct SingboxClashApiClient {
    base_url: String,
    secret: String,
    timeout: f64,
}

impl SingboxClashApiClient {
    pub fn new(secret: &str) -> Self {
        Self::with_options(SINGBOX_CLASH_API_PORT, secret, 2.0, PROXY_HOST)
    }

    pub fn with_options(port: u16, secret: &str, timeout: f64, host: &str) -> Self {
        Self {
            base_url: String::from("http://") + host + ":" + &port.to_string(),
            secret: secret.to_string(),
            timeout: if timeout.is_finite() { timeout.max(0.1) } else { 2.0 },
        }
    }

    pub fn get_proxy(&self, name: &str) -> Result<Value, ClashApiError> {
        let payload = self.request("GET", &(String::from("/proxies/") + name), None, true)?;
        if !payload.is_object() {
            return Err(ClashApiError::new("sing-box Clash API returned a non-object proxy response"));
        }
        Ok(payload)
    }

    pub fn put_proxy(&self, name: &str, target: &str) -> Result<(), ClashApiError> {
        self.request("PUT", &(String::from("/proxies/") + name), Some(json!({"name": target})), false)?;
        Ok(())
    }

    pub fn switch_selector(&self, selector: &str, target: &str) -> Result<SelectorSwitchResult, ClashApiError> {
        let selector = selector.trim();
        let target = target.trim();
        if selector.is_empty() || target.is_empty() {
            return Err(ClashApiError::new("selector and target are required"));
        }
        let before = self.get_proxy(selector)?;
        let previous = selector_now(&before, selector)?;
        let available = selector_members(&before, selector)?;
        if !available.iter().any(|m| m == target) {
            return Err(ClashApiError(format!("selector `{selector}` does not contain target `{target}`")));
        }
        if !available.iter().any(|m| *m == previous) {
            return Err(ClashApiError(format!("selector `{selector}` has invalid current target `{previous}`")));
        }
        if previous == target {
            return Ok(SelectorSwitchResult {
                selector: selector.into(),
                previous: previous.clone(),
                current: previous,
                changed: false,
            });
        }
        let attempt = || -> Result<(), ClashApiError> {
            self.put_proxy(selector, target)?;
            let after = self.get_proxy(selector)?;
            let current = selector_now(&after, selector)?;
            if current != target {
                return Err(ClashApiError(format!(
                    "selector `{selector}` did not confirm target `{target}` (current `{current}`)"
                )));
            }
            Ok(())
        };
        if let Err(error) = attempt() {
            let rollback_error = self.rollback(selector, &previous);
            if !rollback_error.is_empty() {
                return Err(ClashApiError(format!("{error}; rollback failed: {rollback_error}")));
            }
            return Err(error);
        }
        Ok(SelectorSwitchResult { selector: selector.into(), previous, current: target.into(), changed: true })
    }

    fn rollback(&self, selector: &str, previous: &str) -> String {
        let result = (|| -> Result<String, ClashApiError> {
            self.put_proxy(selector, previous)?;
            selector_now(&self.get_proxy(selector)?, selector)
        })();
        match result {
            Ok(restored) if restored != previous => format!("current target is `{restored}`"),
            Ok(_) => String::new(),
            Err(error) => error.0,
        }
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>, expect_json: bool) -> Result<Value, ClashApiError> {
        if self.secret.is_empty() {
            return Err(ClashApiError::new("sing-box Clash API secret is not configured"));
        }
        let path_without_query = path.split('?').next().unwrap_or_default();
        let Some((prefix, leaf)) = path_without_query.rsplit_once('/') else {
            return Err(ClashApiError(format!("invalid Clash API path `{path}`")));
        };
        if leaf.is_empty() {
            return Err(ClashApiError(format!("invalid Clash API path `{path}`")));
        }
        let url = self.base_url.clone() + prefix + "/" + &utf8_percent_encode(leaf, QUOTE_SET).to_string();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs_f64(self.timeout))
            .build()
            .map_err(|e| ClashApiError(format!("Clash API {method} {path} failed: {e}")))?;
        let http_method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| ClashApiError(format!("invalid Clash API method `{method}`")))?;
        let mut request = client
            .request(http_method, &url)
            .header("Authorization", String::from("Bearer ") + &self.secret)
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(pyjson::dumps(&body, true, None));
        }
        let response = request.send().map_err(|e| ClashApiError(format!("Clash API {method} {path} failed: {e}")))?;
        let status = response.status().as_u16();
        if status >= 400 {
            let mut detail = Vec::new();
            let _ = response.take(4096).read_to_end(&mut detail);
            let detail = String::from_utf8_lossy(&detail).trim().to_string();
            let suffix = if detail.is_empty() {
                String::new()
            } else {
                String::from(": ") + &detail.chars().take(300).collect::<String>()
            };
            return Err(ClashApiError(format!("Clash API {method} {path} failed with HTTP {status}{suffix}")));
        }
        let mut raw = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut raw)
            .map_err(|e| ClashApiError(format!("Clash API {method} {path} failed: {e}")))?;
        if !expect_json {
            return Ok(Value::Null);
        }
        if raw.is_empty() {
            return Err(ClashApiError(format!("Clash API {method} {path} returned an empty response")));
        }
        let invalid = || ClashApiError(format!("Clash API {method} {path} returned invalid JSON"));
        let text = std::str::from_utf8(&raw).map_err(|_| invalid())?;
        serde_json::from_str(text).map_err(|_| invalid())
    }
}

/// `str(value or "")`.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(false) => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Number(n) if n.as_f64() == Some(0.0) => String::new(),
        Value::Array(a) if a.is_empty() => String::new(),
        Value::Object(o) if o.is_empty() => String::new(),
        other => other.to_string(),
    }
}

fn selector_now(payload: &Value, selector: &str) -> Result<String, ClashApiError> {
    let current = value_text(payload.get("now").unwrap_or(&Value::Null)).trim().to_string();
    if current.is_empty() {
        return Err(ClashApiError(format!("Clash API proxy `{selector}` has no current target")));
    }
    Ok(current)
}

fn selector_members(payload: &Value, selector: &str) -> Result<Vec<String>, ClashApiError> {
    let Some(raw) = payload.get("all").and_then(Value::as_array) else {
        return Err(ClashApiError(format!("Clash API proxy `{selector}` is not a selector")));
    };
    let mut members: Vec<String> = Vec::new();
    for item in raw {
        let text = value_text(item).trim().to_string();
        if !text.is_empty() && !members.contains(&text) {
            members.push(text);
        }
    }
    if members.is_empty() {
        return Err(ClashApiError(format!("Clash API selector `{selector}` has no members")));
    }
    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Minimal fake Clash API: GET returns the selector, PUT changes `now`.
    fn serve(now: &str, members: &[&str], confirm: bool) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(Mutex::new(now.to_string()));
        let members: Vec<String> = members.iter().map(|s| s.to_string()).collect();
        let log2 = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0usize;
                let mut auth = String::new();
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    let lower = header.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = header.trim().to_string();
                    }
                }
                let mut body = vec![0u8; length];
                std::io::Read::read_exact(&mut reader, &mut body).unwrap();
                log2.lock().unwrap().push(line.trim().to_string() + " " + &auth);
                let response_body = if line.starts_with("PUT") {
                    let parsed: Value = serde_json::from_slice(&body).unwrap();
                    if confirm {
                        *state.lock().unwrap() = parsed["name"].as_str().unwrap().to_string();
                    }
                    String::new()
                } else {
                    json!({"now": state.lock().unwrap().clone(), "all": members}).to_string()
                };
                let status = if line.starts_with("PUT") { "204 No Content" } else { "200 OK" };
                let reply = String::from("HTTP/1.1 ")
                    + status
                    + "\r\nContent-Length: "
                    + &response_body.len().to_string()
                    + "\r\nConnection: close\r\n\r\n"
                    + &response_body;
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        (port, log)
    }

    #[test]
    fn switches_and_confirms() {
        let (port, log) = serve("a", &["a", "b c"], true);
        let client = SingboxClashApiClient::with_options(port, "s3", 2.0, "127.0.0.1");
        let result = client.switch_selector("proxy", "b c").unwrap();
        assert!(result.changed);
        assert_eq!(result.previous, "a");
        assert_eq!(result.current, "b c");
        let unchanged = client.switch_selector("proxy", "b c").unwrap();
        assert!(!unchanged.changed);
        let log = log.lock().unwrap();
        assert!(log.iter().all(|l| l.ends_with("Bearer s3")));
        assert!(log.iter().any(|l| l.starts_with("PUT /proxies/proxy ")));
    }

    #[test]
    fn rejects_unknown_target_and_reports_unconfirmed_switch() {
        let (port, _) = serve("a", &["a", "b"], false);
        let client = SingboxClashApiClient::with_options(port, "s", 2.0, "127.0.0.1");
        let error = client.switch_selector("proxy", "zzz").unwrap_err();
        assert_eq!(error.0, "selector `proxy` does not contain target `zzz`");
        let error = client.switch_selector("proxy", "b").unwrap_err();
        assert_eq!(error.0, "selector `proxy` did not confirm target `b` (current `a`)");
        assert_eq!(
            SingboxClashApiClient::with_options(port, "", 2.0, "127.0.0.1").get_proxy("proxy").unwrap_err().0,
            "sing-box Clash API secret is not configured"
        );
    }
}
