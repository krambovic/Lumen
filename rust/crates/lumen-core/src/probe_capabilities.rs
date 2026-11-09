//! Single probe contract: endpoint reachability is never profile health.
//!
//! Port of `windows/xray_fluent/probe_capabilities.py`.

use serde_json::Value;

use crate::models::Node;
use crate::subscription::text::py_or_str;

pub const XRAY_PROTOCOLS: [&str; 7] = ["vless", "vmess", "trojan", "shadowsocks", "ss", "socks", "http"];
pub const NATIVE_PROTOCOLS: [&str; 14] = [
    "awg",
    "wireguard",
    "warp",
    "hysteria",
    "hysteria2",
    "hy",
    "hy2",
    "tuic",
    "masque",
    "openvpn",
    "mieru",
    "naive",
    "anytls",
    "snell",
];
pub const UDP_PROTOCOLS: [&str; 9] = [
    "awg",
    "wireguard",
    "warp",
    "hysteria",
    "hysteria2",
    "hy",
    "hy2",
    "tuic",
    "masque",
];
pub const GROUP_TYPES: [&str; 3] = ["selector", "url-test", "urltest"];
pub const NON_PROXY_TYPES: [&str; 5] = ["direct", "freedom", "block", "blackhole", "dns"];

/// `ProbeCapability` dataclass from the Python module.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeCapability {
    pub supported: bool,
    pub method: String,
    pub backend: String,
    pub reason: String,
}

/// `node_protocol(node)` from the Python module.
pub fn node_protocol(node: &Node) -> String {
    let outbound = if node.outbound.is_object() {
        &node.outbound
    } else {
        &NULL_VALUE
    };
    // Python: str(outbound.get("protocol") or outbound.get("type") or node.scheme or "")
    py_or_str(&[
        outbound.get("protocol"),
        outbound.get("type"),
        Some(&Value::String(node.scheme.clone())),
    ])
    .trim()
    .to_lowercase()
}

/// `probe_backend(node)` from the Python module.
pub fn probe_backend(node: &Node) -> String {
    let protocol = node_protocol(node);
    let outbound = if node.outbound.is_object() {
        &node.outbound
    } else {
        &NULL_VALUE
    };
    if protocol == "xray_config" {
        return "xray".into();
    }
    if protocol == "singbox_config"
        || NATIVE_PROTOCOLS.contains(&protocol.as_str())
        || (XRAY_PROTOCOLS.contains(&protocol.as_str())
            && outbound
                .get("singbox")
                .map(|value| value.is_object())
                .unwrap_or(false))
    {
        return "singbox".into();
    }
    if XRAY_PROTOCOLS.contains(&protocol.as_str()) {
        "xray".into()
    } else {
        String::new()
    }
}

/// `endpoint_method(node, requested)` from the Python module.
pub fn endpoint_method(node: &Node, requested: &str) -> String {
    if requested == "icmp" {
        return "icmp".into();
    }
    let protocol = node_protocol(node);
    let outbound = if node.outbound.is_object() {
        &node.outbound
    } else {
        &NULL_VALUE
    };
    // Python: native = outbound.get("singbox", outbound)
    let native = match outbound.get("singbox") {
        Some(value) if value.is_object() => value,
        Some(_) => &NULL_VALUE,
        None => outbound,
    };
    // Python: str(native.get("transport") or native.get("network")
    //              or native.get("proto") or native.get("protocol") or "").lower()
    let transport = py_or_str(&[
        native.get("transport"),
        native.get("network"),
        native.get("proto"),
        native.get("protocol"),
    ])
    .to_lowercase();
    // Unsolicited UDP datagrams cannot verify an authenticated handshake.
    if UDP_PROTOCOLS.contains(&protocol.as_str())
        || (protocol == "openvpn" && !transport.starts_with("tcp"))
        || (protocol == "mieru" && transport == "udp")
    {
        return "icmp".into();
    }
    "tcping".into()
}

/// `probe_capability(node, method)` from the Python module.
pub fn probe_capability(node: &Node, method: &str) -> ProbeCapability {
    let backend = probe_backend(node);
    if matches!(method, "real" | "http" | "speed") {
        let supported = !backend.is_empty();
        return ProbeCapability {
            supported,
            method: "proxy".into(),
            backend,
            reason: if supported {
                String::new()
            } else {
                "Нет совместимого адаптера профиля".into()
            },
        };
    }
    let actual = endpoint_method(node, method);
    // Python: 0 < int(node.port or 0) < 65536
    let port_valid = node.port > 0 && node.port < 65536;
    let supported = !node.server.trim().is_empty() && (actual == "icmp" || port_valid);
    ProbeCapability {
        supported,
        method: format!("{actual}_endpoint"),
        backend: String::new(),
        reason: if supported {
            String::new()
        } else {
            "Нет адреса endpoint".into()
        },
    }
}

