//! Proxy chain (v2rayN-style "previous proxy" / "landing proxy").
//!
//! Traffic path when the chain is enabled:
//!
//! ```text
//! app -> front proxy -> selected (main) server -> exit proxy -> target site
//! ```
//!
//! Both hops are optional: only a front hop, only an exit hop or both can be
//! set. The chain is expressed purely through outbound dialing:
//!
//! * Xray: `streamSettings.sockopt.dialerProxy`
//! * sing-box: dial field `detour`
//!
//! (Python module docstring: intentionally free of Qt so it can be
//! unit-tested in isolation.)

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::{bool_field, str_field, Node};

pub const CHAIN_FRONT_TAG: &str = "chain-front";
pub const CHAIN_MAIN_TAG: &str = "chain-main";

const FULL_CONFIG_PROTOCOLS: [&str; 2] = ["xray_config", "singbox_config"];

/// Raised when the configured chain cannot be built for the selected server.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ProxyChainError(pub String);

#[derive(Debug, Clone, Default)]
pub struct ProxyChain {
    pub front: Option<Node>,
    pub exit: Option<Node>,
    /// Human-readable warnings about skipped hops (v2rayN parity: a missing or
    /// unsupported hop is skipped with a warning instead of failing the start).
    pub notes: Vec<String>,
}

impl ProxyChain {
    pub fn active(&self) -> bool {
        self.front.is_some() || self.exit.is_some()
    }

    pub fn hops(&self) -> Vec<&Node> {
        let mut hops = Vec::new();
        if let Some(front) = &self.front {
            hops.push(front);
        }
        if let Some(exit) = &self.exit {
            hops.push(exit);
        }
        hops
    }

    pub fn describe(&self, main: Option<&Node>) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(front) = &self.front {
            parts.push(node_label(front));
        }
        parts.push(match main {
            Some(main) => node_label(main),
            None => "сервер".to_string(),
        });
        if let Some(exit) = &self.exit {
            parts.push(node_label(exit));
        }
        parts.join(" → ")
    }
}

fn node_label(node: &Node) -> String {
    let name = node.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let server = node.server.trim();
    if !server.is_empty() {
        server.to_string()
    } else {
        let id = node.id.trim();
        if id.is_empty() {
            "?".to_string()
        } else {
            id.to_string()
        }
    }
}

