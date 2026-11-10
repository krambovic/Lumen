//! `plan_singbox_runtime` — the top-level sing-box runtime plan builder
//! (`plan_singbox_runtime`, `_plan_hybrid_runtime`, node classification).

use std::path::PathBuf;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use super::bootstrap::{
    ensure_all_server_bootstrap_contracts, ensure_awg3_windows_bind_workaround,
    ensure_awg3_windows_bind_workarounds, ensure_endpoint_server_bootstrap_contract,
    ensure_hybrid_protect_route, ensure_proxy_server_bootstrap_contract, strip_legacy_awg15_fields,
};
use super::chain::{
    apply_singbox_proxy_chain, ensure_full_config_proxy_alias, ensure_singbox_hot_switch_selector,
    find_singbox_dial_by_tag, ChainApplication,
};
use super::dns::{apply_imported_proxy_dns, validate_runtime_dns_contract};
use super::node_runtime::{is_xray_exclusive_node, node_uses_xray_sidecar, xray_only_message};
use super::sidecar::{build_xray_sidecar_config, normalize_openvpn_outbounds};
use super::tun::{
    clamp_tun_mtu_for_proxy, configure_singbox_runtime_inbounds,
    ensure_singbox_discord_proxy_contract, ensure_singbox_metrics_contract,
    ensure_singbox_runtime_contract, remove_singbox_tls_fragment_rule,
};
use super::{
    dial_value_mut, ensure_list, find_free_port, generate_ss_password, is_singbox_endpoint,
    json_str_or, lower_trim, replace_or_append_tagged, DialPath,
    APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG, SS_PROTECT_METHOD,
};
use crate::singbox::document::ParsedSingboxDocument;
use lumen_core::config::proxy_chain::{CHAIN_MAIN_TAG, ProxyChain};
use lumen_core::config::runtime_security::{
    clamp_singbox_local_inbounds, generate_local_proxy_credentials, strip_singbox_proxy_inbounds,
};
use lumen_core::constants::{PROXY_HOST, SS_PROTECT_PORT_END, SS_PROTECT_PORT_START, SINGBOX_XRAY_RELAY_PORT};
use lumen_core::models::{Node, RoutingSettings};
use lumen_core::routing_runtime::apply_singbox_gui_routing;
use lumen_core::wireguard_normalization::normalize_singbox_wireguard_endpoints;

/// Python `SingboxXraySidecarPlan`.
#[derive(Debug, Clone)]
pub struct SingboxXraySidecarPlan {
    pub relay_port: i64,
    pub relay_username: String,
    pub relay_password: String,
    pub protect_port: i64,
    pub protect_password: String,
    pub config: Value,
}

/// Python `SingboxRuntimePlan`.
#[derive(Debug, Clone)]
pub struct SingboxRuntimePlan {
    /// `native_singbox` | `hybrid_xray_sidecar`
    pub outcome: String,
    pub source_path: PathBuf,
    pub text_hash: String,
    pub singbox_config: Value,
    pub has_proxy_outbound: bool,
    pub used_selected_node: bool,
    pub clash_api_secret: String,
    pub xray_sidecar: Option<SingboxXraySidecarPlan>,
    pub clash_api_selector: String,
    pub clash_api_node_signatures: Vec<(String, String)>,
    pub health_proxy_url: String,
}

impl SingboxRuntimePlan {
    pub fn is_hybrid(&self) -> bool {
        self.outcome == "hybrid_xray_sidecar"
    }
}

