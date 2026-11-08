//! Xray config assembly (`engines/xray/config_builder.py`).
//!
//! `routing` / `settings` are the `to_dict()` JSON payloads of
//! `RoutingSettings` / `AppSettings`; every read mirrors the Python attribute
//! defaults for missing keys.

use std::net::IpAddr;

use serde_json::{json, Map, Value};

use crate::constants::{
    DEFAULT_DISCORD_SOCKS_PORT, DEFAULT_XRAY_STATS_API_PORT, PROXY_HOST,
};
use crate::models::{bool_field, int_field, str_field, str_list, str_or, Node};

use super::multiplex::apply_xray_multiplex;
use super::xray_fragments::{apply_xray_final_fragment, apply_xray_outbound_fragment};
use super::xray_inbounds::{build_xray_http_compat_inbound, build_xray_mixed_inbound};

fn normalize_loglevel(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized == "warn" {
        return "warning".to_string();
    }
    if ["debug", "info", "warning", "error", "none"].contains(&normalized.as_str()) {
        return normalized;
    }
    "warning".to_string()
}

fn normalize_tls_certificate_pins(outbound: &mut Value) {
    let Some(stream) = outbound.get_mut("streamSettings").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let Some(tls) = stream.get_mut("tlsSettings").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let pins = match tls.get("pinnedPeerCertSha256") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .find(|item| !item.is_empty())
            .unwrap_or_default(),
        Some(Value::String(pins)) => pins.clone(),
        _ => return,
    };
    let value = pins.trim().to_lowercase();
    if value.is_empty() {
        tls.remove("pinnedPeerCertSha256");
    } else {
        tls.insert("pinnedPeerCertSha256".into(), Value::String(value));
    }
}

fn xray_dns_server(address: &str, transport: &str) -> String {
    let value = address.trim();
    let kind = if transport.is_empty() {
        "udp".to_string()
    } else {
        transport.trim().to_lowercase()
    };
    if value.is_empty() {
        return "8.8.8.8".to_string();
    }
    if value.contains("://") {
        return value.to_string();
    }
    match kind.as_str() {
        "https" => format!("https://{value}/dns-query"),
        "tls" => format!("tls://{value}"),
        "tcp" => format!("tcp://{value}"),
        _ => value.to_string(),
    }
}

fn xray_dns_strategy(values: &[&str]) -> String {
    let normalized: std::collections::HashSet<String> =
        values.iter().map(|value| value.trim().to_lowercase()).collect();
    if normalized.contains("ipv4_only") {
        "UseIPv4".to_string()
    } else if normalized.contains("ipv6_only") {
        "UseIPv6".to_string()
    } else {
        "UseIP".to_string()
    }
}

/// Python `ip_network(value, strict=False)` parse check: address plus an
/// in-range optional prefix.
fn is_ip_network(value: &str) -> bool {
    let (address, prefix) = match value.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (value, None),
    };
    let Ok(parsed) = address.parse::<IpAddr>() else {
        return false;
    };
    if let Some(prefix) = prefix {
        let Ok(prefix) = prefix.parse::<u32>() else {
            return false;
        };
        let max = match parsed {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > max {
            return false;
        }
    }
    true
}

/// Hostname extraction matching `urlsplit(value if "://" in value else
/// f"//{value}").hostname` for realistic DNS server strings.
fn url_hostname(value: &str) -> String {
    if value.contains("://") {
        return match url::Url::parse(value) {
            Ok(parsed) => parsed
                .host_str()
                .unwrap_or("")
                .to_lowercase(),
            Err(_) => String::new(),
        };
    }
    let netloc = value.trim_start_matches("//");
    let netloc = netloc.split(['/', '?', '#']).next().unwrap_or(netloc);
    let netloc = match netloc.rsplit_once('@') {
        Some((_, host)) => host,
        None => netloc,
    };
    let host = if let Some(rest) = netloc.strip_prefix('[') {
        match rest.split_once(']') {
            Some((inner, _)) => inner.to_string(),
            None => netloc.to_string(),
        }
    } else if netloc.matches(':').count() > 1 {
        // Bare IPv6 without brackets: Python's hostname parse yields "" here,
        // which the caller then skips.
        String::new()
    } else {
        match netloc.split_once(':') {
            Some((host, port)) => {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    host.to_string()
                } else {
                    netloc.to_string()
                }
            }
            None => netloc.to_string(),
        }
    };
    host.to_lowercase()
}

