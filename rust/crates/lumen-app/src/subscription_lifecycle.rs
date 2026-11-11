//! Apply worker results to subscription records and their node snapshots.

use std::collections::HashSet;

use chrono::Utc;
use lumen_core::crypt::{is_happ_crypt_link, is_happ_link, is_incy_crypt_link, is_incy_link};
use lumen_core::link_parser::py_str_or;
use lumen_core::models::{Node, str_field};
use lumen_core::subscription::response::{SubscriptionResponse, merge_subscription_info};
use serde_json::{Map, Value, json};
use url::Url;
use uuid::Uuid;

use crate::node_service::{NodeEvent, NodeService};
use crate::subscription_cache::{
    FetchOutcome, current_parser, find_subscription, mark_fetch, timestamp,
};
use crate::subscription_service::{
    PreparedSubscription, SubscriptionApplyResult, SubscriptionTarget, reconcile_subscription,
};
use crate::subscription_worker::{SubscriptionDownload, SubscriptionJob, SubscriptionJobKind};

const STALE_PARSER: &str =
    "Ответ 304 не обновляет устаревший результат парсера; требуется полная загрузка";
const APPLY_FAILED: &str = "Не удалось применить подписку";

impl NodeService<'_> {
    /// Apply a finished download on the state owner and emit one save request.
    pub fn apply_fetched_subscription(
        &mut self,
        job: &SubscriptionJob,
        download: Result<SubscriptionDownload, String>,
    ) -> SubscriptionApplyResult {
        let url = job.url.trim();
        if url.is_empty() {
            return error("Пустой URL подписки");
        }
        let existing = find_subscription(self.state, url);
        if existing.is_none() && job.kind == SubscriptionJobKind::Update {
            return error("Подписка не найдена");
        }
        let mut download = download.unwrap_or_else(|error| SubscriptionDownload {
            response: SubscriptionResponse::default(),
            errors: vec![error],
            prepared: None,
        });
        let now = Utc::now();
        let id = subscription_id(existing.map(|index| &self.state.subscriptions[index]), url);
        let explicit_group = if job.kind == SubscriptionJobKind::Import {
            job.name.trim()
        } else {
            ""
        };
        let mut renamed = false;
        let group = if let Some(index) = existing {
            let record = &mut self.state.subscriptions[index];
            let old_group = str_field(record, "group");
            let old_group = old_group.trim();
            if !explicit_group.is_empty() && old_group != explicit_group {
                if !old_group.is_empty() {
                    for node in &mut self.state.nodes {
                        if node.subscription_id == id
                            || (node.subscription_id.is_empty() && group_of(node) == old_group)
                        {
                            node.group = explicit_group.into();
                            node.subscription_id.clone_from(&id);
                        }
                    }
                }
                record["name"] = json!(explicit_group);
                record["group"] = json!(explicit_group);
                renamed = true;
                explicit_group.into()
            } else if old_group.is_empty() {
                derive_group(url)
            } else {
                old_group.into()
            }
        } else if !explicit_group.is_empty() {
            explicit_group.into()
        } else {
            metadata_group(&download.response.userinfo).unwrap_or_else(|| derive_group(url))
        };

        let response = &mut download.response;
        let mut result = SubscriptionApplyResult::default();
        let mut record_changed = renamed;
        if response.not_modified || response.status == 304 {
            if let Some(index) = existing {
                let record = &mut self.state.subscriptions[index];
                let outcome = if current_parser(record) {
                    FetchOutcome::NotModified
                } else {
                    result.errors.push(STALE_PARSER.into());
                    FetchOutcome::Failed(STALE_PARSER)
                };
                mark_fetch(record, response, outcome, now);
                record_changed = true;
            }
        } else {
            if !response.text.is_empty() {
                let prepared = download.prepared.unwrap_or_else(|| {
                    PreparedSubscription::parse(&response.text, &self.state.settings)
                });
                result = reconcile_subscription(
                    self.state,
                    &response.text,
                    prepared,
                    SubscriptionTarget {
                        id: &id,
                        group: &group,
                        replace_existing_group: existing.is_some(),
                    },
                );
            }
            download.errors.append(&mut result.errors);
            let mut seen = HashSet::new();
            download.errors.retain(|error| seen.insert(error.clone()));
            result.errors = download.errors;
            if result.applied {
                let new_url = premium_url(url, &response.userinfo);
                let (effective_url, id) =
                    if new_url != url && find_subscription(self.state, &new_url).is_none() {
                        let new_id = subscription_id(None, &new_url);
                        for node in &mut self.state.nodes {
                            if node.subscription_id == id {
                                node.subscription_id.clone_from(&new_id);
                            }
                        }
                        (new_url, new_id)
                    } else {
                        (url.into(), id)
                    };
                let record = if let Some(index) = existing {
                    &mut self.state.subscriptions[index]
                } else {
                    self.state.subscriptions.push(json!({}));
                    self.state
                        .subscriptions
                        .last_mut()
                        .expect("inserted subscription")
                };
                record["url"] = json!(effective_url);
                record["id"] = json!(id);
                record["name"] = json!(group);
                record["group"] = json!(group);
                record["node_count"] = json!(result.node_count);
                record["updated_at"] = json!(timestamp(now));
                let mut info = record["userinfo"].as_object().cloned().unwrap_or_default();
                for key in [
                    "_lumen_applied",
                    "_lumen_group",
                    "_lumen_effective_url",
                    "_lumen_response_meta",
                    "_lumen_not_modified",
                ] {
                    response.userinfo.remove(key);
                }
                merge_subscription_info(&mut info, std::mem::take(&mut response.userinfo));
                record["userinfo"] = Value::Object(info);
                mark_fetch(record, response, FetchOutcome::Applied, now);
                result.subscription_id = Some(id);
                record_changed = true;
            } else if let Some(index) = existing {
                let message = result.errors.first().map_or(APPLY_FAILED, String::as_str);
                mark_fetch(
                    &mut self.state.subscriptions[index],
                    response,
                    FetchOutcome::Failed(message),
                    now,
                );
                record_changed = true;
            }
        }
        if renamed && !result.applied {
            self.notify(NodeEvent::NodesChanged);
        }
        self.notify_subscription_nodes(&result);
        if record_changed {
            self.notify(NodeEvent::SubscriptionsChanged);
            self.notify(NodeEvent::SaveRequested);
        }
        result
    }

    pub fn remove_subscription(&mut self, url: &str, delete_nodes: bool) {
        let url = url.trim();
        let Some(index) = find_subscription(self.state, url) else {
            return;
        };
        let record = self.state.subscriptions.remove(index);
        let id = str_field(&record, "id");
        let id = id.trim();
        let group = str_field(&record, "group");
        let group = group.trim();
        self.state
            .subscriptions
            .retain(|entry| entry["url"].as_str() != Some(url));
        self.notify(NodeEvent::SubscriptionsChanged);
        if delete_nodes {
            let ids: HashSet<_> = self
                .state
                .nodes
                .iter()
                .filter(|node| {
                    if id.is_empty() {
                        !group.is_empty()
                            && node.subscription_id.is_empty()
                            && group_of(node) == group
                    } else {
                        node.subscription_id == id
                    }
                })
                .map(|node| node.id.clone())
                .collect();
            if !ids.is_empty() {
                self.remove_nodes(&ids);
                return;
            }
        }
        self.notify(NodeEvent::SaveRequested);
    }
}

