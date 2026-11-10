//! Subset of `application/node_runtime_service.py` required by the runtime
//! planner: core capability checks and the Russian refusal message. The full
//! module (controller-bound helpers) stays in `lumen-app`.

use serde_json::Value;

use super::{is_falsy_json, json_str, json_str_field, json_str_or, lower_trim};
use lumen_core::models::Node;

/// Python `is_xray_exclusive_node`: return True only for nodes that require
/// the Xray runtime.
pub fn is_xray_exclusive_node(node: Option<&Node>) -> bool {
    let Some(node) = node else {
        return false;
    };
    let outbound = &node.outbound;
    if !outbound.is_object() {
        return false;
    }

    let protocol = lower_trim(&json_str_or(outbound.get("protocol"), &node.scheme));
    if protocol == "xray_config" {
        return true;
    }

    let Some(stream_settings) = outbound.get("streamSettings").filter(|stream| stream.is_object()) else {
        return false;
    };

    // Never silently discard Xray-only semantics.  These options have no
    // equivalent in the exact bundled sing-box-extended schema.  Running such
    // a node as an approximate native outbound is worse than using the Xray
    // core/sidecar because the config starts successfully but cannot connect.
    if stream_settings.get("finalmask").is_some_and(|value| !is_falsy_json(value)) {
        return true;
    }

    let network = lower_trim(&json_str_or(stream_settings.get("network"), "tcp"));
    let security = lower_trim(&json_str_or(stream_settings.get("security"), "none"));
    if !matches!(
        network.as_str(),
        "tcp" | "raw" | "ws" | "http" | "h2" | "grpc" | "xhttp" | "httpupgrade" | "quic" | "kcp" | "mkcp"
    ) {
        return true;
    }
    if matches!(protocol.as_str(), "shadowsocks" | "socks")
        && (!matches!(network.as_str(), "tcp" | "raw") || matches!(security.as_str(), "tls" | "reality"))
    {
        return true;
    }
    if protocol == "http" && !matches!(network.as_str(), "tcp" | "raw") {
        return true;
    }
    if network == "quic" {
        return true;
    }

    if network == "grpc" {
        if let Some(grpc) = stream_settings.get("grpcSettings").filter(|value| value.is_object()) {
            let authority = json_str(grpc.get("authority"));
            if !authority.trim().is_empty() {
                return true;
            }
            let multi_mode = grpc
                .get("multiMode")
                .filter(|value| !is_falsy_json(value))
                .or_else(|| grpc.get("multi_mode"))
                .unwrap_or(&Value::Bool(false));
            if !is_falsy_json(multi_mode) {
                return true;
            }
        }
    }

    if network == "httpupgrade" {
        if let Some(settings) = stream_settings.get("httpupgradeSettings").filter(|value| value.is_object()) {
            let path = json_str(settings.get("path")).to_lowercase();
            let ed = json_str_field(settings, "ed");
            let max_early_data = json_str_field(settings, "max_early_data");
            if !ed.trim().is_empty() || !max_early_data.trim().is_empty() || path.contains("?ed=") {
                return true;
            }
        }
    }

    if let Some(tls) = stream_settings.get("tlsSettings").filter(|value| value.is_object()) {
        let verify = json_str_field(tls, "verifyPeerCertByName");
        if !verify.trim().is_empty() {
            return true;
        }
        if !singbox_utls_fingerprint_supported(tls.get("fingerprint")) {
            return true;
        }
    }

    if let Some(reality) = stream_settings.get("realitySettings").filter(|value| value.is_object()) {
        if !singbox_utls_fingerprint_supported(reality.get("fingerprint")) {
            return true;
        }
        let mldsa = json_str(reality
            .get("mldsa65Verify")
            .filter(|value| !is_falsy_json(value))
            .or_else(|| reality.get("mldsa65_verify")));
        if !mldsa.trim().is_empty() {
            return true;
        }
        let spider_x = json_str(reality
            .get("spiderX")
            .filter(|value| !is_falsy_json(value))
            .or_else(|| reality.get("spider_x")));
        let spider_x = spider_x.trim().to_string();
        if !spider_x.is_empty() && spider_x != "/" {
            return true;
        }
    }

    false
}

/// Python `_singbox_utls_fingerprint_supported`.
fn singbox_utls_fingerprint_supported(value: Option<&Value>) -> bool {
    let compact = json_str(value).trim().to_lowercase().replace('_', "-");
    if compact.is_empty() {
        return true;
    }
    if matches!(
        compact.as_str(),
        "chrome-psk" | "chrome-psk-shuffle" | "chrome-padding-psk-shuffle" | "chrome-pq" | "chrome-pq-psk"
    ) {
        return true;
    }
    ["chrome", "firefox", "edge", "safari", "ios", "android", "qq", "360", "random"]
        .iter()
        .any(|prefix| compact.starts_with(prefix))
}

/// Python `node_uses_xray_sidecar`: Auto TUN only — sing-box keeps the tunnel
/// and Xray carries the server.
pub fn node_uses_xray_sidecar(node: Option<&Node>, preference: &str) -> bool {
    preference == "auto" && node.is_some() && is_xray_exclusive_node(node)
}

/// Python `xray_only_message`.
pub fn xray_only_message(node: Option<&Node>) -> String {
    let name = match node {
        Some(node) => {
            if node.name.is_empty() {
                node.server.clone()
            } else {
                node.name.clone()
            }
        }
        None => "Этот сервер".to_string(),
    };
    format!(
        "{name} требует ядро Xray, а основное ядро — sing-box extended. \
Выберите Auto или Xray в «Настройки → Сеть → Основное ядро»."
    )
}
