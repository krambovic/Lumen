//! User-initiated node imports, edits, groups and selection.

use std::collections::HashSet;
use std::time::Duration;

use caseless::default_case_fold_str;
use lumen_core::country_flags::detect_country;
use lumen_core::link_parser::{
    normalize_node_outbound, parse_links_text, py_str_or, py_truthy, validate_node_outbound,
};
use lumen_core::models::{AppState, Node};
use serde_json::{Map, Value};

use crate::subscription_service::{
    PreparedSubscription, SubscriptionApplyResult, SubscriptionTarget, reconcile_subscription,
};

const DEFAULT_GROUP: &str = "Default";
const COUNTRY_LOOKUP_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub enum NodeEvent {
    NodesChanged,
    SelectionChanged,
    SubscriptionsChanged,
    ResetAutoSwitch,
    SaveRequested,
    ScheduleSave,
    ResolveCountries(Duration),
    TransitionRequested(&'static str),
}

#[derive(Default)]
pub struct ImportOptions<'a> {
    pub group: Option<&'a str>,
    pub auto_connect: Option<bool>,
    pub select_imported: bool,
}

#[derive(Default)]
pub struct NodePatch {
    pub name: Option<String>,
    pub group: Option<String>,
    pub scheme: Option<String>,
    pub server: Option<String>,
    pub port: Option<i64>,
    pub outbound: Option<Map<String, Value>>,
    pub link: Option<String>,
}

pub enum MoveNode {
    Up,
    Down,
    Top,
    Bottom,
}

pub struct NodeService<'a> {
    pub state: &'a mut AppState,
    pub connected: bool,
    pub desired_connected: &'a mut bool,
    pub can_connect_without_selected_node: bool,
    pub emit: &'a mut dyn FnMut(NodeEvent, &AppState),
}