fn value_as_str(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub fn node_protocol(node: &Node) -> String {
    if !node.outbound.is_object() {
        return String::new();
    }
    str_field(&node.outbound, "protocol").trim().to_lowercase()
}

pub fn is_full_config_node(node: &Node) -> bool {
    FULL_CONFIG_PROTOCOLS.contains(&node_protocol(node).as_str())
}

/// Protocols that only the native sing-box core can dial.  Kept in sync with
/// `application.node_runtime_service.is_native_singbox_only_node` but local so
/// this module stays importable without Qt.
const SINGBOX_ONLY_PROTOCOLS: [&str; 13] = [
    "warp",
    "wireguard",
    "awg",
    "amneziawg",
    "hysteria",
    "hysteria2",
    "tuic",
    "mieru",
    "masque",
    "openvpn",
    "naive",
    "anytls",
    "singbox_config",
];
const XRAY_STANDARD_PROTOCOLS: [&str; 6] = ["vless", "vmess", "trojan", "shadowsocks", "socks", "http"];

pub fn is_xray_dialable_node(node: &Node) -> bool {
    let Some(outbound) = node.outbound.as_object() else {
        return false;
    };
    if outbound.is_empty() {
        return false;
    }
    let protocol = node_protocol(node);
    if protocol.is_empty() || SINGBOX_ONLY_PROTOCOLS.contains(&protocol.as_str()) || protocol == "xray_config" {
        return false;
    }
    if node.outbound.get("singbox").map(|v| v.is_object()).unwrap_or(false) {
        return XRAY_STANDARD_PROTOCOLS.contains(&protocol.as_str());
    }
    true
}

/// Python takes the AppSettings object; here it is the `to_dict()` value.
pub fn chain_node_ids(settings: &Value) -> (String, String) {
    if !bool_field(settings, "proxy_chain_enabled", false) {
        return (String::new(), String::new());
    }
    let front_id = str_field(settings, "proxy_chain_front_node_id").trim().to_string();
    let exit_id = str_field(settings, "proxy_chain_exit_node_id").trim().to_string();
    (front_id, exit_id)
}

/// Return the hops configured in settings for the given main server.
///
/// `None` means the chain is disabled (or no hop is configured).  Hops that
/// cannot be used are skipped and explained in [`ProxyChain::notes`].
pub fn resolve_proxy_chain(settings: &Value, nodes: &[Node], main: Option<&Node>) -> Option<ProxyChain> {
    let (front_id, exit_id) = chain_node_ids(settings);
    if front_id.is_empty() && exit_id.is_empty() {
        return None;
    }
    let mut chain = ProxyChain::default();
    if let Some(main) = main {
        if is_full_config_node(main) {
            chain.notes.push(format!(
                "Цепочка прокси пропущена: сервер «{}» — полный конфиг, его маршруты не меняются.",
                node_label(main)
            ));
            return Some(chain);
        }
    }
    let by_id: HashMap<&str, &Node> = nodes.iter().map(|node| (node.id.trim(), node)).collect();
    let main_id = main.map(|node| node.id.trim()).unwrap_or("");

    let pick = |chain: &mut ProxyChain, node_id: &str, role: &str| -> Option<Node> {
        if node_id.is_empty() {
            return None;
        }
        let Some(node) = by_id.get(node_id) else {
            chain
                .notes
                .push(format!("{role} прокси не найден (сервер удалён) — звено пропущено."));
            return None;
        };
        if node_id == main_id {
            chain
                .notes
                .push(format!("{role} прокси совпадает с выбранным сервером — звено пропущено."));
            return None;
        }
        if is_full_config_node(node) {
            chain.notes.push(format!(
                "{role} прокси «{}» — полный конфиг и не может быть звеном цепочки — пропущено.",
                node_label(node)
            ));
            return None;
        }
        Some((*node).clone())
    };

    chain.front = pick(&mut chain, &front_id, "Предыдущий");
    chain.exit = pick(&mut chain, &exit_id, "Выходной");
    Some(chain)
}

/// Stable value for runtime signatures: changes when a hop or its link changes.
pub fn proxy_chain_signature(settings: &Value, nodes: &[Node]) -> Vec<(String, String)> {
    let (front_id, exit_id) = chain_node_ids(settings);
    if front_id.is_empty() && exit_id.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<&str, &Node> = nodes.iter().map(|node| (node.id.trim(), node)).collect();

    let hop = |node_id: &str| -> (String, String) {
        if node_id.is_empty() {
            return (node_id.to_string(), String::new());
        }
        let Some(node) = by_id.get(node_id) else {
            return (node_id.to_string(), String::new());
        };
        // Python falls back to repr(outbound); a stable JSON serialization
        // plays the same role here.
        let marker = if node.link.trim().is_empty() {
            serde_json::to_string(&node.outbound).unwrap_or_default()
        } else {
            node.link.clone()
        };
        (node_id.to_string(), marker)
    };

    vec![hop(&front_id), hop(&exit_id)]
}

fn xray_sockopt<'a>(outbound: &'a mut Value) -> &'a mut Value {
    if !outbound.get("streamSettings").map(|v| v.is_object()).unwrap_or(false) {
        outbound["streamSettings"] = json!({});
    }
    let stream = outbound.get_mut("streamSettings").unwrap();
    if !stream.get("sockopt").map(|v| v.is_object()).unwrap_or(false) {
        stream["sockopt"] = json!({});
    }
    stream.get_mut("sockopt").unwrap()
}

fn xray_hop_outbound(chain: &mut ProxyChain, node: &Node, role: &str) -> Option<Value> {
    if !is_xray_dialable_node(node) {
        let protocol = node_protocol(node);
        let shown = if protocol.is_empty() { "?" } else { protocol.as_str() };
        chain.notes.push(format!(
            "{role} прокси «{}» ({shown}) не поддерживается ядром Xray — звено пропущено.",
            node_label(node)
        ));
        return None;
    }
    let mut outbound = node.outbound.clone();
    if let Some(obj) = outbound.as_object_mut() {
        obj.remove("singbox");
    }
    Some(outbound)
}

