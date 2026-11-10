//! DNS runtime contract (`_ensure_singbox_dns_runtime_contract` and friends).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::net::IpAddr;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::{
    ensure_dict, ensure_list, ensure_map_list, is_dhcp_auto_dns, is_domain_name, ip_is_private,
    json_str, json_str_field, lower_trim, py_str_list_item, replace_or_append_tagged,
};
use lumen_core::models::{Node, RoutingSettings};

pub const DEFAULT_DIRECT_DNS_SERVER: &str = "1.1.1.1";
pub const DEFAULT_DIRECT_DNS_TYPE: &str = "https";
pub const DEFAULT_PROXY_DNS_SERVER: &str = "cloudflare-dns.com";
pub const DEFAULT_PROXY_DNS_TYPE: &str = "https";

/// Python `_KNOWN_DOH_IP_HOSTS`.
const KNOWN_DOH_IP_HOSTS: &[(&str, &str)] = &[
    ("1.0.0.1", "cloudflare-dns.com"),
    ("1.1.1.1", "cloudflare-dns.com"),
    ("8.8.4.4", "dns.google"),
    ("8.8.8.8", "dns.google"),
    ("9.9.9.9", "dns.quad9.net"),
    ("149.112.112.112", "dns.quad9.net"),
];

/// Python `_BROWSER_DOH_DOMAIN_SUFFIXES`.
pub const BROWSER_DOH_DOMAIN_SUFFIXES: [&str; 13] = [
    "cloudflare-dns.com",
    "dns.adguard.com",
    "dns.alidns.com",
    "dns.cloudflare.com",
    "dns.google",
    "dns.nextdns.io",
    "dns.quad9.net",
    "doh.dns.sb",
    "doh.opendns.com",
    "doh.pub",
    "doh.sb",
    "mozilla.cloudflare-dns.com",
    "one.one.one.one",
];

static EMPTY_HOSTS: BTreeMap<String, Vec<String>> = BTreeMap::new();

fn known_doh_host(address: &str) -> Option<&'static str> {
    KNOWN_DOH_IP_HOSTS
        .iter()
        .find(|(ip, _)| *ip == address)
        .map(|(_, hostname)| *hostname)
}

/// Python `_dns_strategy`.
pub fn dns_strategy(value: &str) -> String {
    let strategy = lower_trim(value).replace('-', "_");
    if matches!(
        strategy.as_str(),
        "prefer_ipv4" | "prefer_ipv6" | "ipv4_only" | "ipv6_only"
    ) {
        strategy
    } else {
        "prefer_ipv4".to_string()
    }
}

/// Python `_is_default_direct_dns`.
pub fn is_default_direct_dns(routing: Option<&RoutingSettings>) -> bool {
    match routing {
        None => true,
        Some(routing) => {
            routing.dns_bootstrap_server.trim().to_lowercase() == DEFAULT_DIRECT_DNS_SERVER
                && routing.dns_bootstrap_type.trim().to_lowercase() == DEFAULT_DIRECT_DNS_TYPE
        }
    }
}

/// Python `_is_default_proxy_dns`.
pub fn is_default_proxy_dns(routing: Option<&RoutingSettings>) -> bool {
    match routing {
        None => true,
        Some(routing) => {
            routing.dns_proxy_server.trim().to_lowercase() == DEFAULT_PROXY_DNS_SERVER
                && routing.dns_proxy_type.trim().to_lowercase() == DEFAULT_PROXY_DNS_TYPE
        }
    }
}