static NULL_VALUE: Value = Value::Null;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::text::py_truthy;
    use serde_json::json;

    fn make_node(scheme: &str, outbound: Value) -> Node {
        let mut node = Node::new();
        node.scheme = scheme.into();
        node.outbound = outbound;
        node
    }

    #[test]
    fn backend_selection_matches_python() {
        let xray_node = make_node("vless", json!({"protocol": "vless"}));
        assert_eq!(node_protocol(&xray_node), "vless");
        assert_eq!(probe_backend(&xray_node), "xray");

        let native_node = make_node("hysteria2", json!({"protocol": "hysteria2"}));
        assert_eq!(probe_backend(&native_node), "singbox");

        let hybrid = make_node(
            "vless",
            json!({"protocol": "vless", "singbox": {"type": "vless"}}),
        );
        assert_eq!(probe_backend(&hybrid), "singbox");

        let config = make_node("xray_config", json!({"protocol": "xray_config"}));
        assert_eq!(probe_backend(&config), "xray");
        let singbox_config = make_node("singbox_config", json!({"protocol": "singbox_config"}));
        assert_eq!(probe_backend(&singbox_config), "singbox");

        assert_eq!(probe_backend(&make_node("weird", json!({}))), "");
    }

    #[test]
    fn endpoint_method_uses_icmp_for_udp_protocols() {
        let node = make_node("hysteria2", json!({"protocol": "hysteria2"}));
        assert_eq!(endpoint_method(&node, "tcp"), "icmp");
        assert_eq!(endpoint_method(&node, "icmp"), "icmp");
        let tcp_node = make_node("vless", json!({"protocol": "vless"}));
        assert_eq!(endpoint_method(&tcp_node, "tcp"), "tcping");
        let openvpn_tcp = make_node(
            "openvpn",
            json!({"singbox": {"type": "openvpn", "proto": "tcp"}}),
        );
        assert_eq!(endpoint_method(&openvpn_tcp, "tcp"), "tcping");
        let openvpn_udp = make_node(
            "openvpn",
            json!({"singbox": {"type": "openvpn", "proto": "udp"}}),
        );
        assert_eq!(endpoint_method(&openvpn_udp, "tcp"), "icmp");
    }

    #[test]
    fn probe_capability_contract() {
        let mut node = make_node("hysteria2", json!({"protocol": "hysteria2"}));
        node.server = "vpn.example.com".into();
        node.port = 443;

        let cap = probe_capability(&node, "tcp");
        assert!(cap.supported);
        assert_eq!(cap.method, "icmp_endpoint");
        assert_eq!(cap.backend, "");

        let real = probe_capability(&node, "real");
        assert!(real.supported);
        assert_eq!(real.method, "proxy");
        assert_eq!(real.backend, "singbox");
        assert_eq!(real.reason, "");

        let mut no_server = make_node("vless", json!({"protocol": "vless"}));
        no_server.port = 443;
        let cap = probe_capability(&no_server, "tcp");
        assert!(!cap.supported);
        assert_eq!(cap.reason, "Нет адреса endpoint");
        assert_eq!(cap.method, "tcping_endpoint");

        let mut no_port = make_node("vless", json!({"protocol": "vless"}));
        no_port.server = "1.2.3.4".into();
        no_port.port = 0;
        let cap = probe_capability(&no_port, "tcp");
        assert!(!cap.supported);
        assert_eq!(cap.reason, "Нет адреса endpoint");

        let mut no_backend = make_node("weird", json!({"protocol": "weird"}));
        no_backend.server = "host.example.com".into();
        no_backend.port = 443;
        let cap = probe_capability(&no_backend, "real");
        assert!(!cap.supported);
        assert_eq!(cap.reason, "Нет совместимого адаптера профиля");
    }

    #[test]
    fn protocol_falls_back_through_type_and_scheme() {
        let node = make_node("hysteria", json!({"type": "hysteria2"}));
        assert_eq!(node_protocol(&node), "hysteria2");
        let by_scheme = make_node("tuic", json!({}));
        assert_eq!(node_protocol(&by_scheme), "tuic");
        assert_eq!(probe_backend(&by_scheme), "singbox");
        // Unused in the ported helpers but exported for parity consumers.
        assert!(GROUP_TYPES.contains(&"urltest"));
        assert!(NON_PROXY_TYPES.contains(&"blackhole"));
        assert!(py_truthy(&json!(true)));
    }
}
