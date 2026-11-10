//! Authenticated, cancellable loopback Clash API reads without ambient proxies.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value;

pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// Process-relative monotonic clock, the equivalent of Python `time.monotonic()`.
pub fn monotonic_seconds() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_secs_f64()
}

#[derive(Debug, Clone)]
pub struct MetricsApiError {
    pub reason: String,
    pub status: i32,
    pub unsupported: bool,
}

impl MetricsApiError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into(), status: 0, unsupported: false }
    }

    fn with_status(reason: impl Into<String>, status: u16, unsupported: bool) -> Self {
        Self { reason: reason.into(), status: i32::from(status), unsupported }
    }
}

impl std::fmt::Display for MetricsApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for MetricsApiError {}

/// Python freezes parsed documents into `MappingProxyType` trees; serde_json
/// `Value` is immutable behind `&`, so no explicit freeze is needed here.
pub struct ClashApiClient {
    port: u16,
    secret: String,
    closed: AtomicBool,
    active: Mutex<Option<TcpStream>>,
}

impl ClashApiClient {
    pub fn new(port: u16, secret: &str) -> Self {
        Self {
            port,
            secret: secret.to_string(),
            closed: AtomicBool::new(false),
            active: Mutex::new(None),
        }
    }

    pub fn get(&self, path: &str, timeout: f64) -> Result<Value, MetricsApiError> {
        if self.secret.is_empty() {
            return Err(MetricsApiError::new("Clash API authentication unavailable"));
        }
        if self.port == 0 || !path.starts_with('/') || path.starts_with("//") {
            return Err(MetricsApiError::new("Invalid loopback API target"));
        }
        let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.05).min(3.0));
        // Check the closed flag before touching the network, exactly like the
        // Python client checks it before its first `request()` call.
        if self.closed.load(Ordering::SeqCst) {
            return Err(MetricsApiError::new("stopped"));
        }
        let addr: SocketAddr = format!("127.0.0.1:{}", self.port)
            .parse()
            .map_err(|_| MetricsApiError::new("Invalid loopback API target"))?;
        let mut stream = TcpStream::connect_timeout(&addr, to_duration(timeout))
            .map_err(|_| MetricsApiError::new("Clash API unavailable (OSError)"))?;
        stream
            .set_read_timeout(Some(to_duration(timeout)))
            .and_then(|_| stream.set_write_timeout(Some(to_duration(timeout))))
            .map_err(|_| MetricsApiError::new("Clash API unavailable (OSError)"))?;
        {
            let mut slot = self.active.lock();
            if self.closed.load(Ordering::SeqCst) {
                return Err(MetricsApiError::new("stopped"));
            }
            match stream.try_clone() {
                Ok(clone) => *slot = Some(clone),
                Err(_) => return Err(MetricsApiError::new("Clash API unavailable (OSError)")),
            }
        }
        let result = self.read_document(&mut stream, path, deadline, timeout);
        let _ = stream.shutdown(Shutdown::Both);
        {
            let mut slot = self.active.lock();
            slot.take();
        }
        result
    }

    fn read_document(
        &self,
        stream: &mut TcpStream,
        path: &str,
        deadline: Instant,
        timeout: f64,
    ) -> Result<Value, MetricsApiError> {
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept-Encoding: identity\r\nAuthorization: Bearer {secret}\r\nConnection: close\r\n\r\n",
            port = self.port,
            secret = self.secret,
        );
        if let Err(_) = stream.write_all(request.as_bytes()) {
            return Err(MetricsApiError::new("Clash API unavailable (OSError)"));
        }
        // HTTPConnection ignores HTTP(S)_PROXY and NO_PROXY, and never follows
        // redirects. The bearer credential cannot escape this loopback peer.
        let mut buffer: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = [0u8; 65536];
        let mut head_done = false;
        let (status, content_length, chunked) = loop {
            if head_done {
                break parse_head(&buffer)?;
            }
            let read = stream.read(&mut chunk);
            match read {
                Ok(0) => return Err(MetricsApiError::new("Clash API unavailable (BadStatusLine)")),
                Ok(size) => {
                    buffer.extend_from_slice(&chunk[..size]);
                    if find_head_end(&buffer).is_some() {
                        head_done = true;
                    }
                }
                Err(err) => return Err(MetricsApiError::new(io_reason(&err))),
            }
        };
        let head_len = find_head_end(&buffer).unwrap_or(buffer.len());
        if status != 200 {
            // Core delay handlers may report unsupported transports as 504.
            // Inspect a bounded error body without logging it or treating
            // capability errors as failed profile health.
            let body_len = buffer.len() - head_len;
            if body_len < 4096 {
                let _ = read_more(stream, &mut buffer, 4096 - body_len);
            }
            let body = &buffer[head_len..];
            let text = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_lowercase();
            let unsupported = ["unsupported", "not supported", "not implemented"]
                .iter()
                .any(|term| text.contains(term));
            return Err(MetricsApiError::with_status(
                format!("Clash API HTTP {status}"),
                status,
                unsupported,
            ));
        }
        let mut size = buffer.len() - head_len;
        if size > MAX_DOCUMENT_BYTES {
            return Err(MetricsApiError::new("Clash API document exceeds limit"));
        }
        loop {
            if body_complete(&buffer[head_len..], content_length, chunked) {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(MetricsApiError::new("Clash API deadline exceeded"));
            }
            if self.closed.load(Ordering::SeqCst) {
                return Err(MetricsApiError::new("stopped"));
            }
            let _ = stream.set_read_timeout(Some(remaining.max(Duration::from_millis(1))));
            let wanted = ((MAX_DOCUMENT_BYTES + 1).saturating_sub(size)).min(65536);
            match read_more(stream, &mut buffer, wanted) {
                Ok(0) => break,
                Ok(_) => {
                    size = buffer.len() - head_len;
                    if size > MAX_DOCUMENT_BYTES {
                        return Err(MetricsApiError::new("Clash API document exceeds limit"));
                    }
                }
                Err(err) => return Err(MetricsApiError::new(io_reason(&err))),
            }
        }
        let body = &buffer[head_len..];
        if chunked && !body.ends_with(b"0\r\n\r\n") {
            return Err(MetricsApiError::new("Clash API unavailable (IncompleteRead)"));
        }
        let document: Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => return Err(MetricsApiError::new("Clash API unavailable (JSONDecodeError)")),
        };
        if !document.is_object() {
            return Err(MetricsApiError::new("Invalid Clash API document"));
        }
        Ok(document)
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let connection = self.active.lock().take();
        if let Some(socket) = connection {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

fn to_duration(seconds: f64) -> Duration {
    if seconds <= 0.0 {
        Duration::from_millis(1)
    } else {
        Duration::from_secs_f64(seconds)
    }
}

fn io_reason(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            "Clash API unavailable (TimeoutError)".to_string()
        }
        _ => "Clash API unavailable (OSError)".to_string(),
    }
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n").map(|pos| pos + 4)
}

