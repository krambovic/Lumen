//! Warm-ping transport: one established HTTP CONNECT tunnel with verified
//! TLS, the Rust counterpart of Python's
//! `http.client.HTTPSConnection(...)` + `set_tunnel(...)` + `connect()`.
//!
//! The worker must never silently reconnect inside a timed sample, so the
//! tunnel is a raw socket + rustls session; reqwest pooling cannot guarantee
//! that property.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const READ_LIMIT: usize = 64 * 1024;
const MAXLINE: usize = 65537;
const HEAD_LIMIT: usize = 256 * 1024;

/// Result of one GET round trip, mirroring the `http.client` fields the
/// worker inspects: `response.status`, `response.will_close` and
/// `response.isclosed()` after `response.read(64 * 1024)`.
pub struct TunnelGetResult {
    pub status: u16,
    pub will_close: bool,
    /// Python `response.isclosed()`: true when the body finished within the
    /// 64 KiB read (the file object was closed on completion).
    pub fully_consumed: bool,
}

pub trait WarmTunnel: Send {
    /// CONNECT and TLS handshake, outside the measured samples.
    fn connect(&mut self) -> Result<()>;
    /// Python `connection.sock is None`: never silently reconnect inside a
    /// timed sample.
    fn sock_closed(&self) -> bool;
    /// One GET round trip: request, response head, body read up to 64 KiB.
    fn get(&mut self, path: &str) -> Result<TunnelGetResult>;
    fn close(&mut self);
}

pub struct TlsTunnel {
    proxy_host: String,
    proxy_port: u16,
    target_host: String,
    target_port: u16,
    timeout: Duration,
    tls: Option<StreamOwned<ClientConnection, TcpStream>>,
}

impl TlsTunnel {
    pub fn new(
        proxy_host: &str,
        proxy_port: u16,
        target_host: &str,
        target_port: u16,
        timeout: f64,
    ) -> Self {
        Self {
            proxy_host: proxy_host.to_string(),
            proxy_port,
            target_host: target_host.to_string(),
            target_port,
            timeout: Duration::from_secs_f64(timeout.max(0.001)),
            tls: None,
        }
    }

    fn socket_addr(&self, host: &str, port: u16) -> Result<SocketAddr> {
        (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| anyhow!("no address resolved for {host}:{port}"))
    }
}

impl WarmTunnel for TlsTunnel {
    fn connect(&mut self) -> Result<()> {
        let addr = self.socket_addr(&self.proxy_host, self.proxy_port)?;
        let mut sock = TcpStream::connect_timeout(&addr, self.timeout)?;
        sock.set_read_timeout(Some(self.timeout))?;
        sock.set_write_timeout(Some(self.timeout))?;

        // Python `_tunnel()`: `CONNECT host:port HTTP/1.0` with no extra
        // headers because `set_tunnel` was called without them.
        let connect = format!(
            "CONNECT {}:{} HTTP/1.0\r\n\r\n",
            self.target_host, self.target_port
        );
        sock.write_all(connect.as_bytes())?;
        sock.flush()?;

        let status = read_tunnel_response(&mut sock)?;
        if status != 200 {
            bail!("Tunnel connection failed: {status}");
        }

        let config = client_tls_config()?;
        let server_name = ServerName::try_from(self.target_host.clone())
            .map_err(|e| anyhow!("invalid tunnel hostname: {e}"))?;
        let connection = ClientConnection::new(Arc::new(config), server_name)
            .map_err(|e| anyhow!("TLS connection setup failed: {e}"))?;
        let mut tls = StreamOwned::new(connection, sock);
        drive_handshake(&mut tls)?;
        self.tls = Some(tls);
        Ok(())
    }

    fn sock_closed(&self) -> bool {
        self.tls.is_none()
    }