fn group_of(node: &Node) -> &str {
    if node.group.is_empty() {
        "Default"
    } else {
        &node.group
    }
}

fn error(message: &str) -> SubscriptionApplyResult {
    SubscriptionApplyResult {
        errors: vec![message.into()],
        ..Default::default()
    }
}

fn subscription_id(record: Option<&Value>, url: &str) -> String {
    let id = record
        .map(|record| str_field(record, "id"))
        .unwrap_or_default();
    if id.trim().is_empty() {
        Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes()).to_string()
    } else {
        id.trim().into()
    }
}

fn metadata_group(info: &Map<String, Value>) -> Option<String> {
    ["profileTitle", "subscriptionName", "name", "title"]
        .into_iter()
        .find_map(|key| {
            let value = py_str_or(info.get(key), "");
            let value = value.trim();
            (!value.is_empty()).then(|| value.chars().take(160).collect())
        })
}

fn derive_group(source: &str) -> String {
    if is_happ_link(source) {
        "Подписка Happ".into()
    } else if is_incy_link(source) {
        "Подписка Incy".into()
    } else if let Some(host) = Url::parse(source)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    {
        format!("Подписка {host}")
    } else {
        "Подписка".into()
    }
}

fn premium_url(source: &str, info: &Map<String, Value>) -> String {
    if !is_happ_crypt_link(source)
        && !is_incy_crypt_link(source)
        && let Some(premium) = info.get("premiumFeatures")
    {
        let replacement = str_field(premium, "new-url");
        let replacement = replacement.trim();
        if Url::parse(replacement).is_ok_and(|url| url.scheme() == "https" && url.has_host()) {
            return replacement.into();
        }
        let domain = str_field(premium, "new-domain");
        let domain = domain
            .trim()
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        if !domain.is_empty()
            && let Some((scheme, rest)) = source.split_once("://")
            && matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
        {
            let suffix = &rest[rest.find(['/', '?', '#']).unwrap_or(rest.len())..];
            let replacement = format!("{scheme}://{domain}{suffix}");
            if Url::parse(&replacement).is_ok_and(|url| url.has_host()) {
                return replacement;
            }
        }
    }
    source.into()
}