/// Python: `_xray_dns_entries(addresses, transport, *, domains=None)`.
fn xray_dns_entries(addresses: &[String], transport: &str, domains: &[String]) -> Vec<Value> {
    let mut entries: Vec<Value> = Vec::new();
    let mut domain_rules: Vec<String> = Vec::new();
    for raw in domains {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let lowered = value.to_lowercase();
        if lowered.starts_with("geoip:") || lowered.starts_with("ip:") {
            continue;
        }
        if value.starts_with("domain:")
            || value.starts_with("full:")
            || value.starts_with("regexp:")
            || value.starts_with("keyword:")
            || value.starts_with("geosite:")
            || value.starts_with("ext:")
        {
            domain_rules.push(value.to_string());
            continue;
        }
        if !is_ip_network(value) {
            domain_rules.push(format!("domain:{value}"));
        }
    }
    for address in addresses {
        let value = xray_dns_server(address, transport);
        if !domain_rules.is_empty() {
            entries.push(json!({
                "address": value,
                "domains": domain_rules.clone(),
                "skipFallback": true,
            }));
        } else {
            entries.push(Value::String(value));
        }
    }
    entries
}

/// Pin remote DNS transports to the proxy even in direct-fallback presets.
fn xray_proxy_dns_route_rule(addresses: &[String], transport: &str) -> Option<Value> {
    let mut domains: Vec<String> = Vec::new();
    let mut ips: Vec<String> = Vec::new();
    for address in addresses {
        let value = xray_dns_server(address, transport);
        let host = url_hostname(&value);
        let host = host.trim().trim_matches(|c| c == '[' || c == ']').to_string();
        if host.is_empty() {
            continue;
        }
        if host.parse::<IpAddr>().is_ok() {
            if !ips.contains(&host) {
                ips.push(host);
            }
        } else {
            let domain = format!("full:{host}");
            if !domains.contains(&domain) {
                domains.push(domain);
            }
        }
    }
    if domains.is_empty() && ips.is_empty() {
        return None;
    }
    let mut rule = json!({
        "type": "field",
        "network": "tcp,udp",
        "outboundTag": "proxy",
    });
    if !domains.is_empty() {
        rule["domain"] = json!(domains);
    }
    if !ips.is_empty() {
        rule["ip"] = json!(ips);
    }
    Some(rule)
}

