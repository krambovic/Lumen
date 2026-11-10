//! Node selection, runtime preparation and background country resolution.

use std::collections::{BTreeMap, btree_map::Entry};
use std::future::Future;

use lumen_core::country_flags::{detect_country, resolve_countries};
use lumen_core::link_parser::{
    normalize_node_outbound, py_str_or, repair_node_outbound_from_link, validate_node_outbound,
};
use lumen_core::models::{AppSettings, Node};
use tokio::task::JoinHandle;

pub use lumen_engines::singbox::runtime_planner::{
    is_xray_exclusive_node, node_uses_xray_sidecar, xray_only_message,
};

use crate::health_listener::PROXY_PROTOCOLS;

/// Returns whether country changes should be scheduled for persistence.
pub fn detect_countries_sync(nodes: &mut [Node]) -> bool {
    let mut changed = false;
    for node in nodes.iter_mut().filter(|node| node.country_code.is_empty()) {
        let code = detect_country(&node.name, &node.server);
        if !code.is_empty() {
            node.country_code = code;
            changed = true;
        }
    }
    changed
}

/// Owns the single background lookup for a controller. Completion can be
/// sent back to the controller's event loop through `on_resolved`.
#[derive(Default)]
pub struct CountryResolver {
    task: Option<JoinHandle<()>>,
}

impl CountryResolver {
    pub fn start_country_ip_resolution(
        &mut self,
        nodes: &[Node],
        on_resolved: impl FnOnce(BTreeMap<String, String>) + Send + 'static,
    ) -> bool {
        self.start_with(
            nodes,
            |hosts| async move { resolve_countries(&hosts).await },
            on_resolved,
        )
    }

    fn start_with<F>(
        &mut self,
        nodes: &[Node],
        resolve: impl FnOnce(Vec<String>) -> F + Send + 'static,
        on_resolved: impl FnOnce(BTreeMap<String, String>) + Send + 'static,
    ) -> bool
    where
        F: Future<Output = Option<BTreeMap<String, String>>> + Send,
    {
        if self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return false;
        }
        let mut unresolved = nodes
            .iter()
            .filter(|node| node.country_code.is_empty())
            .peekable();
        if unresolved.peek().is_none() {
            return false;
        }
        let mut hosts = Vec::new();
        let mut ids_by_host: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in unresolved {
            let host = node.server.trim();
            if host.is_empty() {
                continue;
            }
            let ids = match ids_by_host.entry(host.to_owned()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    hosts.push(host.to_owned());
                    entry.insert(Vec::new())
                }
            };
            ids.push(node.id.clone());
        }
        self.task = Some(tokio::spawn(async move {
            let countries = if hosts.is_empty() {
                BTreeMap::new()
            } else if let Some(countries) = resolve(hosts).await {
                countries
            } else {
                return;
            };
            let mut results = BTreeMap::new();
            for (host, code) in countries {
                if let Some(ids) = ids_by_host.remove(&host) {
                    for id in ids {
                        results.insert(id, code.clone());
                    }
                }
            }
            on_resolved(results);
        }));
        true
    }

    pub fn cancel(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for CountryResolver {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Returns whether to save immediately and emit the updated node list.
pub fn on_countries_resolved(nodes: &mut [Node], results: &BTreeMap<String, String>) -> bool {
    for node in nodes {
        if let Some(code) = results.get(&node.id) {
            node.country_code.clone_from(code);
        }
    }
    !results.is_empty()
}

pub fn get_node_by_id<'a>(nodes: &'a [Node], node_id: Option<&str>) -> Option<&'a Node> {
    let node_id = node_id.filter(|id| !id.is_empty())?;
    nodes.iter().find(|node| node.id == node_id)
}

pub fn is_native_singbox_only_node(node: Option<&Node>) -> bool {
    let Some(node) = node.filter(|node| node.outbound.is_object()) else {
        return false;
    };
    let protocol = py_str_or(node.outbound.get("protocol"), &node.scheme)
        .trim()
        .to_lowercase();
    matches!(
        protocol.as_str(),
        "warp"
            | "wireguard"
            | "awg"
            | "hysteria"
            | "hysteria2"
            | "tuic"
            | "mieru"
            | "masque"
            | "openvpn"
            | "naive"
            | "singbox_config"
    ) || (node.outbound["singbox"].is_object() && !PROXY_PROTOCOLS.contains(&protocol.as_str()))
}

pub fn proxy_core_for_node(node: Option<&Node>, preference: &str) -> &'static str {
    match preference {
        "xray" => "xray",
        "singbox" => "singbox",
        _ if node.is_none() || is_xray_exclusive_node(node) => "xray",
        _ => "singbox",
    }
}

pub fn node_supported_by_core(node: Option<&Node>, preference: &str) -> bool {
    match preference {
        "auto" => true,
        "xray" => !is_native_singbox_only_node(node),
        _ => !is_xray_exclusive_node(node),
    }
}

pub fn core_preference_of(settings: Option<&AppSettings>) -> &str {
    settings
        .map(|settings| settings.core_preference.as_str())
        .filter(|preference| !preference.is_empty())
        .unwrap_or("auto")
}

pub fn native_singbox_only_message(node: Option<&Node>) -> String {
    let name = match node {
        Some(node) if !node.name.is_empty() => &node.name,
        Some(node) => &node.server,
        None => "Этот сервер",
    };
    format!(
        "{name} не поддерживается ядром Xray. \
         Выберите Auto или sing-box extended в «Настройки → Сеть → Основное ядро»: \
         Lumen запустит этот сервер через sing-box extended как в системном прокси, так и в TUN."
    )
}

