//! Preparation and reconciliation of downloaded subscription nodes.

use std::collections::{HashMap, HashSet, VecDeque};

use caseless::default_case_fold_str;
use lumen_core::country_flags::detect_country;
use lumen_core::link_parser::{normalize_node_outbound, parse_links_text, validate_node_outbound};
use lumen_core::models::{AppSettings, AppState, Node};
use lumen_core::pyjson::dumps_compact_sorted_ascii;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::node_service::group_key;

pub struct PreparedSubscription {
    nodes: Vec<Node>,
    errors: Vec<String>,
    invalid_filters: bool,
    filters: (String, String),
    digest: [u8; 32],
}

impl PreparedSubscription {
    /// Parse downloaded content without resolving local file references.
    pub fn parse(text: &str, settings: &AppSettings) -> Self {
        let (nodes, mut errors) = parse_links_text(text, false)
            .unwrap_or_else(|error| (Vec::new(), vec![error.to_string()]));
        let before_filters = errors.len();
        let includes = compile_patterns(
            &settings.subscription_include_regex,
            "Include regex",
            &mut errors,
        );
        let excludes = compile_patterns(
            &settings.subscription_exclude_regex,
            "Exclude regex",
            &mut errors,
        );
        let invalid_filters = errors.len() != before_filters;
        let mut prepared = Vec::new();
        if !invalid_filters {
            for mut node in nodes {
                if !includes.is_empty() || !excludes.is_empty() {
                    let fields =
                        [&*node.name, &*node.server, &*node.scheme, &*node.link].join("\n");
                    if (!includes.is_empty()
                        && !includes.iter().any(|regex| regex.is_match(&fields)))
                        || excludes.iter().any(|regex| regex.is_match(&fields))
                    {
                        continue;
                    }
                }
                normalize_node_outbound(&mut node);
                if let Some(error) = validate_node_outbound(&node) {
                    errors.push(error);
                    continue;
                }
                if !node.link.is_empty() {
                    if node.country_code.is_empty() {
                        node.country_code = detect_country(&node.name, &node.server);
                    }
                    prepared.push(node);
                }
            }
        }
        Self {
            nodes: prepared,
            errors,
            invalid_filters,
            filters: (
                settings.subscription_include_regex.clone(),
                settings.subscription_exclude_regex.clone(),
            ),
            digest: digest(text),
        }
    }
}

pub struct SubscriptionTarget<'a> {
    pub id: &'a str,
    pub group: &'a str,
    /// Adopt unlinked nodes in the exact group when refreshing a legacy subscription.
    pub replace_existing_group: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SubscriptionApplyResult {
    pub applied: bool,
    pub node_count: usize,
    pub errors: Vec<String>,
    pub retained_selected_node: bool,
    pub active_outbound_changed: bool,
}

pub(crate) fn reconcile_subscription(
    state: &mut AppState,
    text: &str,
    prepared: PreparedSubscription,
    target: SubscriptionTarget<'_>,
) -> SubscriptionApplyResult {
    let mut result = SubscriptionApplyResult::default();
    if target.id.trim().is_empty() {
        result
            .errors
            .push("У подписки отсутствует идентификатор".into());
        return result;
    }
    if prepared.filters.0 != state.settings.subscription_include_regex
        || prepared.filters.1 != state.settings.subscription_exclude_regex
        || prepared.digest != digest(text)
    {
        result
            .errors
            .push("Фильтры или содержимое изменились: повторите обновление подписки".into());
        return result;
    }
    let mut incoming = prepared.nodes;
    result.errors = prepared.errors;
    if prepared.invalid_filters || incoming.is_empty() {
        if result.errors.is_empty() {
            result
                .errors
                .push("Подписка не содержит серверов, подходящих под regex-фильтры".into());
        }
        return result;
    }
    let previous: Vec<_> = state
        .nodes
        .iter()
        .filter(|node| {
            node.subscription_id == target.id
                || (target.replace_existing_group
                    && node.subscription_id.is_empty()
                    && (if node.group.is_empty() {
                        "Default"
                    } else {
                        &node.group
                    }) == target.group)
        })
        .collect();
    if !previous.is_empty() && !result.errors.is_empty() {
        result
            .errors
            .push("Неполный разбор подписки: прежние серверы сохранены".into());
        return result;
    }
    let previous_ids: HashSet<_> = previous.iter().map(|node| node.id.as_str()).collect();
    let target_key = group_key(target.group);
    let occupied: HashSet<_> = state
        .nodes
        .iter()
        .filter(|node| {
            !node.link.is_empty()
                && !previous_ids.contains(node.id.as_str())
                && group_key(&node.group) == target_key
        })
        .map(|node| node.link.as_str())
        .collect();
    incoming.retain(|node| !occupied.contains(node.link.as_str()));
    if incoming.is_empty() {
        result
            .errors
            .push("Все серверы подписки уже есть в этой группе".into());
        return result;
    }
    for node in &mut incoming {
        node.group = target.group.into();
        node.subscription_id = target.id.into();
        node.source_key = source_key(node);
    }
    let mut order = state
        .nodes
        .iter()
        .filter(|node| !previous_ids.contains(node.id.as_str()))
        .map(|node| node.sort_order)
        .max()
        .unwrap_or_default();
    let selected = state.selected_node_id.as_deref();
    let active = previous
        .iter()
        .copied()
        .find(|node| Some(node.id.as_str()) == selected);
    result.active_outbound_changed =
        reconcile_nodes(&previous, &mut incoming, selected, &mut order);

    if let Some(active) = active {
        let active_key = source_key(active);
        let replacement = incoming
            .iter()
            .find(|node| node.id == active.id)
            .or_else(|| incoming.iter().find(|node| node.source_key == active_key))
            .or_else(|| incoming.iter().find(|node| node.link == active.link));
        if let Some(replacement) = replacement {
            state.selected_node_id = Some(replacement.id.clone());
        } else {
            incoming.push(active.clone());
            result.retained_selected_node = true;
        }
    }
    let previous_ids: HashSet<String> = previous_ids.into_iter().map(str::to_owned).collect();
    result.node_count = incoming.len();
    state.nodes.retain(|node| !previous_ids.contains(&node.id));
    state.nodes.extend(incoming);
    result.applied = true;
    result
}

