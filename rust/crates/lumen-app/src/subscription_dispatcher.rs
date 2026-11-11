//! Queue and apply subscription batches from the application state owner.

use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use chrono::{DateTime, Utc};
use lumen_core::deeplinks::parse_lumen_deep_link;
use lumen_core::i18n::{tr, translate_dynamic};
use lumen_core::models::{AppState, str_field};
use serde_json::Value;

use crate::node_service::NodeService;
use crate::subscription_cache::{configure_job, find_subscription, refresh_jobs};
use crate::subscription_refresh_timer::SubscriptionRefreshTimer;
use crate::subscription_worker::{
    SubscriptionDownload, SubscriptionFetchWorker, SubscriptionJob, SubscriptionJobKind,
    SubscriptionWorkerEvent, merge_subscription_reasons,
};

#[derive(Clone, Copy, Default)]
pub struct SubscriptionDispatchContext<'a> {
    pub profile_loaded: bool,
    pub locked: bool,
    pub effective_proxy_url: Option<&'a str>,
}

pub enum SubscriptionRequest {
    Import { url: String, name: String },
    ImportLines(String),
    Update(String),
    UpdateAll,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubscriptionEvent {
    ImportStatus(Option<String>),
    Imported(String),
    Status {
        level: &'static str,
        message: String,
    },
}

pub enum SubscriptionDispatchEvent {
    Worker(SubscriptionWorkerEvent),
    WorkerClosed(u64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchKind {
    Import,
    Update,
    UpdateAll,
    Automatic,
}

struct PendingBatch {
    id: u64,
    kind: BatchKind,
    jobs: Vec<SubscriptionJob>,
}

struct ActiveBatch {
    id: u64,
    kind: BatchKind,
    worker: SubscriptionFetchWorker,
    added: usize,
    errors: Vec<String>,
    changes: Vec<SubscriptionChange>,
}

struct SubscriptionChange {
    name: String,
    added: usize,
    removed: usize,
}

#[derive(Default)]
pub struct SubscriptionDispatcher {
    active: Option<ActiveBatch>,
    pending: VecDeque<PendingBatch>,
    sequence: u64,
    closed: bool,
    timer: SubscriptionRefreshTimer,
}

impl SubscriptionDispatcher {
    pub fn is_running(&self) -> bool {
        self.active.is_some()
    }

    pub fn is_importing(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|batch| batch.kind == BatchKind::Import)
            || self
                .pending
                .iter()
                .any(|batch| batch.kind == BatchKind::Import)
    }

    pub fn dispatch(
        &mut self,
        state: &AppState,
        context: SubscriptionDispatchContext<'_>,
        request: SubscriptionRequest,
        mut emit: impl FnMut(SubscriptionEvent),
    ) -> bool {
        if self.closed || !context.profile_loaded {
            return false;
        }
        let mut job = match request {
            SubscriptionRequest::Import { url, name } => {
                let url = url.trim();
                if url.is_empty() {
                    status(&mut emit, "warning", tr("Введите ссылку на подписку", &[]));
                    return false;
                }
                let mut job = SubscriptionJob::new(url.into(), SubscriptionJobKind::Import);
                job.name = name.trim().into();
                configure_job(state, &mut job, context.effective_proxy_url);
                emit(SubscriptionEvent::ImportStatus(Some(tr(
                    "Загрузка подписки...",
                    &[],
                ))));
                self.enqueue(BatchKind::Import, vec![job]);
                return true;
            }
            SubscriptionRequest::ImportLines(text) => {
                let mut seen = HashSet::new();
                let jobs: Vec<_> = text
                    .lines()
                    .map(str::trim)
                    .filter(|url| !url.is_empty() && seen.insert(*url))
                    .map(|url| {
                        let mut job = SubscriptionJob::new(url.into(), SubscriptionJobKind::Import);
                        configure_job(state, &mut job, context.effective_proxy_url);
                        job
                    })
                    .collect();
                if jobs.is_empty() {
                    status(
                        &mut emit,
                        "warning",
                        tr("Введите хотя бы один URL подписки", &[]),
                    );
                    return false;
                }
                emit(SubscriptionEvent::ImportStatus(Some(tr(
                    "Загрузка подписок...",
                    &[],
                ))));
                self.enqueue(BatchKind::Import, jobs);
                return true;
            }
            SubscriptionRequest::Update(url) => {
                let url = url.trim();
                if url.is_empty() {
                    return false;
                }
                SubscriptionJob::new(url.into(), SubscriptionJobKind::Update)
            }
            SubscriptionRequest::UpdateAll => {
                let jobs = refresh_jobs(state, context.effective_proxy_url, false, Utc::now());
                if jobs.is_empty() {
                    status(&mut emit, "info", tr("Подписок нет", &[]));
                    return false;
                }
                status(&mut emit, "info", tr("Подписки обновляются...", &[]));
                self.enqueue(BatchKind::UpdateAll, jobs);
                return true;
            }
        };
        configure_job(state, &mut job, context.effective_proxy_url);
        status(&mut emit, "info", tr("Подписка обновляется...", &[]));
        self.enqueue(BatchKind::Update, vec![job]);
        true
    }

    pub fn dispatch_deep_link(
        &mut self,
        state: &AppState,
        context: SubscriptionDispatchContext<'_>,
        value: &str,
        mut emit: impl FnMut(SubscriptionEvent),
    ) -> bool {
        match parse_lumen_deep_link(value) {
            Ok(Some(request)) => self.dispatch(
                state,
                context,
                SubscriptionRequest::Import {
                    url: request.url,
                    name: request.name,
                },
                emit,
            ),
            Ok(None) => false,
            Err(_) => {
                status(&mut emit, "warning", tr("Некорректная ссылка lumen", &[]));
                false
            }
        }
    }

    pub fn reconfigure_auto_refresh(&mut self, state: &AppState, now: Instant) {
        if !self.closed {
            self.timer
                .reconfigure(state.settings.subscription_auto_update_minutes, now);
        }
    }

    pub fn auto_refresh_deadline(&self) -> Option<Instant> {
        self.timer.deadline()
    }

    /// Advance the timer even when the profile is locked, as the recurring Qt timer does.
    pub fn tick_auto_refresh(
        &mut self,
        state: &AppState,
        context: SubscriptionDispatchContext<'_>,
        now: Instant,
        utc_now: DateTime<Utc>,
    ) -> bool {
        if !self.timer.take_due(now) || !context.profile_loaded || context.locked {
            return false;
        }
        let jobs = refresh_jobs(state, context.effective_proxy_url, true, utc_now);
        if jobs.is_empty() {
            return false;
        }
        self.enqueue(BatchKind::Automatic, jobs);
        true
    }

    /// Receive while running in the controller's select loop, then apply with the current state owner.
    pub async fn next_worker_event(&mut self) -> Option<SubscriptionDispatchEvent> {
        let batch = self.active.as_mut()?;
        Some(match batch.worker.recv().await {
            Some(event) => SubscriptionDispatchEvent::Worker(event),
            None => SubscriptionDispatchEvent::WorkerClosed(batch.id),
        })
    }

    pub fn handle_worker_event(
        &mut self,
        event: SubscriptionDispatchEvent,
        nodes: &mut NodeService<'_>,
        profile_loaded: bool,
        mut emit: impl FnMut(SubscriptionEvent),
    ) {
        if !profile_loaded {
            self.clear_batches(&mut emit);
            return;
        }
        match event {
            SubscriptionDispatchEvent::Worker(SubscriptionWorkerEvent::Fetched {
                batch_id,
                job,
                download,
            }) => {
                if let Some(batch) = self.active.as_mut().filter(|batch| batch.id == batch_id) {
                    batch.apply(nodes, job, download, &mut emit);
                }
            }
            SubscriptionDispatchEvent::Worker(SubscriptionWorkerEvent::Completed {
                batch_id,
                ..
            }) => {
                self.complete(batch_id, false, &mut emit);
            }
            SubscriptionDispatchEvent::WorkerClosed(batch_id) => {
                self.complete(batch_id, true, &mut emit);
            }
        }
    }

    pub fn cancel_import(&mut self, mut emit: impl FnMut(SubscriptionEvent)) -> bool {
        if !self.is_importing() {
            return false;
        }
        self.clear_batches(&mut emit);
        status(&mut emit, "info", tr("Импорт подписки отменён", &[]));
        true
    }

    /// Drop queued jobs and active network work before replacing the current profile.
    pub fn clear_batches(&mut self, mut emit: impl FnMut(SubscriptionEvent)) {
        let importing = self.is_importing();
        self.active = None;
        self.pending.clear();
        if importing {
            emit(SubscriptionEvent::ImportStatus(None));
        }
    }

    pub fn shutdown(&mut self, emit: impl FnMut(SubscriptionEvent)) {
        self.closed = true;
        self.timer = SubscriptionRefreshTimer::default();
        self.clear_batches(emit);
    }

    fn enqueue(&mut self, kind: BatchKind, jobs: Vec<SubscriptionJob>) {
        self.sequence = self.sequence.wrapping_add(1);
        self.pending.push_back(PendingBatch {
            id: self.sequence,
            kind,
            jobs,
        });
        self.start_next();
    }

    fn start_next(&mut self) {
        if self.active.is_none()
            && let Some(batch) = self.pending.pop_front()
        {
            self.active = Some(ActiveBatch {
                id: batch.id,
                kind: batch.kind,
                worker: SubscriptionFetchWorker::start(batch.id, batch.jobs),
                added: 0,
                errors: Vec::new(),
                changes: Vec::new(),
            });
        }
    }

    fn complete(&mut self, id: u64, stopped: bool, emit: &mut impl FnMut(SubscriptionEvent)) {
        if self.active.as_ref().is_some_and(|batch| batch.id == id)
            && let Some(mut batch) = self.active.take()
        {
            if stopped {
                batch.errors.push(tr("Не удалось загрузить подписку", &[]));
            }
            if batch.kind == BatchKind::Import && !self.is_importing() {
                emit(SubscriptionEvent::ImportStatus(None));
            }
            batch.report(emit);
            self.start_next();
        }
    }
}

impl ActiveBatch {
    fn apply(
        &mut self,
        nodes: &mut NodeService<'_>,
        job: SubscriptionJob,
        download: Result<SubscriptionDownload, String>,
        emit: &mut impl FnMut(SubscriptionEvent),
    ) {
        let before = find_subscription(nodes.state, job.url.trim()).map(|index| {
            SubscriptionSnapshot::from_record(nodes.state, &nodes.state.subscriptions[index])
        });
        let result = nodes.apply_fetched_subscription(&job, download);
        self.added += result.node_count;
        let after = result.subscription_id.as_ref().and_then(|id| {
            nodes
                .state
                .subscriptions
                .iter()
                .find(|record| record["id"].as_str() == Some(id))
        });
        if job.kind == SubscriptionJobKind::Import {
            self.errors.extend(result.errors);
            if let Some(record) = after.filter(|record| {
                before
                    .as_ref()
                    .is_none_or(|snapshot| snapshot.id != record["id"])
            }) {
                let group = record_label(record, &["group", "name"], "Default");
                emit(SubscriptionEvent::Imported(group));
            }
        } else if let Some(before) = before {
            if !result.errors.is_empty() {
                self.errors.push(update_error(&before.name, &result.errors));
            }
            if let Some(record) = after {
                let after = SubscriptionSnapshot::from_record(nodes.state, record);
                let added = after.nodes.difference(&before.nodes).count();
                let removed = before.nodes.difference(&after.nodes).count();
                if added != 0 || removed != 0 {
                    self.changes.push(SubscriptionChange {
                        name: before.name,
                        added,
                        removed,
                    });
                }
            }
        } else {
            self.errors.extend(result.errors);
        }
    }

