use std::path::Path;

use lumen_core::models::{AppSettings, AppState, Node};
use serde_json::json;

use super::*;

#[test]
fn replaces_imported_api_with_loopback_stats_and_preserves_shared_routes() -> anyhow::Result<()> {
    let mut payload = json!({
        "api":{"tag":"old-api","listen":"0.0.0.0:10085","services":["HandlerService","RoutingService"]},
        "inbounds":[
            {"tag":"old-api-in","listen":"0.0.0.0","port":10085,"protocol":"dokodemo-door"},
            {"tag":"socks-in","protocol":"socks","listen":"0.0.0.0","port":10808},
            {"tag":"__app_metrics_api_in","port":"19085","listen":"0.0.0.0"},
            {"protocol":"tun"}
        ],
        "outbounds":[{"tag":"old-api","protocol":"freedom"},{"tag":"proxy","protocol":"vless"}],
        "routing":{"rules":[
            {"type":"field","inboundTag":["old-api-in"],"outboundTag":"old-api"},
            {"type":"field","inboundTag":["socks-in","old-api-in"],"outboundTag":"proxy"}
        ]}
    });
    let settings = AppSettings::default();
    let (port, tags) = ensure_xray_metrics_contract(&mut payload, &settings, false, |_| {})?;
    assert_eq!(port, 19085);
    assert_eq!(tags, ["socks-in", "__app_user_inbound_1"]);
    assert_eq!(
        payload["api"],
        json!({"tag":"__app_metrics_api","services":["StatsService"]})
    );
    assert_eq!(payload["inbounds"][0]["listen"], "127.0.0.1");
    assert_eq!(payload["inbounds"][2]["listen"], "127.0.0.1");
    assert_eq!(payload["outbounds"].as_array().unwrap().len(), 2);
    assert_eq!(
        payload["routing"]["rules"][1]["inboundTag"],
        json!(["socks-in"])
    );

    let first = payload.clone();
    ensure_xray_metrics_contract(&mut payload, &settings, false, |_| {})?;
    assert_eq!(payload, first);
    Ok(())
}

#[test]
fn preserves_full_auto_profile_balancing_and_skips_gui_overrides() -> anyhow::Result<()> {
    let auto = json!({
        "inbounds":[{"tag":"__app_metrics_api_in","port":19085}],
        "outbounds":[{"tag":"auto-node","protocol":"vless"}],
        "routing":{"balancers":[{"tag":"auto","strategy":{"type":"leastPing"}}],
                   "rules":[{"type":"field","network":"tcp,udp","balancerTag":"auto"}]},
        "observatory":{"subjectSelector":["auto-node"],"probeInterval":"30s"}
    });
    let mut state = AppState {
        settings: AppSettings {
            enable_xray_fragment: true,
            enable_final_fragment: true,
            ..Default::default()
        },
        nodes: vec![Node::from_dict(
            &json!({"id":"auto","server":"auto.example","port":443,
            "outbound":{"protocol":"xray_config","xray_config":auto}}),
        )],
        ..Default::default()
    };
    let runtime = build_runtime_xray_config(
        Path::new("default.json"),
        "malformed",
        &mut state,
        Some("auto"),
        |_| {},
    )?;
    assert!(runtime.used_selected_node && runtime.has_proxy_outbound);
    // Complete JSON emitted by the Python runtime service for this profile.
    assert_eq!(
        runtime.config,
        json!({
            "inbounds": [
                {"tag":"socks-in","listen":"127.0.0.1","port":10808,"protocol":"mixed",
                 "settings":{"auth":"noauth","udp":true,"allowTransparent":false},
                 "sniffing":{"enabled":true,"destOverride":["http","tls"],"routeOnly":false}},
                {"tag":"http-in","listen":"127.0.0.1","port":10809,"protocol":"http","settings":{},
                 "sniffing":{"enabled":true,"destOverride":["http","tls"],"routeOnly":false}},
                {"tag":"__app_metrics_api_in","listen":"127.0.0.1","port":19085,
                 "protocol":"dokodemo-door","settings":{"address":"127.0.0.1"}}
            ],
            "outbounds":[{"tag":"auto-node","protocol":"vless"},
                         {"tag":"__app_metrics_api","protocol":"freedom","settings":{}}],
            "routing":{"balancers":[{"tag":"auto","strategy":{"type":"leastPing"}}],
                       "rules":[{"type":"field","inboundTag":["__app_metrics_api_in"],"outboundTag":"__app_metrics_api"},
                                {"type":"field","network":"tcp,udp","balancerTag":"auto"}]},
            "observatory":{"subjectSelector":["auto-node"],"probeInterval":"30s"},
            "stats":{},
            "policy":{"system":{"statsInboundUplink":true,"statsInboundDownlink":true,
                                "statsOutboundUplink":true,"statsOutboundDownlink":true}},
            "api":{"tag":"__app_metrics_api","services":["StatsService"]}
        })
    );
    assert!(runtime.health_proxy_url.is_empty());
    assert_eq!(
        (runtime.ping_host.as_str(), runtime.ping_port),
        ("auto.example", 443)
    );
    Ok(())
}

