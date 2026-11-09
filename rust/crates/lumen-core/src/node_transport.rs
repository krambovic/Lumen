//! Compact transport and protocol labels for server lists and sorting.
//!
//! Port of `windows/xray_fluent/node_transport.py` (pure labeling from
//! `Node.outbound`).

use serde_json::Value;

use crate::models::Node;
use crate::subscription::text::{py_or_str, py_str, py_truthy};

const UDP_PROTOCOLS: [&str; 7] = [
    "awg",
    "hysteria",
    "hysteria2",
    "masque",
    "tuic",
    "warp",
    "wireguard",
];
const TCP_PROTOCOLS: [&str; 6] = ["http", "shadowsocks", "socks", "trojan", "vless", "vmess"];

const PROTOCOL_NAMES: [(&str, &str); 27] = [
    ("vless", "VLESS"),
    ("vmess", "VMess"),
    ("trojan", "Trojan"),
    ("ss", "Shadowsocks"),
    ("shadowsocks", "Shadowsocks"),
    ("socks", "SOCKS"),
    ("socks5", "SOCKS"),
    ("http", "HTTP"),
    ("https", "HTTPS"),
    ("hysteria", "Hysteria"),
    ("hy", "Hysteria"),
    ("hysteria2", "Hysteria2"),
    ("hy2", "Hysteria2"),
    ("tuic", "TUIC"),
    ("wireguard", "WireGuard"),
    ("wg", "WireGuard"),
    ("awg", "AmneziaWG"),
    ("warp", "WARP"),
    ("naive", "Naive"),
    ("masque", "MASQUE"),
    ("mieru", "Mieru"),
    ("anytls", "AnyTLS"),
    ("openvpn", "OpenVPN"),
    ("snell", "Snell"),
    ("xray_config", "Xray JSON"),
    ("singbox_config", "sing-box JSON"),
    ("auto", "AUTO"),
];

/// `_normalize_transport(value)` from the Python module.
fn normalize_transport(value: &Value) -> String {
    let network = py_or_str(&[Some(value)]).trim().to_lowercase();
    match network.as_str() {
        "raw" => "TCP".into(),
        "http" => "H2".into(),
        "" => String::new(),
        other => other.to_uppercase(),
    }
}

fn object_or_null(value: Option<&Value>) -> &Value {
    match value {
        Some(value) if value.is_object() => value,
        _ => &NULL_VALUE,
    }
}

static NULL_VALUE: Value = Value::Null;

/// Return a compact transport label for server lists and sorting.
pub fn node_transport(node: &Node) -> String {
    let empty = Value::Null;
    let outbound = if node.outbound.is_object() {
        &node.outbound
    } else {
        &empty
    };

    if let Some(stream) = outbound.get("streamSettings").filter(|value| value.is_object()) {
        let network = normalize_transport(stream.get("network").unwrap_or(&empty));
        if !network.is_empty() {
            return network;
        }
    }

    if let Some(native) = outbound.get("singbox").filter(|value| value.is_object()) {
        let native_type = py_or_str(&[native.get("type")]).trim().to_lowercase();
        if native_type == "naive" {
            return if native.get("quic") == Some(&Value::Bool(true)) {
                "QUIC".into()
            } else {
                "H2".into()
            };
        }
        if matches!(native_type.as_str(), "openvpn" | "openvpn-client") {
            // Python: native.get("proto") or native.get("network")
            let proto = match native.get("proto").filter(|value| py_truthy(value)) {
                Some(value) => value.clone(),
                None => native.get("network").cloned().unwrap_or(Value::Null),
            };
            let normalized = normalize_transport(&proto);
            return if normalized.is_empty() {
                "UDP".into()
            } else {
                normalized
            };
        }
        let transport = native.get("transport");
        if let Some(transport) = transport {
            if transport.is_string() && native_type == "mieru" {
                return format!("MIERU/{}", py_str(transport).to_uppercase());
            }
        }
        if matches!(native_type.as_str(), "mieru" | "masque") {
            return native_type.to_uppercase();
        }
        if let Some(transport) = transport.filter(|value| value.is_object()) {
            let transport_type = normalize_transport(transport.get("type").unwrap_or(&empty));
            if !transport_type.is_empty() {
                return transport_type;
            }
        }
    }

    // Python: str(outbound.get("protocol") or node.scheme or "")
    let protocol = py_or_str(&[
        outbound.get("protocol"),
        Some(&Value::String(node.scheme.clone())),
    ])
    .trim()
    .to_lowercase();
    if UDP_PROTOCOLS.contains(&protocol.as_str()) {
        return "UDP".into();
    }
    if protocol == "xray_config" {
        return "AUTO".into();
    }
    if matches!(protocol.as_str(), "mieru" | "naive" | "singbox_config") {
        return protocol.to_uppercase();
    }
    if TCP_PROTOCOLS.contains(&protocol.as_str()) {
        return "TCP".into();
    }
    "—".into()
}

