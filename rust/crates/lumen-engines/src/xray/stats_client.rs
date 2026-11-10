//! Persistent, loopback-only Xray StatsService client (`xray_stats_client.py`).

use std::collections::BTreeMap;
use std::error::Error;
use std::time::Duration;

use parking_lot::Mutex;
use prost::bytes::{Buf, BufMut};
use prost::encoding::{decode_varint, encode_varint};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::Instant;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::codegen::http::uri::PathAndQuery;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Response, Status, TimeoutExpired};

pub const STATS_METHOD: &str = "/xray.app.stats.command.StatsService/QueryStats";
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub type Stats = BTreeMap<String, i64>;

pub fn encode_query_stats_request(pattern: &str) -> Vec<u8> {
    let mut request = Vec::with_capacity(pattern.len() + 2);
    request.push(0x0a);
    encode_varint(pattern.len() as u64, &mut request);
    request.extend_from_slice(pattern.as_bytes());
    // Leave reset=false so polling does not affect other clients.
    request
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StatsDecodeError(&'static str);

enum WireValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

fn take_bytes<'a>(data: &mut &'a [u8], len: u64) -> Result<&'a [u8], StatsDecodeError> {
    let len = usize::try_from(len).map_err(|_| StatsDecodeError("truncated protobuf field"))?;
    let (bytes, rest) = data
        .split_at_checked(len)
        .ok_or(StatsDecodeError("truncated protobuf field"))?;
    *data = rest;
    Ok(bytes)
}

fn read_varint(data: &mut &[u8]) -> Result<u64, StatsDecodeError> {
    decode_varint(data).map_err(|_| StatsDecodeError("invalid protobuf varint"))
}

fn read_field<'a>(data: &mut &'a [u8]) -> Result<(u64, WireValue<'a>), StatsDecodeError> {
    let key = read_varint(data)?;
    let field = key >> 3;
    if field == 0 {
        return Err(StatsDecodeError("invalid protobuf field"));
    }
    let value = match key & 7 {
        0 => WireValue::Varint(read_varint(data)?),
        2 => {
            let len = read_varint(data)?;
            WireValue::Bytes(take_bytes(data, len)?)
        }
        wire @ (1 | 5) => {
            take_bytes(data, if wire == 1 { 8 } else { 4 })?;
            WireValue::Fixed
        }
        _ => return Err(StatsDecodeError("unsupported protobuf wire type")),
    };
    Ok((field, value))
}

pub fn decode_query_stats_response(mut data: &[u8]) -> Result<Stats, StatsDecodeError> {
    if data.len() > MAX_RESPONSE_BYTES {
        return Err(StatsDecodeError("stats response exceeds limit"));
    }
    let mut stats = Stats::new();
    while !data.is_empty() {
        let (field, value) = read_field(&mut data)?;
        if field != 1 {
            continue;
        }
        let WireValue::Bytes(mut message) = value else {
            return Err(StatsDecodeError("invalid Stat message"));
        };
        let (mut name, mut counter) = ("", 0);
        while !message.is_empty() {
            match read_field(&mut message)? {
                (1, WireValue::Bytes(raw)) => {
                    name = std::str::from_utf8(raw)
                        .map_err(|_| StatsDecodeError("invalid Stat name"))?;
                }
                (2, WireValue::Varint(raw)) => counter = raw as i64,
                (1, _) => return Err(StatsDecodeError("invalid Stat name")),
                (2, _) => return Err(StatsDecodeError("invalid Stat value")),
                _ => {}
            }
        }
        if name.is_empty() || counter < 0 || stats.insert(name.to_owned(), counter).is_some() {
            return Err(StatsDecodeError("invalid or duplicate traffic counter"));
        }
    }
    Ok(stats)
}

#[derive(Clone, Copy)]
struct RawProtobuf;

impl Codec for RawProtobuf {
    type Encode = Vec<u8>;
    type Decode = Vec<u8>;
    type Encoder = Self;
    type Decoder = Self;

    fn encoder(&mut self) -> Self {
        *self
    }

    fn decoder(&mut self) -> Self {
        *self
    }
}

impl Encoder for RawProtobuf {
    type Item = Vec<u8>;
    type Error = Status;

    fn encode(&mut self, message: Vec<u8>, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        dst.put_slice(&message);
        Ok(())
    }
}

impl Decoder for RawProtobuf {
    type Item = Vec<u8>;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Vec<u8>>, Status> {
        let mut message = vec![0; src.remaining()];
        src.copy_to_slice(&mut message);
        Ok(Some(message))
    }
}

struct ClientState {
    channel: Option<Channel>,
    failed_at: Option<Instant>,
    reason: String,
}

