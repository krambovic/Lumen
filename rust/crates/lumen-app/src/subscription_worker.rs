//! Background fetch and preparation for an ordered batch of subscriptions.

use std::collections::BTreeMap;

use lumen_core::constants::APP_VERSION;
use lumen_core::link_parser::MAX_IMPORT_BYTES;
use lumen_core::models::AppSettings;
use lumen_core::secret_scrubber::scrub_text;
use lumen_core::subscription::{fetcher::fetch_subscription_http, response::SubscriptionResponse};
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::subscription_service::PreparedSubscription;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionJobKind {
    Import,
    Update,
}

pub struct SubscriptionJob {
    pub url: String,
    pub kind: SubscriptionJobKind,
    pub name: String,
    pub client_profile: String,
    pub headers: BTreeMap<String, String>,
    pub use_proxy_tun: bool,
    pub proxy_url: String,
    pub etag: String,
    pub last_modified: String,
    pub include_regex: String,
    pub exclude_regex: String,
}

impl SubscriptionJob {
    pub fn new(url: String, kind: SubscriptionJobKind) -> Self {
        Self {
            url,
            kind,
            name: String::new(),
            client_profile: "Lumen".into(),
            headers: BTreeMap::from([
                ("accept".into(), "*/*".into()),
                (
                    "user-agent".into(),
                    format!("Lumen-Subscription/Windows-{APP_VERSION}"),
                ),
            ]),
            use_proxy_tun: false,
            proxy_url: String::new(),
            etag: String::new(),
            last_modified: String::new(),
            include_regex: String::new(),
            exclude_regex: String::new(),
        }
    }

    fn request_headers(&self) -> BTreeMap<String, String> {
        let mut headers: BTreeMap<_, _> = self
            .headers
            .iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value.clone()))
            .collect();
        for (name, value) in [
            ("if-none-match", &self.etag),
            ("if-modified-since", &self.last_modified),
        ] {
            if !value.trim().is_empty() {
                headers.insert(name.into(), value.trim().into());
            }
        }
        headers
    }
}

pub struct SubscriptionDownload {
    pub response: SubscriptionResponse,
    pub prepared: Option<PreparedSubscription>,
}

pub enum SubscriptionWorkerEvent {
    Fetched {
        batch_id: u64,
        job: SubscriptionJob,
        download: Result<SubscriptionDownload, String>,
    },
    Completed {
        batch_id: u64,
        total: usize,
    },
}

pub struct SubscriptionFetchWorker {
    stop: watch::Sender<bool>,
    events: mpsc::Receiver<SubscriptionWorkerEvent>,
}

impl SubscriptionFetchWorker {
    /// Start a batch with each job's resolved HTTP URL and client profile.
    pub fn start(batch_id: u64, jobs: Vec<SubscriptionJob>) -> Self {
        let (stop, mut stopping) = watch::channel(false);
        let (send, events) = mpsc::channel(8);
        tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = stopping.wait_for(|stopped| *stopped) => {}
                _ = run_batch(batch_id, jobs, send) => {}
            }
        });
        Self { stop, events }
    }

    /// Cancel the batch and discard any results still in the queue.
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }

    pub async fn recv(&mut self) -> Option<SubscriptionWorkerEvent> {
        if *self.stop.borrow() {
            return None;
        }
        self.events.recv().await.filter(|_| !*self.stop.borrow())
    }
}

impl Drop for SubscriptionFetchWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn run_batch(
    batch_id: u64,
    jobs: Vec<SubscriptionJob>,
    events: mpsc::Sender<SubscriptionWorkerEvent>,
) {
    let total = jobs.len();
    for job in jobs {
        let download = download(&job).await;
        let event = SubscriptionWorkerEvent::Fetched {
            batch_id,
            job,
            download,
        };
        if events.send(event).await.is_err() {
            return;
        }
    }
    let _ = events
        .send(SubscriptionWorkerEvent::Completed { batch_id, total })
        .await;
}