/// Human protocol label for compact server captions, e.g. `VLESS Reality`.
pub fn node_protocol_label(node: &Node) -> String {
    let empty = Value::Null;
    let outbound = if node.outbound.is_object() {
        &node.outbound
    } else {
        &empty
    };
    let native = object_or_null(outbound.get("singbox"));

    // Python: str(node.scheme or outbound.get("protocol") or native.get("type") or "")
    let raw = py_or_str(&[
        Some(&Value::String(node.scheme.clone())),
        outbound.get("protocol"),
        native.get("type"),
    ])
    .trim()
    .to_lowercase();
    let name = PROTOCOL_NAMES
        .iter()
        .find(|(key, _)| *key == raw)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| raw.to_uppercase());

    let mut security = String::new();
    if let Some(stream) = outbound.get("streamSettings").filter(|value| value.is_object()) {
        security = py_or_str(&[stream.get("security")]).trim().to_lowercase();
    }
    let tls = object_or_null(native.get("tls"));
    if security.is_empty() && py_truthy(tls) {
        let reality = tls.get("reality");
        let reality_active = reality
            .map(|value| {
                value.is_object()
                    && (value.get("enabled").map(py_truthy).unwrap_or(false) || py_truthy(value))
            })
            .unwrap_or(false);
        if reality_active {
            security = "reality".into();
        } else if tls.get("enabled").map(py_truthy).unwrap_or(false) {
            security = "tls".into();
        }
    }

    if security == "reality" {
        return format!("{name} Reality").trim().to_string();
    }
    if security == "tls" && matches!(raw.as_str(), "vless" | "vmess" | "trojan") {
        return format!("{name} TLS").trim().to_string();
    }
    if name.is_empty() {
        "—".into()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_node(scheme: &str, outbound: Value) -> Node {
        let mut node = Node::new();
        node.scheme = scheme.into();
        node.outbound = outbound;
        node
    }

    #[test]
    fn vless_reality_labels() {
        let node = make_node(
            "vless",
            json!({"protocol": "vless", "streamSettings": {"network": "tcp", "security": "reality"}}),
        );
        assert_eq!(node_transport(&node), "TCP");
        assert_eq!(node_protocol_label(&node), "VLESS Reality");
    }

    #[test]
    fn hysteria2_labels() {
        let node = make_node("hysteria2", json!({"protocol": "hysteria2"}));
        assert_eq!(node_transport(&node), "UDP");
        assert_eq!(node_protocol_label(&node), "Hysteria2");
    }

    #[test]
    fn vless_tls_label() {
        let node = make_node(
            "vmess",
            json!({"protocol": "vmess", "streamSettings": {"network": "ws", "security": "tls"}}),
        );
        assert_eq!(node_transport(&node), "WS");
        assert_eq!(node_protocol_label(&node), "VMess TLS");
    }

    #[test]
    fn singbox_naive_mieru_and_reality() {
        let naive = make_node("naive", json!({"singbox": {"type": "naive"}}));
        assert_eq!(node_transport(&naive), "H2");
        assert_eq!(node_protocol_label(&naive), "Naive");
        let quic = make_node("naive", json!({"singbox": {"type": "naive", "quic": true}}));
        assert_eq!(node_transport(&quic), "QUIC");
        let mieru = make_node(
            "mieru",
            json!({"singbox": {"type": "mieru", "transport": "BR"}}),
        );
        assert_eq!(node_transport(&mieru), "MIERU/BR");
        assert_eq!(node_protocol_label(&mieru), "Mieru");
        let reality = make_node(
            "vless",
            json!({"singbox": {"type": "vless", "tls": {"enabled": true, "reality": {"enabled": true}}}}),
        );
        assert_eq!(node_protocol_label(&reality), "VLESS Reality");
    }

    #[test]
    fn openvpn_and_unknown_fallbacks() {
        let openvpn = make_node(
            "openvpn",
            json!({"singbox": {"type": "openvpn", "proto": "udp"}}),
        );
        assert_eq!(node_transport(&openvpn), "UDP");
        let tcp_openvpn = make_node(
            "openvpn",
            json!({"singbox": {"type": "openvpn-client", "proto": "tcp"}}),
        );
        assert_eq!(node_transport(&tcp_openvpn), "TCP");
        let unknown = make_node("weird", json!({"protocol": "weird-proto"}));
        assert_eq!(node_transport(&unknown), "—");
        // Python chains node.scheme first, so the scheme label wins.
        assert_eq!(node_protocol_label(&unknown), "WEIRD");
        let raw_network = make_node(
            "vless",
            json!({"protocol": "vless", "streamSettings": {"network": "raw"}}),
        );
        assert_eq!(node_transport(&raw_network), "TCP");
        let auto = make_node("xray_config", json!({"protocol": "xray_config"}));
        assert_eq!(node_transport(&auto), "AUTO");
        assert_eq!(node_protocol_label(&auto), "Xray JSON");
    }
}
