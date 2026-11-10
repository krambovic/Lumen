//! Stable signatures used by connection transitions and hot switching.

use std::path::Path;

use anyhow::{Context, Result};
use lumen_core::config::proxy_chain::proxy_chain_signature;
use lumen_core::models::{AppSettings, AppState, Node, RoutingSettings};
use lumen_core::pyjson::dumps_compact_sorted_ascii;
use lumen_engines::singbox::document::SingboxDocumentState;
use lumen_engines::singbox::runtime_planner::classify_node_for_singbox;
use serde_json::{Value, json};

use crate::node_runtime_service::{core_preference_of, get_node_by_id, proxy_core_for_node};
use crate::xray_runtime_service::XrayConfigInspection;

pub fn signature(payload: Value) -> String {
    dumps_compact_sorted_ascii(payload)
}

/// State and profile inspections for one signature calculation. Only the
/// inspection used by the chosen runtime is required.
pub struct SignatureService<'a> {
    pub settings: &'a AppSettings,
    pub routing: &'a RoutingSettings,
    pub nodes: &'a [Node],
    pub selected_node: Option<&'a Node>,
    pub xray_profile: Option<&'a XrayConfigInspection>,
    pub singbox_profile: Option<&'a SingboxDocumentState>,
    /// The controller uses editor profiles. Generated runtimes use the fallback payloads.
    pub use_raw_profiles: bool,
}

impl<'a> SignatureService<'a> {
    pub fn from_state(state: &'a AppState) -> Self {
        Self {
            settings: &state.settings,
            routing: &state.routing,
            nodes: &state.nodes,
            selected_node: get_node_by_id(&state.nodes, state.selected_node_id.as_deref()),
            xray_profile: None,
            singbox_profile: None,
            use_raw_profiles: true,
        }
    }

    pub fn routing_signature(&self) -> String {
        signature(self.routing.to_dict())
    }

    pub fn system_proxy_bypass_lan(&self) -> bool {
        self.settings.system_proxy_bypass_lan
    }

    pub fn transition_signature(&self, node: Option<&Node>, include_node: bool) -> Result<String> {
        let settings = self.settings;
        let node = if include_node {
            node.or(self.selected_node)
        } else {
            None
        };
        let preference = core_preference_of(Some(settings));
        if self.uses_singbox_proxy(node) {
            let profile = self
                .singbox_profile
                .context("missing sing-box profile inspection")?;
            return Ok(self.sign_runtime(json!({
                "mode": "singbox-proxy",
                "singbox_path": settings.singbox_path,
                "config_file": config_file_name(&profile.source_path),
                "config_hash": profile.text_hash,
                "has_proxy_outbound": profile.has_proxy_outbound,
                "node_id": node.map(|node| &node.id),
                "node_outbound": node.map(|node| &node.outbound),
                "proxy_enabled": settings.enable_system_proxy,
                "proxy_bypass_lan": self.system_proxy_bypass_lan(),
                "firefox_proxy_integration": settings.firefox_proxy_integration,
                "socks_port": settings.local_socks_port,
                "http_port": settings.local_http_port,
                "regional_preset": settings.regional_preset,
            })));
        }
        if self.is_singbox_editor_mode() {
            let profile = self
                .singbox_profile
                .context("missing sing-box profile inspection")?;
            let node = node.filter(|_| profile.has_proxy_outbound);
            let outcome = classify_node_for_singbox(node, preference);
            let mut payload = json!({
                "mode": "singbox-editor",
                "singbox_path": settings.singbox_path,
                "config_file": config_file_name(&profile.source_path),
                "config_hash": profile.text_hash,
                "has_proxy_outbound": profile.has_proxy_outbound,
                "planner_outcome": outcome,
                "core_preference": preference,
                "node_id": node.map(|node| &node.id),
                "node_outbound": node.map(|node| &node.outbound),
                "regional_preset": settings.regional_preset,
                "tun_runtime": tun_runtime(settings),
            });
            if outcome == "hybrid_xray_sidecar" {
                payload["xray_path"] = json!(settings.xray_path);
            }
            return Ok(self.sign_runtime(payload));
        }
        if self.uses_xray_raw_config() {
            let mut payload = self.xray_profile_payload(node)?;
            payload["regional_preset"] = json!(settings.regional_preset);
            payload["proxy_enabled"] = json!(settings.enable_system_proxy);
            payload["proxy_bypass_lan"] = json!(self.system_proxy_bypass_lan());
            payload["firefox_proxy_integration"] = json!(settings.firefox_proxy_integration);
            if settings.tun_mode {
                payload["xray_tun"] = tun_runtime(settings);
                payload["dns"] = json!([
                    self.routing.dns_bootstrap_servers,
                    self.routing.dns_bootstrap_type,
                    self.routing.dns_hijack_enabled,
                    self.routing.dns_mode,
                ]);
            }
            return Ok(self.sign_runtime(payload));
        }
        Ok(self.sign_runtime(json!({
            "node_id": node.map(|node| &node.id),
            "tun_mode": settings.tun_mode,
            "proxy_enabled": settings.enable_system_proxy,
            "proxy_bypass_lan": self.routing.bypass_lan,
            "firefox_proxy_integration": settings.firefox_proxy_integration,
            "proxy_allow_lan": settings.proxy_allow_lan,
            "sniff_route_only": settings.sniff_route_only,
            "tun_settings": [settings.tun_strict_route, settings.tun_stack, settings.tun_mtu,
                settings.tun_endpoint_independent_nat, settings.tun_block_quic],
            "socks_port": settings.local_socks_port,
            "http_port": settings.local_http_port,
            "xray_path": settings.xray_path,
            "singbox_path": settings.singbox_path,
            "regional_preset": settings.regional_preset,
        })))
    }