/// Keyword arguments of Python `plan_singbox_runtime` (defaults preserved).
#[derive(Debug)]
pub struct PlanRuntimeOptions<'a> {
    pub routing: Option<RoutingSettings>,
    pub enable_final_fragment: bool,
    pub fragment_packets: String,
    pub fragment_length: String,
    pub fragment_delay: String,
    pub tail_fragment_enabled: bool,
    pub multiplex_enabled: bool,
    pub multiplex_concurrency: i64,
    pub discord_proxy_enabled: bool,
    pub tun_strict_route: bool,
    pub tun_stack: String,
    pub tun_mtu: i64,
    pub tun_endpoint_independent_nat: bool,
    pub tun_block_quic: bool,
    pub local_socks_port: i64,
    pub local_http_port: i64,
    pub proxy_auth_username: String,
    pub proxy_auth_password: String,
    pub preferred_relay_port: i64,
    pub preferred_protect_port: i64,
    pub preferred_protect_password: String,
    pub system_dns_servers: Vec<String>,
    pub tun_mode: bool,
    pub enable_hot_switch: bool,
    pub hot_switch_nodes: Vec<Node>,
    pub proxy_chain: Option<&'a mut ProxyChain>,
    pub core_preference: String,
}

impl Default for PlanRuntimeOptions<'_> {
    fn default() -> Self {
        Self {
            routing: None,
            enable_final_fragment: true,
            fragment_packets: "tlshello".to_string(),
            fragment_length: "50-100".to_string(),
            fragment_delay: "10-20".to_string(),
            tail_fragment_enabled: false,
            multiplex_enabled: false,
            multiplex_concurrency: 8,
            discord_proxy_enabled: false,
            tun_strict_route: true,
            tun_stack: "mixed".to_string(),
            tun_mtu: 9000,
            tun_endpoint_independent_nat: false,
            tun_block_quic: true,
            local_socks_port: lumen_core::constants::DEFAULT_SOCKS_PORT as i64,
            local_http_port: lumen_core::constants::DEFAULT_HTTP_PORT as i64,
            proxy_auth_username: String::new(),
            proxy_auth_password: String::new(),
            preferred_relay_port: 0,
            preferred_protect_port: 0,
            preferred_protect_password: String::new(),
            system_dns_servers: Vec::new(),
            tun_mode: true,
            enable_hot_switch: false,
            hot_switch_nodes: Vec::new(),
            proxy_chain: None,
            core_preference: "auto".to_string(),
        }
    }
}

/// Python `_node_is_full_singbox_config`.
pub fn node_is_full_singbox_config(node: Option<&Node>) -> bool {
    let Some(node) = node else {
        return false;
    };
    let outbound = &node.outbound;
    if !outbound.is_object() {
        return false;
    }
    let protocol = json_str_or(outbound.get("protocol"), &node.scheme);
    lower_trim(&protocol) == "singbox_config" && outbound.get("singbox_config").is_some_and(Value::is_object)
}

/// Python `_node_should_use_xray_sidecar`: Xray carries the server: it
/// requires Xray, or Xray is the chosen core.
pub fn node_should_use_xray_sidecar(node: Option<&Node>, core_preference: &str) -> bool {
    if node_is_full_singbox_config(node) {
        return false;
    }
    node_uses_xray_sidecar(node, core_preference)
}

/// Python `classify_node_for_singbox`.
pub fn classify_node_for_singbox(node: Option<&Node>, core_preference: &str) -> String {
    let Some(node) = node else {
        return "native_singbox".to_string();
    };
    if node_is_full_singbox_config(Some(node)) {
        return "native_singbox".to_string();
    }
    if node_should_use_xray_sidecar(Some(node), core_preference) {
        return "hybrid_xray_sidecar".to_string();
    }
    match crate::singbox::config_builder::build_singbox_outbound(node, "proxy", false, 8) {
        Ok(_) => "native_singbox".to_string(),
        Err(_) => "hybrid_xray_sidecar".to_string(),
    }
}

/// Python `_normalize_singbox_runtime_copy`: the shared normalizer sequence
/// applied to every runtime copy (full imported profiles and the document).
#[allow(clippy::too_many_arguments)]
fn normalize_runtime_copy(runtime_config: &mut Value) -> Result<()> {
    crate::singbox::config_builder::preserve_or_reject_semantic_fields(runtime_config)?;
    crate::singbox::config_builder::strip_removed_transport_fields(runtime_config)?;
    crate::singbox::config_builder::normalize_vmess_security(runtime_config)?;
    crate::singbox::config_builder::normalize_vless_vision(runtime_config);
    crate::singbox::config_builder::normalize_shadowsocks_methods(runtime_config)?;
    normalize_singbox_wireguard_endpoints(runtime_config)?;
    Ok(())
}