impl NodeService<'_> {
    pub fn apply_subscription_nodes(
        &mut self,
        text: &str,
        prepared: PreparedSubscription,
        target: SubscriptionTarget<'_>,
    ) -> SubscriptionApplyResult {
        let result = reconcile_subscription(self.state, text, prepared, target);
        if result.applied {
            self.notify(NodeEvent::NodesChanged);
            self.notify(NodeEvent::SelectionChanged);
            if result.active_outbound_changed && self.connection_requested() {
                *self.desired_connected = true;
                self.notify(NodeEvent::TransitionRequested(
                    "active subscription updated",
                ));
            }
        }
        result
    }

    /// Only this user-initiated entry point treats text as a local file reference.
    pub fn import_from_text(
        &mut self,
        text: &str,
        options: ImportOptions<'_>,
    ) -> (usize, Vec<String>) {
        match parse_links_text(text, true) {
            Ok((nodes, errors)) => self.import_parsed(nodes, errors, options),
            Err(error) => (0, vec![error.to_string()]),
        }
    }

    pub fn import_parsed(
        &mut self,
        nodes: Vec<Node>,
        mut errors: Vec<String>,
        options: ImportOptions<'_>,
    ) -> (usize, Vec<String>) {
        if nodes.is_empty() {
            return (0, errors);
        }
        let mut existing: HashSet<_> = self
            .state
            .nodes
            .iter()
            .filter(|node| !node.link.is_empty())
            .map(|node| (node.link.clone(), group_key(&node.group)))
            .collect();
        let initial_count = self.state.nodes.len();
        let mut order = self.max_order();
        for mut node in nodes {
            if let Err(problem) = validate_for_import(&mut node) {
                errors.push(problem);
                continue;
            }
            node.group = normalized_group(options.group.unwrap_or(&node.group)).into();
            if !node.link.is_empty()
                && !existing.insert((node.link.clone(), group_key(&node.group)))
            {
                continue;
            }
            order = order.saturating_add(1);
            finish_node(&mut node, order);
            self.state.nodes.push(node);
        }

        let added = self.state.nodes.len() - initial_count;
        let mut selection_changed = false;
        if options.select_imported
            && let Some(first) = self.state.nodes.get(initial_count)
            && !first.id.is_empty()
            && self.state.selected_node_id.as_deref() != Some(&first.id)
        {
            self.state.selected_node_id = Some(first.id.clone());
            selection_changed = true;
            self.notify(NodeEvent::ResetAutoSwitch);
        }
        self.notify(NodeEvent::NodesChanged);
        self.notify(NodeEvent::SelectionChanged);
        self.notify(NodeEvent::SaveRequested);
        self.notify(NodeEvent::ResolveCountries(COUNTRY_LOOKUP_DELAY));

        let auto_connect = options
            .auto_connect
            .unwrap_or(self.state.settings.auto_connect_on_import);
        let reconcile =
            options.auto_connect != Some(false) && selection_changed && self.connection_requested();
        if added > 0 && (auto_connect || reconcile) {
            *self.desired_connected = true;
            self.notify(NodeEvent::TransitionRequested("new node imported"));
        }
        (added, errors)
    }

    pub fn add_manual(&mut self, mut node: Node) -> Result<String, String> {
        validate_for_import(&mut node)?;
        node.group = normalized_group(&node.group).into();
        let name = [&node.name[..], &node.server, &node.scheme, "Server"]
            .into_iter()
            .find(|part| !part.is_empty())
            .unwrap_or("Server")
            .trim()
            .to_owned();
        node.name = name;
        finish_node(&mut node, self.max_order().saturating_add(1));
        let id = node.id.clone();
        self.state.nodes.push(node);
        self.save_nodes();
        self.notify(NodeEvent::ResolveCountries(COUNTRY_LOOKUP_DELAY));
        Ok(id)
    }

    pub fn remove_nodes(&mut self, node_ids: &HashSet<String>) {
        if node_ids.is_empty() {
            return;
        }
        let removed_selected = self
            .state
            .selected_node_id
            .as_ref()
            .is_some_and(|id| node_ids.contains(id));
        let reconcile = removed_selected && self.connection_requested();
        self.state.nodes.retain(|node| !node_ids.contains(&node.id));
        if removed_selected {
            self.state.selected_node_id = self.state.nodes.first().map(|node| node.id.clone());
            self.notify(NodeEvent::ResetAutoSwitch);
        }
        self.notify(NodeEvent::NodesChanged);
        self.notify(NodeEvent::SelectionChanged);
        self.notify(NodeEvent::SaveRequested);
        if reconcile {
            if self.state.selected_node_id.is_some() {
                *self.desired_connected = true;
            } else if !self.can_connect_without_selected_node {
                *self.desired_connected = false;
            }
            self.notify(NodeEvent::TransitionRequested("active node removed"));
        }
    }

    pub fn update_node(&mut self, node_id: &str, patch: NodePatch) -> bool {
        if node_id.is_empty() {
            return false;
        }
        let Some(node) = self.state.nodes.iter_mut().find(|node| node.id == node_id) else {
            return false;
        };
        if let Some(name) = patch.name {
            node.name = name;
        }
        if let Some(group) = patch.group {
            node.group = group;
        }
        if let Some(scheme) = patch.scheme {
            node.scheme = scheme;
        }
        if let Some(server) = patch.server {
            node.server = server;
        }
        if let Some(port) = patch.port {
            node.port = port;
        }
        if let Some(outbound) = patch.outbound {
            node.outbound = Value::Object(outbound);
        }
        if let Some(link) = patch.link {
            node.link = link;
        }
        self.save_nodes();
        if self.connection_requested() {
            self.notify(NodeEvent::TransitionRequested("node updated"));
        }
        true
    }

    pub fn bulk_update_group(&mut self, node_ids: &HashSet<String>, group: &str) -> usize {
        let mut updated = 0;
        for node in &mut self.state.nodes {
            if node_ids.contains(&node.id) {
                if !group.is_empty() {
                    node.group = group.into();
                }
                updated += 1;
            }
        }
        if updated > 0 {
            self.save_nodes();
        }
        updated
    }

    pub fn delete_group(&mut self, group: &str) -> bool {
        let key = default_case_fold_str(group.trim());
        if key.is_empty() || key == "default" {
            return false;
        }

        let mut removed_subscriptions = HashSet::new();
        let subscriptions_before = self.state.subscriptions.len();
        self.state.subscriptions.retain(|entry| {
            let group = entry
                .get("group")
                .filter(|value| py_truthy(value))
                .or_else(|| entry.get("name"));
            if default_case_fold_str(py_str_or(group, "").trim()) != key {
                return true;
            }
            let id = py_str_or(entry.get("id"), "");
            if !id.trim().is_empty() {
                removed_subscriptions.insert(id.trim().to_owned());
            }
            false
        });
        let subscriptions_changed = subscriptions_before != self.state.subscriptions.len();
        if subscriptions_changed {
            self.notify(NodeEvent::SubscriptionsChanged);
        }

        let groups_before = self.state.manual_groups.len();
        self.state
            .manual_groups
            .retain(|group| default_case_fold_str(group.trim()) != key);
        let node_ids = self
            .state
            .nodes
            .iter()
            .filter(|node| {
                group_key(&node.group) == key
                    || removed_subscriptions.contains(&node.subscription_id)
            })
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        if !node_ids.is_empty() {
            self.remove_nodes(&node_ids);
            return true;
        }
        let changed = subscriptions_changed || groups_before != self.state.manual_groups.len();
        if changed {
            self.notify(NodeEvent::SaveRequested);
        }
        changed
    }

    pub fn reorder(&mut self, node_id: &str, direction: MoveNode) {
        let mut ordered: Vec<_> = self.state.nodes.iter_mut().collect();
        ordered.sort_by_key(|node| node.sort_order);
        let Some(index) = ordered.iter().position(|node| node.id == node_id) else {
            return;
        };
        let last = ordered.len() - 1;
        match direction {
            MoveNode::Up if index > 0 => ordered.swap(index, index - 1),
            MoveNode::Down if index < last => ordered.swap(index, index + 1),
            MoveNode::Top if index > 0 => ordered[..=index].rotate_right(1),
            MoveNode::Bottom if index < last => ordered[index..].rotate_left(1),
            _ => return,
        }
        for (index, node) in ordered.into_iter().enumerate() {
            node.sort_order = index as i64 + 1;
        }
        self.save_nodes();
    }

    pub fn set_selected_node(&mut self, node_id: &str) {
        if self.state.selected_node_id.as_deref() == Some(node_id) {
            return;
        }
        self.state.selected_node_id = Some(node_id.into());
        self.notify(NodeEvent::ResetAutoSwitch);
        self.notify(NodeEvent::SelectionChanged);
        self.notify(NodeEvent::ScheduleSave);
        if self.connection_requested() {
            *self.desired_connected = true;
            self.notify(NodeEvent::TransitionRequested("node switched"));
        }
    }

    fn max_order(&self) -> i64 {
        self.state
            .nodes
            .iter()
            .map(|node| node.sort_order)
            .max()
            .unwrap_or(0)
    }

    fn connection_requested(&self) -> bool {
        self.connected || *self.desired_connected
    }

    fn notify(&mut self, event: NodeEvent) {
        (self.emit)(event, self.state);
    }

    fn save_nodes(&mut self) {
        self.notify(NodeEvent::NodesChanged);
        self.notify(NodeEvent::SaveRequested);
    }
}