    pub fn xray_layer_signature(&self, node: Option<&Node>) -> Result<String> {
        let settings = self.settings;
        let node = node.or(self.selected_node);
        if self.uses_singbox_proxy(node) {
            return Ok(String::new());
        }
        if self.uses_xray_raw_config() {
            return Ok(self.sign_runtime(self.xray_profile_payload(node)?));
        }
        Ok(self.sign_runtime(json!({
            "node_id": node.map(|node| &node.id),
            "tun_mode": settings.tun_mode,
            "socks_port": settings.local_socks_port,
            "http_port": settings.local_http_port,
            "xray_path": settings.xray_path,
        })))
    }

    pub fn tun_layer_signature(&self) -> Result<String> {
        if !self.settings.tun_mode {
            return Ok(String::new());
        }
        if self.is_singbox_editor_mode() {
            return self.transition_signature(None, false);
        }
        Ok(self.sign_runtime(json!({
            "mode": "singbox-native",
            "xray_path": self.settings.xray_path,
            "singbox_path": self.settings.singbox_path,
        })))
    }

    fn uses_singbox_proxy(&self, node: Option<&Node>) -> bool {
        !self.settings.tun_mode
            && proxy_core_for_node(node, core_preference_of(Some(self.settings))) == "singbox"
    }

    fn is_singbox_editor_mode(&self) -> bool {
        self.use_raw_profiles && self.settings.tun_mode && self.settings.core_preference != "xray"
    }

    fn uses_xray_raw_config(&self) -> bool {
        self.use_raw_profiles && !self.is_singbox_editor_mode()
    }

    fn xray_profile_payload(&self, node: Option<&Node>) -> Result<Value> {
        let profile = self
            .xray_profile
            .context("missing Xray profile inspection")?;
        let node = node.filter(|_| profile.has_proxy_outbound);
        Ok(json!({
            "mode": "xray-direct",
            "xray_path": self.settings.xray_path,
            "config_file": config_file_name(&profile.source_path),
            "config_hash": profile.text_hash,
            "has_proxy_outbound": profile.has_proxy_outbound,
            "node_id": node.map(|node| &node.id),
            "node_outbound": node.map(|node| &node.outbound),
            "socks_port": profile.socks_port,
            "http_port": profile.http_port,
            "api_port": profile.api_port,
            "proxy_allow_lan": self.settings.proxy_allow_lan,
            "sniff_route_only": self.settings.sniff_route_only,
        }))
    }

    fn sign_runtime(&self, mut payload: Value) -> String {
        payload["routing"] = self.routing.to_dict();
        payload["discord_proxy_enabled"] = json!(self.settings.discord_proxy_enabled);
        payload["prefer_ipv6"] = json!(self.settings.prefer_ipv6);
        payload["proxy_chain"] = json!(proxy_chain_signature(&self.settings.to_dict(), self.nodes));
        signature(payload)
    }
}