fn apply_gui_routing(runtime_config: &mut Value, routing: Option<&RoutingSettings>) -> Result<()> {
    if let Some(routing) = routing {
        apply_singbox_gui_routing(runtime_config, &routing.to_dict())?;
    }
    Ok(())
}

/// Python `plan_singbox_runtime`.
pub fn plan_singbox_runtime(
    document: &ParsedSingboxDocument,
    node: Option<&Node>,
    options: PlanRuntimeOptions<'_>,
) -> Result<SingboxRuntimePlan> {
    let PlanRuntimeOptions {
        routing,
        enable_final_fragment,
        fragment_packets,
        fragment_length,
        fragment_delay,
        tail_fragment_enabled,
        multiplex_enabled,
        multiplex_concurrency,
        discord_proxy_enabled,
        tun_strict_route,
        tun_stack,
        tun_mtu,
        tun_endpoint_independent_nat,
        tun_block_quic,
        local_socks_port,
        local_http_port,
        proxy_auth_username,
        proxy_auth_password,
        preferred_relay_port,
        preferred_protect_port,
        preferred_protect_password,
        system_dns_servers,
        tun_mode,
        mut enable_hot_switch,
        hot_switch_nodes,
        proxy_chain,
        core_preference,
    } = options;
    let mut proxy_chain = proxy_chain;
    if proxy_chain.as_ref().is_some_and(|chain| chain.active()) {
        // The clash selector would bypass the chain hops; a restart is cheap.
        enable_hot_switch = false;
    }
    if node_is_full_singbox_config(node) {
        let mut runtime_config = node
            .map(|node| &node.outbound)
            .filter(|outbound| outbound.is_object())
            .and_then(|outbound| outbound.get("singbox_config"))
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        normalize_runtime_copy(&mut runtime_config)?;
        // Keep legacy AWG 1.5 imports readable on disk, but never pass their
        // removed members to the strict extended 2.6.x decoder.  Full imported
        // profiles bypass the selected-node path, so sanitize them here too.
        strip_legacy_awg15_fields(&mut runtime_config);
        ensure_awg3_windows_bind_workarounds(&mut runtime_config);
        normalize_openvpn_outbounds(&mut runtime_config)?;
        strip_singbox_proxy_inbounds(&mut runtime_config, &[]);
        configure_singbox_runtime_inbounds(
            &mut runtime_config,
            tun_mode,
            local_socks_port,
            local_http_port,
            &proxy_auth_username,
            &proxy_auth_password,
        );
        let clash_api_secret = ensure_singbox_metrics_contract(&mut runtime_config);
        ensure_singbox_runtime_contract(
            &mut runtime_config,
            tun_mode,
            routing.as_ref(),
            enable_final_fragment,
            &system_dns_servers,
            tun_strict_route,
            &tun_stack,
            tun_mtu,
            tun_endpoint_independent_nat,
            tun_block_quic,
            local_socks_port,
            local_http_port,
            &proxy_auth_username,
            &proxy_auth_password,
        );
        ensure_full_config_proxy_alias(&mut runtime_config);
        apply_gui_routing(&mut runtime_config, routing.as_ref())?;
        // Add bootstrap loop protection after GUI rules so it keeps the runtime's
        // reserved priority without being confused with imported user routes.
        ensure_all_server_bootstrap_contracts(&mut runtime_config);
        ensure_singbox_discord_proxy_contract(&mut runtime_config, discord_proxy_enabled)?;
        clamp_singbox_local_inbounds(&mut runtime_config);
        validate_runtime_dns_contract(&runtime_config)?;
        return Ok(SingboxRuntimePlan {
            outcome: "native_singbox".to_string(),
            source_path: document.source_path.clone(),
            text_hash: document.text_hash.clone(),
            singbox_config: runtime_config,
            has_proxy_outbound: true,
            used_selected_node: true,
            clash_api_secret,
            xray_sidecar: None,
            clash_api_selector: String::new(),
            clash_api_node_signatures: Vec::new(),
            health_proxy_url: String::new(),
        });
    }

    let mut runtime_config = document.payload.clone();
    normalize_runtime_copy(&mut runtime_config)?;
    normalize_openvpn_outbounds(&mut runtime_config)?;
    strip_singbox_proxy_inbounds(&mut runtime_config, &[]);
    configure_singbox_runtime_inbounds(
        &mut runtime_config,
        tun_mode,
        local_socks_port,
        local_http_port,
        &proxy_auth_username,
        &proxy_auth_password,
    );
    let clash_api_secret = ensure_singbox_metrics_contract(&mut runtime_config);
    ensure_singbox_runtime_contract(
        &mut runtime_config,
        tun_mode,
        routing.as_ref(),
        enable_final_fragment,
        &system_dns_servers,
        tun_strict_route,
        &tun_stack,
        tun_mtu,
        tun_endpoint_independent_nat,
        tun_block_quic,
        local_socks_port,
        local_http_port,
        &proxy_auth_username,
        &proxy_auth_password,
    );
    let proxy_index = super::find_proxy_outbound_index(runtime_config.get("outbounds").and_then(Value::as_array));
    let Some(proxy_index) = proxy_index else {
        // The runtime contracts reference outbound `proxy`; a document whose own
        // proxy outbound is tagged differently needs the alias to resolve them.
        ensure_full_config_proxy_alias(&mut runtime_config);
        strip_legacy_awg15_fields(&mut runtime_config);
        ensure_awg3_windows_bind_workarounds(&mut runtime_config);
        apply_gui_routing(&mut runtime_config, routing.as_ref())?;
        ensure_all_server_bootstrap_contracts(&mut runtime_config);
        ensure_singbox_discord_proxy_contract(&mut runtime_config, discord_proxy_enabled)?;
        clamp_singbox_local_inbounds(&mut runtime_config);
        validate_runtime_dns_contract(&runtime_config)?;
        return Ok(SingboxRuntimePlan {
            outcome: "native_singbox".to_string(),
            source_path: document.source_path.clone(),
            text_hash: document.text_hash.clone(),
            singbox_config: runtime_config,
            has_proxy_outbound: false,
            used_selected_node: false,
            clash_api_secret,
            xray_sidecar: None,
            clash_api_selector: String::new(),
            clash_api_node_signatures: Vec::new(),
            health_proxy_url: String::new(),
        });
    };

    let Some(node) = node else {
        bail!("В конфиге есть outbound tag `proxy`. Выберите сервер для запуска sing-box.");
    };
    if core_preference == "singbox" && is_xray_exclusive_node(Some(node)) {
        // sing-box extended only: no Xray sidecar.
        bail!("{}", xray_only_message(Some(node)));
    }

    let force_sidecar = node_should_use_xray_sidecar(Some(node), &core_preference);
    let native_proxy = if force_sidecar {
        None
    } else {
        crate::singbox::config_builder::build_singbox_outbound(
            node,
            "proxy",
            multiplex_enabled,
            multiplex_concurrency,
        )
        .ok()
    };

    let Some(mut native_proxy) = native_proxy else {
        let sidecar = plan_hybrid_runtime(
            &mut runtime_config,
            proxy_index,
            node,
            enable_final_fragment,
            &fragment_packets,
            &fragment_length,
            &fragment_delay,
            tail_fragment_enabled,
            multiplex_enabled,
            multiplex_concurrency,
            preferred_relay_port,
            preferred_protect_port,
            &preferred_protect_password,
            proxy_chain.as_deref_mut(),
        )?;
        apply_gui_routing(&mut runtime_config, routing.as_ref())?;
        ensure_all_server_bootstrap_contracts(&mut runtime_config);
        ensure_singbox_discord_proxy_contract(&mut runtime_config, discord_proxy_enabled)?;
        clamp_singbox_local_inbounds(&mut runtime_config);
        validate_runtime_dns_contract(&runtime_config)?;
        return Ok(SingboxRuntimePlan {
            outcome: "hybrid_xray_sidecar".to_string(),
            source_path: document.source_path.clone(),
            text_hash: document.text_hash.clone(),
            singbox_config: runtime_config,
            has_proxy_outbound: true,
            used_selected_node: true,
            clash_api_secret,
            xray_sidecar: Some(sidecar),
            clash_api_selector: String::new(),
            clash_api_node_signatures: Vec::new(),
            health_proxy_url: String::new(),
        });
    };

    let native_is_endpoint = is_singbox_endpoint(&native_proxy);
    if native_is_endpoint {
        runtime_config["outbounds"]
            .as_array_mut()
            .expect("outbounds list")
            .remove(proxy_index);
        strip_legacy_awg15_fields(&mut native_proxy);
        ensure_awg3_windows_bind_workaround(&mut runtime_config, &mut native_proxy);
        let endpoints = ensure_list(&mut runtime_config, "endpoints");
        replace_or_append_tagged(endpoints, "proxy", &native_proxy);
    } else {
        let outbounds = runtime_config.get_mut("outbounds").unwrap().as_array_mut().unwrap();
        outbounds[proxy_index] = native_proxy;
    }
    let chain_result: ChainApplication = apply_singbox_proxy_chain(
        &mut runtime_config,
        proxy_chain.as_deref_mut(),
        multiplex_enabled,
        multiplex_concurrency,
    );
    let mut clash_api_selector = String::new();
    let mut clash_api_node_signatures: Vec<(String, String)> = Vec::new();
    if enable_hot_switch {
        let (selector, signatures) = ensure_singbox_hot_switch_selector(
            &mut runtime_config,
            node,
            &hot_switch_nodes,
            multiplex_enabled,
            multiplex_concurrency,
            &core_preference,
        );
        clash_api_selector = selector;
        clash_api_node_signatures = signatures;
    }
    let hot_switch_applied = !clash_api_selector.is_empty();
    apply_gui_routing(&mut runtime_config, routing.as_ref())?;
    ensure_all_server_bootstrap_contracts(&mut runtime_config);
    // Location of the native proxy dial after the chain/hot-switch mutations.
    // When the hot-switch selector replaced the `proxy` dial, the Python
    // planner mutated the orphaned dict here — no config effect, so skip.
    let native_path: Option<DialPath> = if hot_switch_applied {
        None
    } else if native_is_endpoint {
        find_singbox_dial_by_tag(&runtime_config, "proxy").map(|(section, index)| DialPath::from_section(section, index))
    } else if chain_result.exit_applied {
        find_singbox_dial_by_tag(&runtime_config, CHAIN_MAIN_TAG)
            .map(|(section, index)| DialPath::from_section(section, index))
    } else {
        find_singbox_dial_by_tag(&runtime_config, "proxy").map(|(section, index)| DialPath::from_section(section, index))
    };
    if let Some(path) = native_path {
        if native_is_endpoint {
            ensure_endpoint_server_bootstrap_contract(&mut runtime_config, path);
        } else {
            let server = node.server.clone();
            ensure_proxy_server_bootstrap_contract(&mut runtime_config, path, &server);
        }
    }
    if let Some((front_node, front_path, front_is_endpoint)) = chain_result.front {
        if front_is_endpoint {
            ensure_endpoint_server_bootstrap_contract(&mut runtime_config, front_path);
        } else {
            let server = front_node.server.clone();
            ensure_proxy_server_bootstrap_contract(&mut runtime_config, front_path, &server);
        }
    }
    apply_imported_proxy_dns(&mut runtime_config, node, routing.as_ref());
    if let Some(path) = native_path {
        let proxy_snapshot = dial_value_mut(&mut runtime_config, path).clone();
        clamp_tun_mtu_for_proxy(&mut runtime_config, &proxy_snapshot, tun_mode);
    }
    ensure_singbox_discord_proxy_contract(&mut runtime_config, discord_proxy_enabled)?;
    clamp_singbox_local_inbounds(&mut runtime_config);
    validate_runtime_dns_contract(&runtime_config)?;
    Ok(SingboxRuntimePlan {
        outcome: "native_singbox".to_string(),
        source_path: document.source_path.clone(),
        text_hash: document.text_hash.clone(),
        singbox_config: runtime_config,
        has_proxy_outbound: true,
        used_selected_node: true,
        clash_api_secret,
        xray_sidecar: None,
        clash_api_selector,
        clash_api_node_signatures,
        health_proxy_url: String::new(),
    })
}

