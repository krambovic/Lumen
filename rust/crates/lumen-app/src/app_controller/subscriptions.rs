//! Node imports/edits/selection and subscription entry points on top of the
//! ported `NodeService` and `SubscriptionDispatcher`.

use std::collections::HashSet;
use std::time::Instant;

use lumen_core::models::Node;
use serde_json::Value;

use crate::app_controller::{AppController, AppEvent, DelayedAction};
use crate::node_service::{ImportOptions, MoveNode, NodeEvent, NodePatch, NodeService, all_groups};
use crate::subscription_dispatcher::{
    SubscriptionDispatchContext, SubscriptionDispatchEvent, SubscriptionEvent, SubscriptionRequest,
};
use crate::subscription_service::PreparedSubscription;
use crate::subscription_worker::{SubscriptionDownload, SubscriptionJob, SubscriptionJobKind};

impl AppController {
    /// Run a `NodeService` mutation against the live state and forward the
    /// emitted events to the controller signals.
    pub(crate) fn node_edit<T>(&mut self, f: impl FnOnce(&mut NodeService<'_>) -> T) -> T {
        let mut events = Vec::new();
        let can_connect = self.can_connect_without_selected_node();
        let mut desired = self.runtime.desired_connected;
        let result = {
            let mut service = NodeService {
                state: &mut self.state,
                connected: self.connected,
                desired_connected: &mut desired,
                can_connect_without_selected_node: can_connect,
                emit: &mut |event, _| events.push(event),
            };
            f(&mut service)
        };
        self.runtime.desired_connected = desired;
        for event in events {
            self.dispatch_node_event(event);
        }
        result
    }

    pub(crate) fn dispatch_node_event(&mut self, event: NodeEvent) {
        match event {
            NodeEvent::NodesChanged => self.emit(AppEvent::NodesChanged),
            NodeEvent::SelectionChanged => {
                self.emit(AppEvent::SelectionChanged(self.selected_node()));
            }
            NodeEvent::SubscriptionsChanged => self.emit(AppEvent::SubscriptionsChanged),
            NodeEvent::ResetAutoSwitch => {
                // Python `_reset_auto_switch_state(reset_cooldown=False,
                // reset_cycle=True)`.
                self.auto_switch.reset_for_connection(false, true);
            }
            NodeEvent::SaveRequested => self.save(),
            NodeEvent::ScheduleSave => self.schedule_save(),
            NodeEvent::ResolveCountries(delay) => {
                self.schedule_delayed(delay, DelayedAction::StartCountryIpResolution);
            }
            NodeEvent::TransitionRequested(reason) => {
                self.runtime.desired_connected = true;
                self.request_transition(reason);
            }
        }
    }

    fn subscription_proxy_url(&mut self) -> Option<String> {
        if self.connected {
            self.get_effective_http_proxy_url()
        } else {
            None
        }
    }

    fn dispatch_subscription_event(&mut self, event: SubscriptionEvent) {
        match event {
            SubscriptionEvent::ImportStatus(current) => {
                self.emit(AppEvent::SubscriptionImportStatus(current));
            }
            SubscriptionEvent::Imported(group) => {
                self.emit(AppEvent::SubscriptionImported(group));
            }
            SubscriptionEvent::Status { level, message } => self.emit_status(level, &message),
        }
    }

    // ── imports and node editing ───────────────────────────────────────────

    /// Python `import_nodes_from_text`.
    pub fn import_nodes_from_text(
        &mut self,
        text: &str,
        group: Option<&str>,
        auto_connect: Option<bool>,
        select_imported: bool,
    ) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        self.node_edit(|service| {
            service.import_from_text(
                text,
                ImportOptions {
                    group,
                    auto_connect,
                    select_imported,
                },
            )
        })
    }

