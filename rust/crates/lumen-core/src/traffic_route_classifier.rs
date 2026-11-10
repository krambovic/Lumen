//! Classify Clash and runtime routes using outbound types and detours.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClass {
    Direct,
    Proxy,
    Unknown,
}

enum Kind {
    Direct,
    Proxy,
    Group,
    Unknown,
}

struct Outbound {
    kind: Kind,
    choices: Vec<String>,
    selected: String,
    detour: String,
}

impl Outbound {
    fn from_record(record: &Value, use_default: bool) -> Self {
        let kind = match first_text([&record["type"], &record["protocol"]])
            .to_lowercase()
            .replace(['-', '_', ' '], "")
            .as_str()
        {
            "direct" | "freedom" => Kind::Direct,
            "socks" | "socks5" | "http" | "https" | "shadowsocks" | "shadowsocksr" | "ss"
            | "ssr" | "vmess" | "vless" | "trojan" | "hysteria" | "hysteria2" | "hy2" | "tuic"
            | "wireguard" | "amneziawg" | "awg" | "ssh" | "tor" | "naive" | "naiveproxy"
            | "anytls" | "shadowtls" | "masque" | "openvpn" | "openvpnclient" => Kind::Proxy,
            "selector" | "select" | "urltest" | "fallback" | "loadbalance" | "relay" => Kind::Group,
            _ => Kind::Unknown,
        };
        let members = if use_default || !truthy(&record["all"]) {
            &record["outbounds"]
        } else {
            &record["all"]
        };
        let mut seen = HashSet::new();
        let choices = members
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|tag| !tag.is_empty() && seen.insert(*tag))
            .map(str::to_owned)
            .collect();
        let selected = first_text([&record[if use_default { "default" } else { "now" }]]);
        let detour = first_text([
            &record["detour"],
            &record["proxySettings"]["tag"],
            &record["streamSettings"]["sockopt"]["dialerProxy"],
        ]);
        Self {
            kind,
            choices,
            selected,
            detour,
        }
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn first_text<'a>(values: impl IntoIterator<Item = &'a Value>) -> String {
    match values.into_iter().find(|value| truthy(value)) {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(true)) => "True".into(),
        Some(value) => value.to_string(),
    }
}

/// An immutable routing graph containing only route metadata.
#[derive(Default)]
pub struct RouteClassifier {
    nodes: HashMap<String, Outbound>,
}

impl RouteClassifier {
    pub fn new(document: &Value) -> Self {
        if document["outbounds"].is_array() {
            return Self::from_runtime(document, None);
        }
        let source = document.get("proxies").unwrap_or(document);
        let nodes = source
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, record)| record.is_object())
            .map(|(tag, record)| (tag.clone(), Outbound::from_record(record, false)))
            .collect();
        Self { nodes }
    }

    /// For a new health probe, use the target selector's configured default.
    /// Existing connections use `new` and the recorded chain or Clash's `now`.
    pub fn for_health_probe(config: &Value, target: &str) -> Self {
        Self::from_runtime(config, Some(target))
    }

    fn from_runtime(document: &Value, probe_target: Option<&str>) -> Self {
        let nodes = ["outbounds", "endpoints"]
            .into_iter()
            .flat_map(|key| document[key].as_array().into_iter().flatten())
            .filter_map(|record| {
                let tag = record["tag"].as_str().filter(|tag| !tag.is_empty())?;
                let use_default = probe_target == Some(tag) && record["type"] == "selector";
                Some((tag.to_owned(), Outbound::from_record(record, use_default)))
            })
            .collect();
        Self { nodes }
    }

    pub fn classify(&self, chains: &[&str]) -> RouteClass {
        if chains
            .iter()
            .any(|tag| tag.is_empty() || !self.nodes.contains_key(*tag))
        {
            return RouteClass::Unknown;
        }
        let chain = chains.iter().copied().collect();
        consensus(
            chains
                .iter()
                .map(|tag| self.resolve(tag, &chain, &mut Vec::new())),
        )
    }

    fn resolve<'a>(
        &'a self,
        tag: &'a str,
        chain: &HashSet<&str>,
        visiting: &mut Vec<&'a str>,
    ) -> RouteClass {
        let Some(node) = self.nodes.get(tag) else {
            return RouteClass::Unknown;
        };
        if visiting.len() >= 32 || visiting.contains(&tag) {
            return RouteClass::Unknown;
        }
        visiting.push(tag);
        let class = match node.kind {
            Kind::Direct if node.detour.is_empty() => RouteClass::Direct,
            Kind::Direct => self.resolve(&node.detour, chain, visiting),
            Kind::Proxy => {
                if !node.detour.is_empty()
                    && self.resolve(&node.detour, chain, visiting) == RouteClass::Unknown
                {
                    RouteClass::Unknown
                } else {
                    RouteClass::Proxy
                }
            }
            Kind::Group => {
                let members = node
                    .choices
                    .iter()
                    .filter(|member| chain.contains(member.as_str()));
                if members.clone().next().is_some() {
                    consensus(members.map(|member| self.resolve(member, chain, visiting)))
                } else if node.selected.is_empty() {
                    consensus(
                        node.choices
                            .iter()
                            .map(|member| self.resolve(member, chain, visiting)),
                    )
                } else if node.choices.is_empty() || node.choices.contains(&node.selected) {
                    self.resolve(&node.selected, chain, visiting)
                } else {
                    RouteClass::Unknown
                }
            }
            Kind::Unknown => RouteClass::Unknown,
        };
        visiting.pop();
        class
    }
}