fn reconcile_nodes(
    previous: &[&Node],
    incoming: &mut [Node],
    selected: Option<&str>,
    order: &mut i64,
) -> bool {
    let incoming_keys: HashSet<_> = incoming
        .iter()
        .map(|node| node.source_key.clone())
        .collect();
    let incoming_links: HashSet<_> = incoming.iter().map(|node| node.link.clone()).collect();
    let mut by_key: HashMap<_, VecDeque<_>> = HashMap::new();
    let mut by_link = HashMap::new();
    let mut by_endpoint: HashMap<_, Vec<_>> = HashMap::new();
    let mut reserved = HashSet::new();
    for &node in previous {
        let key = source_key(node);
        if incoming_keys.contains(&key) || incoming_links.contains(&node.link) {
            reserved.insert(node.id.as_str());
        }
        by_key.entry(key).or_default().push_back(node);
        if !node.link.is_empty() {
            by_link.insert(node.link.as_str(), node);
        }
        by_endpoint
            .entry(endpoint_key(node))
            .or_default()
            .push(node);
    }
    let mut used = HashSet::new();
    let mut reconnect = false;
    for node in incoming {
        let old = by_key
            .get_mut(&node.source_key)
            .and_then(|queue| {
                std::iter::from_fn(|| queue.pop_front()).find(|old| !used.contains(old.id.as_str()))
            })
            .or_else(|| {
                by_link
                    .get(node.link.as_str())
                    .copied()
                    .filter(|old| !used.contains(old.id.as_str()))
            })
            .or_else(|| {
                let mut matches = by_endpoint
                    .get(&endpoint_key(node))?
                    .iter()
                    .copied()
                    .filter(|old| {
                        !used.contains(old.id.as_str()) && !reserved.contains(old.id.as_str())
                    });
                let first = matches.next()?;
                matches.next().is_none().then_some(first)
            });
        if let Some(old) = old {
            used.insert(old.id.as_str());
            let unchanged = dumps_compact_sorted_ascii(outbound(old))
                == dumps_compact_sorted_ascii(outbound(node));
            reconnect |= !unchanged && Some(old.id.as_str()) == selected;
            preserve_node_metadata(node, old, unchanged);
        } else {
            *order = order.saturating_add(1);
            node.sort_order = *order;
        }
    }
    reconnect
}

fn preserve_node_metadata(node: &mut Node, old: &Node, unchanged: bool) {
    node.id.clone_from(&old.id);
    node.sort_order = old.sort_order;
    node.last_used_at.clone_from(&old.last_used_at);
    node.created_at.clone_from(&old.created_at);
    node.tags.clone_from(&old.tags);
    node.description.clone_from(&old.description);
    if !old.country_code.is_empty() {
        node.country_code.clone_from(&old.country_code);
    }
    if unchanged {
        node.ping_kind.clone_from(&old.ping_kind);
        node.ping_ms = old.ping_ms;
        node.speed_mbps = old.speed_mbps;
        node.is_alive = old.is_alive;
        node.ping_history.clone_from(&old.ping_history);
        node.speed_history.clone_from(&old.speed_history);
    } else {
        node.ping_kind.clear();
        node.ping_ms = None;
        node.speed_mbps = None;
        node.is_alive = None;
        node.ping_history.clear();
        node.speed_history.clear();
    }
}

fn compile_patterns(text: &str, label: &str, errors: &mut Vec<String>) -> Vec<Regex> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            match RegexBuilder::new(line).case_insensitive(true).build() {
                Ok(pattern) => Some(pattern),
                Err(error) => {
                    errors.push(format!("{label}, строка {}: {error}", index + 1));
                    None
                }
            }
        })
        .collect()
}

