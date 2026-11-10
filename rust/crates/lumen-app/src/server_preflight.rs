//! Selected-server checks before the connection starts either core.

use lumen_core::link_parser::{py_int_or, py_str_or, py_truthy, validate_node_outbound};
use lumen_core::models::{AppSettings, Node};
use lumen_engines::singbox::config_builder::build_singbox_outbound;
use serde_json::{Map, Value};

use crate::health_listener::PROXY_PROTOCOLS;
use crate::node_runtime_service::{
    core_preference_of, native_singbox_only_message, node_supported_by_core,
    prepare_node_for_runtime, proxy_core_for_node, xray_only_message,
};

const INVALID_PROFILE: &str = "Некорректная структура или обязательные поля профиля.";

/// Prepare the selected node in the same order as the connection flow.
pub fn prepare_connection_node(
    node: Option<&mut Node>,
    settings: &AppSettings,
    on_changed: impl FnOnce(&Node),
) -> Result<(), String> {
    let Some(node) = node else {
        return Ok(());
    };
    if let Some(problem) = prepare_node_for_runtime(Some(node), on_changed)
        .or_else(|| validate_server_preflight(Some(node), settings))
    {
        return Err(problem);
    }
    let preference = core_preference_of(Some(settings));
    if node_supported_by_core(Some(node), preference) {
        Ok(())
    } else if preference == "xray" {
        Err(native_singbox_only_message(Some(node)))
    } else {
        Err(xray_only_message(Some(node)))
    }
}

pub fn validate_server_preflight(node: Option<&Node>, settings: &AppSettings) -> Option<String> {
    let node = node?;
    let Some(outbound) = node
        .outbound
        .as_object()
        .filter(|outbound| !outbound.is_empty())
    else {
        return Some("Профиль не содержит outbound/protocol.".into());
    };
    let protocol = py_str_or(outbound.get("protocol"), &node.scheme)
        .trim()
        .to_lowercase();
    if protocol.is_empty() {
        return Some("Профиль не содержит outbound/protocol.".into());
    }
    if !matches!(
        protocol.as_str(),
        "singbox_config" | "xray_config" | "masque" | "warp" | "awg" | "wireguard"
    ) && let Some(problem) = validate_node_outbound(node)
    {
        return Some(problem);
    }

    if let Some(stream) = outbound.get("streamSettings").and_then(Value::as_object) {
        if py_str_or(stream.get("network"), "")
            .trim()
            .eq_ignore_ascii_case("xhttp")
            && let Some(problem) = validate_xhttp(stream)
        {
            return Some(problem);
        }
        if py_str_or(stream.get("security"), "")
            .trim()
            .eq_ignore_ascii_case("reality")
            && let Err(problem) = validate_reality(stream)
        {
            return Some(problem.into());
        }
    }

    let native = outbound
        .get("singbox")
        .and_then(Value::as_object)
        .unwrap_or(outbound);
    let result = match protocol.as_str() {
        "warp" | "wireguard" | "awg" => validate_warp_awg(native),
        "hysteria" | "hysteria2" => validate_hysteria(native),
        "mieru" => validate_mieru(native),
        "masque" => validate_masque(native),
        "singbox_config" => return validate_singbox_profile(outbound).err().map(str::to_owned),
        "xray_config" => return validate_xray_profile(outbound).err().map(str::to_owned),
        _ => Ok(()),
    };
    if let Err(problem) = result {
        return Some(problem.into());
    }

    let tun_singbox = settings.tun_mode && settings.core_preference != "xray";
    if (tun_singbox || proxy_core_for_node(Some(node), &settings.core_preference) == "singbox")
        && let Err(error) = build_singbox_outbound(node, "proxy", false, 8)
        && !PROXY_PROTOCOLS.contains(&protocol.as_str())
    {
        return Some(error.to_string());
    }
    None
}

fn validate_xhttp(stream: &Map<String, Value>) -> Option<String> {
    let settings = stream.get("xhttpSettings").and_then(Value::as_object)?;
    let mode = py_str_or(settings.get("mode"), "");
    let mode = mode.trim();
    if !matches!(mode, "" | "auto" | "packet-up" | "stream-up" | "stream-one") {
        return Some(format!(
            "Режим XHTTP `{mode}` не поддерживается sing-box extended. Используйте auto, packet-up, stream-up или stream-one."
        ));
    }
    if settings
        .get("extra")
        .is_some_and(|extra| !extra.is_null() && !extra.is_object())
    {
        return Some("Поле XHTTP extra должно быть JSON-объектом.".into());
    }
    None
}

