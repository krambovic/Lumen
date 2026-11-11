//! Connection lifecycle: connect/disconnect/reconnect/switch through the
//! ported operations flows, session capture, runtime planning and the Zapret
//! server-binding group (`AppController.connect_selected` ... `_reconnect`,
//! `sync_zapret_server_binding` and friends).

use std::time::{Duration, Instant};

use lumen_core::config::proxy_chain::{ProxyChain, resolve_proxy_chain};
use lumen_core::constants::PROXY_HOST;
use lumen_core::models::{AppSettings, Node, RoutingSettings};
use lumen_core::zapret_server_scope::{
    cached_server_ips, make_server_scope, node_endpoint,
};
use lumen_engines::singbox::document::{ParsedSingboxDocument, parse_singbox_document};
use lumen_engines::singbox::operations as singbox_ops;
use lumen_engines::singbox::runtime_planner::{PlanRuntimeOptions, SingboxRuntimePlan};
use lumen_engines::xray::operations::{self as xray_ops, SessionCapture};
use lumen_platform::startup::is_process_elevated;

use crate::app_controller::{AppController, DelayedAction, ZapretResolve};
use crate::connection_service::{
    DisconnectOptions, connect_selected as connect_selected_op,
    disconnect_current as disconnect_current_op, reconnect as reconnect_op,
};
use crate::node_runtime_service::core_preference_of;
use crate::xray_runtime_service::XrayRuntimeConfig;

/// `SessionCapture` assembled from the previous live session (the kwargs of
/// Python `_capture_active_session` inside `_apply_proxy_runtime_change`).
fn session_capture_from_session(
    session: Option<&crate::session_state::ActiveSessionSnapshot>,
    active_core: &str,
    xray_api_port: i64,
    socks_port: i64,
    http_port: i64,
) -> SessionCapture {
    SessionCapture {
        tun: session.map(|session| session.tun_mode).unwrap_or(false),
        core: session
            .map(|session| session.active_core.clone())
            .unwrap_or_else(|| active_core.to_owned()),
        api_port: session
            .map(|session| session.api_port)
            .unwrap_or(xray_api_port),
        clash_api_secret: session
            .map(|session| session.clash_api_secret.clone())
            .unwrap_or_default(),
        clash_api_selector: session
            .map(|session| session.clash_api_selector.clone())
            .unwrap_or_default(),
        clash_api_node_signatures: session
            .map(|session| session.clash_api_node_signatures.clone())
            .unwrap_or_default(),
        hybrid: session.map(|session| session.hybrid).unwrap_or(false),
        socks_port: Some(socks_port),
        http_port: Some(http_port),
        xray_inbound_tags: Some(
            session
                .map(|session| session.xray_inbound_tags.clone())
                .unwrap_or_default(),
        ),
        sidecar_relay_port: session
            .map(|session| session.sidecar_relay_port)
            .unwrap_or(0),
        protect_ss_port: session.map(|session| session.protect_ss_port).unwrap_or(0),
        protect_ss_password: session
            .map(|session| session.protect_ss_password.clone())
            .unwrap_or_default(),
        ping_host: session
            .map(|session| session.ping_host.clone())
            .unwrap_or_default(),
        ping_port: session.map(|session| session.ping_port).unwrap_or(0),
        health_proxy_url: session
            .map(|session| session.health_proxy_url.clone())
            .unwrap_or_default(),
    }
}