/// Python `_build_dns_server` (the `strategy` parameter is accepted for
/// signature parity and unused, exactly like the Python body).
pub fn build_dns_server(tag: &str, server: &str, server_type: &str, strategy: &str) -> Value {
    let _ = strategy;
    let mut dns_type = lower_trim(server_type);
    let address = {
        let trimmed = server.trim();
        if trimmed.is_empty() {
            if tag == "proxy-dns" { "8.8.8.8" } else { "1.1.1.1" }.to_string()
        } else {
            trimmed.to_string()
        }
    };
    if is_dhcp_auto_dns(&address) {
        return json!({
            "tag": tag,
            "type": "dhcp",
        });
    }
    if !matches!(dns_type.as_str(), "udp" | "tcp" | "tls" | "https") {
        dns_type = if tag == "proxy-dns" { "https" } else { "udp" }.to_string();
    }
    let mut payload = json!({
        "tag": tag,
        "type": dns_type,
        "server": address.clone(),
    });
    let map = payload.as_object_mut().unwrap();
    match dns_type.as_str() {
        "https" => {
            map.entry("server_port".to_string()).or_insert(json!(443));
            map.entry("path".to_string()).or_insert(json!("/dns-query"));
            if let Some(known_tls_name) = known_doh_host(&address.to_lowercase()) {
                map.insert("tls".to_string(), json!({"server_name": known_tls_name}));
                if !tag.starts_with("proxy-dns") {
                    // Direct DoH by IP is visible to the ISP: no SNI in the
                    // ClientHello, the certificate is still checked for the name.
                    if let Some(tls) = map.get_mut("tls").and_then(Value::as_object_mut) {
                        tls.insert("disable_sni".to_string(), json!(true));
                    }
                }
            }
        }
        "tls" => {
            map.entry("server_port".to_string()).or_insert(json!(853));
        }
        "udp" | "tcp" => {
            map.entry("server_port".to_string()).or_insert(json!(53));
        }
        _ => {}
    }
    payload
}

/// Python `_set_dns_server_dial_contract`.
pub fn set_dns_server_dial_contract(server: &mut Value, detour: &str, resolver: &str) {
    server["detour"] = json!(detour);
    let address = json_str_field(server, "server");
    if is_domain_name(&address) {
        server["domain_resolver"] = json!(if resolver.is_empty() { "system-dns" } else { resolver });
    } else if let Some(map) = server.as_object_mut() {
        map.remove("domain_resolver");
    }
}

/// Python `_apply_imported_proxy_dns`: use an internal profile DNS without
/// replacing the configured resolver.
///
/// Clash WireGuard/AWG profiles often carry public UDP resolvers such as
/// 1.1.1.1.  Some providers block UDP/53 inside the tunnel, so replacing the
/// user's working DoH resolver with that hint makes the endpoint appear dead
/// even after a successful handshake.  Keep the configured proxy resolver;
/// only a private profile DNS may override the default resolver, and never a
/// resolver explicitly customized by the user.
pub fn apply_imported_proxy_dns(payload: &mut Value, node: &Node, routing: Option<&RoutingSettings>) {
    let node_outbound = if node.outbound.is_object() {
        node.outbound.clone()
    } else {
        json!({})
    };
    let mut addresses: Vec<String> = Vec::new();
    if let Some(Value::Array(raw_values)) = node_outbound.get("_dns") {
        for item in raw_values {
            let text = py_str_list_item(item).trim().to_string();
            if !text.is_empty() {
                addresses.push(text);
            }
        }
    }
    // Python `list(dict.fromkeys(addresses))`.
    let mut unique: Vec<String> = Vec::new();
    for address in addresses {
        if !unique.contains(&address) {
            unique.push(address);
        }
    }
    let addresses = unique;

    if !payload.get("dns").is_some_and(Value::is_object) {
        return;
    }
    if !payload["dns"].get("servers").is_some_and(Value::is_array) {
        return;
    }

    let configured_proxy_dns = payload["dns"]["servers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|server| {
            server.is_object() && {
                let tag = json_str_field(server, "tag");
                tag == "proxy-dns" || tag.starts_with("proxy-dns-")
            }
        })
        .count();
    let mut private_dns = String::new();
    for address in &addresses {
        if let Ok(parsed) = address.parse::<IpAddr>() {
            if ip_is_private(parsed) {
                private_dns = address.clone();
                break;
            }
        }
    }

    if configured_proxy_dns > 0
        && (private_dns.is_empty()
            || routing.map(|routing| !is_default_proxy_dns(Some(routing))).unwrap_or(false))
    {
        return;
    }
    let selected_dns = if !private_dns.is_empty() {
        private_dns
    } else {
        addresses.first().cloned().unwrap_or_default()
    };
    if selected_dns.is_empty() {
        return;
    }

    let servers = ensure_map_list(payload.get_mut("dns").unwrap().as_object_mut().unwrap(), "servers");
    servers.retain(|server| {
        !(server.is_object() && {
            let tag = json_str_field(server, "tag");
            tag == "proxy-dns" || tag.starts_with("proxy-dns-")
        })
    });
    let strategy = dns_strategy(routing.map(|routing| routing.dns_proxy_strategy.as_str()).unwrap_or(""));
    let mut server = build_dns_server("proxy-dns", &selected_dns, "udp", &strategy);
    set_dns_server_dial_contract(&mut server, "proxy", "bootstrap-dns");
    servers.push(server);
}