#[cfg(test)]
mod tests {
    use lumen_core::constants::SUBSCRIPTION_PARSER_REVISION;
    use lumen_core::models::AppState;

    use super::*;

    const URL: &str = "https://provider.example/token";
    const MOVED: &str = "https://moved.example/token";
    const LINK: &str =
        "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=tls#First";

    #[test]
    fn import_migrates_the_url_and_refresh_merges_quota_without_losing_node_identity() {
        let mut state = AppState::default();
        let mut connected = false;
        let mut service = NodeService {
            state: &mut state,
            connected: false,
            desired_connected: &mut connected,
            can_connect_without_selected_node: false,
            emit: &mut |_, _| {},
        };
        let mut response = SubscriptionResponse {
            text: LINK.into(),
            status: 200,
            userinfo: json!({
                "profileTitle": "Provider",
                "upload": 42, "download": 100,
                "premiumFeatures": {"new-domain": "moved.example", "mux-enable": "1"},
            })
            .as_object()
            .unwrap()
            .clone(),
            headers: [("etag".into(), "old".into())].into(),
            ..Default::default()
        };
        let import = SubscriptionJob::new(URL.into(), SubscriptionJobKind::Import);
        let result = service.apply_fetched_subscription(
            &import,
            Ok(SubscriptionDownload {
                response,
                errors: Vec::new(),
                prepared: None,
            }),
        );
        assert!(result.applied && result.errors.is_empty());
        let record = &service.state.subscriptions[0];
        assert_eq!(record["id"], "52f65277-f42c-558e-ad3b-33463c43a10c");
        assert_eq!(record["url"], MOVED);
        assert_eq!(record["group"], "Provider");
        assert_eq!(service.state.nodes[0].subscription_id, record["id"]);
        let node_id = service.state.nodes[0].id.clone();

        response = SubscriptionResponse {
            text: LINK.into(),
            status: 200,
            userinfo: json!({"download": 0, "premiumFeatures": {"mux-quic": "1"}})
                .as_object()
                .unwrap()
                .clone(),
            ..Default::default()
        };
        let update = SubscriptionJob::new(MOVED.into(), SubscriptionJobKind::Update);
        assert!(
            service
                .apply_fetched_subscription(
                    &update,
                    Ok(SubscriptionDownload {
                        response,
                        errors: Vec::new(),
                        prepared: None,
                    })
                )
                .applied
        );
        let record = &service.state.subscriptions[0];
        assert_eq!(record["userinfo"]["upload"], 42);
        assert_eq!(record["userinfo"]["download"], 0);
        assert_eq!(record["userinfo"]["premiumFeatures"]["mux-enable"], "1");
        assert_eq!(record["userinfo"]["premiumFeatures"]["mux-quic"], "1");
        assert!(record.get("etag").is_none());
        assert_eq!(record["parser_revision"], SUBSCRIPTION_PARSER_REVISION);
        assert_eq!(service.state.nodes[0].id, node_id);
        let restored = AppState::from_dict(&service.state.to_dict());
        assert_eq!(restored.subscriptions, service.state.subscriptions);

        service.remove_subscription(MOVED, true);
        assert!(service.state.subscriptions.is_empty());
        assert!(service.state.nodes.is_empty());
    }

    #[test]
    fn stale_parser_304_keeps_the_existing_validator_and_records_one_failure() {
        let mut state = AppState {
            subscriptions: vec![
                json!({"url": URL, "id": "sub", "parser_revision": 1, "etag": "old"}),
            ],
            ..Default::default()
        };
        let mut connected = false;
        let mut service = NodeService {
            state: &mut state,
            connected: false,
            desired_connected: &mut connected,
            can_connect_without_selected_node: false,
            emit: &mut |_, _| {},
        };
        let job = SubscriptionJob::new(URL.into(), SubscriptionJobKind::Update);
        let result = service.apply_fetched_subscription(
            &job,
            Ok(SubscriptionDownload {
                response: SubscriptionResponse {
                    status: 304,
                    not_modified: true,
                    headers: [("etag".into(), "ignored".into())].into(),
                    ..Default::default()
                },
                errors: Vec::new(),
                prepared: None,
            }),
        );
        assert_eq!(result.errors, [STALE_PARSER]);
        assert_eq!(state.subscriptions[0]["failure_count"], 1);
        assert_eq!(state.subscriptions[0]["last_status"], 304);
        assert_eq!(state.subscriptions[0]["etag"], "old");
        assert_eq!(state.subscriptions[0]["parser_revision"], 1);
    }
}