    fn get(&mut self, path: &str) -> Result<TunnelGetResult> {
        let tls = self
            .tls
            .as_mut()
            .ok_or_else(|| anyhow!("tunnel socket is closed"))?;
        // http.client `request()`: Host from the tunnel target (port only when
        // non-default), `Accept-Encoding: identity`, then the caller headers.
        let host = if self.target_port == 443 {
            self.target_host.clone()
        } else {
            format!("{}:{}", self.target_host, self.target_port)
        };
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept-Encoding: identity\r\nUser-Agent: Lumen/SpeedTest\r\nConnection: keep-alive\r\nCache-Control: no-cache\r\n\r\n"
        );
        tls.write_all(request.as_bytes())?;
        tls.flush()?;

        read_get_response(tls)
    }

    fn close(&mut self) {
        self.tls = None;
    }
}

fn read_get_response(reader: impl Read) -> Result<TunnelGetResult> {
    let mut reader = BufReader::new(reader);
    let head = read_response_head(&mut reader)?;
    let will_close = check_close(&head);
    let fully_consumed = read_body_bounded(&mut reader, &head)?;
    Ok(TunnelGetResult {
        status: head.status,
        will_close,
        fully_consumed,
    })
}

/// Read the CONNECT response: status line plus headers up to the blank line.
fn read_tunnel_response(sock: &mut TcpStream) -> Result<u16> {
    let mut reader = BufReader::new(sock);
    let status_line = read_crlf_line(&mut reader, MAXLINE)?;
    let status = parse_status_line(&status_line)?;
    let mut total = status_line.len();
    loop {
        let line = read_crlf_line(&mut reader, MAXLINE)?;
        total += line.len();
        if total > HEAD_LIMIT {
            bail!("tunnel response headers too large");
        }
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
    }
    Ok(status)
}

pub(crate) fn parse_status_line(line: &str) -> Result<u16> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.split(' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        bail!("bad tunnel status line: {line}");
    }
    let code = parts.next().unwrap_or("");
    code.parse::<u16>()
        .map_err(|_| anyhow!("bad tunnel status code: {line}"))
}

fn read_crlf_line(reader: &mut dyn Read, cap: usize) -> Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            break;
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
        if line.len() > cap {
            bail!("header line too long");
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

struct ResponseHead {
    status: u16,
    http_version_10: bool,
    headers: Vec<(String, String)>,
    content_length: Option<u64>,
    chunked: bool,
}

fn read_response_head(reader: &mut impl Read) -> Result<ResponseHead> {
    let status_line = read_crlf_line(reader, MAXLINE)?;
    let trimmed = status_line.trim_end_matches(['\r', '\n']);
    let mut parts = trimmed.split(' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        bail!("bad response status line: {trimmed}");
    }
    let minor = version
        .strip_prefix("HTTP/1.")
        .and_then(|rest| rest.chars().next());
    let http_version_10 = minor == Some('0');
    let status = parts
        .next()
        .unwrap_or("")
        .parse::<u16>()
        .map_err(|_| anyhow!("bad response status: {trimmed}"))?;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut total = status_line.len();
    loop {
        let line = read_crlf_line(reader, MAXLINE)?;
        total += line.len();
        if total > HEAD_LIMIT {
            bail!("response headers too large");
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let header = |name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    };
    let no_body = (100..200).contains(&status) || matches!(status, 204 | 304);
    let chunked = !no_body
        && header("transfer-encoding").is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
    let content_length = if no_body {
        Some(0)
    } else {
        header("content-length").and_then(|value| value.parse::<u64>().ok())
    };
    Ok(ResponseHead {
        status,
        http_version_10,
        headers,
        content_length,
        chunked,
    })
}

/// CPython `HTTPResponse._check_close`.
fn check_close(head: &ResponseHead) -> bool {
    if head.content_length.is_none() && !head.chunked {
        return true;
    }
    let connection = head
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("connection"))
        .map(|(_, value)| value.to_ascii_lowercase());
    if !head.http_version_10 {
        connection
            .map(|value| value.contains("close"))
            .unwrap_or(false)
    } else {
        let keep_alive = head
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("keep-alive"))
            || connection
                .map(|value| value.contains("keep-alive"))
                .unwrap_or(false);
        !keep_alive
    }
}