/// Python `_singbox_uses_builtin_dns`.
pub fn singbox_uses_builtin_dns(routing: Option<&RoutingSettings>) -> bool {
    let mode = routing.map(|routing| routing.dns_mode.as_str()).unwrap_or("builtin");
    lower_trim(mode) == "builtin"
}

/// Python `_ensure_singbox_dns_runtime_contract`.
pub fn ensure_singbox_dns_runtime_contract(
    payload: &mut Value,
    routing: Option<&RoutingSettings>,
    system_dns_servers: &[String],
) {
    let direct_server = routing
        .map(|routing| routing.dns_bootstrap_server.clone())
        .unwrap_or_else(|| DEFAULT_DIRECT_DNS_SERVER.to_string());
    let direct_type = routing
        .map(|routing| routing.dns_bootstrap_type.clone())
        .unwrap_or_else(|| DEFAULT_DIRECT_DNS_TYPE.to_string());
    let direct_strategy = dns_strategy(routing.map(|routing| routing.dns_bootstrap_strategy.as_str()).unwrap_or(""));
    let proxy_server = routing
        .map(|routing| routing.dns_proxy_server.clone())
        .unwrap_or_else(|| DEFAULT_PROXY_DNS_SERVER.to_string());
    let proxy_type = routing
        .map(|routing| routing.dns_proxy_type.clone())
        .unwrap_or_else(|| DEFAULT_PROXY_DNS_TYPE.to_string());
    let proxy_strategy = dns_strategy(routing.map(|routing| routing.dns_proxy_strategy.as_str()).unwrap_or(""));
    let direct_servers_raw: Vec<String> = routing
        .map(|routing| routing.dns_bootstrap_servers.clone())
        .unwrap_or_else(|| vec![direct_server.clone()]);
    let proxy_servers_raw: Vec<String> = routing
        .map(|routing| routing.dns_proxy_servers.clone())
        .unwrap_or_else(|| vec![proxy_server.clone()]);
    let mut direct_servers: Vec<String> = direct_servers_raw
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if direct_servers.is_empty() {
        direct_servers = vec![direct_server.clone()];
    }
    let mut proxy_servers: Vec<String> = proxy_servers_raw
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if proxy_servers.is_empty() {
        proxy_servers = vec![proxy_server.clone()];
    }
    let use_builtin_dns = singbox_uses_builtin_dns(routing);
    // FakeIP follows the user setting (off by default, matching v2rayN). When
    // enabled it keeps ECH sites reachable; when disabled the browser resolves
    // real IPs, avoiding Chrome Local Network Access prompts under TUN.
    let fake_enabled = routing.map(|routing| routing.dns_fake_enabled).unwrap_or(false);
    let proxy_is_default = is_default_proxy_dns(routing);
    let runtime_direct_server = direct_server.trim().to_string();
    let mut runtime_direct_type = direct_type.trim().to_string();
    let mut runtime_proxy_server = proxy_server.trim().to_string();
    let mut runtime_proxy_type = proxy_type.trim().to_string();
    if proxy_is_default {
        runtime_proxy_server = DEFAULT_PROXY_DNS_SERVER.to_string();
        runtime_proxy_type = DEFAULT_PROXY_DNS_TYPE.to_string();
    }
    // Migrate the historical default (Cloudflare over UDP) at runtime too, so
    // already-running installs with a serialized legacy setting cannot send
    // plaintext DNS to an Iranian ISP that transparently intercepts port 53.
    if runtime_direct_server.to_lowercase() == DEFAULT_DIRECT_DNS_SERVER
        && runtime_direct_type.to_lowercase() == "udp"
    {
        runtime_direct_type = "https".to_string();
    }

    let hosts: &BTreeMap<String, Vec<String>> = routing.map(|routing| &routing.dns_hosts).unwrap_or(&EMPTY_HOSTS);
    let hosts_present = !hosts.is_empty();

    {
        let dns = ensure_dict(payload, "dns");
        // sing-box 1.14 keys DNS cache entries by transport automatically;
        // independent_cache is deprecated and will be removed in 1.16.
        dns.insert("strategy".to_string(), json!(direct_strategy));
        dns.insert("reverse_mapping".to_string(), json!(true));
        let servers = ensure_map_list(dns, "servers");
        servers.retain(|server| {
            !(server.is_object() && {
                let tag = json_str_field(server, "tag");
                tag.starts_with("direct-dns-") || tag.starts_with("proxy-dns-") || tag == "hosts-dns"
            })
        });
        let mut system_server = system_dns_servers
            .iter()
            .map(|item| item.trim().to_string())
            .find(|item| !item.is_empty())
            .unwrap_or_default();
        if system_server.is_empty() {
            system_server = if !is_domain_name(&runtime_direct_server) {
                runtime_direct_server.clone()
            } else {
                DEFAULT_DIRECT_DNS_SERVER.to_string()
            };
        }
        let system_type = if !system_dns_servers.is_empty() || is_domain_name(&runtime_direct_server) {
            "udp".to_string()
        } else {
            runtime_direct_type.clone()
        };
        if use_builtin_dns {
            let mut system_dns = build_dns_server("system-dns", &system_server, &system_type, &direct_strategy);
            set_dns_server_dial_contract(&mut system_dns, "direct", "");
            replace_or_append_tagged(servers, "system-dns", &system_dns);
            let mut bootstrap_dns =
                build_dns_server("bootstrap-dns", &runtime_direct_server, &runtime_direct_type, &direct_strategy);
            set_dns_server_dial_contract(&mut bootstrap_dns, "direct", "");
            let mut direct_dns =
                build_dns_server("direct-dns", &runtime_direct_server, &runtime_direct_type, &direct_strategy);
            set_dns_server_dial_contract(&mut direct_dns, "direct", "");
            replace_or_append_tagged(servers, "bootstrap-dns", &bootstrap_dns);
            replace_or_append_tagged(servers, "direct-dns", &direct_dns);
        } else {
            // In system mode keep direct/bootstrap DNS pinned to the physical
            // adapter snapshot captured before TUN starts. dhcp://auto can pick the
            // just-created virtual adapter on Windows and recurse into the tunnel.
            let mut bootstrap_dns = build_dns_server("bootstrap-dns", &system_server, &system_type, &direct_strategy);
            let mut direct_dns = build_dns_server("direct-dns", &system_server, &system_type, &direct_strategy);
            set_dns_server_dial_contract(&mut bootstrap_dns, "direct", "");
            set_dns_server_dial_contract(&mut direct_dns, "direct", "");
            replace_or_append_tagged(servers, "bootstrap-dns", &bootstrap_dns);
            replace_or_append_tagged(servers, "direct-dns", &direct_dns);
        }
        let mut proxy_dns = build_dns_server("proxy-dns", &runtime_proxy_server, &runtime_proxy_type, &proxy_strategy);
        set_dns_server_dial_contract(&mut proxy_dns, "proxy", "bootstrap-dns");
        replace_or_append_tagged(servers, "proxy-dns", &proxy_dns);
        if use_builtin_dns {
            for (offset, address) in direct_servers.iter().skip(1).enumerate() {
                let index = offset + 2;
                let tag = format!("direct-dns-{index}");
                let mut extra_direct = build_dns_server(&tag, address, &runtime_direct_type, &direct_strategy);
                set_dns_server_dial_contract(&mut extra_direct, "direct", "");
                replace_or_append_tagged(servers, &tag, &extra_direct);
            }
        }
        for (offset, address) in proxy_servers.iter().skip(1).enumerate() {
            let index = offset + 2;
            let tag = format!("proxy-dns-{index}");
            let mut extra_proxy = build_dns_server(&tag, address, &runtime_proxy_type, &proxy_strategy);
            set_dns_server_dial_contract(&mut extra_proxy, "proxy", "bootstrap-dns");
            replace_or_append_tagged(servers, &tag, &extra_proxy);
        }

        if hosts_present {
            let predefined: Map<String, Value> = hosts
                .iter()
                .map(|(domain, addresses)| (domain.clone(), json!(addresses)))
                .collect();
            replace_or_append_tagged(
                servers,
                "hosts-dns",
                &json!({
                    "tag": "hosts-dns",
                    "type": "hosts",
                    "predefined": predefined,
                }),
            );
        }
        if fake_enabled {
            replace_or_append_tagged(
                servers,
                "fake-dns",
                &json!({
                    "tag": "fake-dns",
                    "type": "fakeip",
                    "inet4_range": "198.18.0.0/15",
                    "inet6_range": "fc00::/18",
                }),
            );
        } else {
            servers.retain(|server| !(server.is_object() && json_str_field(server, "tag") == "fake-dns"));
        }

        for server in servers.iter_mut() {
            if !server.is_object() {
                continue;
            }
            let tag = json_str_field(server, "tag");
            let server_type = lower_trim(&json_str_field(server, "type"));
            let address = json_str_field(server, "server").trim().to_lowercase();
            if tag.starts_with("proxy-dns")
                && matches!(server_type.as_str(), "tls" | "https")
                && known_doh_host(&address).is_some()
            {
                // Remote/proxied DNS must use a hostname with a valid TLS identity.
                // DoH/DoT to a bare IP can silently break domains because the TLS
                // handshake has no provider hostname/SNI to present.
                let hostname = known_doh_host(&address).unwrap();
                let mut replacement = json!({
                    "tag": tag,
                    "type": server_type,
                    "server": hostname,
                    "server_port": if server_type == "https" { 443 } else { 853 },
                    "domain_resolver": "bootstrap-dns",
                    "detour": "proxy",
                });
                if server_type == "https" {
                    replacement["path"] = json!("/dns-query");
                }
                *server = replacement;
            }
        }
    }

    let dns_value = payload.get_mut("dns").unwrap();
    ensure_singbox_https_dns_reject_rule(dns_value);
    ensure_singbox_hosts_dns_rule(dns_value, hosts_present);
}

