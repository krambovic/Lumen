//! Persisted fetch metadata and conditional refresh job preparation.

use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use lumen_core::constants::SUBSCRIPTION_PARSER_REVISION;
use lumen_core::models::{AppState, Node, int_field, str_field};
use lumen_core::secret_scrubber::scrub_text;
use lumen_core::subscription::response::SubscriptionResponse;
use rand::Rng;
use serde_json::{Value, json};

use crate::subscription_worker::{SubscriptionJob, SubscriptionJobKind};

pub(crate) fn find_subscription(state: &AppState, url: &str) -> Option<usize> {
    state
        .subscriptions
        .iter()
        .position(|record| record.get("url").and_then(Value::as_str) == Some(url))
}

pub(crate) fn current_parser(record: &Value) -> bool {
    int_field(record, "parser_revision", 0) == SUBSCRIPTION_PARSER_REVISION
}

fn snapshot_matches(record: &Value, nodes: &[Node]) -> bool {
    let expected = int_field(record, "node_count", -1);
    let id = str_field(record, "id");
    let id = id.trim();
    // Legacy records without a snapshot count or ID still accept validators.
    expected < 0
        || id.is_empty()
        || nodes
            .iter()
            .filter(|node| node.subscription_id.trim() == id)
            .count() as i64
            == expected
}

/// Capture settings and validators before dispatching a background request.
/// Pass the effective HTTP proxy URL when the active connection exposes one.
pub fn configure_job(state: &AppState, job: &mut SubscriptionJob, proxy_url: Option<&str>) {
    let record = state.subscriptions.iter().find(|record| {
        record["url"]
            .as_str()
            .is_some_and(|url| url.trim() == job.url.trim())
    });
    configure_with_record(state, job, proxy_url, record);
}

fn configure_with_record(
    state: &AppState,
    job: &mut SubscriptionJob,
    proxy_url: Option<&str>,
    record: Option<&Value>,
) {
    let settings = &state.settings;
    job.url = job.url.trim().into();
    job.include_regex
        .clone_from(&settings.subscription_include_regex);
    job.exclude_regex
        .clone_from(&settings.subscription_exclude_regex);
    job.user_agent = settings.subscription_user_agent.trim().into();
    job.hwid = settings.subscription_hwid.trim().into();
    job.use_real_hwid = settings.subscription_use_real_hwid;
    job.use_proxy_tun = settings.subscription_use_proxy_tun;
    job.proxy_url = if job.use_proxy_tun {
        proxy_url.unwrap_or_default().into()
    } else {
        String::new()
    };
    job.converter_url = if settings.subscription_converter_enabled {
        settings.subscription_converter_url.trim().into()
    } else {
        String::new()
    };
    job.etag.clear();
    job.last_modified.clear();
    job.headers.retain(|name, _| {
        !name.eq_ignore_ascii_case("if-none-match")
            && !name.eq_ignore_ascii_case("if-modified-since")
    });
    if let Some(record) =
        record.filter(|record| current_parser(record) && snapshot_matches(record, &state.nodes))
    {
        job.etag = str_field(record, "etag");
        job.last_modified = str_field(record, "last_modified");
    }
}

pub fn refresh_jobs(
    state: &AppState,
    proxy_url: Option<&str>,
    automatic: bool,
    now: DateTime<Utc>,
) -> Vec<SubscriptionJob> {
    state
        .subscriptions
        .iter()
        .filter_map(|record| {
            let url = str_field(record, "url");
            let url = url.trim();
            if url.is_empty()
                || (automatic
                    && parse_timestamp(&str_field(record, "backoff_until"))
                        .is_some_and(|until| until > now))
            {
                return None;
            }
            let mut job = SubscriptionJob::new(url.into(), SubscriptionJobKind::Update);
            configure_with_record(state, &mut job, proxy_url, Some(record));
            Some(job)
        })
        .collect()
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.to_utc())
        .ok()
        .or_else(|| {
            value
                .parse::<NaiveDateTime>()
                .ok()
                .map(|time| time.and_utc())
        })
}

pub(crate) enum FetchOutcome<'a> {
    Applied,
    NotModified,
    Failed(&'a str),
}

