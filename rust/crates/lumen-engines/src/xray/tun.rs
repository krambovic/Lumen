//! Xray-owned Wintun configuration (`engines/xray/tun.py`).

use std::net::Ipv4Addr;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use lumen_core::config::xray_inbounds::build_xray_sniffing;
use lumen_core::constants::xray_path_default;
use lumen_core::models::{AppSettings, RoutingSettings};
use lumen_core::path_utils::resolve_configured_path;
use lumen_core::routing_runtime::canonical_ip_network;
use lumen_platform::network_route_context::WindowsDefaultRouteContext;
use serde_json::{Map, Value, json};

pub const XRAY_TUN_INBOUND_TAG: &str = "tun-in";
pub const XRAY_TUN_INTERFACE: &str = "lumen_xray";
pub const XRAY_DNS_OUTBOUND_TAG: &str = "dns-out";
pub const XRAY_DNS_TAG: &str = "dns-internal";

const TUN_DNS: Ipv4Addr = Ipv4Addr::new(172, 19, 0, 2);
const FALLBACK_DNS: [&str; 2] = ["https://1.1.1.1/dns-query", "https://8.8.8.8/dns-query"];

/// Locate Wintun beside the resolved Xray executable.
pub fn xray_wintun_dll(xray_path: &str) -> Option<PathBuf> {
    let exe = resolve_configured_path(xray_path, Some(&xray_path_default()), true, true)?;
    let dll = exe.parent()?.join("wintun.dll");
    dll.is_file().then_some(dll)
}

fn ensure_outbound(outbounds: &mut Vec<Value>, tag: &str, protocol: &str) {
    if !outbounds.iter().any(|outbound| outbound["tag"] == tag) {
        outbounds.push(json!({"tag": tag, "protocol": protocol}));
    }
}