pub fn prepare_node_for_runtime(
    node: Option<&mut Node>,
    on_changed: impl FnOnce(&Node),
) -> Option<String> {
    let node = node?;
    let repaired = repair_node_outbound_from_link(node);
    if normalize_node_outbound(node) || repaired {
        on_changed(node);
    }
    validate_node_outbound(node)
}

pub fn get_fastest_alive_node<'a>(
    nodes: &'a [Node],
    selected: Option<&'a Node>,
) -> Option<&'a Node> {
    let has_alive = nodes.iter().any(|node| node.is_alive == Some(true));
    let candidates = nodes.iter().filter(|node| {
        if has_alive {
            node.is_alive == Some(true)
        } else {
            node.ping_ms.is_some()
        }
    });
    fastest_node(candidates).or(selected)
}

pub(crate) fn fastest_node<'a>(
    candidates: impl Iterator<Item = &'a Node> + Clone,
) -> Option<&'a Node> {
    let with_speed = candidates.clone().filter_map(|node| {
        node.speed_mbps
            .filter(|speed| *speed > 0.0)
            .map(|speed| (node, speed))
    });
    if let Some((node, _)) =
        with_speed.reduce(|first, next| if next.1 > first.1 { next } else { first })
    {
        return Some(node);
    }
    candidates.min_by_key(|node| (node.ping_ms.is_none(), node.ping_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[test]
    fn core_selection_preserves_native_and_xray_only_semantics() {
        let native = Node {
            scheme: "tuic".into(),
            outbound: json!({"protocol": ""}),
            ..Node::default()
        };
        let exclusive = Node {
            outbound: json!({"protocol": "vless", "streamSettings": {"finalmask": {"enabled": true}}}),
            ..Node::default()
        };
        assert_eq!(proxy_core_for_node(None, "auto"), "xray");
        assert_eq!(proxy_core_for_node(Some(&native), "auto"), "singbox");
        assert_eq!(proxy_core_for_node(Some(&exclusive), "auto"), "xray");
        assert!(!node_supported_by_core(Some(&native), "xray"));
        assert!(!node_supported_by_core(Some(&exclusive), "singbox"));
        assert!(node_supported_by_core(Some(&exclusive), "auto"));
    }

    #[test]
    fn fastest_node_prefers_alive_then_speed_then_ping_and_keeps_first_tie() {
        let first = Node {
            id: "first".into(),
            is_alive: Some(true),
            ping_ms: Some(40),
            speed_mbps: Some(3.0),
            ..Node::default()
        };
        let mut nodes = [
            first.clone(),
            Node {
                id: "second".into(),
                ping_ms: Some(10),
                ..first.clone()
            },
            Node {
                id: "third".into(),
                is_alive: Some(false),
                ping_ms: Some(5),
                speed_mbps: Some(99.0),
                ..first.clone()
            },
        ];
        assert_eq!(get_fastest_alive_node(&nodes, None).unwrap().id, "first");
        nodes[0].speed_mbps = None;
        nodes[1].speed_mbps = Some(0.0);
        assert_eq!(get_fastest_alive_node(&nodes, None).unwrap().id, "second");
        nodes[0].is_alive = None;
        nodes[1].is_alive = None;
        assert_eq!(get_fastest_alive_node(&nodes, None).unwrap().id, "third");
        assert_eq!(
            get_fastest_alive_node(&[], Some(&first)).unwrap().id,
            "first"
        );
        assert!(get_node_by_id(&nodes, Some("")).is_none());
        assert_eq!(get_node_by_id(&nodes, Some("third")).unwrap().id, "third");
    }

    #[tokio::test]
    async fn country_resolution_deduplicates_hosts_and_keeps_running_worker() {
        let mut resolver = CountryResolver::default();
        let mut nodes = [
            Node {
                id: "first".into(),
                server: " example.test ".into(),
                ..Node::default()
            },
            Node {
                id: "second".into(),
                server: "example.test".into(),
                ..Node::default()
            },
            Node {
                id: "third".into(),
                name: "Frankfurt".into(),
                server: "known.test".into(),
                ..Node::default()
            },
        ];
        assert!(detect_countries_sync(&mut nodes));
        assert_eq!(nodes[2].country_code, "DE");
        assert!(!detect_countries_sync(&mut nodes));
        let (release, wait) = oneshot::channel();
        let (emit, results) = oneshot::channel();
        assert!(resolver.start_with(
            &nodes,
            move |hosts| async move {
                assert_eq!(hosts, ["example.test"]);
                wait.await.unwrap();
                Some(BTreeMap::from([("example.test".into(), "US".into())]))
            },
            move |results| {
                emit.send(results).unwrap();
            }
        ));
        assert!(!resolver.start_with(&nodes, |_| async { panic!("duplicate worker") }, |_| {}));
        release.send(()).unwrap();
        let results = tokio::time::timeout(Duration::from_secs(2), results)
            .await
            .unwrap()
            .unwrap();
        assert!(on_countries_resolved(&mut nodes, &results));
        assert_eq!(nodes.map(|node| node.country_code), ["US", "US", "DE"]);
    }

    #[tokio::test]
    async fn cancelling_a_country_lookup_suppresses_completion() {
        let mut resolver = CountryResolver::default();
        let nodes = [Node {
            id: "node".into(),
            server: "example.test".into(),
            ..Node::default()
        }];
        let (started, ready) = oneshot::channel();
        let (emit, results) = oneshot::channel();
        assert!(resolver.start_with(
            &nodes,
            |_| async move {
                started.send(()).unwrap();
                std::future::pending().await
            },
            |results| {
                emit.send(results).unwrap();
            },
        ));
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        resolver.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), results)
                .await
                .unwrap()
                .is_err()
        );
    }
}