/// Python `_plan_hybrid_runtime`: rewire the sing-box config onto a loopback
/// socks relay and build the Xray sidecar plan.
#[allow(clippy::too_many_arguments)]
fn plan_hybrid_runtime(
    runtime_config: &mut Value,
    proxy_index: usize,
    node: &Node,
    enable_final_fragment: bool,
    fragment_packets: &str,
    fragment_length: &str,
    fragment_delay: &str,
    tail_fragment_enabled: bool,
    multiplex_enabled: bool,
    multiplex_concurrency: i64,
    preferred_relay_port: i64,
    preferred_protect_port: i64,
    preferred_protect_password: &str,
    proxy_chain: Option<&mut ProxyChain>,
) -> Result<SingboxXraySidecarPlan> {
    let relay_port = if preferred_relay_port > 0 {
        preferred_relay_port
    } else {
        find_free_port(SINGBOX_XRAY_RELAY_PORT as i64, None, &[])?
    };
    let excluded_ports = [relay_port];
    let protect_port = if preferred_protect_port > 0 {
        preferred_protect_port
    } else {
        find_free_port(
            SS_PROTECT_PORT_START as i64,
            Some(SS_PROTECT_PORT_START as i64..SS_PROTECT_PORT_END as i64),
            &excluded_ports,
        )?
    };
    let protect_password = if preferred_protect_password.is_empty() {
        generate_ss_password(24)
    } else {
        preferred_protect_password.to_string()
    };
    let (relay_username, relay_password) = generate_local_proxy_credentials("sidecar", 24);

    {
        let outbounds = ensure_list(runtime_config, "outbounds");
        outbounds[proxy_index] = json!({
            "type": "socks",
            "tag": "proxy",
            "server": PROXY_HOST,
            "server_port": relay_port,
            "username": relay_username,
            "password": relay_password,
            // Keep the relay on loopback so sing-box does not bind it to the
            // physical adapter via auto-detect rules.
            "inet4_bind_address": PROXY_HOST,
        });
    }
    {
        let inbounds = ensure_list(runtime_config, "inbounds");
        replace_or_append_tagged(
            inbounds,
            APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG,
            &json!({
                "type": "shadowsocks",
                "tag": APP_SINGBOX_HYBRID_PROTECT_INBOUND_TAG,
                "listen": PROXY_HOST,
                "listen_port": protect_port,
                "method": SS_PROTECT_METHOD,
                "password": protect_password,
            }),
        );
    }
    ensure_hybrid_protect_route(runtime_config);
    remove_singbox_tls_fragment_rule(runtime_config);
    clamp_singbox_local_inbounds(runtime_config);
    validate_runtime_dns_contract(runtime_config)?;

    let config = build_xray_sidecar_config(
        node,
        relay_port,
        &relay_username,
        &relay_password,
        protect_port,
        &protect_password,
        enable_final_fragment,
        fragment_packets,
        fragment_length,
        fragment_delay,
        tail_fragment_enabled,
        multiplex_enabled,
        multiplex_concurrency,
        proxy_chain,
    )?;
    Ok(SingboxXraySidecarPlan {
        relay_port,
        relay_username,
        relay_password,
        protect_port,
        protect_password,
        config,
    })
}