// DNS query types 64 (SVCB) and 65 (HTTPS) carry ECH config and HTTP/3 hints.
// Answering them lets browsers encrypt the SNI (breaking sniff) and prefer QUIC.
// Rejecting them forces a clean fallback to A/AAAA (FakeIP) and TCP, which is how
// v2rayN-style TUN profiles keep ECH-enabled sites (OpenAI/Anthropic/Google)
// reachable.
fn https_dns_reject_rule() -> Value {
    json!({"query_type": ["HTTPS", "SVCB"], "action": "reject"})
}

/// Python `_is_https_dns_reject_rule`.
pub fn is_https_dns_reject_rule(rule: &Value) -> bool {
    if !rule.is_object() || json_str_field(rule, "action") != "reject" {
        return false;
    }
    let Some(Value::Array(query_type)) = rule.get("query_type") else {
        return false;
    };
    let names: BTreeSet<String> = query_type
        .iter()
        .map(|item| py_str_list_item(item).trim().to_uppercase())
        .collect();
    names == BTreeSet::from(["HTTPS".to_string(), "SVCB".to_string()])
}

/// Python `_is_browser_doh_dns_reject_rule`.
pub fn is_browser_doh_dns_reject_rule(rule: &Value) -> bool {
    if !rule.is_object() || json_str_field(rule, "action") != "reject" {
        return false;
    }
    let Some(Value::Array(suffixes)) = rule.get("domain_suffix") else {
        return false;
    };
    let items: Vec<String> = suffixes.iter().map(py_str_list_item).collect();
    items == BROWSER_DOH_DOMAIN_SUFFIXES.iter().map(|suffix| suffix.to_string()).collect::<Vec<_>>()
}

