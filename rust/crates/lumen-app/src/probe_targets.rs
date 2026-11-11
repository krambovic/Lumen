//! Select profile or endpoint targets for measurement workers.

use std::collections::{HashMap, HashSet};

use lumen_core::link_parser::{py_int_or, py_str_or, py_truthy};
use lumen_core::models::Node;
use lumen_core::probe_capabilities::{
    GROUP_TYPES, NATIVE_PROTOCOLS, NON_PROXY_TYPES, XRAY_PROTOCOLS, node_protocol, probe_capability,
};
use serde_json::{Value, json};

pub(crate) fn prepare_target(node: &Node, method: &str) -> Option<Node> {
    let protocol = node_protocol(node);
    if !matches!(protocol.as_str(), "xray_config" | "singbox_config") {
        return probe_capability(node, method)
            .supported
            .then(|| node.clone());
    }
    let xray = protocol == "xray_config";
    let candidates = candidate_outbounds(&node.outbound[&protocol], xray);
    let supports = |candidate: &Value| {
        let protocol = outbound_protocol(candidate);
        XRAY_PROTOCOLS.contains(&protocol.as_str())
            || (!xray
                && (NATIVE_PROTOCOLS.contains(&protocol.as_str()) || protocol == "openvpn-client"))
    };
    if matches!(method, "http" | "real" | "speed") {
        return (!candidates.is_empty() && candidates.iter().all(|candidate| supports(candidate)))
            .then(|| node.clone());
    }
    for candidate in candidates
        .into_iter()
        .filter(|candidate| supports(candidate))
    {
        let (server, port) = outbound_endpoint(candidate);
        if server.is_empty() || port <= 0 {
            continue;
        }
        let mut target = node.clone();
        target.server = server;
        target.port = port;
        target.scheme = match outbound_protocol(candidate).as_str() {
            "openvpn-client" => "openvpn".into(),
            protocol => protocol.into(),
        };
        target.outbound = if xray {
            candidate.clone()
        } else {
            json!({"protocol": target.scheme, "singbox": candidate})
        };
        return Some(target);
    }
    None
}

fn items(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn field(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(key).filter(|value| py_truthy(value)))
        .map_or_else(String::new, |value| py_str_or(Some(value), ""))
        .trim()
        .to_owned()
}

fn port(value: &Value, keys: &[&str], fallback: i64) -> i64 {
    let value = keys
        .iter()
        .find_map(|key| value.get(key).filter(|value| py_truthy(value)));
    value.map_or(fallback, |value| {
        py_int_or(Some(value), 0).unwrap_or_default()
    })
}

fn outbound_protocol(value: &Value) -> String {
    field(value, &["protocol", "type"]).to_lowercase()
}

fn outbound_endpoint(outbound: &Value) -> (String, i64) {
    let mut candidate = outbound
        .get("singbox")
        .filter(|value| value.is_object())
        .unwrap_or(outbound);
    for key in ["peers", "servers"] {
        if let Some(peer) = items(&candidate[key])
            .first()
            .filter(|value| value.is_object())
        {
            candidate = peer;
        }
    }
    let settings = &candidate["settings"];
    let mut server = field(candidate, &["server", "address"]);
    let mut number = port(candidate, &["server_port", "port"], 0);
    for key in ["vnext", "servers"] {
        if let Some(peer) = items(&settings[key])
            .first()
            .filter(|value| value.is_object())
        {
            let address = field(peer, &["address", "server"]);
            if !address.is_empty() {
                server = address;
            }
            number = port(peer, &["port", "server_port"], number);
            break;
        }
    }
    if server.is_empty() {
        server = field(settings, &["address", "server"]);
    }
    if number <= 0 {
        number = port(settings, &["port", "server_port"], 0);
    }
    (server, number)
}