async fn download(job: &SubscriptionJob) -> Result<SubscriptionDownload, String> {
    let payload = fetch_subscription_http(
        &job.url,
        &job.request_headers(),
        20.0,
        MAX_IMPORT_BYTES,
        job.use_proxy_tun,
        &job.proxy_url,
        false,
        None,
    )
    .await
    .map_err(scrub_error)?;
    let profile = job.client_profile.clone();
    let include = job.include_regex.clone();
    let exclude = job.exclude_regex.clone();
    tokio::task::spawn_blocking(move || {
        let response = SubscriptionResponse::from_http(payload, &profile);
        let prepared = (!response.text.is_empty() && !response.not_modified).then(|| {
            let settings = AppSettings {
                subscription_include_regex: include,
                subscription_exclude_regex: exclude,
                ..AppSettings::default()
            };
            PreparedSubscription::parse(&response.text, &settings)
        });
        SubscriptionDownload { response, prepared }
    })
    .await
    .map_err(scrub_error)
}

fn scrub_error(error: impl ToString) -> String {
    scrub_text(&Value::String(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::timeout;

    use super::*;

    const LINK: &str =
        "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=tls#First";
    const TEST_TIMEOUT: Duration = Duration::from_secs(10);

    async fn request(listener: &TcpListener) -> (BufReader<TcpStream>, String) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut headers = String::new();
        while !headers.ends_with("\r\n\r\n") {
            assert_ne!(stream.read_line(&mut headers).await.unwrap(), 0);
        }
        (stream, headers)
    }

    async fn next(worker: &mut SubscriptionFetchWorker) -> Option<SubscriptionWorkerEvent> {
        timeout(TEST_TIMEOUT, worker.recv()).await.unwrap()
    }

    #[tokio::test]
    async fn batches_success_failure_and_not_modified_in_job_order() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, first) = request(&listener).await;
            assert!(first.starts_with("GET /import "));
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{LINK}",
                LINK.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
            drop(stream);

            let (mut stream, last) = request(&listener).await;
            assert!(last.starts_with("GET /update "));
            let last = last.to_ascii_lowercase();
            assert!(last.contains("if-none-match: \"saved\""));
            assert!(last.contains("if-modified-since: sat, 29 aug 2026 00:00:00 gmt"));
            stream
                .write_all(
                    b"HTTP/1.1 304 Not Modified\r\nETag: \"current\"\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let mut import =
            SubscriptionJob::new(format!("{base}/import"), SubscriptionJobKind::Import);
        import.use_proxy_tun = true;
        import.include_regex = "First".into();
        let mut failure = SubscriptionJob::new("invalid URL".into(), SubscriptionJobKind::Import);
        failure.use_proxy_tun = true;
        let mut update =
            SubscriptionJob::new(format!("{base}/update"), SubscriptionJobKind::Update);
        update.use_proxy_tun = true;
        update.etag = "\"saved\"".into();
        update.last_modified = "Sat, 29 Aug 2026 00:00:00 GMT".into();
        let mut worker = SubscriptionFetchWorker::start(7, vec![import, failure, update]);

        let Some(SubscriptionWorkerEvent::Fetched {
            batch_id: 7,
            job,
            download: Ok(first),
        }) = next(&mut worker).await
        else {
            panic!("expected the imported subscription");
        };
        assert!(job.kind == SubscriptionJobKind::Import);
        assert_eq!(first.response.text, LINK);
        assert!(first.prepared.is_some());
        assert!(matches!(
            next(&mut worker).await,
            Some(SubscriptionWorkerEvent::Fetched {
                batch_id: 7,
                download: Err(_),
                ..
            })
        ));

        let Some(SubscriptionWorkerEvent::Fetched {
            batch_id: 7,
            job,
            download: Ok(last),
        }) = next(&mut worker).await
        else {
            panic!("expected the conditional update");
        };
        assert!(job.kind == SubscriptionJobKind::Update);
        assert!(last.response.not_modified);
        assert_eq!(last.response.headers["etag"], "\"current\"");
        assert!(last.prepared.is_none());
        assert!(matches!(
            next(&mut worker).await,
            Some(SubscriptionWorkerEvent::Completed {
                batch_id: 7,
                total: 3
            })
        ));
        assert!(next(&mut worker).await.is_none());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancellation_closes_an_active_request_without_completion() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut job = SubscriptionJob::new(url, SubscriptionJobKind::Update);
        job.use_proxy_tun = true;
        let mut worker = SubscriptionFetchWorker::start(1, vec![job]);
        let (mut stream, _) = timeout(TEST_TIMEOUT, request(&listener)).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
            .await
            .unwrap();

        worker.stop();

        assert!(next(&mut worker).await.is_none());
        let mut buffer = [0];
        assert_eq!(
            timeout(TEST_TIMEOUT, stream.read(&mut buffer))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}