    fn report(&self, emit: &mut impl FnMut(SubscriptionEvent)) {
        if self.kind == BatchKind::Import {
            if self.added != 0 {
                status(
                    emit,
                    "success",
                    tr(
                        "Импортировано серверов: {count}",
                        &[("count", &self.added.to_string())],
                    ),
                );
            } else if self.errors.is_empty() {
                status(emit, "info", tr("Новых серверов не найдено", &[]));
            }
        } else if !self.changes.is_empty() {
            let (level, template) = match self.kind {
                BatchKind::Automatic => ("info", "Авто-обновление подписок: {changes}"),
                BatchKind::Update => ("success", "Подписка обновлена: {changes}"),
                _ => ("success", "Подписки обновлены: {changes}"),
            };
            status(
                emit,
                level,
                tr(template, &[("changes", &change_summary(&self.changes))]),
            );
        } else if self.kind != BatchKind::Automatic && self.errors.is_empty() {
            let message = if self.kind == BatchKind::Update {
                "Изменений в подписке нет"
            } else {
                "Изменений в подписках нет"
            };
            status(emit, "info", tr(message, &[]));
        }
        if !self.errors.is_empty() {
            status(emit, "warning", join_messages(&self.errors));
        }
    }
}

struct SubscriptionSnapshot {
    id: String,
    name: String,
    nodes: HashSet<String>,
}

impl SubscriptionSnapshot {
    fn from_record(state: &AppState, record: &Value) -> Self {
        let id = str_field(record, "id").trim().to_owned();
        let group = record_label(record, &["group", "name"], "");
        let nodes = state
            .nodes
            .iter()
            .filter(|node| {
                (!id.is_empty() && node.subscription_id == id)
                    || (node.subscription_id.is_empty() && !group.is_empty() && node.group == group)
            })
            .map(|node| node.id.clone())
            .collect();
        let name = record_label(record, &["name", "group"], &str_field(record, "url"));
        Self { id, name, nodes }
    }
}

fn record_label(record: &Value, keys: &[&str], fallback: &str) -> String {
    keys.iter()
        .find_map(|key| {
            let text = str_field(record, key);
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        })
        .unwrap_or_else(|| fallback.to_owned())
}

fn update_error(name: &str, errors: &[String]) -> String {
    let reasons = merge_subscription_reasons(errors);
    let detail = reasons
        .iter()
        .map(|reason| {
            let mut chars = reason.trim_end_matches('.').chars();
            chars
                .next()
                .map(|first| format!("{}{}.", first.to_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    tr(
        "Подписка «{name}» не обновлена. {error}",
        &[("name", name), ("error", &detail)],
    )
}

fn join_messages(errors: &[String]) -> String {
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for error in errors.iter().map(|error| error.trim()) {
        if !error.is_empty() && seen.insert(error) && lines.len() < 3 {
            lines.push(translate_dynamic(error).unwrap_or_else(|| tr(error, &[])));
        }
    }
    let remaining = seen.len().saturating_sub(lines.len());
    if remaining != 0 {
        lines.push(tr("…и ещё {count}", &[("count", &remaining.to_string())]));
    }
    lines.join("\n")
}

fn change_summary(changes: &[SubscriptionChange]) -> String {
    changes
        .iter()
        .map(|change| {
            let delta = match (change.added, change.removed) {
                (added, 0) => format!("+{added}"),
                (0, removed) => format!("-{removed}"),
                (added, removed) => format!("+{added} / -{removed}"),
            };
            format!("{} ({delta})", change.name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn status(emit: &mut impl FnMut(SubscriptionEvent), level: &'static str, message: String) {
    emit(SubscriptionEvent::Status { level, message });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::timeout;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);
    const LINK: &str = "vless://00000000-0000-0000-0000-000000000001@node.example:443?security=tls";
    const LOADED: SubscriptionDispatchContext<'static> = SubscriptionDispatchContext {
        profile_loaded: true,
        locked: false,
        effective_proxy_url: None,
    };

    fn local_state() -> AppState {
        let mut state = AppState::default();
        state.settings.subscription_use_proxy_tun = true;
        state.settings.subscription_use_real_hwid = false;
        state
    }

    async fn request(listener: &TcpListener) -> (BufReader<TcpStream>, String) {
        timeout(TIMEOUT, async {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut headers = String::new();
            while !headers.ends_with("\r\n\r\n") {
                assert_ne!(stream.read_line(&mut headers).await.unwrap(), 0);
            }
            (stream, headers)
        })
        .await
        .unwrap()
    }

    async fn apply_next(
        dispatcher: &mut SubscriptionDispatcher,
        state: &mut AppState,
        events: &mut Vec<SubscriptionEvent>,
    ) {
        let event = timeout(TIMEOUT, dispatcher.next_worker_event())
            .await
            .unwrap()
            .unwrap();
        apply(dispatcher, state, event, events);
    }

    fn apply(
        dispatcher: &mut SubscriptionDispatcher,
        state: &mut AppState,
        event: SubscriptionDispatchEvent,
        events: &mut Vec<SubscriptionEvent>,
    ) {
        let mut desired = false;
        let mut nodes = NodeService {
            state,
            connected: false,
            desired_connected: &mut desired,
            can_connect_without_selected_node: false,
            emit: &mut |_, _| {},
        };
        dispatcher.handle_worker_event(event, &mut nodes, true, |event| events.push(event));
    }

    #[tokio::test]
    async fn unloaded_profile_does_not_start_an_import() {
        let mut dispatcher = SubscriptionDispatcher::default();
        let mut events = Vec::new();
        assert!(!dispatcher.dispatch(
            &lumen_core::models::AppState::default(),
            SubscriptionDispatchContext::default(),
            SubscriptionRequest::Import {
                url: "https://sub.example".into(),
                name: String::new()
            },
            |event| events.push(event),
        ));
        assert!(events.is_empty());
        assert!(!dispatcher.is_importing());
    }

    #[tokio::test]
    async fn queued_imports_apply_in_order_and_keep_the_import_status_until_the_last_batch() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let mut state = local_state();
        let mut dispatcher = SubscriptionDispatcher::default();
        let mut events = Vec::new();
        assert!(dispatcher.dispatch(
            &state,
            LOADED,
            SubscriptionRequest::ImportLines(format!(" {base}/first\r\n{base}/first\n")),
            |event| events.push(event),
        ));
        assert!(dispatcher.dispatch_deep_link(
            &state,
            LOADED,
            &format!("lumen://add?url={base}/second&name=Second"),
            |event| events.push(event),
        ));
        for path in ["first", "second"] {
            let (mut stream, headers) = request(&listener).await;
            assert!(headers.starts_with(&format!("GET /{path} ")));
            let body = format!("{LINK}#{path}");
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
            drop(stream);
            apply_next(&mut dispatcher, &mut state, &mut events).await;
            apply_next(&mut dispatcher, &mut state, &mut events).await;
            if path == "first" {
                assert!(dispatcher.is_importing());
                assert!(!events.contains(&SubscriptionEvent::ImportStatus(None)));
            }
        }
        assert!(!dispatcher.is_running());
        assert!(!dispatcher.is_importing());
        assert_eq!(state.subscriptions.len(), 2);
        assert_eq!(state.nodes.len(), 2);
        assert!(events.contains(&SubscriptionEvent::Imported("Second".into())));
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == SubscriptionEvent::ImportStatus(None))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn cancelling_import_drops_the_connection_and_queued_batches() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut state = local_state();
        let mut dispatcher = SubscriptionDispatcher::default();
        let mut events = Vec::new();
        dispatcher.dispatch(
            &state,
            LOADED,
            SubscriptionRequest::ImportLines(url.clone()),
            |_| {},
        );
        dispatcher.dispatch(&state, LOADED, SubscriptionRequest::Update(url), |_| {});
        let (mut stream, _) = request(&listener).await;
        let stale_id = dispatcher.active.as_ref().unwrap().id;
        assert!(dispatcher.cancel_import(|event| events.push(event)));
        assert_eq!(
            timeout(TIMEOUT, stream.read(&mut [0]))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        assert!(dispatcher.next_worker_event().await.is_none());
        apply(
            &mut dispatcher,
            &mut state,
            SubscriptionDispatchEvent::WorkerClosed(stale_id),
            &mut events,
        );
        assert!(state.subscriptions.is_empty());
        assert!(dispatcher.pending.is_empty());
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn automatic_refresh_consumes_locked_ticks_and_skips_backoff() {
        let mut state = local_state();
        state.settings.subscription_auto_update_minutes = 1;
        let wall = Utc::now();
        let start = Instant::now();
        state.subscriptions = vec![serde_json::json!({
            "url": "http://127.0.0.1:9/sub",
            "backoff_until": (wall + chrono::TimeDelta::seconds(150)).to_rfc3339(),
        })];
        let mut dispatcher = SubscriptionDispatcher::default();
        dispatcher.reconfigure_auto_refresh(&state, start);
        let tick = |dispatcher: &mut SubscriptionDispatcher, seconds, locked| {
            dispatcher.tick_auto_refresh(
                &state,
                SubscriptionDispatchContext { locked, ..LOADED },
                start + Duration::from_secs(seconds),
                wall + chrono::TimeDelta::seconds(seconds as i64),
            )
        };
        assert!(!tick(&mut dispatcher, 60, true));
        assert_eq!(
            dispatcher.auto_refresh_deadline(),
            Some(start + Duration::from_secs(120))
        );
        assert!(!tick(&mut dispatcher, 120, false));
        assert!(tick(&mut dispatcher, 180, false));
        dispatcher.shutdown(|_| {});
        dispatcher.reconfigure_auto_refresh(&state, start);
        assert!(dispatcher.auto_refresh_deadline().is_none());
        assert!(!dispatcher.is_running());
    }
}
