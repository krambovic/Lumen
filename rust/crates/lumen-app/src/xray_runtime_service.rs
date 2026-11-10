//! Prepare a loaded Xray profile and selected node for the process manager.

mod metrics;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use lumen_core::config::multiplex::apply_xray_multiplex;
use lumen_core::config::proxy_chain::{apply_xray_proxy_chain, resolve_proxy_chain};
use lumen_core::config::xray_fragments::{apply_xray_final_fragment, apply_xray_outbound_fragment};
use lumen_core::config::xray_inbounds::ensure_xray_mixed_proxy_inbound;
use lumen_core::constants::{DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};
use lumen_core::link_parser::{
    normalize_node_outbound, repair_node_outbound_from_link, validate_node_outbound,
};
use lumen_core::models::{AppSettings, AppState, Node, str_or};
use lumen_core::routing_runtime::{apply_xray_gui_routing, routing_with_ip_preference};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::runtime_introspection::{
    collect_xray_inbound_ports, config_has_proxy_outbound, extract_xray_runtime_ports,
    find_free_api_port, infer_xray_ping_target,
};
use lumen_core::config::json_document::format_json_error_message;

use crate::health_listener::PROXY_PROTOCOLS;
pub use crate::health_listener::add_xray_health_listener;
pub use metrics::{
    APP_DISCORD_PROXY_INBOUND_TAG, APP_METRICS_API_INBOUND_TAG, APP_METRICS_API_TAG,
    ensure_xray_metrics_contract,
};

const INVALID_ROOT: &str = "Корень xray config должен быть JSON-объектом.";
const PROXY_TAG: &str = "proxy";