fn consensus(mut classes: impl Iterator<Item = RouteClass>) -> RouteClass {
    let first = classes.next().unwrap_or(RouteClass::Unknown);
    if first != RouteClass::Unknown && classes.all(|class| class == first) {
        first
    } else {
        RouteClass::Unknown
    }
}

/// Classify a JSON chain. Missing and malformed tags yield Unknown.
pub fn classify_route(chains: &Value, document: &Value) -> RouteClass {
    let tags = chains
        .as_array()
        .and_then(|items| items.iter().map(Value::as_str).collect::<Option<Vec<_>>>());
    tags.map_or(RouteClass::Unknown, |tags| {
        RouteClassifier::new(document).classify(&tags)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recorded_chain_wins_over_selector_switches_in_either_order() {
        let graph = RouteClassifier::new(&json!({"proxies": {
            "GLOBAL":{"type":"Selector","all":["AUTO","plain"],"now":"AUTO"},
            "AUTO":{"type":"URLTest","all":["Japan"],"now":"Japan"},
            "Japan":{"type":"Hysteria2"},
            "plain":{"type":"Direct"}
        }}));
        assert_eq!(
            graph.classify(&["Japan", "AUTO", "GLOBAL"]),
            RouteClass::Proxy
        );
        assert_eq!(
            graph.classify(&["GLOBAL", "AUTO", "Japan"]),
            RouteClass::Proxy
        );
        assert_eq!(graph.classify(&["GLOBAL"]), RouteClass::Proxy);
        assert_eq!(graph.classify(&["plain", "GLOBAL"]), RouteClass::Direct);
        assert_eq!(graph.classify(&["Japan", "plain"]), RouteClass::Unknown);
    }

    #[test]
    fn handles_detours_cycles_and_unknown_types_conservatively() {
        let config = json!({"outbounds":[
            {"tag":"proxy","type":"direct","streamSettings":{"sockopt":{"dialerProxy":"exit"}}},
            {"tag":"direct","type":"unrecognized"},
            {"tag":"cyclic","type":"selector","outbounds":["cyclic"]},
            {"tag":"ambiguous","type":"selector","outbounds":["exit","plain"],"default":"exit"},
            {"tag":"plain","protocol":"freedom"}
        ],"endpoints":[{"tag":"exit","type":"Wire-Guard"}]});
        let graph = RouteClassifier::new(&config);
        assert_eq!(graph.classify(&["proxy"]), RouteClass::Proxy);
        assert_eq!(graph.classify(&["direct"]), RouteClass::Unknown);
        assert_eq!(graph.classify(&["cyclic"]), RouteClass::Unknown);
        assert_eq!(graph.classify(&["ambiguous"]), RouteClass::Unknown);
        assert_eq!(classify_route(&json!([null]), &config), RouteClass::Unknown);
    }
}