fn config_file_name(path: &Path) -> std::borrow::Cow<'_, str> {
    path.file_name().unwrap_or_default().to_string_lossy()
}

fn tun_runtime(settings: &AppSettings) -> Value {
    let credentials = if settings.proxy_auth_enabled {
        (
            settings.proxy_auth_username.as_str(),
            settings.proxy_auth_password.as_str(),
        )
    } else {
        ("", "")
    };
    json!({
        "enable_final_fragment": settings.enable_final_fragment,
        "fragment_packets": settings.fragment_packets,
        "fragment_length": settings.fragment_length,
        "fragment_delay": settings.fragment_delay,
        "tail_fragment_enabled": settings.tail_fragment_enabled,
        "multiplex_enabled": settings.multiplex_enabled,
        "multiplex_concurrency": settings.multiplex_concurrency,
        "tun_strict_route": settings.tun_strict_route,
        "tun_stack": settings.tun_stack,
        "tun_mtu": settings.tun_mtu,
        "tun_endpoint_independent_nat": settings.tun_endpoint_independent_nat,
        "tun_block_quic": settings.tun_block_quic,
        "local_socks_port": settings.local_socks_port,
        "local_http_port": settings.local_http_port,
        "proxy_auth_enabled": settings.proxy_auth_enabled,
        "proxy_auth_username": credentials.0,
        "proxy_auth_password": credentials.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use lumen_engines::singbox::document::inspect_singbox_document_text;

    fn node(id: &str) -> Node {
        Node {
            id: id.into(),
            outbound: json!({
                "protocol": "vless",
                "settings": {"vnext": [{"address": "example.test", "port": 443,
                    "users": [{"id": "00000000-0000-0000-0000-000000000000"}]}]},
            }),
            ..Default::default()
        }
    }

    #[test]
    fn signatures_sort_and_escape_nested_json_like_python() {
        assert_eq!(
            signature(json!({"я": {"z": 1e-7, "a": 1.0}, "a": ["😀", "one, two: three"]})),
            r#"{"a":["\ud83d\ude00","one, two: three"],"\u044f":{"a":1.0,"z":1e-07}}"#
        );
    }

    #[test]
    fn singbox_tun_signature_tracks_runtime_settings_but_ignores_node_in_tun_layer() -> Result<()> {
        let mut state = AppState::default();
        state.settings.tun_mode = true;
        let profile = inspect_singbox_document_text(
            Path::new("default.json"),
            r#"{"outbounds":[{"tag":"proxy"}]}"#,
        );
        let mut service = SignatureService::from_state(&state);
        service.singbox_profile = Some(&profile);
        let first = node("first");
        let second = node("second");
        let transition = service.transition_signature(Some(&first), true)?;
        let tun_layer = service.tun_layer_signature()?;
        assert_ne!(
            transition,
            service.transition_signature(Some(&second), true)?
        );
        service.selected_node = Some(&second);
        assert_eq!(tun_layer, service.tun_layer_signature()?);
        let updated = AppSettings {
            tun_mtu: 1400,
            ..state.settings.clone()
        };
        service.settings = &updated;
        assert_ne!(
            transition,
            service.transition_signature(Some(&first), true)?
        );
        assert_ne!(tun_layer, service.tun_layer_signature()?);
        Ok(())
    }

    #[test]
    fn raw_xray_layer_ignores_system_proxy_changes_and_uses_profile_ports() -> Result<()> {
        let mut state = AppState::default();
        state.settings.core_preference = "xray".into();
        state.nodes.push(node("selected"));
        state.selected_node_id = Some("selected".into());
        let profile = XrayConfigInspection {
            source_path: Path::new("configs").join("custom.json"),
            text_hash: "abc".into(),
            has_proxy_outbound: true,
            socks_port: 1234,
            http_port: 1235,
            api_port: 1236,
        };
        let mut service = SignatureService::from_state(&state);
        service.xray_profile = Some(&profile);
        let base = service.transition_signature(None, true)?;
        let layer = service.xray_layer_signature(None)?;
        let payload: Value = serde_json::from_str(&base)?;
        assert_eq!(payload["mode"], "xray-direct");
        assert_eq!(payload["config_file"], "custom.json");
        assert_eq!(payload["socks_port"], 1234);
        assert_eq!(payload["node_id"], "selected");
        let changed = AppSettings {
            enable_system_proxy: !state.settings.enable_system_proxy,
            ..state.settings.clone()
        };
        service.settings = &changed;
        assert_ne!(base, service.transition_signature(None, true)?);
        assert_eq!(layer, service.xray_layer_signature(None)?);
        assert!(service.tun_layer_signature()?.is_empty());

        let xray_tun = AppSettings {
            tun_mode: true,
            ..state.settings.clone()
        };
        service.settings = &xray_tun;
        let before = service.transition_signature(None, true)?;
        let routing = RoutingSettings {
            dns_bootstrap_servers: vec!["1.1.1.1".into()],
            ..state.routing.clone()
        };
        service.routing = &routing;
        let after = service.transition_signature(None, true)?;
        assert_ne!(before, after);
        let payload: Value = serde_json::from_str(&after)?;
        assert_eq!(payload["xray_tun"]["tun_mtu"], xray_tun.tun_mtu);
        assert_eq!(payload["dns"][0], json!(["1.1.1.1"]));
        Ok(())
    }

    #[test]
    fn singbox_system_proxy_signature_uses_the_node_even_for_a_direct_profile() -> Result<()> {
        let state = AppState::default();
        let profile = inspect_singbox_document_text(Path::new("direct.json"), "{}");
        let node = node("selected");
        let mut service = SignatureService::from_state(&state);
        service.singbox_profile = Some(&profile);
        let payload: Value =
            serde_json::from_str(&service.transition_signature(Some(&node), true)?)?;
        assert_eq!(payload["mode"], "singbox-proxy");
        assert_eq!(payload["has_proxy_outbound"], false);
        assert_eq!(payload["node_id"], "selected");
        assert_eq!(payload["socks_port"], state.settings.local_socks_port);
        assert!(service.xray_layer_signature(Some(&node))?.is_empty());
        assert!(service.tun_layer_signature()?.is_empty());
        Ok(())
    }

    #[test]
    fn hybrid_tun_tracks_xray_path_and_omits_disabled_proxy_credentials() -> Result<()> {
        let state = AppState {
            settings: AppSettings {
                tun_mode: true,
                core_preference: "auto".into(),
                xray_path: "custom.exe".into(),
                proxy_auth_enabled: false,
                proxy_auth_username: "user".into(),
                proxy_auth_password: "password".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut exclusive = node("exclusive");
        exclusive.outbound["streamSettings"] = json!({"finalmask": {"udp": [{}]}});
        let profile = inspect_singbox_document_text(
            Path::new("default.json"),
            r#"{"outbounds":[{"tag":"proxy"}]}"#,
        );
        let mut service = SignatureService::from_state(&state);
        service.singbox_profile = Some(&profile);
        let transition: Value =
            serde_json::from_str(&service.transition_signature(Some(&exclusive), true)?)?;
        assert_eq!(transition["planner_outcome"], "hybrid_xray_sidecar");
        assert_eq!(transition["xray_path"], "custom.exe");
        assert_eq!(transition["tun_runtime"]["proxy_auth_password"], "");
        let layer: Value = serde_json::from_str(&service.tun_layer_signature()?)?;
        assert!(layer.get("xray_path").is_none());
        assert_eq!(layer["planner_outcome"], "native_singbox");
        let enabled = AppSettings {
            proxy_auth_enabled: true,
            ..state.settings.clone()
        };
        service.settings = &enabled;
        let transition: Value =
            serde_json::from_str(&service.transition_signature(Some(&exclusive), true)?)?;
        assert_eq!(transition["tun_runtime"]["proxy_auth_username"], "user");
        Ok(())
    }

    #[test]
    fn regional_preset_changes_generated_runtime_signature() -> Result<()> {
        let state = AppState::default();
        let mut service = SignatureService::from_state(&state);
        service.use_raw_profiles = false;
        let base = service.transition_signature(None, true)?;
        let routing = service.routing_signature();
        let settings = AppSettings {
            regional_preset: "china".into(),
            ..state.settings.clone()
        };
        service.settings = &settings;
        assert_ne!(base, service.transition_signature(None, true)?);
        assert_eq!(routing, service.routing_signature());
        Ok(())
    }
}