/// Lazily opens a reusable gRPC channel to the Xray loopback stats port.
pub struct XrayStatsClient {
    port: Option<u16>,
    timeout: Duration,
    retry_interval: Duration,
    state: Mutex<ClientState>,
    query_gate: AsyncMutex<()>,
    closed: watch::Sender<bool>,
}

impl XrayStatsClient {
    pub fn new(port: i64) -> Self {
        Self::with_options(port, Duration::from_millis(800), Duration::from_secs(3))
    }

    pub fn with_options(port: i64, timeout: Duration, retry_interval: Duration) -> Self {
        Self {
            port: u16::try_from(port).ok().filter(|port| *port != 0),
            timeout: timeout.clamp(Duration::from_millis(50), Duration::from_secs(2)),
            retry_interval: retry_interval.max(Duration::from_millis(100)),
            state: Mutex::new(ClientState {
                channel: None,
                failed_at: None,
                reason: "not sampled".into(),
            }),
            query_gate: AsyncMutex::new(()),
            closed: watch::Sender::new(false),
        }
    }

    pub fn reason(&self) -> String {
        self.state.lock().reason.clone()
    }

    pub async fn query(&self) -> Option<Stats> {
        let _sample = self.query_gate.lock().await;
        let mut closed = self.closed.subscribe();
        let channel = {
            let mut state = self.state.lock();
            if *closed.borrow() {
                return None;
            }
            let Some(port) = self.port else {
                state.reason = "Xray stats API port unavailable".into();
                return None;
            };
            if state
                .failed_at
                .is_some_and(|time| time.elapsed() < self.retry_interval)
            {
                return None;
            }
            state
                .channel
                .get_or_insert_with(|| {
                    // A direct TCP channel to a fixed IPv4 loopback address.
                    Endpoint::from_shared(format!("http://127.0.0.1:{port}"))
                        .expect("valid loopback URI")
                        .connect_lazy()
                })
                .clone()
        };
        let result = tokio::select! {
            _ = closed.wait_for(|closed| *closed) => return None,
            result = tokio::time::timeout(self.timeout, self.query_channel(channel)) => result,
        };
        let mut state = self.state.lock();
        if *closed.borrow() {
            return None;
        }
        let reason = match result {
            Ok(Ok(stats)) => {
                state.reason.clear();
                state.failed_at = None;
                return Some(stats);
            }
            Ok(Err(status)) => {
                let code = if std::iter::successors(status.source(), |&source| source.source())
                    .any(|source| source.is::<TimeoutExpired>())
                {
                    tonic::Code::DeadlineExceeded
                } else {
                    status.code()
                };
                format!("Xray stats unavailable ({code:?})")
            }
            Err(_) => "Xray stats unavailable (DeadlineExceeded)".into(),
        };
        state.reason = reason;
        state.failed_at = Some(Instant::now());
        state.channel = None;
        None
    }

    async fn query_channel(&self, channel: Channel) -> Result<Stats, Status> {
        let mut client = Grpc::new(channel).max_decoding_message_size(MAX_RESPONSE_BYTES);
        client
            .ready()
            .await
            .map_err(|error| Status::unavailable(error.to_string()))?;
        let mut request = Request::new(encode_query_stats_request("inbound>>>"));
        request.set_timeout(self.timeout);
        let response: Response<Vec<u8>> = client
            .unary(
                request,
                PathAndQuery::from_static(STATS_METHOD),
                RawProtobuf,
            )
            .await?;
        decode_query_stats_response(&response.into_inner())
            .map_err(|error| Status::data_loss(error.to_string()))
    }