/// Python `_ensure_singbox_https_dns_reject_rule`.
pub fn ensure_singbox_https_dns_reject_rule(dns: &mut Value) {
    let rules = ensure_list(dns, "rules");
    rules.retain(|rule| !is_https_dns_reject_rule(rule) && !is_browser_doh_dns_reject_rule(rule));
    rules.insert(
        0,
        json!({
            "domain_suffix": BROWSER_DOH_DOMAIN_SUFFIXES,
            "action": "reject",
        }),
    );
    rules.insert(0, https_dns_reject_rule());
}

/// Python `_is_singbox_hosts_dns_rule`.
pub fn is_singbox_hosts_dns_rule(rule: &Value) -> bool {
    rule.is_object()
        && rule.get("ip_accept_any") == Some(&Value::Bool(true))
        && json_str_field(rule, "server") == "hosts-dns"
}

/// Python `_ensure_singbox_hosts_dns_rule`.
pub fn ensure_singbox_hosts_dns_rule(dns: &mut Value, enabled: bool) {
    let rules = ensure_list(dns, "rules");
    rules.retain(|rule| !is_singbox_hosts_dns_rule(rule));
    if enabled {
        rules.insert(0, json!({"ip_accept_any": true, "server": "hosts-dns"}));
    }
}

/// Python `_extract_dns_server_tag`.
pub fn extract_dns_server_tag(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Object(map)) => map
            .get("server")
            .map(|server| json_str(Some(server)))
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn require_dns_tag(server_tags: &HashSet<String>, missing_refs: &mut Vec<String>, tag: &str, owner: &str) {
    if tag.is_empty() || server_tags.contains(tag) {
        return;
    }
    missing_refs.push(format!("{owner} -> {tag}"));
}