/// `(status, content_length, chunked)` parsed from the response head.
fn parse_head(buffer: &[u8]) -> Result<(u16, Option<u64>, bool), MetricsApiError> {
    let head_len = find_head_end(buffer)
        .ok_or_else(|| MetricsApiError::new("Clash API unavailable (BadStatusLine)"))?;
    let head = String::from_utf8_lossy(&buffer[..head_len]);
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| MetricsApiError::new("Clash API unavailable (BadStatusLine)"))?;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            content_length = value.parse().ok();
        } else if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
    }
    Ok((status, content_length, chunked))
}

fn read_more(stream: &mut TcpStream, buffer: &mut Vec<u8>, max: usize) -> std::io::Result<usize> {
    if max == 0 {
        return Ok(0);
    }
    let mut chunk = vec![0u8; max];
    let size = stream.read(&mut chunk)?;
    buffer.extend_from_slice(&chunk[..size]);
    Ok(size)
}

fn body_complete(body: &[u8], content_length: Option<u64>, chunked: bool) -> bool {
    if let Some(length) = content_length {
        return body.len() as u64 >= length;
    }
    if chunked {
        return body.ends_with(b"0\r\n\r\n");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::net::TcpListener;

    fn serve_once(response: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let _ = stream.write_all(response.as_bytes());
        });
        port
    }

    #[test]
    fn reads_authenticated_document() {
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\": 42}",
        );
        let client = ClashApiClient::new(port, "s3cret");
        let document = client.get("/proxies/x/delay", 2.0).unwrap();
        assert_eq!(document["delay"], 42);
    }

    #[test]
    fn flags_unsupported_transport_status() {
        let body = "HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 21\r\nConnection: close\r\n\r\nunsupported transport";
        let port = serve_once(body);
        let client = ClashApiClient::new(port, "s3cret");
        let error = client.get("/x", 2.0).unwrap_err();
        assert_eq!(error.status, 504);
        assert!(error.unsupported);
        assert_eq!(error.reason, "Clash API HTTP 504");
    }

    #[test]
    fn rejects_unauthenticated_and_invalid_targets() {
        let client = ClashApiClient::new(19090, "");
        assert_eq!(
            client.get("/x", 1.0).unwrap_err().reason,
            "Clash API authentication unavailable"
        );
        let client = ClashApiClient::new(19090, "secret");
        assert_eq!(client.get("//x", 1.0).unwrap_err().reason, "Invalid loopback API target");
        assert_eq!(client.get("x", 1.0).unwrap_err().reason, "Invalid loopback API target");
    }

    #[test]
    fn closed_client_refuses_before_connecting() {
        let client = ClashApiClient::new(1, "secret");
        client.close();
        assert_eq!(client.get("/x", 1.0).unwrap_err().reason, "stopped");
    }
}