/// Add the TUN inbound, bind outbounds to the physical interface, and route
/// Xray's DNS traffic outside the tunnel. The context is captured before
/// starting the tunnel.
pub fn apply_xray_tun(
    payload: &mut Value,
    settings: &AppSettings,
    routing: &RoutingSettings,
    route_context: Option<&WindowsDefaultRouteContext>,
) -> Result<()> {
    let config = payload
        .as_object_mut()
        .context("Xray config must be an object")?;
    for key in ["inbounds", "outbounds"] {
        ensure!(
            config.get(key).is_none_or(Value::is_array),
            "Xray {key} must be an array"
        );
    }
    if let Some(route) = config.get("routing") {
        ensure!(route.is_object(), "Xray routing must be an object");
        ensure!(
            route.get("rules").is_none_or(Value::is_array),
            "Xray routing rules must be an array"
        );
    }

    // The route context skips tunnel adapters even while a previous tunnel
    // is still going away.
    let interface = route_context
        .map(|context| context.interface_alias.trim())
        .filter(|alias| !alias.is_empty() && *alias != XRAY_TUN_INTERFACE)
        .unwrap_or("auto");
    let mtu = if settings.tun_mtu == 0 {
        1500
    } else {
        settings.tun_mtu
    };
    let mut sniffing = build_xray_sniffing(false);
    sniffing["destOverride"] = json!(["http", "tls", "quic"]);
    let tun_inbound = json!({
        "tag": XRAY_TUN_INBOUND_TAG,
        "protocol": "tun",
        "settings": {
            "name": XRAY_TUN_INTERFACE,
            "mtu": mtu.clamp(1280, 9000),
            "gateway": ["172.19.0.1/30", "fdfe:dcba:9877::1/126"],
            "dns": [TUN_DNS.to_string()],
            "autoSystemRoutingTable": ["0.0.0.0/0", "::/0"],
            "autoOutboundsInterface": interface,
        },
        "sniffing": sniffing,
    });
    let inbounds = config
        .entry("inbounds")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap();
    inbounds.retain(|inbound| inbound["tag"] != XRAY_TUN_INBOUND_TAG);
    inbounds.push(tun_inbound);

    let outbounds = config
        .entry("outbounds")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap();
    ensure_outbound(outbounds, "direct", "freedom");
    ensure_outbound(outbounds, "block", "blackhole");
    ensure_outbound(outbounds, XRAY_DNS_OUTBOUND_TAG, "dns");
    for outbound in outbounds.iter_mut().filter_map(Value::as_object_mut) {
        let protocol = outbound
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("");
        let bind_interface = interface != "auto" && !matches!(protocol, "blackhole" | "dns");
        if protocol == "freedom"
            && let Some(settings) = outbound
                .entry("settings")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        {
            settings
                .entry("domainStrategy")
                .or_insert_with(|| json!("UseIPv4"));
        }
        if bind_interface
            && let Some(stream) = outbound
                .entry("streamSettings")
                .or_insert_with(|| json!({}))
                .as_object_mut()
            && let Some(sockopt) = stream
                .entry("sockopt")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        {
            sockopt
                .entry("interface")
                .or_insert_with(|| json!(interface));
        }
    }

    let servers: Vec<String> = route_context
        .into_iter()
        .flat_map(|context| &context.dns_servers)
        .filter_map(|value| value.trim().parse::<Ipv4Addr>().ok())
        .filter(|address| !address.is_loopback() && *address != TUN_DNS)
        .map(|address| address.to_string())
        .chain(FALLBACK_DNS.map(str::to_owned))
        .collect();
    let mut dns = json!({"servers": servers, "queryStrategy": "UseIPv4", "tag": XRAY_DNS_TAG});
    if !routing.dns_hosts.is_empty() {
        dns["hosts"] = Value::Object(
            routing
                .dns_hosts
                .iter()
                .filter(|(_, addresses)| !addresses.is_empty())
                .map(|(name, addresses)| (name.clone(), json!(addresses)))
                .collect::<Map<_, _>>(),
        );
    }
    config.insert("dns".into(), dns);

    let route = config
        .entry("routing")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap();
    let rules = route
        .entry("rules")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap();
    let mut head = vec![
        json!({"type": "field", "inboundTag": [XRAY_TUN_INBOUND_TAG], "port": "53", "outboundTag": XRAY_DNS_OUTBOUND_TAG}),
        json!({"type": "field", "inboundTag": [XRAY_DNS_TAG], "outboundTag": "direct"}),
    ];
    let excluded: Vec<String> = routing
        .tun_route_exclude_address
        .iter()
        .filter_map(|value| canonical_ip_network(value))
        .collect();
    if !excluded.is_empty() {
        head.push(json!({"type": "field", "ip": excluded, "outboundTag": "direct"}));
    }
    if settings.tun_block_quic {
        head.push(json!({
            "type": "field", "inboundTag": [XRAY_TUN_INBOUND_TAG],
            "network": "udp", "port": "443", "outboundTag": "block",
        }));
    }
    rules.splice(..0, head);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_the_physical_adapter_and_prepends_tun_rules() -> anyhow::Result<()> {
        let mut payload = json!({
            "inbounds": [{"tag": "socks-in"}, {"tag": "tun-in", "old": true}],
            "outbounds": [
                {"tag": "proxy", "protocol": "vless"},
                {"tag": "direct", "protocol": "freedom",
                 "settings": {"domainStrategy": "UseIPv6"},
                 "streamSettings": {"sockopt": {"interface": "Custom"}}}
            ],
            "routing": {"rules": [{"type": "field", "network": "tcp,udp", "outboundTag": "direct"}]},
            "dns": {"servers": ["localhost"]},
        });
        let settings = AppSettings {
            tun_mtu: 1200,
            sniff_route_only: true,
            ..AppSettings::default()
        };
        let routing = RoutingSettings {
            tun_route_exclude_address: [
                "192.168.50.7/255.255.255.0",
                "bad",
                "10.0.0.1",
                "2001:DB8::5/64",
                "::ffff:192.0.2.1/120",
                "fe80::1%5",
            ]
            .map(str::to_owned)
            .into(),
            dns_hosts: [
                ("local.test".into(), vec!["127.0.0.1".into()]),
                ("empty.test".into(), vec![]),
            ]
            .into(),
            ..RoutingSettings::default()
        };
        let context = WindowsDefaultRouteContext {
            interface_alias: " Ethernet ".into(),
            dns_servers: [
                "159.194.200.33",
                "fe80::1",
                "127.1.0.1",
                "172.19.0.2",
                "invalid",
            ]
            .map(str::to_owned)
            .into(),
            tun_active: true,
            ..WindowsDefaultRouteContext::default()
        };

        apply_xray_tun(&mut payload, &settings, &routing, Some(&context))?;

        assert_eq!(
            payload,
            json!({
                "inbounds": [{"tag": "socks-in"}, {
                    "tag": "tun-in",
                    "protocol": "tun",
                    "settings": {
                        "name": "lumen_xray", "mtu": 1280,
                        "gateway": ["172.19.0.1/30", "fdfe:dcba:9877::1/126"],
                        "dns": ["172.19.0.2"],
                        "autoSystemRoutingTable": ["0.0.0.0/0", "::/0"],
                        "autoOutboundsInterface": "Ethernet"
                    },
                    "sniffing": {"enabled": true, "destOverride": ["http", "tls", "quic"], "routeOnly": false}
                }],
                "outbounds": [
                    {"tag": "proxy", "protocol": "vless", "streamSettings": {"sockopt": {"interface": "Ethernet"}}},
                    {"tag": "direct", "protocol": "freedom", "settings": {"domainStrategy": "UseIPv6"},
                     "streamSettings": {"sockopt": {"interface": "Custom"}}},
                    {"tag": "block", "protocol": "blackhole"},
                    {"tag": "dns-out", "protocol": "dns"}
                ],
                "routing": {"rules": [
                    {"type": "field", "inboundTag": ["tun-in"], "port": "53", "outboundTag": "dns-out"},
                    {"type": "field", "inboundTag": ["dns-internal"], "outboundTag": "direct"},
                    {"type": "field", "ip": ["192.168.50.0/24", "10.0.0.1/32", "2001:db8::/64", "::ffff:c000:200/120", "fe80::1%5/128"], "outboundTag": "direct"},
                    {"type": "field", "inboundTag": ["tun-in"], "network": "udp", "port": "443", "outboundTag": "block"},
                    {"type": "field", "network": "tcp,udp", "outboundTag": "direct"}
                ]},
                "dns": {
                    "servers": ["159.194.200.33", "https://1.1.1.1/dns-query", "https://8.8.8.8/dns-query"],
                    "queryStrategy": "UseIPv4", "tag": "dns-internal", "hosts": {"local.test": ["127.0.0.1"]}
                },
            })
        );
        Ok(())
    }

    #[test]
    fn uses_fallback_dns_and_respects_disabled_quic_blocking() -> anyhow::Result<()> {
        let mut payload = json!({});
        let settings = AppSettings {
            tun_mtu: 0,
            tun_block_quic: false,
            ..AppSettings::default()
        };
        let context = WindowsDefaultRouteContext {
            interface_alias: "lumen_xray".into(),
            ..WindowsDefaultRouteContext::default()
        };

        apply_xray_tun(
            &mut payload,
            &settings,
            &RoutingSettings::default(),
            Some(&context),
        )?;

        assert_eq!(payload["inbounds"][0]["settings"]["mtu"], 1500);
        assert_eq!(
            payload["inbounds"][0]["settings"]["autoOutboundsInterface"],
            "auto"
        );
        assert_eq!(
            payload["dns"]["servers"],
            json!(["https://1.1.1.1/dns-query", "https://8.8.8.8/dns-query"])
        );
        assert_eq!(
            payload["outbounds"][0]["settings"]["domainStrategy"],
            "UseIPv4"
        );
        assert!(payload["outbounds"][0].get("streamSettings").is_none());
        assert_eq!(payload["routing"]["rules"].as_array().unwrap().len(), 2);
        Ok(())
    }

    #[test]
    fn rejects_invalid_sections_before_mutating_config() {
        let mut payload = json!({"inbounds": [], "routing": {"rules": null}});
        let before = payload.clone();
        assert!(
            apply_xray_tun(
                &mut payload,
                &AppSettings::default(),
                &RoutingSettings::default(),
                None
            )
            .is_err()
        );
        assert_eq!(payload, before);
    }

    #[test]
    fn finds_only_a_file_beside_the_configured_executable() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let exe = dir.path().join("xray-custom.exe");
        let exe = exe.to_str().unwrap();
        let dll = dir.path().join("wintun.dll");

        assert_eq!(xray_wintun_dll(exe), None);
        std::fs::create_dir(&dll)?;
        assert_eq!(xray_wintun_dll(exe), None);
        std::fs::remove_dir(&dll)?;
        std::fs::write(&dll, [])?;
        assert_eq!(xray_wintun_dll(exe), Some(dll));
        Ok(())
    }
}