    /// Stop sampling and cancel any in-flight RPC immediately.
    pub fn close(&self) {
        let mut state = self.state.lock();
        self.closed.send_replace(true);
        state.channel = None;
        state.reason = "stopped".into();
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::net::SocketAddr;

    use super::*;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tonic::body::Body;
    use tonic::codegen::{BoxFuture, Context, Poll, Service, http};
    use tonic::server::UnaryService;
    use tonic::transport::{Server, server::TcpIncoming};

    #[test]
    fn matches_xray_protobuf_and_keeps_large_counters() -> anyhow::Result<()> {
        assert_eq!(encode_query_stats_request("x"), b"\x0a\x01x");
        assert_eq!(encode_query_stats_request(""), b"\x0a\x00");
        assert_eq!(
            decode_query_stats_response(b"\x0a\x03\x0a\x01x")?,
            Stats::from([("x".into(), 0)])
        );
        let mut message = b"\x0a\x01x\x10".to_vec();
        prost::encoding::encode_varint(1 << 55, &mut message);
        let mut response = vec![0x0a, message.len() as u8];
        response.extend_from_slice(&message);
        assert_eq!(
            decode_query_stats_response(&response)?,
            Stats::from([("x".into(), 1 << 55)])
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_or_duplicate_counters_and_oversized_payloads() {
        for payload in [
            &b"\x0a\xff"[..],
            b"\x00",
            b"\x08\x01",
            b"\x0a\x01\x80",
            b"\x0a\x03\x0a\x01\xff",
            b"\x0a\x0e\x0a\x01x\x10\xff\xff\xff\xff\xff\xff\xff\xff\xff\x01",
            b"\x0a\x05\x0a\x01x\x10\x01\x0a\x05\x0a\x01x\x10\x02",
        ] {
            assert!(decode_query_stats_response(payload).is_err(), "{payload:?}");
        }
        assert!(decode_query_stats_response(&vec![0; MAX_RESPONSE_BYTES + 1]).is_err());
    }

    #[derive(Clone)]
    struct FakeStats(mpsc::UnboundedSender<(Vec<u8>, SocketAddr)>);

    impl UnaryService<Vec<u8>> for FakeStats {
        type Response = Vec<u8>;
        type Future = BoxFuture<Response<Vec<u8>>, Status>;

        fn call(&mut self, request: Request<Vec<u8>>) -> Self::Future {
            let peer = request.remote_addr().unwrap();
            let _ = self.0.send((request.into_inner(), peer));
            Box::pin(async { Ok(Response::new(b"\x0a\x05\x0a\x01x\x10\x2a".to_vec())) })
        }
    }

    impl Service<http::Request<Body>> for FakeStats {
        type Response = http::Response<Body>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<Body>) -> Self::Future {
            assert_eq!(request.uri().path(), STATS_METHOD);
            assert!(request.headers().contains_key("grpc-timeout"));
            let service = self.clone();
            Box::pin(async move {
                Ok(tonic::server::Grpc::new(RawProtobuf)
                    .unary(service, request)
                    .await)
            })
        }
    }

    #[tokio::test]
    async fn samples_over_loopback_and_reuses_the_channel() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let (seen, mut requests) = mpsc::unbounded_channel();
        let client = XrayStatsClient::new(listener.local_addr()?.port().into());
        let server = tokio::spawn(async move {
            Server::builder()
                .serve_with_incoming(FakeStats(seen), TcpIncoming::from(listener))
                .await
        });
        assert_eq!(client.reason(), "not sampled");
        for _ in 0..2 {
            assert_eq!(client.query().await, Some(Stats::from([("x".into(), 42)])));
            assert_eq!(client.reason(), "");
        }
        let (request, peer) = requests.try_recv()?;
        assert_eq!(request, b"\x0a\x0ainbound>>>");
        assert_eq!(requests.try_recv()?, (request, peer));
        client.close();
        assert_eq!(client.query().await, None);
        assert_eq!(client.reason(), "stopped");
        server.abort();
        Ok(())
    }

    #[tokio::test]
    async fn closes_an_inflight_call_without_waiting_for_its_deadline() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let client = std::sync::Arc::new(XrayStatsClient::with_options(
            listener.local_addr()?.port().into(),
            Duration::from_secs(2),
            Duration::from_secs(3),
        ));
        let pending = tokio::spawn({
            let client = client.clone();
            async move { client.query().await }
        });
        let (_stream, _) =
            tokio::time::timeout(Duration::from_secs(1), listener.accept()).await??;
        client.close();
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), pending).await??,
            None
        );
        assert_eq!(client.reason(), "stopped");
        Ok(())
    }

    #[tokio::test]
    async fn retries_on_a_fresh_channel_after_the_backoff() -> anyhow::Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let retry_interval = Duration::from_millis(250);
        let client = XrayStatsClient::with_options(
            listener.local_addr()?.port().into(),
            Duration::from_millis(500),
            retry_interval,
        );

        // The socket accepts connections but nobody speaks HTTP/2 yet.
        assert_eq!(client.query().await, None);
        assert_eq!(client.reason(), "Xray stats unavailable (DeadlineExceeded)");
        let failed_at = client.state.lock().failed_at;
        assert_eq!(client.query().await, None);
        assert_eq!(client.state.lock().failed_at, failed_at);

        let (seen, _requests) = mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            Server::builder()
                .serve_with_incoming(FakeStats(seen), TcpIncoming::from(listener))
                .await
        });
        tokio::time::sleep(retry_interval).await;
        assert_eq!(client.query().await, Some(Stats::from([("x".into(), 42)])));
        assert_eq!(client.reason(), "");
        server.abort();
        Ok(())
    }
}