/// Wire the chain into an Xray outbound list in place.
///
/// The main server outbound must already be tagged `proxy_tag`.  After the
/// call routing rules that point to `proxy_tag` reach the exit hop (or the
/// main server when there is no exit hop).  Returns the applied roles.
pub fn apply_xray_proxy_chain(
    outbounds: &mut Vec<Value>,
    chain: Option<&mut ProxyChain>,
    proxy_tag: &str,
) -> Vec<String> {
    let Some(chain) = chain else {
        return Vec::new();
    };
    if !chain.active() {
        return Vec::new();
    }
    let Some(index) = outbounds
        .iter()
        .position(|item| item.get("tag").and_then(Value::as_str) == Some(proxy_tag))
    else {
        chain
            .notes
            .push("В конфиге нет outbound `proxy` — цепочка прокси не применена.".to_string());
        return Vec::new();
    };

    let front_node = chain.front.clone();
    let exit_node = chain.exit.clone();
    let front = front_node.as_ref().and_then(|front| xray_hop_outbound(chain, front, "Предыдущий"));
    let exit = exit_node.as_ref().and_then(|exit| xray_hop_outbound(chain, exit, "Выходной"));
    let mut applied: Vec<String> = Vec::new();

    if let Some(mut front) = front {
        let inherited = outbounds[index]
            .get("streamSettings")
            .and_then(|stream| stream.get("sockopt"))
            .and_then(|sockopt| sockopt.get("dialerProxy"))
            .map(|v| value_as_str(Some(v)))
            .unwrap_or_default()
            .trim()
            .to_string();
        if let Some(obj) = front.as_object_mut() {
            obj.insert("tag".into(), json!(CHAIN_FRONT_TAG));
        }
        if inherited.is_empty() {
            if let Some(sockopt) = xray_sockopt(&mut front).as_object_mut() {
                sockopt.remove("dialerProxy");
            }
        } else {
            xray_sockopt(&mut front)["dialerProxy"] = json!(inherited);
        }
        xray_sockopt(&mut outbounds[index])["dialerProxy"] = json!(CHAIN_FRONT_TAG);
        outbounds.insert(index + 1, front);
        applied.push("front".to_string());
    }

    if let Some(mut exit) = exit {
        outbounds[index]["tag"] = json!(CHAIN_MAIN_TAG);
        if let Some(obj) = exit.as_object_mut() {
            obj.insert("tag".into(), json!(proxy_tag));
        }
        xray_sockopt(&mut exit)["dialerProxy"] = json!(CHAIN_MAIN_TAG);
        outbounds.insert(index, exit);
        applied.push("exit".to_string());
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, protocol: &str) -> Node {
        Node {
            id: id.into(),
            name: id.into(),
            link: format!("link://{id}"),
            outbound: json!({"protocol": protocol, "settings": {}}),
            ..Default::default()
        }
    }

    #[test]
    fn chain_resolution_skips_bad_hops() {
        let settings = json!({
            "proxy_chain_enabled": true,
            "proxy_chain_front_node_id": "front",
            "proxy_chain_exit_node_id": "missing",
        });
        let main = node("main", "vless");
        let nodes = vec![node("front", "vless"), main.clone()];
        let chain = resolve_proxy_chain(&settings, &nodes, Some(&main)).unwrap();
        assert!(chain.front.is_some());
        assert!(chain.exit.is_none());
        assert_eq!(chain.notes.len(), 1);
        assert!(chain.notes[0].contains("Выходной прокси не найден"));
    }

    #[test]
    fn full_config_main_skips_chain() {
        let settings = json!({
            "proxy_chain_enabled": true,
            "proxy_chain_front_node_id": "front",
        });
        let main = node("main", "xray_config");
        let nodes = vec![node("front", "vless")];
        let chain = resolve_proxy_chain(&settings, &nodes, Some(&main)).unwrap();
        assert!(chain.front.is_none());
        assert!(chain.notes[0].contains("полный конфиг"));
    }

    #[test]
    fn signature_changes_with_hop_link() {
        let settings = json!({
            "proxy_chain_enabled": true,
            "proxy_chain_front_node_id": "front",
            "proxy_chain_exit_node_id": "",
        });
        let nodes = vec![node("front", "vless")];
        let sig = proxy_chain_signature(&settings, &nodes);
        // Python always returns the two-hop tuple; a missing hop is ("", "").
        assert_eq!(sig.len(), 2);
        assert_eq!(sig[0], ("front".to_string(), "link://front".to_string()));
        assert_eq!(sig[1], (String::new(), String::new()));
    }

    #[test]
    fn apply_wires_front_and_exit() {
        let mut outbounds = vec![json!({"tag": "proxy", "protocol": "vless", "settings": {}})];
        let mut chain = ProxyChain {
            front: Some(node("front", "vless")),
            exit: Some(node("exit", "trojan")),
            notes: Vec::new(),
        };
        let applied = apply_xray_proxy_chain(&mut outbounds, Some(&mut chain), "proxy");
        assert_eq!(applied, vec!["front", "exit"]);
        assert_eq!(outbounds[0]["tag"], json!("proxy"));
        assert_eq!(outbounds[0]["protocol"], json!("trojan"));
        assert_eq!(outbounds[0]["streamSettings"]["sockopt"]["dialerProxy"], json!("chain-main"));
        assert_eq!(outbounds[1]["tag"], json!("chain-main"));
        assert_eq!(outbounds[2]["tag"], json!("chain-front"));
        assert_eq!(outbounds[1]["streamSettings"]["sockopt"]["dialerProxy"], json!("chain-front"));
    }

    #[test]
    fn apply_without_proxy_tag_notes() {
        let mut outbounds = vec![json!({"tag": "other", "protocol": "vless"})];
        let mut chain = ProxyChain {
            front: Some(node("front", "vless")),
            exit: None,
            notes: Vec::new(),
        };
        let applied = apply_xray_proxy_chain(&mut outbounds, Some(&mut chain), "proxy");
        assert!(applied.is_empty());
        assert!(chain.notes[0].contains("нет outbound `proxy`"));
    }

    #[test]
    fn dialable_checks() {
        assert!(!is_xray_dialable_node(&node("w", "warp")));
        assert!(!is_xray_dialable_node(&node("x", "xray_config")));
        assert!(is_xray_dialable_node(&node("v", "vless")));
        let mut hybrid = node("h", "hysteria2");
        hybrid.outbound["singbox"] = json!({});
        assert!(!is_xray_dialable_node(&hybrid));
    }
}