pub struct XrayRuntimeConfig {
    pub config: Value,
    pub source_path: PathBuf,
    pub has_proxy_outbound: bool,
    pub used_selected_node: bool,
    pub socks_port: i64,
    pub http_port: i64,
    pub api_port: i64,
    pub inbound_tags: Vec<String>,
    pub ping_host: String,
    pub ping_port: i64,
    pub health_proxy_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrayRuntimeEvent {
    Log(String),
    /// A stored outbound was repaired or normalized and should be persisted.
    NodeChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrayConfigInspection {
    pub source_path: PathBuf,
    pub text_hash: String,
    pub has_proxy_outbound: bool,
    pub socks_port: i64,
    pub http_port: i64,
    pub api_port: i64,
}

pub fn inspect_active_xray_config(
    source_path: &Path,
    text: &str,
    settings: &AppSettings,
    log: impl FnMut(String),
) -> XrayConfigInspection {
    let mut inspection = XrayConfigInspection {
        source_path: source_path.into(),
        text_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        has_proxy_outbound: false,
        socks_port: 0,
        http_port: 0,
        api_port: 0,
    };
    if let Ok(mut payload) = serde_json::from_str::<Value>(text)
        && payload.is_object()
        && ensure_xray_metrics_contract(&mut payload, settings, false, log).is_ok()
    {
        inspection.has_proxy_outbound = config_has_proxy_outbound(&payload);
        (
            inspection.socks_port,
            inspection.http_port,
            inspection.api_port,
        ) = extract_xray_runtime_ports(&payload);
    }
    inspection
}

/// Assemble the runtime JSON from the active profile text and an optional
/// selected node ID. The profile file itself is kept separate from runtime JSON.
pub fn build_runtime_xray_config(
    source_path: &Path,
    text: &str,
    state: &mut AppState,
    node_id: Option<&str>,
    mut emit: impl FnMut(XrayRuntimeEvent),
) -> Result<XrayRuntimeConfig> {
    let selected = node_id.and_then(|id| state.nodes.iter().position(|node| node.id == id));
    let full_config = selected
        .map(|index| &state.nodes[index].outbound)
        .filter(|outbound| {
            outbound["protocol"]
                .as_str()
                .is_some_and(|protocol| protocol.trim().eq_ignore_ascii_case("xray_config"))
        })
        .map(|outbound| &outbound["xray_config"])
        .filter(|config| config.is_object());
    let using_full_config = full_config.is_some();
    let mut payload: Value = match full_config {
        Some(config) => config.clone(),
        None => serde_json::from_str(text).map_err(|error| {
            anyhow::anyhow!(
                "{}: {}",
                source_path
                    .file_name()
                    .unwrap_or(source_path.as_os_str())
                    .to_string_lossy(),
                format_json_error_message(text, &error)
            )
        })?,
    };
    ensure!(payload.is_object(), INVALID_ROOT);

    let settings = &state.settings;
    let username = settings.proxy_auth_username.trim();
    let password = settings.proxy_auth_password.as_str();
    let (username, password) =
        if settings.proxy_auth_enabled && !username.is_empty() && !password.is_empty() {
            (username, password)
        } else {
            ("", "")
        };
    let normalized = ensure_xray_mixed_proxy_inbound(
        &mut payload,
        u16::try_from(settings.local_socks_port).unwrap_or(DEFAULT_SOCKS_PORT),
        u16::try_from(settings.local_http_port).unwrap_or(DEFAULT_HTTP_PORT),
        settings.sniff_route_only,
        username,
        password,
    );
    if normalized != 0 {
        emit(XrayRuntimeEvent::Log(format!(
            "[xray] local proxy inbound normalized for mixed mode ({normalized} change(s))"
        )));
    }
    let (api_port, inbound_tags) =
        ensure_xray_metrics_contract(&mut payload, settings, true, |message| {
            emit(XrayRuntimeEvent::Log(message))
        })?;

    let mut used_selected_node = using_full_config;
    if !using_full_config {
        let outbounds = payload["outbounds"]
            .as_array_mut()
            .expect("metrics ensures outbounds");
        if let Some(outbound) = outbounds
            .iter_mut()
            .find(|outbound| outbound["tag"] == PROXY_TAG)
        {
            let index = selected.context(
                "В конфиге есть outbound tag `proxy`. Выберите сервер для запуска xray.",
            )?;
            let node = &mut state.nodes[index];
            if is_native_singbox_only(node) {
                let name = if node.name.is_empty() {
                    &node.server
                } else {
                    &node.name
                };
                bail!(
                    "{name} не поддерживается ядром Xray. \
                     Выберите Auto или sing-box extended в «Настройки → Сеть → Основное ядро»: \
                     Lumen запустит этот сервер через sing-box extended как в системном прокси, так и в TUN."
                );
            }
            let repaired = repair_node_outbound_from_link(node);
            if normalize_node_outbound(node) || repaired {
                emit(XrayRuntimeEvent::NodeChanged(node.id.clone()));
            }
            if let Some(problem) = validate_node_outbound(node) {
                bail!(problem);
            }
            *outbound = node.outbound.clone();
            outbound["tag"] = json!(PROXY_TAG);
            apply_xray_multiplex(
                outbound,
                settings.multiplex_enabled,
                settings.multiplex_concurrency,
            );
            used_selected_node = true;

            let settings_json = settings.to_dict();
            let node = &state.nodes[index];
            if let Some(mut chain) = resolve_proxy_chain(&settings_json, &state.nodes, Some(node)) {
                let applied = apply_xray_proxy_chain(outbounds, Some(&mut chain), PROXY_TAG);
                for note in &chain.notes {
                    emit(XrayRuntimeEvent::Log(format!("[chain] {note}")));
                }
                if !applied.is_empty() {
                    emit(XrayRuntimeEvent::Log(format!(
                        "[chain] xray: {}",
                        chain.describe(Some(node))
                    )));
                }
            }
        }
        // Full AUTO profiles own their observatory and balancer routing.
        apply_gui_overrides(&mut payload, state, &mut emit);
    }

    let (socks_port, http_port, _) = extract_xray_runtime_ports(&payload);
    let selected_node = selected
        .filter(|_| used_selected_node)
        .map(|index| &state.nodes[index]);
    let (ping_host, ping_port) = infer_xray_ping_target(&payload, selected_node);
    let mut health_proxy_url = String::new();
    if used_selected_node && !using_full_config {
        match find_free_api_port(None, &collect_xray_inbound_ports(&payload))
            .and_then(|port| add_xray_health_listener(&mut payload, port, PROXY_TAG))
        {
            Ok(url) => health_proxy_url = url,
            Err(_) => emit(XrayRuntimeEvent::Log(
                "[metrics] dedicated health listener unavailable".into(),
            )),
        }
    }

    Ok(XrayRuntimeConfig {
        config: payload,
        source_path: source_path.into(),
        has_proxy_outbound: used_selected_node,
        used_selected_node,
        socks_port,
        http_port,
        api_port,
        inbound_tags,
        ping_host,
        ping_port,
        health_proxy_url,
    })
}

fn apply_gui_overrides(
    payload: &mut Value,
    state: &AppState,
    emit: &mut impl FnMut(XrayRuntimeEvent),
) {
    let settings = &state.settings;
    let routing = routing_with_ip_preference(&state.routing.to_dict(), settings.prefer_ipv6);
    apply_xray_gui_routing(payload, &routing, &settings.to_dict());
    if settings.enable_xray_fragment {
        let patched = apply_xray_outbound_fragment(
            payload,
            &settings.fragment_packets,
            &settings.fragment_length,
            &settings.fragment_delay,
            settings.tail_fragment_enabled,
        );
        if patched != 0 {
            emit(XrayRuntimeEvent::Log(format!(
                "[xray] outbound fragment enabled for {patched} outbound(s)"
            )));
        }
    }
    if settings.enable_final_fragment {
        let patched = apply_xray_final_fragment(
            payload,
            PROXY_TAG,
            &settings.fragment_packets,
            &settings.fragment_length,
            &settings.fragment_delay,
            settings.tail_fragment_enabled,
        );
        if patched != 0 {
            emit(XrayRuntimeEvent::Log(format!(
                "[xray] final TLS fragment enabled for {patched} proxy outbound(s)"
            )));
        }
    }
    if !settings.prefer_ipv6 {
        for outbound in payload["outbounds"]
            .as_array_mut()
            .expect("metrics ensures outbounds")
        {
            if matches!(outbound["protocol"].as_str(), Some("blackhole" | "dns")) {
                continue;
            }
            if let Some(outbound) = outbound.as_object_mut()
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
                    .entry("domainStrategy")
                    .or_insert_with(|| json!("UseIPv4"));
            }
        }
    }
}

fn is_native_singbox_only(node: &Node) -> bool {
    if !node.outbound.is_object() {
        return false;
    }
    let protocol = str_or(&node.outbound, "protocol", &node.scheme)
        .trim()
        .to_ascii_lowercase();
    matches!(
        protocol.as_str(),
        "warp"
            | "wireguard"
            | "awg"
            | "hysteria"
            | "hysteria2"
            | "tuic"
            | "mieru"
            | "masque"
            | "openvpn"
            | "naive"
            | "singbox_config"
    ) || (node.outbound["singbox"].is_object() && !PROXY_PROTOCOLS.contains(&protocol.as_str()))
}

#[cfg(test)]
mod tests;