fn validate_reality(stream: &Map<String, Value>) -> Result<(), &'static str> {
    let reality = stream.get("realitySettings").unwrap_or(&Value::Null);
    let key = reality
        .get("publicKey")
        .filter(|value| py_truthy(value))
        .or_else(|| reality.get("password"));
    if py_str_or(key, "").trim().is_empty() {
        return Err("Сервер Reality без publicKey не может быть запущен корректно.");
    }
    Ok(())
}

fn validate_warp_awg(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    if let Some(peers) = outbound.get("peers").and_then(Value::as_array) {
        let peer = peers
            .first()
            .ok_or("Конфигурация WARP/AWG без peers не может быть запущена.")?;
        let address = peer
            .get("address")
            .filter(|value| py_truthy(value))
            .or_else(|| peer.get("server"));
        if py_str_or(address, "").trim().is_empty() {
            return Err("Peer WARP/AWG без адреса endpoint не может быть запущен.");
        }
    }
    Ok(())
}

fn validate_hysteria(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    if py_str_or(outbound.get("server"), "").trim().is_empty() {
        return Err("Сервер Hysteria/Hysteria2 без адреса не может быть запущен.");
    }
    let port = outbound
        .get("server_port")
        .filter(|value| py_truthy(value))
        .or_else(|| outbound.get("port"));
    if py_int_or(port, 0).map_err(|_| INVALID_PROFILE)? == 0 {
        return Err("Сервер Hysteria/Hysteria2 без порта не может быть запущен.");
    }
    Ok(())
}

fn validate_mieru(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    if py_str_or(outbound.get("server"), "").trim().is_empty() {
        return Err("Сервер Mieru без адреса не может быть запущен.");
    }
    let has_port = py_int_or(outbound.get("server_port"), 0).map_err(|_| INVALID_PROFILE)? != 0;
    let has_ranges = outbound
        .get("server_ports")
        .and_then(Value::as_array)
        .is_some_and(|ports| !ports.is_empty());
    if !has_port && !has_ranges {
        return Err("Сервер Mieru должен содержать server_port или server_ports.");
    }
    let transport = py_str_or(outbound.get("transport"), "");
    if !matches!(
        transport.trim().to_ascii_uppercase().as_str(),
        "TCP" | "UDP"
    ) {
        return Err("Транспорт Mieru должен быть TCP или UDP.");
    }
    if ["username", "password"]
        .into_iter()
        .any(|key| py_str_or(outbound.get(key), "").trim().is_empty())
    {
        return Err("Сервер Mieru должен содержать username и password.");
    }
    Ok(())
}

fn validate_masque(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    if outbound
        .get("profile")
        .is_some_and(|profile| !profile.is_null() && !profile.is_object())
    {
        return Err("Профиль MASQUE должен быть JSON-объектом.");
    }
    if outbound
        .get("allowed_ips")
        .is_some_and(|ips| !ips.is_null() && !ips.is_array())
    {
        return Err("Поле MASQUE allowed_ips должно быть списком.");
    }
    Ok(())
}

fn validate_singbox_profile(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    let config = outbound
        .get("singbox_config")
        .and_then(Value::as_object)
        .ok_or(
            "Конфигурация провайдера sing-box extended повреждена: отсутствует полный JSON-конфиг.",
        )?;
    if !config.get("outbounds").is_some_and(Value::is_array) {
        return Err("Конфигурация провайдера sing-box extended должна содержать список outbounds.");
    }
    Ok(())
}