fn source_key(node: &Node) -> String {
    if !node.source_key.trim().is_empty() {
        return node.source_key.trim().into();
    }
    let payload = dumps_compact_sorted_ascii(json!({
        "scheme": node.scheme.trim().to_lowercase(),
        "server": node.server.trim().to_lowercase(),
        "port": node.port,
        "outbound": outbound(node),
    }));
    format!("{:x}", Sha256::digest(payload))
}

fn outbound(node: &Node) -> Value {
    if node.outbound.is_object() {
        node.outbound.clone()
    } else {
        json!({})
    }
}

fn endpoint_key(node: &Node) -> (String, String, i64, String) {
    (
        node.scheme.trim().to_lowercase(),
        node.server.trim().to_lowercase(),
        node.port,
        default_case_fold_str(node.name.trim()),
    )
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_service::{NodeEvent, NodeService};

    const ORIGINAL: &str = "vless://00000000-0000-0000-0000-000000000001@a.example:443#First";
    const CHANGED: &str = "vless://00000000-0000-0000-0000-000000000002@a.example:443#First";
    const OTHER: &str = "vless://00000000-0000-0000-0000-000000000003@b.example:443#Other";

    fn target() -> SubscriptionTarget<'static> {
        SubscriptionTarget {
            id: "sub-id",
            group: "Sub",
            replace_existing_group: false,
        }
    }

    fn apply(state: &mut AppState, text: &str) -> SubscriptionApplyResult {
        let prepared = PreparedSubscription::parse(text, &state.settings);
        reconcile_subscription(state, text, prepared, target())
    }

    #[test]
    fn refresh_keeps_identity_and_only_clears_measurements_for_changed_outbounds() {
        let mut state = AppState::default();
        let initial = format!("{ORIGINAL}\n{OTHER}");
        assert_eq!(apply(&mut state, &initial).node_count, 2);
        let first_id = state.nodes[0].id.clone();
        let other_id = state.nodes[1].id.clone();
        state.selected_node_id = Some(first_id.clone());
        state.nodes[0].description = "personal note".into();
        state.nodes[0].ping_ms = Some(10);
        state.nodes[1].ping_ms = Some(20);
        let update = format!("{OTHER}\n{CHANGED}");
        let prepared = PreparedSubscription::parse(&update, &state.settings);
        let mut desired = false;
        let mut events = Vec::new();
        let result = NodeService {
            state: &mut state,
            connected: true,
            desired_connected: &mut desired,
            can_connect_without_selected_node: false,
            emit: &mut |event, _| events.push(event),
        }
        .apply_subscription_nodes(&update, prepared, target());
        assert!(result.applied && result.active_outbound_changed && desired);
        assert_eq!(state.selected_node_id.as_deref(), Some(first_id.as_str()));
        assert_eq!(
            (&state.nodes[0].id, state.nodes[0].ping_ms),
            (&other_id, Some(20))
        );
        assert_eq!(
            (&state.nodes[1].id, state.nodes[1].ping_ms),
            (&first_id, None)
        );
        assert_eq!(state.nodes[1].description, "personal note");
        assert_eq!(
            events,
            [
                NodeEvent::NodesChanged,
                NodeEvent::SelectionChanged,
                NodeEvent::TransitionRequested("active subscription updated")
            ]
        );
    }

    #[test]
    fn omitted_active_node_is_retained_until_another_node_is_selected() {
        let mut state = AppState::default();
        assert!(apply(&mut state, ORIGINAL).applied);
        let active = state.nodes[0].id.clone();
        state.selected_node_id = Some(active.clone());
        let result = apply(&mut state, OTHER);
        assert!(result.retained_selected_node && !result.active_outbound_changed);
        assert_eq!(state.nodes[1].id, active);
        state.selected_node_id = Some(state.nodes[0].id.clone());
        assert_eq!(apply(&mut state, OTHER).node_count, 1);
    }

    #[test]
    fn partial_parse_and_stale_preparation_leave_nodes_untouched() {
        let mut state = AppState::default();
        assert!(apply(&mut state, ORIGINAL).applied);
        let original = state.nodes.clone();
        let partial = format!("{OTHER}\ninvalid://bad");
        let result = apply(&mut state, &partial);
        assert!(!result.applied);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.contains("прежние серверы сохранены"))
        );
        let prepared = PreparedSubscription::parse(OTHER, &state.settings);
        state.settings.subscription_include_regex = "other".into();
        assert!(!reconcile_subscription(&mut state, OTHER, prepared, target()).applied);
        assert_eq!(state.nodes, original);
    }

    #[test]
    fn filters_are_case_insensitive_and_excludes_win() {
        let mut state = AppState::default();
        state.settings.subscription_include_regex = "FIRST\nOTHER".into();
        state.settings.subscription_exclude_regex = r"^b\.example$|OTHER".into();
        let text = format!("{ORIGINAL}\n{OTHER}");
        assert_eq!(apply(&mut state, &text).node_count, 1);
        assert_eq!(state.nodes[0].server, "a.example");
        state.settings.subscription_exclude_regex = "\n[".into();
        let result = apply(&mut state, &text);
        assert!(!result.applied);
        assert!(result.errors[0].starts_with("Exclude regex, строка 2:"));
    }
}