pub fn all_groups(state: &AppState) -> Vec<String> {
    let groups: HashSet<_> = state
        .nodes
        .iter()
        .map(|node| node.group.as_str())
        .chain(state.manual_groups.iter().map(String::as_str))
        .map(str::trim)
        .filter(|group| !group.is_empty() && default_case_fold_str(group) != "default")
        .collect();
    let mut groups: Vec<_> = groups.into_iter().map(str::to_owned).collect();
    groups.sort_by_cached_key(|group| (group.to_lowercase(), group.clone()));
    groups.insert(0, DEFAULT_GROUP.into());
    groups
}

fn normalized_group(group: &str) -> &str {
    let group = group.trim();
    if group.is_empty() {
        DEFAULT_GROUP
    } else {
        group
    }
}

pub(crate) fn group_key(group: &str) -> String {
    let group = if group.is_empty() {
        DEFAULT_GROUP
    } else {
        group.trim()
    };
    default_case_fold_str(group)
}

fn validate_for_import(node: &mut Node) -> Result<(), String> {
    normalize_node_outbound(node);
    match validate_node_outbound(node) {
        Some(problem) => Err(problem),
        None => Ok(()),
    }
}

fn finish_node(node: &mut Node, order: i64) {
    if node.country_code.is_empty() {
        node.country_code = detect_country(&node.name, &node.server);
    }
    node.sort_order = order;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, group: &str, order: i64) -> Node {
        Node {
            id: id.into(),
            group: group.into(),
            sort_order: order,
            scheme: "vless".into(),
            server: "example.test".into(),
            port: 443,
            outbound: json!({"protocol": "vless"}),
            ..Node::default()
        }
    }

    #[test]
    fn import_preserves_active_selection_until_user_selects_the_new_node() {
        let mut state = AppState {
            nodes: vec![node("active", "Default", 1)],
            selected_node_id: Some("active".into()),
            ..AppState::default()
        };
        state.settings.auto_connect_on_import = false;
        let mut desired_connected = true;
        let mut events = Vec::new();
        let mut service = NodeService {
            state: &mut state,
            connected: true,
            desired_connected: &mut desired_connected,
            can_connect_without_selected_node: false,
            emit: &mut |event, _| events.push(event),
        };
        let (added, errors) = service.import_from_text(
            "vless://bbbbbbbb-0000-4000-8000-000000000000@example.test:443",
            ImportOptions::default(),
        );
        assert_eq!((added, errors), (1, vec![]));
        assert_eq!(service.state.selected_node_id.as_deref(), Some("active"));
        let fresh = service.state.nodes[1].id.clone();
        service.set_selected_node(&fresh);
        assert_eq!(state.selected_node_id, Some(fresh));
        assert_eq!(
            events,
            [
                NodeEvent::NodesChanged,
                NodeEvent::SelectionChanged,
                NodeEvent::SaveRequested,
                NodeEvent::ResolveCountries(COUNTRY_LOOKUP_DELAY),
                NodeEvent::ResetAutoSwitch,
                NodeEvent::SelectionChanged,
                NodeEvent::ScheduleSave,
                NodeEvent::TransitionRequested("node switched")
            ]
        );
    }

    #[test]
    fn import_deduplicates_links_within_case_folded_groups() {
        let mut original = node("old", "Straße", 4);
        original.link = "original".into();
        let mut duplicate = original.clone();
        duplicate.id = "duplicate".into();
        duplicate.group = " STRASSE ".into();
        let mut new = duplicate.clone();
        new.id = "new".into();
        new.group = " Other ".into();
        let mut state = AppState {
            nodes: vec![original],
            ..AppState::default()
        };
        let mut desired_connected = false;
        let mut service = NodeService {
            state: &mut state,
            connected: false,
            desired_connected: &mut desired_connected,
            can_connect_without_selected_node: false,
            emit: &mut |_, _| {},
        };
        assert_eq!(
            service.import_parsed(
                vec![duplicate, new],
                vec![],
                ImportOptions {
                    auto_connect: Some(false),
                    select_imported: true,
                    ..ImportOptions::default()
                }
            ),
            (1, vec![])
        );
        assert_eq!(state.selected_node_id.as_deref(), Some("new"));
        assert_eq!(
            (&*state.nodes[1].group, state.nodes[1].sort_order),
            ("Other", 5)
        );
        assert!(!desired_connected);
    }

    #[test]
    fn delete_group_removes_linked_nodes_and_switches_to_remaining_selection() {
        let mut active = node("active", "Elsewhere", 1);
        active.subscription_id = "sub-id".into();
        let mut state = AppState {
            nodes: vec![
                active,
                node("keep", "Default", 3),
                node("grouped", "Temporary", 4),
            ],
            selected_node_id: Some("active".into()),
            manual_groups: vec!["Temporary".into(), "Other".into()],
            subscriptions: vec![json!({"id":"sub-id", "group":"Temporary"})],
            ..AppState::default()
        };
        let mut desired_connected = false;
        let mut events = Vec::new();
        let mut service = NodeService {
            state: &mut state,
            connected: true,
            desired_connected: &mut desired_connected,
            can_connect_without_selected_node: false,
            emit: &mut |event, _| events.push(event),
        };
        assert!(!service.delete_group("Default"));
        assert!(service.delete_group(" temporary "));
        assert!(desired_connected);
        assert!(state.subscriptions.is_empty());
        assert_eq!(all_groups(&state), ["Default", "Other"]);
        assert_eq!(state.selected_node_id.as_deref(), Some("keep"));
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(
            events,
            [
                NodeEvent::SubscriptionsChanged,
                NodeEvent::ResetAutoSwitch,
                NodeEvent::NodesChanged,
                NodeEvent::SelectionChanged,
                NodeEvent::SaveRequested,
                NodeEvent::TransitionRequested("active node removed")
            ]
        );
    }

    #[test]
    fn moving_a_node_updates_sort_order_without_replacing_storage_order() {
        let mut state = AppState {
            nodes: vec![
                node("a", "Default", 10),
                node("b", "Default", 5),
                node("c", "Default", 15),
            ],
            ..AppState::default()
        };
        let mut desired_connected = false;
        let mut events = Vec::new();
        let mut service = NodeService {
            state: &mut state,
            connected: false,
            desired_connected: &mut desired_connected,
            can_connect_without_selected_node: false,
            emit: &mut |event, _| events.push(event),
        };
        service.reorder("c", MoveNode::Top);
        assert_eq!(
            state
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node.sort_order))
                .collect::<Vec<_>>(),
            [("a", 3), ("b", 2), ("c", 1)]
        );
        assert_eq!(events, [NodeEvent::NodesChanged, NodeEvent::SaveRequested]);
    }
}