pub(crate) fn mark_fetch(
    record: &mut Value,
    response: &SubscriptionResponse,
    outcome: FetchOutcome<'_>,
    now: DateTime<Utc>,
) {
    let checked_at = timestamp(now);
    record["last_checked_at"] = json!(checked_at);
    let status = match (response.status, &outcome) {
        (0, FetchOutcome::Applied) => 200,
        (0, FetchOutcome::NotModified) => 304,
        (status, _) => status,
    };
    record["last_status"] = json!(status);
    if let FetchOutcome::Failed(error) = outcome {
        let failures = int_field(record, "failure_count", 0)
            .max(0)
            .saturating_add(1);
        let jitter = rand::thread_rng().gen_range(0.8..=1.2);
        let delay = retry_delay(
            failures,
            response_header(response, "retry-after"),
            now,
            jitter,
        );
        record["failure_count"] = json!(failures);
        record["backoff_until"] = json!(timestamp(now + delay));
        record["last_error"] = json!(
            scrub_text(&json!(error))
                .chars()
                .take(500)
                .collect::<String>()
        );
        return;
    }
    let applied = matches!(outcome, FetchOutcome::Applied);
    for (header, field) in [("etag", "etag"), ("last-modified", "last_modified")] {
        if let Some(value) = response_header(response, header).filter(|value| !value.is_empty()) {
            record[field] = json!(value);
        } else if applied && let Some(record) = record.as_object_mut() {
            record.shift_remove(field);
        }
    }
    if applied {
        record["parser_revision"] = json!(SUBSCRIPTION_PARSER_REVISION);
    }
    record["last_success_at"] = json!(checked_at);
    record["failure_count"] = json!(0);
    record["backoff_until"] = json!("");
    record["last_error"] = json!("");
}

pub(crate) fn timestamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

fn response_header<'a>(response: &'a SubscriptionResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find_map(|(header, value)| header.eq_ignore_ascii_case(name).then_some(value.as_str()))
}

fn retry_delay(
    failures: i64,
    retry_after: Option<&str>,
    now: DateTime<Utc>,
    jitter: f64,
) -> TimeDelta {
    let delay = (900 * (1i64 << (failures - 1).min(5))).min(21_600) as f64 * jitter;
    let retry_seconds = retry_after
        .map(str::trim)
        .and_then(|value| {
            value.parse::<f64>().ok().or_else(|| {
                let time = DateTime::<Utc>::from(httpdate::parse_http_date(value).ok()?);
                Some((time - now).num_microseconds()? as f64 / 1_000_000.0)
            })
        })
        .unwrap_or_default();
    let seconds = delay.max(retry_seconds).min(86_400.0);
    TimeDelta::microseconds((seconds * 1_000_000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://provider.example/sub";

    #[test]
    fn refresh_validators_require_a_current_complete_snapshot() {
        let mut state = AppState {
            subscriptions: vec![json!({
                "url": URL, "id": "sub", "node_count": 1, "etag": "cached",
                "parser_revision": SUBSCRIPTION_PARSER_REVISION, "last_modified": "date",
            })],
            nodes: vec![Node {
                subscription_id: "sub".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut job = SubscriptionJob::new(URL.into(), SubscriptionJobKind::Update);
        configure_job(&state, &mut job, None);
        assert_eq!(job.etag, "cached");
        assert_eq!(job.last_modified, "date");
        state.nodes.clear();
        job.headers.insert("If-None-Match".into(), "cached".into());
        configure_job(&state, &mut job, None);
        assert!(job.etag.is_empty() && job.last_modified.is_empty() && job.headers.is_empty());
        state.nodes.push(Node {
            subscription_id: "sub".into(),
            ..Default::default()
        });
        state.subscriptions[0]["parser_revision"] = json!(SUBSCRIPTION_PARSER_REVISION - 1);
        configure_job(&state, &mut job, None);
        assert!(job.etag.is_empty());
    }

    #[test]
    fn retry_after_controls_automatic_refresh_and_leaves_validators_untouched() {
        let now = Utc::now();
        let mut record =
            json!({"url": URL, "etag": "cached", "parser_revision": 1, "failure_count": 2});
        let response = SubscriptionResponse {
            status: 429,
            headers: [("Retry-After".into(), "7200".into())].into(),
            ..Default::default()
        };
        mark_fetch(
            &mut record,
            &response,
            FetchOutcome::Failed("HTTP 429"),
            now,
        );
        assert_eq!(record["parser_revision"], 1);
        assert_eq!(record["etag"], "cached");
        assert_eq!(record["failure_count"], 3);
        assert_eq!(record["last_status"], 429);
        assert_eq!(
            record["backoff_until"],
            timestamp(now + TimeDelta::hours(2))
        );
        let state = AppState {
            subscriptions: vec![record],
            ..Default::default()
        };
        assert!(refresh_jobs(&state, None, true, now).is_empty());
        assert_eq!(refresh_jobs(&state, None, false, now).len(), 1);
        assert_eq!(
            refresh_jobs(&state, None, true, now + TimeDelta::hours(2)).len(),
            1
        );
        let retry_date = httpdate::fmt_http_date((now + TimeDelta::hours(3)).into());
        let delay = retry_delay(1, Some(&retry_date), now, 1.0);
        assert!((10_799..=10_800).contains(&delay.num_seconds()));
    }
}