#[test]
fn inspection_keeps_user_routes_and_does_not_allocate_an_api_port() -> anyhow::Result<()> {
    let mut payload = json!({
        "api":{"tag":"direct","services":["RoutingService"]},
        "inbounds":[{"tag":"socks-in","protocol":"mixed","port":10808}],
        "outbounds":[{"tag":"direct","protocol":"freedom"},{"tag":"proxy","protocol":"vless"}],
        "routing":{"rules":[{"type":"field","inboundTag":["socks-in"],"outboundTag":"direct"}]}
    });
    let settings = AppSettings::default();
    let text = payload.to_string();
    let inspection =
        inspect_active_xray_config(Path::new("profile.json"), &text, &settings, |_| {});
    assert!(inspection.has_proxy_outbound);
    assert_eq!(
        (
            inspection.socks_port,
            inspection.http_port,
            inspection.api_port
        ),
        (10808, 10808, 0)
    );
    assert_eq!(inspection.source_path, Path::new("profile.json"));
    assert_eq!(inspection.text_hash.len(), 64);
    let unparseable = inspect_active_xray_config(Path::new("profile.json"), "[", &settings, |_| {});
    assert!(!unparseable.has_proxy_outbound);
    assert_eq!(unparseable.socks_port, 0);
    assert_ne!(unparseable.text_hash, inspection.text_hash);
    ensure_xray_metrics_contract(&mut payload, &settings, false, |_| {})?;
    assert_eq!(payload["outbounds"][0]["tag"], "direct");
    assert_eq!(payload["routing"]["rules"][1]["outboundTag"], "direct");
    Ok(())
}

#[test]
fn rejects_a_proxy_slot_without_a_selected_or_supported_node() {
    let mut state = AppState {
        nodes: vec![Node::from_dict(&json!({"id":"native","name":"Native node",
            "outbound":{"protocol":"hysteria2"}}))],
        ..Default::default()
    };
    let text = r#"{"inbounds":[{"tag":"__app_metrics_api_in","port":19085}],
                   "outbounds":[{"tag":"proxy","protocol":"freedom"}]}"#;
    for (selected, message) in [
        (None, "Выберите сервер для запуска xray."),
        (Some("native"), "Native node не поддерживается ядром Xray."),
    ] {
        let error = build_runtime_xray_config(
            Path::new("profile.json"),
            text,
            &mut state,
            selected,
            |_| {},
        )
        .err()
        .expect("profile should be rejected");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn prepares_the_selected_proxy_and_isolates_authenticated_health_traffic() -> anyhow::Result<()> {
    let mut state = AppState {
        settings: AppSettings {
            proxy_auth_enabled: true,
            proxy_auth_username: " user ".into(),
            proxy_auth_password: "secret".into(),
            ..Default::default()
        },
        nodes: vec![Node::from_dict(
            &json!({"id":"node","server":"example.test","port":443,
            "scheme":"vless","link":"vless://bbbbbbbb-0000-4000-8000-000000000000@example.test:443",
            "outbound":{}}),
        )],
        ..Default::default()
    };
    let profile = r#"{"outbounds":[{"tag":"proxy","protocol":"freedom"}],"routing":{"rules":[]}}"#;
    let mut events = Vec::new();
    let runtime = build_runtime_xray_config(
        Path::new("default.json"),
        profile,
        &mut state,
        Some("node"),
        |event| events.push(event),
    )?;
    assert!(runtime.used_selected_node && runtime.has_proxy_outbound);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, XrayRuntimeEvent::NodeChanged(id) if id == "node"))
    );
    assert_eq!(runtime.config["outbounds"][0]["protocol"], "vless");
    assert_eq!(
        runtime.config["outbounds"][0]["streamSettings"]["sockopt"]["domainStrategy"],
        "UseIPv4"
    );
    let health = runtime.config["inbounds"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let health_tag = health["tag"].as_str().unwrap();
    assert!(health_tag.starts_with("__app_health_"));
    assert!(!runtime.inbound_tags.iter().any(|tag| tag == health_tag));
    assert_eq!(health["listen"], "127.0.0.1");
    assert_eq!(health["settings"]["accounts"][0]["user"], "lumen-health");
    assert!(runtime.health_proxy_url.starts_with("http://lumen-health:"));
    assert_eq!(
        runtime.config["routing"]["rules"][0]["outboundTag"],
        "proxy"
    );
    assert_eq!(
        runtime.config["routing"]["rules"][0]["inboundTag"],
        json!([health_tag])
    );
    assert_ne!(health["port"], runtime.api_port);
    assert_eq!(
        runtime.config["inbounds"][0]["settings"]["accounts"][0]["user"],
        "user"
    );
    Ok(())
}