fn candidate_outbounds(config: &Value, xray: bool) -> Vec<&Value> {
    let outbounds: Vec<_> = items(&config["outbounds"])
        .iter()
        .chain(items(&config["endpoints"]))
        .filter(|item| item.is_object())
        .collect();
    if !xray {
        return singbox_candidates(config, &outbounds);
    }
    let candidates: Vec<_> = outbounds
        .into_iter()
        .filter(|item| {
            let kind = outbound_protocol(item);
            !NON_PROXY_TYPES.contains(&kind.as_str()) && !GROUP_TYPES.contains(&kind.as_str())
        })
        .collect();
    let prefixes: Vec<_> = items(&config["routing"]["balancers"])
        .iter()
        .flat_map(|balancer| items(&balancer["selector"]))
        .chain(items(&config["observatory"]["subjectSelector"]))
        .chain(items(&config["burstObservatory"]["subjectSelector"]))
        .map(|item| py_str_or(Some(item), "").trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect();
    let selected: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|item| {
            let tag = field(item, &["tag"]);
            prefixes.iter().any(|prefix| tag.starts_with(prefix))
        })
        .collect();
    if selected.is_empty() {
        candidates
    } else {
        selected
    }
}

fn singbox_candidates<'a>(config: &Value, outbounds: &[&'a Value]) -> Vec<&'a Value> {
    let Some(first) = outbounds.first() else {
        return Vec::new();
    };
    let tags: HashMap<_, _> = outbounds
        .iter()
        .copied()
        .map(|item| (field(item, &["tag"]), item))
        .filter(|(tag, _)| !tag.is_empty())
        .collect();
    let mut root = field(&config["route"], &["final"]);
    if root.is_empty() {
        let default = outbounds
            .iter()
            .find(|item| GROUP_TYPES.contains(&outbound_protocol(item).as_str()))
            .unwrap_or(first);
        root = field(default, &["tag"]);
    }
    let mut pending = vec![root];
    let mut visited = HashSet::new();
    let mut selected = Vec::new();
    while let Some(tag) = pending.pop() {
        let Some(item) = tags
            .get(&tag)
            .copied()
            .filter(|_| visited.insert(tag.clone()))
        else {
            continue;
        };
        let kind = outbound_protocol(item);
        if GROUP_TYPES.contains(&kind.as_str()) {
            pending.extend(
                items(&item["outbounds"])
                    .iter()
                    .rev()
                    .map(|item| py_str_or(Some(item), "").trim().to_owned()),
            );
        } else if !NON_PROXY_TYPES.contains(&kind.as_str()) {
            selected.push(item);
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use lumen_core::models::Node;
    use serde_json::json;

    use super::*;

    #[test]
    fn xray_pool_endpoint_uses_the_selected_supported_member() {
        let node = Node {
            id: "auto".into(),
            scheme: "auto".into(),
            outbound: json!({"protocol":"xray_config","xray_config":{
                "outbounds":[
                    {"tag":"proxy-awg","protocol":"awg","settings":{"address":"skip.test","port":443}},
                    {"tag":"other","protocol":"vless","settings":{"vnext":[{"address":"other.test","port":443}]}},
                    {"tag":"proxy-vless","protocol":"vless","settings":{"vnext":[{"address":"node.test","port":8443}]}}
                ],
                "routing":{"balancers":[{"selector":["proxy"]}]}
            }}),
            ..Default::default()
        };
        let target = prepare_target(&node, "tcping").unwrap();
        assert_eq!(
            (target.id.as_str(), target.server.as_str(), target.port),
            ("auto", "node.test", 8443)
        );
        assert!(prepare_target(&node, "speed").is_none());
    }

    #[test]
    fn singbox_pool_follows_nested_groups_and_ignores_cycles_and_direct() {
        let node = Node {
            id: "native-auto".into(),
            scheme: "auto".into(),
            outbound: json!({"protocol":"singbox_config","singbox_config":{
                "outbounds":[
                    {"type":"selector","tag":"root","outbounds":["nested","direct"]},
                    {"type":"urltest","tag":"nested","outbounds":["root","vpn"]},
                    {"type":"direct","tag":"direct"},
                    {"type":"unsupported","tag":"unused"}
                ],
                "endpoints":[
                    {"type":"openvpn-client","tag":"vpn","servers":[{"server":"ovpn.test","server_port":1194}]}
                ],
                "route":{"final":"root"}
            }}),
            ..Default::default()
        };
        let endpoint = prepare_target(&node, "tcping").unwrap();
        assert_eq!(
            (
                endpoint.scheme.as_str(),
                endpoint.server.as_str(),
                endpoint.port
            ),
            ("openvpn", "ovpn.test", 1194)
        );
        assert_eq!(
            prepare_target(&node, "http").unwrap().outbound,
            node.outbound
        );
    }
}