impl AppController {
    /// Python `connect_selected`.
    pub fn connect_selected(&mut self, allow_during_reconnect: bool) -> bool {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return false;
        }
        connect_selected_op(self, allow_during_reconnect)
    }

    /// Python `disconnect_current`.
    pub fn disconnect_current(&mut self, disable_proxy: bool, emit_status: bool, fast: bool) -> bool {
        disconnect_current_op(
            self,
            DisconnectOptions {
                disable_proxy,
                emit_status,
                fast,
            },
        )
    }

    /// Python `_reconnect`.
    pub(crate) fn reconnect(&mut self, reason: &str) -> bool {
        reconnect_op(self, reason)
    }

    /// Python `_restart_proxy_core`.
    pub(crate) fn restart_proxy_core(&mut self, reason: &str) -> bool {
        xray_ops::restart_proxy_core(self, reason)
    }

    /// Python `_restart_singbox_runtime`.
    pub(crate) fn restart_singbox_runtime(&mut self, reason: &str) -> bool {
        singbox_ops::restart_runtime(self, reason)
    }

    /// Python `_hot_swap_node`: native sing-box node switch without
    /// restarting its runtime.
    pub(crate) fn hot_swap_node(&mut self, reason: &str) -> bool {
        let Some(session) = self.active_session.clone() else {
            self.runtime.auto_switch_transitioning = false;
            self.auto_switch.finish_transition();
            return false;
        };

        self.xray_api_port = session.api_port;
        self.protect_ss_port = session.protect_ss_port;
        self.protect_ss_password = session.protect_ss_password.clone();

        if self.active_core == "singbox" {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if singbox_ops::try_hot_switch_selector(self, reason) {
                    true
                } else {
                    self.restart_singbox_runtime(reason)
                }
            }));
            // Python `finally:` block.
            self.runtime.auto_switch_transitioning = false;
            self.auto_switch.finish_transition();
            return result.unwrap_or(false);
        }

        self.reconnect(&format!("{reason} (sing-box config change)"))
    }

    /// Python `_apply_proxy_runtime_change`.
    pub(crate) fn apply_proxy_runtime_change(&mut self) -> bool {
        let settings = self.state.settings.clone();
        let session = self.active_session.clone();
        let bypass_lan = self.system_proxy_bypass_lan();
        let (socks_port, http_port) = match session.as_ref() {
            Some(session) => (session.socks_port, session.http_port),
            None => self.get_effective_proxy_ports(),
        };
        let proxy_result = if settings.enable_system_proxy {
            self.proxy_enable(
                http_port,
                socks_port,
                bypass_lan,
                settings.firefox_proxy_integration,
            )
        } else {
            self.proxy_disable(true);
            Ok(())
        };
        if let Err(error) = proxy_result {
            self.set_connection_status(
                "error",
                &format!("Не удалось применить системный прокси: {error}"),
                "error",
            );
            return false;
        }

        let node = if session
            .as_ref()
            .is_none_or(|session| session.node_id.is_some())
        {
            self.selected_node()
        } else {
            None
        };
        if self.connected {
            let capture = session_capture_from_session(
                session.as_ref(),
                &self.active_core,
                self.xray_api_port,
                socks_port,
                http_port,
            );
            self.capture_active_session(node.as_ref(), capture);
            if self.runtime.metrics_running() {
                // Python pushes the new profile into the live worker; the
                // worker is private to ConnectionRuntime, so the sampler is
                // restarted with the captured session instead.
                self.stop_metrics_worker();
                self.start_metrics_worker();
            }
        }
        true
    }

    /// Python `_start_singbox_runtime_plan`.
    pub(crate) fn start_singbox_runtime_plan(&mut self, plan: &SingboxRuntimePlan) -> bool {
        if let Some(sidecar) = plan.xray_sidecar.as_ref() {
            self.protect_ss_port = sidecar.protect_port;
            self.protect_ss_password = sidecar.protect_password.clone();
            self.log(&format!(
                "[tun] starting hybrid xray sidecar relay=127.0.0.1:{} protect=127.0.0.1:{}",
                sidecar.relay_port, sidecar.protect_port
            ));
            let xray_path = self.state.settings.xray_path.clone();
            if !self.xray.start(&xray_path, &sidecar.config, true) {
                self.protect_ss_port = 0;
                self.protect_ss_password = String::new();
                return false;
            }
        } else {
            self.protect_ss_port = 0;
            self.protect_ss_password = String::new();
            if self.xray.is_running() && !self.xray.stop(true, false) {
                self.log("[tun] failed to stop xray before starting sing-box local proxy inbounds");
                return false;
            }
        }

        let sb_ok = self
            .singbox
            .start(&self.state.settings.singbox_path, &plan.singbox_config, true);
        self.log(&format!("[tun] sing-box start result: {sb_ok}"));
        if sb_ok && singbox_ops::enforce_selector_default(self, plan) {
            return true;
        }
        if sb_ok {
            self.singbox.stop(true, false);
        }

        if plan.xray_sidecar.is_some() && self.xray.is_running() {
            self.xray.stop(true, false);
        }
        self.protect_ss_port = 0;
        self.protect_ss_password = String::new();
        false
    }

    /// Python `_build_runtime_xray_config`.
    pub(crate) fn build_runtime_xray_config(
        &mut self,
        node: Option<&Node>,
    ) -> Result<XrayRuntimeConfig, String> {
        let (source_path, text) = self
            .load_active_xray_config_text()
            .map_err(|error| error.to_string())?;
        let node_id = node.map(|node| node.id.clone());
        let mut events = Vec::new();
        let result = crate::xray_runtime_service::build_runtime_xray_config(
            &source_path,
            &text,
            &mut self.state,
            node_id.as_deref(),
            |event| events.push(event),
        );
        for event in events {
            match event {
                crate::xray_runtime_service::XrayRuntimeEvent::Log(line) => self.log(&line),
                crate::xray_runtime_service::XrayRuntimeEvent::NodeChanged(_) => {
                    self.schedule_save();
                }
            }
        }
        result.map_err(|error| error.to_string())
    }

    /// Python `_runtime_routing`.
    pub(crate) fn runtime_routing(&self) -> RoutingSettings {
        self.runtime_routing_for(None, None)
    }

    pub(crate) fn runtime_routing_for(
        &self,
        routing: Option<&RoutingSettings>,
        settings: Option<&AppSettings>,
    ) -> RoutingSettings {
        let settings = settings.unwrap_or(&self.state.settings);
        let routing = routing.unwrap_or(&self.state.routing);
        let routing_value =
            lumen_core::routing_runtime::routing_with_ip_preference(&routing.to_dict(), settings.prefer_ipv6);
        RoutingSettings::from_dict(&routing_value)
    }

    /// Python `_resolve_proxy_chain`: chain hops for the given main server,
    /// or None when the chain is off.
    pub(crate) fn resolve_proxy_chain_for(&self, node: Option<&Node>) -> Option<ProxyChain> {
        let settings = self.state.settings.to_dict();
        resolve_proxy_chain(&settings, &self.state.nodes, node)
    }

    /// Python `_plan_runtime_singbox`.
    pub(crate) fn plan_runtime_singbox(
        &mut self,
        node: Option<&Node>,
        tun_mode: bool,
        hot_switch: bool,
    ) -> Result<SingboxRuntimePlan, String> {
        let state = self
            .singbox_document_state()
            .map_err(|error| error.to_string())?;
        let document_source = state.source_path.clone();
        let document_hash = state.text_hash.clone();
        let document = match self
            .parsed_singbox_document
            .take()
            .filter(|parsed| {
                parsed.source_path == document_source && parsed.text_hash == document_hash
            }) {
            Some(parsed) => parsed,
            None => {
                let parsed = parse_singbox_document(&document_source, &state.text)
                    .map_err(|error| error.to_string())?;
                self.parsed_singbox_document = Some(clone_parsed_document(&parsed));
                parsed
            }
        };
        let route_context = lumen_platform::network_route_context::get_windows_default_route_context(false);
        let system_dns_servers = route_context
            .map(|context| context.dns_servers)
            .unwrap_or_default();
        let mut preferred_relay_port = 0i64;
        let mut preferred_protect_port = 0i64;
        let mut preferred_protect_password = String::new();
        if let Some(session) = self.active_session.as_ref()
            && session.active_core == "singbox"
            && session.hybrid
        {
            preferred_relay_port = session.sidecar_relay_port;
            preferred_protect_port = session.protect_ss_port;
            preferred_protect_password = session.protect_ss_password.clone();
        }
        let routing = self.runtime_routing();
        let mut proxy_chain = self.resolve_proxy_chain_for(node);
        let settings = self.state.settings.clone();
        let hot_switch_nodes = if hot_switch {
            self.state.nodes.clone()
        } else {
            Vec::new()
        };
        let options = PlanRuntimeOptions {
            routing: Some(routing),
            enable_final_fragment: settings.enable_final_fragment,
            fragment_packets: settings.fragment_packets.clone(),
            fragment_length: settings.fragment_length.clone(),
            fragment_delay: settings.fragment_delay.clone(),
            tail_fragment_enabled: settings.tail_fragment_enabled,
            multiplex_enabled: settings.multiplex_enabled,
            multiplex_concurrency: settings.multiplex_concurrency,
            discord_proxy_enabled: settings.discord_proxy_enabled,
            tun_strict_route: settings.tun_strict_route,
            tun_stack: settings.tun_stack.clone(),
            tun_mtu: settings.tun_mtu,
            tun_endpoint_independent_nat: settings.tun_endpoint_independent_nat,
            tun_block_quic: settings.tun_block_quic,
            local_socks_port: settings.local_socks_port,
            local_http_port: settings.local_http_port,
            proxy_auth_username: if settings.proxy_auth_enabled {
                settings.proxy_auth_username.clone()
            } else {
                String::new()
            },
            proxy_auth_password: if settings.proxy_auth_enabled {
                settings.proxy_auth_password.clone()
            } else {
                String::new()
            },
            preferred_relay_port,
            preferred_protect_port,
            preferred_protect_password,
            system_dns_servers,
            tun_mode,
            enable_hot_switch: hot_switch,
            hot_switch_nodes,
            proxy_chain: proxy_chain.as_mut(),
            core_preference: core_preference_of(Some(&settings)).to_owned(),
        };
        let mut plan = lumen_engines::singbox::runtime_planner::plan_singbox_runtime(
            &document, node, options,
        )
        .map_err(|error| error.to_string())?;
        if let Some(chain) = proxy_chain.as_ref() {
            for note in &chain.notes {
                self.log(&format!("[chain] {note}"));
            }
            if chain.active() {
                self.log(&format!(
                    "[chain] sing-box: {}",
                    chain.describe(node)
                ));
            }
        }
        if node.is_some() && plan.used_selected_node {
            let health_port = std::net::TcpListener::bind((PROXY_HOST, 0))
                .and_then(|listener| listener.local_addr())
                .ok()
                .map(|addr| addr.port());
            if let Some(health_port) = health_port {
                let outbound_tag = plan
                    .singbox_config
                    .get("route")
                    .and_then(|route| route.get("final"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("proxy")
                    .to_owned();
                match crate::health_listener::add_singbox_health_listener(
                    &mut plan.singbox_config,
                    health_port,
                    &outbound_tag,
                ) {
                    Ok(url) => plan.health_proxy_url = url,
                    Err(_) => self.log("[metrics] dedicated sing-box health listener unavailable"),
                }
            } else {
                self.log("[metrics] dedicated sing-box health listener unavailable");
            }
        }
        Ok(plan)
    }
}

fn clone_parsed_document(document: &ParsedSingboxDocument) -> ParsedSingboxDocument {
    ParsedSingboxDocument {
        source_path: document.source_path.clone(),
        text: document.text.clone(),
        text_hash: document.text_hash.clone(),
        payload: document.payload.clone(),
        has_proxy_outbound: document.has_proxy_outbound,
    }
}

// ── Zapret: autostart and "run together with server" ────────────────────────

impl AppController {
    /// Python `zapret_preset_for_node`: preset bound to the server, `Some("")`
    /// when bound to the current preset, `None` if unbound.
    pub fn zapret_preset_for_node(&self, node_id: Option<&str>) -> Option<String> {
        let key = node_id.unwrap_or("").trim();
        let presets = self.state.settings.zapret_server_presets.as_object();
        let bound = if key.is_empty() {
            None
        } else {
            presets.and_then(|presets| presets.get(key))
        };
        let bound = bound?;
        let bound = bound
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.state.settings.zapret_preset.trim().to_owned());
        Some(bound)
    }

    /// Python `_on_zapret_stopped_for_binding`: any stop (user, crash) cancels
    /// a server lookup that is still running.
    pub(crate) fn on_zapret_stopped_for_binding(&mut self) {
        self.zapret_scope_token += 1;
        self.zapret_waits.binding = None;
        self.zapret_waits.pre_connect = None;
    }

    /// Python `start_zapret_preset`: start a preset by hand, keeping the bound
    /// preset for the server.
    pub fn start_zapret_preset(&mut self, preset: &str) {
        self.zapret.start(preset, self.zapret_desired_scope.clone());
    }

    /// Python `stop_zapret_manual`: stop the manual strategy; the server-only
    /// part keeps running.
    pub fn stop_zapret_manual(&mut self) {
        let scope = self.zapret.server_scope();
        if scope.is_some()
            && !self.zapret.current_preset().is_empty()
            && is_process_elevated()
        {
            let scope = scope.expect("checked above");
            self.log(&format!(
                "[zapret] ручной пресет остановлен, {} остаётся для сервера",
                scope.title()
            ));
            self.zapret.start("", Some(scope));
            return;
        }
        self.zapret.stop();
    }

    /// Python `_zapret_pre_connect_target`: (node, preset, host) when Zapret
    /// must run for the server before the core dials it.
    fn zapret_pre_connect_target(&self, action: &str) -> Option<(Node, String, String)> {
        if !matches!(
            action,
            "connect" | "reconnect" | "proxy_hot_swap" | "tun_hot_swap"
        ) || self.shutting_down()
        {
            return None;
        }
        let node = self.selected_node()?;
        let preset = self.zapret_preset_for_node(Some(&node.id))?;
        if !is_process_elevated() {
            return None;
        }
        let (host, port) = node_endpoint(&node);
        if host.is_empty() || port == 0 {
            return None;
        }
        Some((node, preset, host))
    }

    /// Python `_zapret_pre_connect_host`: server name that still has to be
    /// resolved before the transition may start.
    pub(crate) fn zapret_pre_connect_host(&self, action: &str) -> Result<String, String> {
        let Some((_, _, host)) = self.zapret_pre_connect_target(action) else {
            return Ok(String::new());
        };
        if cached_server_ips(&host).is_empty() {
            Ok(host)
        } else {
            Ok(String::new())
        }
    }

    /// Python `_prepare_zapret_scope_before_connect`: start the server-only
    /// Zapret part before the core dials the server.
    pub(crate) fn prepare_zapret_scope_before_connect(&mut self, action: &str) -> Result<(), String> {
        let Some((node, preset, host)) = self.zapret_pre_connect_target(action) else {
            return Ok(());
        };
        let ips = cached_server_ips(&host);
        if ips.is_empty() {
            return Ok(());
        }
        let Some(scope) = make_server_scope(&node, &preset, &ips) else {
            return Ok(());
        };
        let running = self.zapret.running();
        let current = if running { self.zapret.server_scope() } else { None };
        if let Some(current) = current.as_ref()
            && current.signature() == scope.signature()
            && current.ips == scope.ips
        {
            return Ok(());
        }
        self.zapret_scope_token += 1;
        self.zapret_desired_scope = Some(scope.clone());
        self.log(&format!(
            "[zapret] {} для сервера {} запускается до подключения",
            scope.title(),
            scope.label
        ));
        let manual = if running {
            self.zapret.current_preset()
        } else {
            String::new()
        };
        self.zapret.start(&manual, Some(scope));
        Ok(())
    }

    /// Python `_prefetch_zapret_server_ips`: learn the real addresses of every
    /// bound server in the background.
    pub(crate) fn prefetch_zapret_server_ips(&mut self) {
        if self.shutting_down() {
            return;
        }
        let bindings = self.state.settings.zapret_server_presets.as_object();
        let mut hosts: Vec<String> = Vec::new();
        if let Some(bindings) = bindings {
            for node_id in bindings.keys() {
                let Some(node) = self.get_node_by_id(Some(node_id)) else {
                    continue;
                };
                let (host, _) = node_endpoint(&node);
                if !host.is_empty() && !hosts.contains(&host) && cached_server_ips(&host).is_empty()
                {
                    hosts.push(host);
                }
            }
        }
        for host in hosts {
            self.zapret_dns_submit(host, ZapretResolve::Prefetch);
        }
    }

    fn zapret_dns_submit(&mut self, host: String, purpose: ZapretResolve) {
        let deadline = Instant::now() + Duration::from_secs(6);
        let sender = self.internal.0.clone();
        self.zapret_dns.submit(sender, host, deadline, purpose);
    }

    /// Python `_resolve_zapret_server_ips` + the `_execute_transition_action`
    /// resume callback: resolve the bound server address off the owner task
    /// with a bounded wait, then continue the transition.
    pub(crate) fn begin_zapret_pre_connect_wait(
        &mut self,
        host: &str,
        action: &'static str,
        reason: &str,
        generation: u64,
    ) {
        let deadline = Instant::now() + Duration::from_secs(6);
        self.zapret_waits.pre_connect =
            Some((action, reason.to_owned(), generation, deadline));
        self.schedule_delayed(
            Duration::from_secs(6),
            DelayedAction::ZapretPreConnectTimeout {
                action,
                reason: reason.to_owned(),
                generation,
            },
        );
        self.zapret_dns_submit(host.to_owned(), ZapretResolve::PreConnect {
            action,
            reason: reason.to_owned(),
            generation,
        });
    }

    /// Python `sync_zapret_server_binding`: apply the bound preset to the
    /// connected server's traffic only.
    pub fn sync_zapret_server_binding(&mut self) {
        if self.shutting_down() {
            return;
        }
        let node_id = if self.connected && self.active_session.is_some() {
            self.active_session
                .as_ref()
                .and_then(|session| session.node_id.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let preset = if !node_id.is_empty() {
            self.zapret_preset_for_node(Some(&node_id))
        } else {
            None
        };
        let node = if preset.is_some() {
            self.get_node_by_id(Some(&node_id))
        } else {
            None
        };
        if preset.is_none() || node.is_none() {
            if preset.as_deref() == Some("") && !node_id.is_empty() {
                self.log("[zapret] сервер привязан к Zapret, но пресет не выбран");
            }
            self.zapret_scope_token += 1;
            self.zapret_waits.binding = None;
            self.zapret_desired_scope = None;
            if self.zapret.server_scope().is_some() {
                let manual = self.zapret.current_preset();
                if !manual.is_empty() && is_process_elevated() {
                    self.log(&format!(
                        "[zapret] сервер отключён — остаётся только пресет {manual}"
                    ));
                    self.zapret.start(&manual, None);
                } else {
                    self.log("[zapret] сервер отключён — останавливаю Zapret для сервера");
                    self.zapret.stop();
                }
            }
            self.prefetch_zapret_server_ips();
            return;
        }
        let node = node.expect("checked above");
        let preset = preset.expect("checked above");
        let (host, port) = node_endpoint(&node);
        let label = if !node.name.is_empty() {
            node.name.clone()
        } else if !host.is_empty() {
            host.clone()
        } else {
            node_id.clone()
        };
        if host.is_empty() || port == 0 {
            self.log(&format!(
                "[zapret] у сервера {label} нет адреса — Zapret для него не запущен"
            ));
            return;
        }
        let running = self.zapret.running();
        let current = if running { self.zapret.server_scope() } else { None };
        let probe_ips: Vec<String> = current
            .as_ref()
            .map(|scope| scope.ips.clone())
            .unwrap_or_else(|| vec!["192.0.2.1".into()]);
        let Some(probe) = make_server_scope(&node, &preset, &probe_ips) else {
            self.log(&format!(
                "[zapret] для сервера {label} не найдена стратегия — Zapret для него не запущен"
            ));
            return;
        };
        if let Some(current) = current.as_ref()
            && current.signature() == probe.signature()
        {
            self.zapret_reconnect_signature = None;
            return;
        }
        if !is_process_elevated() {
            if self.zapret_binding_warned_node != node.id {
                self.zapret_binding_warned_node = node.id.clone();
                self.emit_status(
                    "warning",
                    "Zapret для этого сервера не запущен: нужны права администратора",
                );
            }
            return;
        }
        if let Some(current) = current.as_ref()
            && current.node_id == node.id
            && self.zapret_reconnect_signature.as_ref() != Some(&probe.signature())
            && !self.transitions.active
        {
            // The live session was set up through the previous strategy: let the
            // reconnect start winws2 with the new one before the new handshake.
            self.zapret_reconnect_signature = Some(probe.signature());
            self.log(&format!(
                "[zapret] Zapret для сервера {label} изменён — переподключение через {}",
                probe.title()
            ));
            self.transitions.resume_reconnect_pending = true;
            self.request_transition("zapret server strategy changed");
            return;
        }
        self.zapret_scope_token += 1;
        let token = self.zapret_scope_token;
        self.zapret_waits.binding = Some((
            token,
            node.id.clone(),
            preset,
            label,
            host.clone(),
            Instant::now() + Duration::from_secs(6),
        ));
        self.schedule_delayed(
            Duration::from_secs(6),
            DelayedAction::ZapretBindingTimeout { token },
        );
        self.zapret_dns_submit(host, ZapretResolve::Binding { token });
    }

    /// The `apply(ips)` completion of `sync_zapret_server_binding`.
    pub(crate) fn on_zapret_binding_ips(&mut self, token: u64, host: &str, ips: Vec<String>) {
        let Some((pending_token, node_id, preset, label, _, _)) = self.zapret_waits.binding.take()
        else {
            return;
        };
        if pending_token != token {
            return;
        }
        if token != self.zapret_scope_token || self.shutting_down() {
            return;
        }
        if ips.is_empty() {
            self.log(&format!(
                "[zapret] не удалось определить IP сервера {host} — Zapret для сервера не запущен"
            ));
            self.emit_status(
                "warning",
                "Zapret для сервера не запущен: не удалось определить IP сервера",
            );
            return;
        }
        let Some(node) = self.get_node_by_id(Some(&node_id)) else {
            return;
        };
        let Some(scope) = make_server_scope(&node, &preset, &ips) else {
            return;
        };
        self.zapret_desired_scope = Some(scope.clone());
        let manual = if self.zapret.running() {
            self.zapret.current_preset()
        } else {
            String::new()
        };
        self.log(&format!(
            "[zapret] запуск вместе с сервером {label}: {} (только трафик сервера)",
            scope.title()
        ));
        self.zapret.start(&manual, Some(scope));
    }

    /// Python `_maybe_autostart_zapret`.
    pub(crate) fn maybe_autostart_zapret(&mut self) {
        if self.zapret_autostart_done {
            return;
        }
        self.zapret_autostart_done = true;
        let settings = self.state.settings.clone();
        let preset = settings.zapret_preset.trim();
        if !settings.zapret_autostart || preset.is_empty() || !self.zapret.current_preset().is_empty()
        {
            return;
        }
        if !is_process_elevated() {
            self.log("[zapret] автозапуск пропущен: нужны права администратора");
            return;
        }
        self.log(&format!("[zapret] автозапуск пресета {preset}"));
        self.start_zapret_preset(preset);
    }

    /// Python `auto_connect_if_needed`.
    pub fn auto_connect_if_needed(&mut self) {
        if !self.profile_loaded() {
            self.emit_status("warning", "Сначала откройте зашифрованный профиль");
            return;
        }
        self.maybe_autostart_zapret();
        self.prefetch_zapret_server_ips();
        if !self.state.settings.auto_connect_last || self.locked {
            return;
        }
        if self.selected_node().is_none() && !self.can_connect_without_selected_node() {
            return;
        }
        if self.selected_node().is_some() || self.can_connect_without_selected_node() {
            self.runtime.desired_connected = true;
            self.request_transition("auto connect");
        }
    }
}

// ── core event handlers ─────────────────────────────────────────────────────

impl AppController {
    /// Python `_on_core_stopped`.
    pub(crate) fn on_core_stopped(&mut self, core: &str, exit_code: i64) {
        let expected = self.runtime.disconnecting
            || self.runtime.reconnecting
            || self.runtime.switching
            || self.system_shutdown;
        let suffix = if expected { " (expected)" } else { "" };
        self.log(&format!("[{core}] process stopped with code {exit_code}{suffix}"));
    }

    /// Python `_on_core_state_changed`.
    pub(crate) fn on_core_state_changed(&mut self, _running: bool) {
        crate::runtime_services::on_core_state_changed(self);
    }
}