    /// Python `import_parsed_nodes`.
    pub fn import_parsed_nodes(
        &mut self,
        nodes: Vec<Node>,
        group: Option<&str>,
        auto_connect: Option<bool>,
        select_imported: bool,
    ) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        self.node_edit(|service| {
            service.import_parsed(
                nodes,
                Vec::new(),
                ImportOptions {
                    group,
                    auto_connect,
                    select_imported,
                },
            )
        })
    }

    /// Python `add_manual_node`.
    pub fn add_manual_node(&mut self, node: Node) -> String {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return String::new();
        }
        self.node_edit(|service| service.add_manual(node).unwrap_or_default())
    }

    /// Python `update_node`.
    pub fn update_node(&mut self, node_id: &str, updates: &Value) -> bool {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return false;
        }
        let str_value = |key: &str| {
            updates
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        };
        let patch = NodePatch {
            name: str_value("name"),
            group: str_value("group"),
            scheme: str_value("scheme"),
            server: str_value("server"),
            port: updates.get("port").and_then(|value| value.as_i64()),
            outbound: updates
                .get("outbound")
                .and_then(|value| value.as_object())
                .cloned(),
            link: str_value("link"),
        };
        self.node_edit(|service| service.update_node(node_id, patch))
    }

    /// Python `bulk_update_nodes` (a `{"group": ...}` operations dict).
    pub fn bulk_update_nodes(&mut self, node_ids: &HashSet<String>, operations: &Value) -> usize {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return 0;
        }
        let group = operations
            .get("group")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        self.node_edit(|service| service.bulk_update_group(node_ids, group))
    }

    /// Python `get_all_groups`.
    pub fn get_all_groups(&self) -> Vec<String> {
        all_groups(&self.state)
    }

    /// Python `reorder_nodes` (`direction`: up/down/top/bottom).
    pub fn reorder_nodes(&mut self, node_id: &str, direction: &str) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        let direction = match direction.trim().to_lowercase().as_str() {
            "up" => MoveNode::Up,
            "down" => MoveNode::Down,
            "top" => MoveNode::Top,
            "bottom" => MoveNode::Bottom,
            _ => return,
        };
        self.node_edit(|service| service.reorder(node_id, direction));
    }

    /// Python `set_selected_node`.
    pub fn set_selected_node(&mut self, node_id: &str) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        self.node_edit(|service| service.set_selected_node(node_id));
        if let Some(node) = self.get_node_by_id(Some(node_id)) {
            lumen_engines::singbox::runtime_planner::prime_endpoint_resolution(&node.server);
        }
    }

    /// Python `remove_nodes`.
    pub fn remove_nodes(&mut self, node_ids: &HashSet<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        self.node_edit(|service| service.remove_nodes(node_ids));
    }

    /// Python `delete_group`.
    pub fn delete_group(&mut self, group: &str) -> bool {
        self.node_edit(|service| service.delete_group(group))
    }

    /// Python `remove_subscription`.
    pub fn remove_subscription(&mut self, url: &str, delete_nodes: bool) {
        self.node_edit(|service| service.remove_subscription(url, delete_nodes));
    }

    // ── subscription downloads (queued through the dispatcher) ─────────────

    /// Python `import_subscription`: the download runs through the
    /// subscription worker; import status and node deltas arrive as events.
    pub fn import_subscription(&mut self, url: &str, name: Option<&str>) -> (usize, Vec<String>) {
        self.dispatch_subscription_request(SubscriptionRequest::Import {
            url: url.to_owned(),
            name: name.unwrap_or_default().to_owned(),
        })
    }

    pub fn import_subscriptions(&mut self, text: &str) {
        self.dispatch_subscription_request(SubscriptionRequest::ImportLines(text.to_owned()));
    }

    fn dispatch_subscription_request(&mut self, request: SubscriptionRequest) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        let proxy_url = self.subscription_proxy_url();
        let context = SubscriptionDispatchContext {
            profile_loaded: self.profile_loaded(),
            locked: self.locked,
            effective_proxy_url: proxy_url.as_deref(),
        };
        let mut events = Vec::new();
        self.subscriptions.dispatch(
            &self.state,
            context,
            request,
            |event| events.push(event),
        );
        for event in events {
            self.dispatch_subscription_event(event);
        }
        (0, Vec::new())
    }

    /// Python `update_subscription`.
    pub fn update_subscription(&mut self, url: &str) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        let proxy_url = self.subscription_proxy_url();
        let context = SubscriptionDispatchContext {
            profile_loaded: self.profile_loaded(),
            locked: self.locked,
            effective_proxy_url: proxy_url.as_deref(),
        };
        let mut events = Vec::new();
        self.subscriptions.dispatch(
            &self.state,
            context,
            SubscriptionRequest::Update(url.to_owned()),
            |event| events.push(event),
        );
        for event in events {
            self.dispatch_subscription_event(event);
        }
        (0, Vec::new())
    }

    /// Python `update_all_subscriptions`.
    pub fn update_all_subscriptions(&mut self) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        let proxy_url = self.subscription_proxy_url();
        let context = SubscriptionDispatchContext {
            profile_loaded: self.profile_loaded(),
            locked: self.locked,
            effective_proxy_url: proxy_url.as_deref(),
        };
        let mut events = Vec::new();
        self.subscriptions.dispatch(
            &self.state,
            context,
            SubscriptionRequest::UpdateAll,
            |event| events.push(event),
        );
        for event in events {
            self.dispatch_subscription_event(event);
        }
        (0, Vec::new())
    }

    /// Python `apply_fetched_subscription`: apply an already-downloaded
    /// payload on the state owner.
    pub fn apply_fetched_subscription(
        &mut self,
        url: &str,
        name: Option<&str>,
        kind: &str,
        text: &str,
        userinfo: Option<&Value>,
        errors: Vec<String>,
        response_meta: Option<&Value>,
    ) -> (usize, Vec<String>) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return (0, vec!["Сначала откройте зашифрованный профиль".into()]);
        }
        let url = url.trim();
        if url.is_empty() {
            return (0, vec!["Пустой URL подписки".into()]);
        }
        let kind = if kind == "import" {
            SubscriptionJobKind::Import
        } else {
            SubscriptionJobKind::Update
        };
        let mut job = SubscriptionJob::new(url.to_owned(), kind);
        job.name = name.unwrap_or("").trim().to_owned();
        let mut response = lumen_core::subscription::response::SubscriptionResponse::default();
        if let Some(userinfo) = userinfo.and_then(|value| value.as_object()) {
            for (key, value) in userinfo {
                response.userinfo.insert(key.clone(), value.clone());
            }
        }
        if let Some(meta) = response_meta {
            response.status = meta
                .get("status")
                .and_then(|value| value.as_u64())
                .map(|value| value as u16)
                .unwrap_or(0);
            if let Some(headers) = meta.get("headers").and_then(|value| value.as_object()) {
                for (key, value) in headers {
                    if let Some(value) = value.as_str() {
                        response
                            .headers
                            .insert(key.to_ascii_lowercase(), value.to_owned());
                    }
                }
            }
        }
        let prepared = PreparedSubscription::parse(text, &self.state.settings);
        let download = SubscriptionDownload {
            response,
            errors,
            prepared: Some(prepared),
        };
        let result = self.node_edit(|service| {
            service.apply_fetched_subscription(&job, Ok(download))
        });
        (result.node_count, result.errors)
    }

    /// Python deep-link subscription import (single-instance delivery).
    pub fn dispatch_deep_link(&mut self, value: &str) -> bool {
        let proxy_url = self.subscription_proxy_url();
        let context = SubscriptionDispatchContext {
            profile_loaded: self.profile_loaded(),
            locked: self.locked,
            effective_proxy_url: proxy_url.as_deref(),
        };
        let mut events = Vec::new();
        let queued = self.subscriptions.dispatch_deep_link(
            &self.state,
            context,
            value,
            |event| events.push(event),
        );
        for event in events {
            self.dispatch_subscription_event(event);
        }
        queued
    }

    /// Python `cancel_import` (UI action on the dispatcher queue).
    pub fn cancel_subscription_import(&mut self) -> bool {
        let mut events = Vec::new();
        let cancelled = self
            .subscriptions
            .cancel_import(|event| events.push(event));
        for event in events {
            self.dispatch_subscription_event(event);
        }
        cancelled
    }

    /// Event-loop hook for subscription worker completions.
    pub(crate) fn handle_subscription_dispatch(&mut self, event: SubscriptionDispatchEvent) {
        let profile_loaded = self.profile_loaded();
        let can_connect = self.can_connect_without_selected_node();
        let mut desired = self.runtime.desired_connected;
        let mut node_events = Vec::new();
        let mut events = Vec::new();
        {
            let mut nodes = NodeService {
                state: &mut self.state,
                connected: self.connected,
                desired_connected: &mut desired,
                can_connect_without_selected_node: can_connect,
                emit: &mut |event, _| node_events.push(event),
            };
            self.subscriptions.handle_worker_event(
                event,
                &mut nodes,
                profile_loaded,
                |event| events.push(event),
            );
        }
        self.runtime.desired_connected = desired;
        for event in node_events {
            self.dispatch_node_event(event);
        }
        for event in events {
            self.dispatch_subscription_event(event);
        }
        self.subscriptions.reconfigure_auto_refresh(&self.state, Instant::now());
    }
}
