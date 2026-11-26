//! `strict_proxy.py`: explicit proxy selection that cannot be bypassed by
//! `NO_PROXY`, mapped onto a reqwest client with the safe redirect policy from
//! `lumen_core::http_utils` (`SafeRedirectHandler`).
//!
//! Python's `opener.open(req, timeout=...)` applies the socket timeout to
//! every operation, so a download may run longer than the timeout while data
//! keeps flowing. reqwest's blocking builder has no read timeout, therefore
//! each call drives an async reqwest client on a private current-thread tokio
//! runtime with `connect_timeout` + `read_timeout` — the exact per-operation
//! semantics — and exposes the body through a blocking `Read`.

use std::io::Read;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use url::Url;

use lumen_core::http_utils::redirect_blocked;

/// A streamed response body that reads like Python's `resp.read(n)`: blocks
/// until data arrives (bounded by the read timeout), returns 0 at EOF.
#[derive(Debug)]
pub struct StrictResponse {
    body: reqwest::Response,
    runtime: tokio::runtime::Runtime,
    pending: Vec<u8>,
}

impl Read for StrictResponse {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.pending.is_empty() {
            let chunk = match self.runtime.block_on(self.body.chunk()) {
                Ok(Some(chunk)) => chunk,
                Ok(None) => return Ok(0),
                Err(error) => return Err(std::io::Error::other(error.to_string())),
            };
            self.pending = chunk.to_vec();
        }
        let take = self.pending.len().min(buf.len());
        buf[..take].copy_from_slice(&self.pending[..take]);
        self.pending.drain(..take);
        Ok(take)
    }
}

/// `proxy_opener(...).open(Request(url, headers=...), timeout=...)` for GET
/// requests. Follows redirects like urllib with `SafeRedirectHandler` and
/// raises (like urllib's `HTTPError`) on error codes and blocked redirects.
pub fn strict_proxy_get(
    proxy_url: &str,
    timeout: f64,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<StrictResponse> {
    if proxy_url.trim().is_empty() {
        bail!("Proxy URL is required; direct fallback is forbidden");
    }
    let parsed = Url::parse(proxy_url)?;
    if parsed.scheme() != "http" || parsed.host_str().is_none() {
        bail!("Only an explicit HTTP proxy is supported");
    }
    let timeout = timeout.max(0.001);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // `.no_proxy()` before `.proxy()`: StrictProxyHandler deliberately ignores
    // proxy_bypass/NO_PROXY, including `*`. `Proxy::all` carries the
    // Proxy-Authorization header derived from URL userinfo.
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::all(proxy_url)?)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs_f64(timeout))
        .read_timeout(Duration::from_secs_f64(timeout))
        .build()?;
    let mut current: Url = url.parse().map_err(|e| anyhow!("invalid URL: {e}"))?;
    let mut redirects = 0;
    let mut effective_headers: Vec<(&str, &str)> = headers.to_vec();
    loop {
        let mut request = client.get(current.clone());
        for (name, value) in &effective_headers {
            request = request.header(*name, *value);
        }
        let response = runtime
            // send() creates reqwest's read-timeout timer synchronously.
            // Construct it inside the runtime, not as an argument to block_on.
            .block_on(async { request.send().await })
            .map_err(|e| anyhow!("HTTP request failed: {e}"))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(StrictResponse {
                body: response,
                runtime,
                pending: Vec::new(),
            });
        }
        let followed = matches!(status, 301 | 302 | 303 | 307 | 308);
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if !followed {
            // urllib's default error handler raises for every unhandled
            // status (300/304 and 4xx/5xx included).
            return Err(anyhow!("HTTP Error {status}"));
        }
        let Some(location) = location else {
            return Err(anyhow!("HTTP Error {status}"));
        };
        // urllib consumes the redirect body before following it.
        let _ = runtime.block_on(async {
            let mut body = response;
            while let Ok(Some(_chunk)) = body.chunk().await {}
        });
        let dest = current
            .join(&location)
            .map_err(|e| anyhow!("HTTP Error {status}: {e}"))?;
        // SafeRedirectHandler.redirect_request: block non-http(s), https
        // downgrades and destinations carrying credentials.
        if redirect_blocked(&current, &dest, &location) {
            return Err(anyhow!("Unsafe subscription redirect blocked"));
        }
        if origin(&current) != origin(&dest) {
            // Only non-identifying negotiation headers cross origins. This
            // also strips custom HWID headers, validators and credentials.
            effective_headers.retain(|(name, _)| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "accept" | "accept-encoding" | "user-agent"
                )
            });
        }
        // urllib loop detection: max_redirections = 10, max_repeats = 4.
        redirects += 1;
        if redirects > 10 {
            return Err(anyhow!("HTTP Error {status}: exceeded 10 redirects"));
        }
        current = dest;
    }
}

fn origin(url: &Url) -> (String, String, Option<u16>) {
    (
        url.scheme().into(),
        url.host_str().unwrap_or("").to_lowercase(),
        url.port_or_known_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_proxy_get_body_stall_obeys_read_timeout() -> Result<()> {
        use std::io::Write;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let proxy_url = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            let (mut socket, _) = listener.accept()?;
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request = [0u8; 4096];
            socket.read(&mut request)?;
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n")?;
            std::thread::sleep(Duration::from_millis(400));
            Ok(())
        });

        let mut response =
            strict_proxy_get(&proxy_url, 0.1, "http://speedtest.invalid/probe", &[])?;
        let mut body = Vec::new();
        assert!(response.read_to_end(&mut body).is_err());
        assert!(body.is_empty());
        server.join().expect("local proxy thread")?;
        Ok(())
    }

    #[test]
    fn test_strict_proxy_get_streams_response_on_plain_thread() -> Result<()> {
        use std::io::Write;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let proxy_url = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            // Bound the fixture too: a client panic must not leave a test server waiting.
            listener.set_nonblocking(true)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return Ok(());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte)?;
                request.push(byte[0]);
            }
            assert!(request.starts_with(b"GET http://speedtest.invalid/probe HTTP/1.1\r\n"));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n")?;
            std::thread::sleep(Duration::from_millis(30));
            socket.write_all(b"test")
        });

        let response = strict_proxy_get(&proxy_url, 1.0, "http://speedtest.invalid/probe", &[]);
        let mut body = String::new();
        response?.read_to_string(&mut body)?;
        assert_eq!(body, "test");
        server.join().expect("local proxy thread")?;
        Ok(())
    }

    #[test]
    fn empty_or_non_http_proxy_is_rejected() {
        let error = strict_proxy_get("", 3.0, "https://example.invalid", &[])
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Proxy URL is required; direct fallback is forbidden");
        let error = strict_proxy_get(
            "socks5://127.0.0.1:1080",
            3.0,
            "https://example.invalid",
            &[],
        )
        .unwrap_err()
        .to_string();
        assert_eq!(error, "Only an explicit HTTP proxy is supported");
    }
}