/// Read up to `want` bytes; returns the count actually read (short read only
/// on EOF).
fn read_bounded(reader: &mut dyn Read, buffer: &mut [u8], want: usize) -> Result<usize> {
    let mut got = 0usize;
    while got < want {
        let size = (want - got).min(buffer.len());
        let read = reader.read(&mut buffer[..size])?;
        if read == 0 {
            break;
        }
        got += read;
    }
    Ok(got)
}

/// `response.read(64 * 1024)` with `http.client` length bookkeeping; returns
/// whether the body finished inside the read window (`response.isclosed()`).
fn read_body_bounded(reader: &mut impl Read, head: &ResponseHead) -> Result<bool> {
    let mut buffer = vec![0u8; 4096];
    let mut consumed = 0usize;
    if head.chunked {
        // Chunked transfer: read until the terminating chunk or the budget.
        loop {
            let size_line = read_crlf_line(reader, MAXLINE)?;
            let size_text = size_line.trim_end_matches(['\r', '\n']);
            let size_text = size_text.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_text, 16)
                .map_err(|_| anyhow!("bad chunk size: {size_text}"))?;
            if size > READ_LIMIT - consumed {
                return Ok(false);
            }
            if size == 0 {
                // Consume the trailer section up to the blank line.
                let mut trailer_size = 0;
                loop {
                    let line = read_crlf_line(reader, MAXLINE)?;
                    trailer_size += line.len();
                    if trailer_size > HEAD_LIMIT {
                        bail!("response trailers too large");
                    }
                    if line == "\r\n" || line == "\n" || line.is_empty() {
                        break;
                    }
                }
                return Ok(true);
            }
            let read = read_bounded(reader, &mut buffer, size)?;
            consumed += read;
            if read < size {
                return Ok(false);
            }
            // Chunk data is followed by CRLF.
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf)?;
        }
    }
    if let Some(length) = head.content_length {
        let want = length.min(READ_LIMIT as u64) as usize;
        let read = read_bounded(reader, &mut buffer, want)?;
        return Ok(length <= READ_LIMIT as u64 && read >= want);
    }
    // Close-delimited body: EOF inside the budget closes the connection.
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(true);
        }
        consumed += read;
        if consumed >= READ_LIMIT {
            return Ok(false);
        }
    }
}

/// Drive the TLS handshake to completion over the raw socket.
fn drive_handshake(tls: &mut StreamOwned<ClientConnection, TcpStream>) -> Result<()> {
    while tls.conn.is_handshaking() {
        if tls.conn.wants_write() {
            tls.conn.write_tls(&mut tls.sock)?;
        }
        if tls.conn.wants_read() {
            let read = tls.conn.read_tls(&mut tls.sock)?;
            if read == 0 {
                bail!("TLS handshake interrupted by EOF");
            }
            tls.conn
                .process_new_packets()
                .map_err(|e| anyhow!("TLS handshake failed: {e}"))?;
        }
    }
    Ok(())
}

fn client_tls_config() -> Result<ClientConfig> {
    let mut roots = RootCertStore::empty();
    // rustls-native-certs 0.8 returns a CertificateResult with the loaded
    // certificates plus per-error diagnostics.
    let result = rustls_native_certs::load_native_certs();
    for cert in result.certs {
        let _ = roots.add(cert);
    }
    if roots.is_empty() {
        bail!("no trusted certificates available for the warm ping tunnel");
    }
    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_response_reads_body_buffered_with_headers() -> Result<()> {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ntest";
        let result = read_get_response(&response[..])?;
        assert!(result.fully_consumed);
        assert!(!result.will_close);
        Ok(())
    }

    #[test]
    fn warm_no_content_response_keeps_the_connection_reusable() -> Result<()> {
        let response = b"HTTP/1.1 204 No Content\r\nConnection: keep-alive\r\n\r\n";
        let result = read_get_response(&response[..])?;
        assert_eq!(result.status, 204);
        assert!(result.fully_consumed);
        assert!(!result.will_close);
        Ok(())
    }
}