/// Python `_validate_runtime_dns_contract`.
pub fn validate_runtime_dns_contract(payload: &Value) -> Result<()> {
    let mut server_tags: HashSet<String> = HashSet::new();
    if let Some(dns) = payload.get("dns").filter(|value| value.is_object()) {
        for server in dns.get("servers").and_then(Value::as_array).into_iter().flatten() {
            if !server.is_object() {
                continue;
            }
            let tag = json_str_field(server, "tag").trim().to_string();
            if !tag.is_empty() {
                server_tags.insert(tag);
            }
        }
    }

    let mut missing_refs: Vec<String> = Vec::new();

    if let Some(route) = payload.get("route").filter(|value| value.is_object()) {
        let tag = extract_dns_server_tag(route.get("default_domain_resolver"));
        require_dns_tag(&server_tags, &mut missing_refs, &tag, "route.default_domain_resolver");
    }

    if let Some(dns) = payload.get("dns").filter(|value| value.is_object()) {
        let tag = extract_dns_server_tag(dns.get("final"));
        require_dns_tag(&server_tags, &mut missing_refs, &tag, "dns.final");
        for (index, rule) in dns.get("rules").and_then(Value::as_array).into_iter().flatten().enumerate() {
            if !rule.is_object() {
                continue;
            }
            let tag = extract_dns_server_tag(rule.get("server"));
            require_dns_tag(&server_tags, &mut missing_refs, &tag, &format!("dns.rules[{index}].server"));
        }
    }

    for (index, outbound) in payload
        .get("outbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if !outbound.is_object() {
            continue;
        }
        let tag = extract_dns_server_tag(outbound.get("domain_resolver"));
        require_dns_tag(&server_tags, &mut missing_refs, &tag, &format!("outbounds[{index}].domain_resolver"));
    }

    if missing_refs.is_empty() {
        return Ok(());
    }

    let mut details_items: Vec<String> = Vec::new();
    for item in missing_refs {
        if !details_items.contains(&item) {
            details_items.push(item);
        }
    }
    bail!(
        "В sing-box конфиге отсутствует DNS-сервер с нужным tag. Проверьте раздел dns.servers: {}. \
Обычно для стандартного шаблона должны существовать теги `bootstrap-dns` и `proxy-dns`.",
        details_items.join("; ")
    )
}