/// Local mirror of `routing_runtime.collect_service_route_domains` operating on
/// the `RoutingSettings.to_dict()` value (the routing_runtime port keeps the
/// typed counterpart).
fn collect_service_route_domains(routing: &Value) -> (Vec<String>, Vec<String>, Vec<String>) {
    use crate::presets::SERVICE_PRESETS;
    use crate::routing_presets::{ROUTING_PRESET_BLOCKED, ROUTING_PRESET_BLOCKED_CN};

    let mut service_direct: Vec<String> = Vec::new();
    let mut service_proxy: Vec<String> = Vec::new();
    let service_block: Vec<String> = Vec::new();
    let preset_id = str_field(routing, "preset_id").trim().to_lowercase();
    let blocked_preset = preset_id == ROUTING_PRESET_BLOCKED || preset_id == ROUTING_PRESET_BLOCKED_CN;
    let service_routes = routing.get("service_routes").cloned().unwrap_or(Value::Null);
    for preset in SERVICE_PRESETS {
        // A missing per-service override inherits the built-in blocked preset's
        // catalog action. In every other preset it remains unmatched and uses
        // route.final instead of becoming a synthetic explicit service rule.
        let raw_action = service_routes.get(preset.id).filter(|v| !v.is_null());
        let mut action = match raw_action {
            Some(Value::String(s)) => s.trim().to_lowercase(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        if raw_action.is_none() && blocked_preset {
            action = preset.default_action.to_string();
        }
        if action != "direct" && action != "proxy" {
            continue;
        }
        if action == "direct" {
            service_direct.extend(preset.domains.iter().map(|d| d.to_string()));
        } else {
            service_proxy.extend(preset.domains.iter().map(|d| d.to_string()));
        }
    }
    (service_direct, service_proxy, service_block)
}

/// Build the full Xray runtime config for a node.
///
/// Python defaults: `api_port=0` (falls back to
/// `DEFAULT_XRAY_STATS_API_PORT`), `socks_port=DEFAULT_SOCKS_PORT`,
/// `http_port=DEFAULT_HTTP_PORT`.
pub fn build_xray_config(
    node: &Node,
    routing: &Value,
    settings: &Value,
    api_port: u16,
    socks_port: u16,
    http_port: u16,
) -> Value {
    let api_port = if api_port == 0 {
        DEFAULT_XRAY_STATS_API_PORT
    } else {
        api_port
    };
    let mut proxy_outbound = node.outbound.clone();
    if !proxy_outbound.is_object() {
        proxy_outbound = json!({});
    }
    proxy_outbound["tag"] = json!("proxy");
    normalize_tls_certificate_pins(&mut proxy_outbound);
    let multiplex_enabled = bool_field(settings, "multiplex_enabled", false);
    let multiplex_concurrency = int_field(settings, "multiplex_concurrency", 8);
    apply_xray_multiplex(&mut proxy_outbound, multiplex_enabled, multiplex_concurrency);

    let mut routing_rules: Vec<Value> = vec![json!({
        "type": "field",
        "inboundTag": ["api"],
        "outboundTag": "api",
    })];
    let discord_proxy_enabled = bool_field(settings, "discord_proxy_enabled", false);
    if discord_proxy_enabled {
        routing_rules.push(json!({
            "type": "field",
            "inboundTag": ["discord-socks-in"],
            "outboundTag": "proxy",
        }));
    }

    routing_rules.extend(crate::routing_runtime::build_xray_gui_routing_rules(routing, settings));

    let route_only = bool_field(settings, "sniff_route_only", false);
    let proxy_auth_enabled = bool_field(settings, "proxy_auth_enabled", false);
    let auth_username_raw = str_field(settings, "proxy_auth_username");
    let auth_password_raw = str_field(settings, "proxy_auth_password");
    let auth_enabled =
        proxy_auth_enabled && !auth_username_raw.trim().is_empty() && !auth_password_raw.is_empty();
    let auth_username = if auth_enabled {
        auth_username_raw.trim().to_string()
    } else {
        String::new()
    };
    let auth_password = if auth_enabled { auth_password_raw } else { String::new() };

    let mut inbounds: Vec<Value> = vec![build_xray_mixed_inbound(
        socks_port,
        "socks-in",
        route_only,
        &auth_username,
        &auth_password,
    )];

    let dns_mode = str_or(routing, "dns_mode", "builtin");
    if dns_mode == "builtin" {
        let dns_proxy_servers = str_list(routing, "dns_proxy_servers");
        let dns_proxy_server = str_or(routing, "dns_proxy_server", "cloudflare-dns.com");
        let proxy_servers: Vec<String> = if dns_proxy_servers.is_empty() {
            vec![dns_proxy_server]
        } else {
            dns_proxy_servers
        };
        let dns_proxy_type = str_or(routing, "dns_proxy_type", "https");
        if let Some(rule) = xray_proxy_dns_route_rule(&proxy_servers, &dns_proxy_type) {
            routing_rules.push(rule);
        }
    }

    if http_port != socks_port {
        inbounds.push(build_xray_http_compat_inbound(
            http_port,
            "http-in",
            route_only,
            &auth_username,
            &auth_password,
        ));
    }
    if discord_proxy_enabled {
        inbounds.push(json!({
            "tag": "discord-socks-in",
            "listen": PROXY_HOST,
            "port": DEFAULT_DISCORD_SOCKS_PORT,
            "protocol": "socks",
            "settings": {
                "auth": "noauth",
                "udp": true,
            },
            "sniffing": {
                "enabled": true,
                "destOverride": ["http", "tls"],
                "routeOnly": false,
            },
        }));
    }
    inbounds.push(json!({
        "tag": "api",
        "listen": PROXY_HOST,
        "port": api_port,
        "protocol": "dokodemo-door",
        "settings": {
            "address": PROXY_HOST,
        },
    }));

    let allow_lan = bool_field(settings, "proxy_allow_lan", false);
    if allow_lan {
        for inbound in inbounds.iter_mut() {
            // v2rayN-style Allow LAN for local proxy ports
            if str_field(inbound, "tag") != "api" {
                inbound["listen"] = json!("0.0.0.0");
            }
        }
    }

    let mut config = json!({
        "log": {
            "loglevel": normalize_loglevel(&str_or(settings, "log_level", "warning")),
        },
        "inbounds": inbounds,
        "outbounds": [
            proxy_outbound,
            {
                "tag": "direct",
                "protocol": "freedom",
                "settings": {},
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": {},
            },
            {
                "tag": "api",
                "protocol": "freedom",
                "settings": {},
            },
        ],
        "policy": {
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true,
            }
        },
        "stats": {},
        "api": {
            "tag": "api",
            "services": ["StatsService"],
        },
        "routing": {
            "domainStrategy": "AsIs",
            "rules": routing_rules,
        },
    });

    if dns_mode == "builtin" {
        let preset_id = str_field(routing, "preset_id");
        let (preset_direct, preset_proxy) = crate::routing_presets::preset_domain_rules(&preset_id);
        let (service_direct, service_proxy, _service_block) = collect_service_route_domains(routing);
        let mut direct_domains = service_direct;
        direct_domains.extend(crate::routing_presets::custom_domain_rules(&str_list(routing, "direct_domains")));
        let mut proxy_domains = service_proxy;
        proxy_domains.extend(crate::routing_presets::custom_domain_rules(&str_list(routing, "proxy_domains")));
        if bool_field(routing, "dns_geo_check", true) {
            direct_domains.extend(preset_direct);
            proxy_domains.extend(preset_proxy);
        }
        let bootstrap_servers: Vec<String> = str_list(routing, "dns_bootstrap_servers")
            .into_iter()
            .filter(|item| !item.trim().is_empty())
            .map(|item| item.trim().to_string())
            .collect();
        let dns_proxy_servers = str_list(routing, "dns_proxy_servers");
        let dns_proxy_server = str_or(routing, "dns_proxy_server", "cloudflare-dns.com");
        let proxy_servers: Vec<String> = if dns_proxy_servers.is_empty() {
            vec![dns_proxy_server]
        } else {
            dns_proxy_servers
        };
        let dns_bootstrap_type = str_or(routing, "dns_bootstrap_type", "https");
        let dns_proxy_type = str_or(routing, "dns_proxy_type", "https");

        let mut servers: Vec<Value> = Vec::new();
        if !direct_domains.is_empty() {
            servers.extend(xray_dns_entries(&bootstrap_servers, &dns_bootstrap_type, &direct_domains));
        }
        if !proxy_domains.is_empty() {
            servers.extend(xray_dns_entries(&proxy_servers, &dns_proxy_type, &proxy_domains));
        }
        // The final resolver must also be proxied.  `localhost` here
        // handed every unmatched query to the Windows/ISP resolver and
        // was the source of the leak visible on browser DNS tests.
        servers.extend(xray_dns_entries(&proxy_servers, &dns_proxy_type, &[]));

        let mut dns_config = json!({
            "servers": servers,
            "queryStrategy": xray_dns_strategy(&[
                &str_or(routing, "dns_proxy_strategy", "ipv4_only"),
                &str_or(routing, "dns_bootstrap_strategy", "ipv4_only"),
            ]),
        });
        if bool_field(routing, "dns_parallel_query", false) {
            dns_config["enableParallelQuery"] = json!(true);
        }
        if bool_field(routing, "dns_optimistic_cache", false) {
            dns_config["serveStale"] = json!(true);
            dns_config["serveExpiredTTL"] = json!(86400);
        }
        if let Some(hosts) = routing.get("dns_hosts").and_then(|v| v.as_object()) {
            if !hosts.is_empty() {
                let mut map = Map::new();
                for (domain, addresses) in hosts {
                    let list: Vec<String> = addresses
                        .as_array()
                        .map(|items| items.iter().map(value_string).collect())
                        .unwrap_or_default();
                    if list.len() == 1 {
                        map.insert(domain.clone(), json!(list[0]));
                    } else {
                        map.insert(domain.clone(), json!(list));
                    }
                }
                dns_config["hosts"] = Value::Object(map);
            }
        }
        config["dns"] = dns_config;
    }

    let fragment_packets = str_or(settings, "fragment_packets", "tlshello");
    let fragment_length = str_or(settings, "fragment_length", "50-100");
    let fragment_delay = str_or(settings, "fragment_delay", "10-20");
    let tail_fragment = bool_field(settings, "tail_fragment_enabled", false);
    if bool_field(settings, "enable_xray_fragment", false) {
        apply_xray_outbound_fragment(
            &mut config,
            &fragment_packets,
            &fragment_length,
            &fragment_delay,
            tail_fragment,
        );
    }
    if bool_field(settings, "enable_final_fragment", false) {
        apply_xray_final_fragment(
            &mut config,
            "proxy",
            &fragment_packets,
            &fragment_length,
            &fragment_delay,
            tail_fragment,
        );
    }

    config
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vless_node() -> Node {
        Node {
            id: "n1".into(),
            name: "test node".into(),
            server: "example.com".into(),
            port: 443,
            outbound: json!({
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": "example.com",
                        "port": 443,
                        "users": [{"id": "3f1b…uuid", "encryption": "none", "flow": ""}],
                    }],
                },
                "streamSettings": {
                    "security": "tls",
                    "tlsSettings": {"serverName": "example.com"},
                },
            }),
            ..Default::default()
        }
    }

    fn routing_value() -> Value {
        json!({"dns_mode": "system", "preset_id": "global"})
    }

    fn settings_value() -> Value {
        json!({"log_level": "warning"})
    }

    #[test]
    fn builds_vless_config_with_inbounds_and_api_block() {
        let config = build_xray_config(
            &vless_node(),
            &routing_value(),
            &settings_value(),
            0,
            10808,
            10809,
        );
        let inbounds = config["inbounds"].as_array().unwrap();
        assert_eq!(inbounds[0]["tag"], json!("socks-in"));
        assert_eq!(inbounds[0]["port"], json!(10808));
        assert_eq!(inbounds[0]["protocol"], json!("mixed"));
        assert_eq!(inbounds[1]["tag"], json!("http-in"));
        assert_eq!(inbounds[1]["port"], json!(10809));
        let api_inbound = inbounds.last().unwrap();
        assert_eq!(api_inbound["tag"], json!("api"));
        assert_eq!(api_inbound["protocol"], json!("dokodemo-door"));
        assert_eq!(api_inbound["port"], json!(19085)); // default stats api port

        let outbounds = config["outbounds"].as_array().unwrap();
        assert_eq!(outbounds[0]["tag"], json!("proxy"));
        assert_eq!(outbounds[0]["protocol"], json!("vless"));
        assert_eq!(outbounds[1]["tag"], json!("direct"));
        assert_eq!(outbounds[2]["tag"], json!("block"));
        assert_eq!(outbounds[3]["tag"], json!("api"));

        assert_eq!(config["api"]["services"], json!(["StatsService"]));
        assert_eq!(config["routing"]["domainStrategy"], json!("AsIs"));
        let rules = config["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["inboundTag"], json!(["api"]));
        assert_eq!(rules[0]["outboundTag"], json!("api"));
        assert_eq!(config["log"]["loglevel"], json!("warning"));
        assert!(config.get("dns").is_none());
        assert_eq!(
            config["policy"]["system"]["statsInboundUplink"],
            json!(true)
        );
    }

    #[test]
    fn same_ports_drop_http_compat_inbound() {
        let config = build_xray_config(&vless_node(), &routing_value(), &settings_value(), 0, 10808, 10808);
        let tags: Vec<&str> = config["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["tag"].as_str())
            .collect();
        assert!(!tags.contains(&"http-in"));
    }

    #[test]
    fn multiplex_inserted_into_proxy_outbound() {
        let settings = json!({"multiplex_enabled": true, "multiplex_concurrency": 12});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        let proxy = &config["outbounds"][0];
        assert_eq!(proxy["mux"]["enabled"], json!(true));
        assert_eq!(proxy["mux"]["concurrency"], json!(12));

        // Explicitly disabled writes the mux-off marker.
        let settings = json!({"multiplex_enabled": false});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        assert_eq!(config["outbounds"][0]["mux"]["concurrency"], json!(-1));
    }

    #[test]
    fn allow_lan_listen_override() {
        let settings = json!({"proxy_allow_lan": true, "discord_proxy_enabled": true});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        for inbound in config["inbounds"].as_array().unwrap() {
            if inbound["tag"] == json!("api") {
                assert_eq!(inbound["listen"], json!(PROXY_HOST));
            } else {
                assert_eq!(inbound["listen"], json!("0.0.0.0"));
            }
        }
        assert!(config["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["tag"] == json!("discord-socks-in")));
    }

    #[test]
    fn final_fragment_wrapper_shape() {
        let settings = json!({"enable_final_fragment": true, "tail_fragment_enabled": true});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        let outbounds = config["outbounds"].as_array().unwrap();
        assert_eq!(outbounds[0]["protocol"], json!("freedom"));
        assert_eq!(outbounds[0]["tag"], json!("proxy"));
        assert_eq!(
            outbounds[0]["streamSettings"]["sockopt"]["dialerProxy"],
            json!("fragment-proxy")
        );
        assert_eq!(outbounds[0]["streamSettings"]["finalmask"]["tcp"].as_array().unwrap().len(), 2);
        assert_eq!(outbounds[1]["tag"], json!("fragment-proxy"));

        // Outbound fragment mode patches in place instead of wrapping.
        let settings = json!({"enable_xray_fragment": true});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        let outbounds = config["outbounds"].as_array().unwrap();
        assert_eq!(outbounds[0]["protocol"], json!("vless"));
        assert!(outbounds[0]["streamSettings"]["finalmask"]["tcp"].is_array());
    }

    #[test]
    fn builtin_dns_block_shape() {
        let routing = json!({
            "dns_mode": "builtin",
            "preset_id": "blocked",
            "dns_geo_check": true,
            "dns_bootstrap_servers": ["77.88.8.8"],
            "dns_bootstrap_type": "udp",
            "dns_bootstrap_strategy": "ipv4_only",
            "dns_proxy_servers": ["dns.google"],
            "dns_proxy_type": "https",
            "dns_proxy_strategy": "ipv4_only",
            "dns_parallel_query": true,
            "dns_optimistic_cache": true,
            "dns_hosts": {"router.local": ["192.168.1.1"]},
            "direct_domains": ["example.org"],
            "proxy_domains": ["geoip:ru-blocked", "bad-site.com", "10.0.0.0/8"],
        });
        let config = build_xray_config(&vless_node(), &routing, &settings_value(), 0, 10808, 10809);
        let dns = &config["dns"];
        assert_eq!(dns["queryStrategy"], json!("UseIPv4"));
        assert_eq!(dns["enableParallelQuery"], json!(true));
        assert_eq!(dns["serveStale"], json!(true));
        assert_eq!(dns["serveExpiredTTL"], json!(86400));
        assert_eq!(dns["hosts"]["router.local"], json!("192.168.1.1"));

        let servers = dns["servers"].as_array().unwrap();
        // Bootstrap (with direct domains) + proxy (with proxy domains) + unpinned proxy.
        assert!(servers.len() >= 3);
        // Bootstrap entries carry direct domains with skipFallback.
        let bootstrap = &servers[0];
        assert_eq!(bootstrap["address"], json!("77.88.8.8"));
        assert_eq!(bootstrap["skipFallback"], json!(true));
        let domains = bootstrap["domains"].as_array().unwrap();
        assert!(domains.iter().any(|d| d == "domain:example.org"));
        // Proxy DNS server normalized to a DoH URL.
        let proxy_dns = servers.last().unwrap();
        assert_eq!(proxy_dns, &json!("https://dns.google/dns-query"));

        // The proxy DNS route rule pins dns.google to the proxy outbound.
        let rules = config["routing"]["rules"].as_array().unwrap();
        assert!(rules.iter().any(|rule| rule["domain"] == json!(["full:dns.google"])
            && rule["outboundTag"] == json!("proxy")
            && rule["network"] == json!("tcp,udp")));
    }

    #[test]
    fn tls_cert_pins_normalized() {
        let mut node = vless_node();
        node.outbound["streamSettings"]["tlsSettings"]["pinnedPeerCertSha256"] =
            json!([" ABCDEF ", ""]);
        let config = build_xray_config(&node, &routing_value(), &settings_value(), 0, 10808, 10809);
        assert_eq!(
            config["outbounds"][0]["streamSettings"]["tlsSettings"]["pinnedPeerCertSha256"],
            json!("abcdef")
        );

        node.outbound["streamSettings"]["tlsSettings"]["pinnedPeerCertSha256"] = json!([""]);
        let config = build_xray_config(&node, &routing_value(), &settings_value(), 0, 10808, 10809);
        assert!(config["outbounds"][0]["streamSettings"]["tlsSettings"]
            .get("pinnedPeerCertSha256")
            .is_none());
    }

    #[test]
    fn auth_credentials_reach_inbounds() {
        let settings = json!({
            "proxy_auth_enabled": true,
            "proxy_auth_username": " user ",
            "proxy_auth_password": "secret",
        });
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        let mixed = &config["inbounds"][0];
        assert_eq!(mixed["settings"]["auth"], json!("password"));
        assert_eq!(mixed["settings"]["accounts"][0]["user"], json!("user"));
        assert_eq!(mixed["settings"]["accounts"][0]["pass"], json!("secret"));
    }

    #[test]
    fn discord_routing_rule_and_inbound() {
        let settings = json!({"discord_proxy_enabled": true});
        let config = build_xray_config(&vless_node(), &routing_value(), &settings, 0, 10808, 10809);
        let rules = config["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules[1]["inboundTag"], json!(["discord-socks-in"]));
        assert_eq!(rules[1]["outboundTag"], json!("proxy"));
        let discord = config["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == json!("discord-socks-in"))
            .unwrap();
        assert_eq!(discord["port"], json!(10818));
        assert_eq!(discord["protocol"], json!("socks"));
    }
}