fn validate_xray_profile(outbound: &Map<String, Value>) -> Result<(), &'static str> {
    let config = outbound
        .get("xray_config")
        .and_then(Value::as_object)
        .ok_or("AUTO-профиль Xray повреждён: отсутствует полный JSON-конфиг.")?;
    if !config.get("outbounds").is_some_and(Value::is_array) {
        return Err("AUTO-профиль Xray должен содержать список outbounds.");
    }
    let Some(balancers) = config
        .get("routing")
        .and_then(|routing| routing.get("balancers"))
    else {
        return Ok(());
    };
    let balancers = balancers
        .as_array()
        .ok_or("routing.balancers должен быть списком.")?;
    let requires_observer = balancers.iter().any(|balancer| {
        let strategy = balancer
            .get("strategy")
            .and_then(|strategy| strategy.get("type"));
        matches!(
            py_str_or(strategy, "").trim().to_lowercase().as_str(),
            "leastping" | "leastload"
        )
    });
    let has_observer = ["observatory", "burstObservatory"]
        .into_iter()
        .any(|key| config.get(key).is_some_and(Value::is_object));
    if requires_observer && !has_observer {
        return Err(
            "AUTO-профиль Xray использует leastPing/leastLoad, но не содержит observatory или burstObservatory.",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(scheme: &str, outbound: Value) -> Node {
        Node {
            name: "test".into(),
            scheme: scheme.into(),
            server: "example.test".into(),
            port: 443,
            outbound,
            ..Node::default()
        }
    }

    fn xray_auto(strategy: Value) -> Node {
        node(
            "auto",
            json!({
                "protocol": "xray_config",
                "xray_config": {
                    "outbounds": [{"protocol": "vless", "tag": "proxy"}],
                    "routing": {"balancers": [{"tag": "auto", "selector": ["proxy"], "strategy": strategy}]}
                }
            }),
        )
    }

    #[test]
    fn xray_auto_requires_observatory_only_for_probe_strategies() {
        let settings = AppSettings::default();
        for strategy in [
            json!({}),
            json!({"type": "random"}),
            json!({"type": "roundRobin"}),
        ] {
            assert_eq!(
                validate_server_preflight(Some(&xray_auto(strategy)), &settings),
                None
            );
        }
        for strategy in ["leastPing", "leastLoad"] {
            let node = xray_auto(json!({"type": strategy}));
            let error = validate_server_preflight(Some(&node), &settings).unwrap();
            assert!(error.contains("observatory или burstObservatory"));
            for observer in ["observatory", "burstObservatory"] {
                let mut with_observer = node.clone();
                with_observer.outbound["xray_config"][observer] = json!({});
                assert_eq!(
                    validate_server_preflight(Some(&with_observer), &settings),
                    None
                );
            }
        }
    }

    #[test]
    fn xhttp_options_are_validated_before_native_conversion() {
        let mut vless = node(
            "vless",
            json!({
                "protocol": "vless",
                "streamSettings": {"network": "XHTTP", "xhttpSettings": {"mode": "invalid"}}
            }),
        );
        let settings = AppSettings::default();
        assert_eq!(
            validate_server_preflight(Some(&vless), &settings).as_deref(),
            Some(
                "Режим XHTTP `invalid` не поддерживается sing-box extended. Используйте auto, packet-up, stream-up или stream-one."
            )
        );
        vless.outbound["streamSettings"]["xhttpSettings"] =
            json!({"mode": "packet-up", "extra": []});
        assert_eq!(
            validate_server_preflight(Some(&vless), &settings).as_deref(),
            Some("Поле XHTTP extra должно быть JSON-объектом.")
        );
    }

    #[test]
    fn validates_full_profile_shapes() {
        let settings = AppSettings::default();
        assert_eq!(validate_server_preflight(None, &settings), None);
        let mut singbox = node("singbox_config", json!({"protocol": "singbox_config"}));
        assert_eq!(
            validate_server_preflight(Some(&singbox), &settings).as_deref(),
            Some(
                "Конфигурация провайдера sing-box extended повреждена: отсутствует полный JSON-конфиг."
            )
        );
        singbox.outbound["singbox_config"] = json!({"outbounds": {}});
        assert_eq!(
            validate_server_preflight(Some(&singbox), &settings).as_deref(),
            Some("Конфигурация провайдера sing-box extended должна содержать список outbounds.")
        );
        singbox.outbound["singbox_config"]["outbounds"] = json!([]);
        assert_eq!(validate_server_preflight(Some(&singbox), &settings), None);

        let mut xray = xray_auto(json!({}));
        xray.outbound["xray_config"]["routing"]["balancers"] = Value::Null;
        assert_eq!(
            validate_server_preflight(Some(&xray), &settings).as_deref(),
            Some("routing.balancers должен быть списком.")
        );
    }

    #[test]
    fn reality_preflight_accepts_the_password_alias_in_full_profiles() {
        let mut xray = xray_auto(json!({}));
        xray.outbound["streamSettings"] = json!({"security": "reality"});
        let settings = AppSettings::default();
        assert_eq!(
            validate_server_preflight(Some(&xray), &settings).as_deref(),
            Some("Сервер Reality без publicKey не может быть запущен корректно.")
        );
        xray.outbound["streamSettings"]["realitySettings"] = json!({"password": "key"});
        assert_eq!(validate_server_preflight(Some(&xray), &settings), None);
    }

    #[test]
    fn validates_native_endpoint_fields_before_building_a_runtime() {
        let settings = AppSettings::default();
        for (scheme, native, message) in [
            (
                "warp",
                json!({"peers": []}),
                "Конфигурация WARP/AWG без peers не может быть запущена.",
            ),
            (
                "awg",
                json!({"peers": [{}]}),
                "Peer WARP/AWG без адреса endpoint не может быть запущен.",
            ),
            (
                "hysteria",
                json!({"server_port": 443}),
                "Сервер Hysteria/Hysteria2 без адреса не может быть запущен.",
            ),
            (
                "hysteria2",
                json!({"server": "example.test"}),
                "Сервер Hysteria/Hysteria2 без порта не может быть запущен.",
            ),
            (
                "hysteria2",
                json!({"server": "example.test", "server_port": [] , "port": "bad"}),
                INVALID_PROFILE,
            ),
            (
                "mieru",
                json!({"server": "example.test", "server_ports": []}),
                "Сервер Mieru должен содержать server_port или server_ports.",
            ),
            (
                "mieru",
                json!({"server": "example.test", "server_ports": ["443"]}),
                "Транспорт Mieru должен быть TCP или UDP.",
            ),
            (
                "mieru",
                json!({"server": "example.test", "server_ports": ["443"], "transport": "TCP"}),
                "Сервер Mieru должен содержать username и password.",
            ),
            (
                "masque",
                json!({"profile": []}),
                "Профиль MASQUE должен быть JSON-объектом.",
            ),
            (
                "masque",
                json!({"profile": {}, "allowed_ips": "0.0.0.0/0"}),
                "Поле MASQUE allowed_ips должно быть списком.",
            ),
        ] {
            let node = node(scheme, json!({"protocol": scheme, "singbox": native}));
            assert_eq!(
                validate_server_preflight(Some(&node), &settings).as_deref(),
                Some(message),
                "{scheme}"
            );
        }
    }

    #[test]
    fn native_conversion_failure_preserves_xray_fallback_only_for_classic_protocols() {
        let settings = AppSettings {
            tun_mode: true,
            ..AppSettings::default()
        };
        let vless = node(
            "vless",
            json!({
                "protocol": "vless",
                "settings": {"vnext": [{"address": "example.test", "port": 443,
                    "users": [{"id": "00000000-0000-0000-0000-000000000000"}]}]},
                "streamSettings": {"network": "quic"}
            }),
        );
        assert!(build_singbox_outbound(&vless, "proxy", false, 8).is_err());
        assert_eq!(validate_server_preflight(Some(&vless), &settings), None);
        let unknown = node("unknown", json!({"protocol": "unknown"}));
        assert!(
            validate_server_preflight(Some(&unknown), &settings)
                .unwrap()
                .contains("protocol `unknown`")
        );
    }

    #[test]
    fn connection_preparation_repairs_the_node_then_checks_core_support() {
        let settings = AppSettings {
            core_preference: "xray".into(),
            ..AppSettings::default()
        };
        let mut vless = node("vless", json!({}));
        vless.link = "vless://bbbbbbbb-0000-4000-8000-000000000000@example.test:443".into();
        let mut changed = false;
        assert_eq!(
            prepare_connection_node(Some(&mut vless), &settings, |node| {
                assert_eq!(node.outbound["protocol"], "vless");
                changed = true;
            }),
            Ok(())
        );
        assert!(changed);

        let mut singbox = node(
            "singbox_config",
            json!({
                "protocol": "singbox_config",
                "singbox_config": {"outbounds": []}
            }),
        );
        let error = prepare_connection_node(Some(&mut singbox), &settings, |_| {}).unwrap_err();
        assert_eq!(error, native_singbox_only_message(Some(&singbox)));
    }
}
