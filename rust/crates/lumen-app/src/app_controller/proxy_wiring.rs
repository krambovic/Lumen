//! System proxy, Firefox integration, effective port/URL resolution and the
//! Discord Voice proxy toggles.

use std::collections::HashSet;
use std::time::Duration;

use lumen_core::constants::{DEFAULT_DISCORD_SOCKS_PORT, PROXY_HOST};
use lumen_platform::process_conflicts::scan_network_conflicts;
use lumen_platform::startup::is_process_elevated;

use crate::app_controller::{AppController, AppEvent, DelayedAction};

impl AppController {
    /// Python `proxy.enable(http_port, socks_port, ...)`: the ported proxy
    /// manager uses the mixed inbound on `socks_port` for both WinINet and
    /// Firefox.
    pub(crate) fn proxy_enable(
        &mut self,
        _http_port: i64,
        socks_port: i64,
        bypass_lan: bool,
        configure_firefox: bool,
    ) -> Result<(), String> {
        let socks_port =
            u16::try_from(socks_port).map_err(|_| format!("invalid socks port {socks_port}"))?;
        self.proxy
            .enable(socks_port, bypass_lan, configure_firefox)
            .map_err(|error| error.to_string())
    }

    /// Python `proxy.disable(restore_previous=...)`.
    pub(crate) fn proxy_disable(&mut self, restore_previous: bool) {
        if let Err(error) = self.proxy.disable(restore_previous) {
            self.log(&format!("[proxy] disable failed: {error}"));
        }
    }

    /// Python `get_effective_proxy_ports`.
    pub fn get_effective_proxy_ports(&mut self) -> (i64, i64) {
        if let Some(session) = self.active_session.as_ref()
            && session.socks_port > 0
            && session.http_port > 0
        {
            return (session.socks_port, session.http_port);
        }
        let (socks_port, http_port) = match self.inspect_active_xray_config() {
            Ok(inspection) => (inspection.socks_port, inspection.http_port),
            Err(_) => (0, 0),
        };
        if socks_port > 0 && http_port > 0 {
            return (socks_port, http_port);
        }
        (
            self.state.settings.local_socks_port,
            self.state.settings.local_http_port,
        )
    }

    /// Session-or-settings ports for `RuntimeController` (the metrics worker
    /// only starts with a live session, so the profile fallback is moot).
    pub(crate) fn effective_proxy_ports_for_metrics(&self) -> (i64, i64) {
        if let Some(session) = self.active_session.as_ref()
            && session.socks_port > 0
            && session.http_port > 0
        {
            return (session.socks_port, session.http_port);
        }
        (
            self.state.settings.local_socks_port,
            self.state.settings.local_http_port,
        )
    }

    /// Python `get_effective_http_proxy_port`.
    pub fn get_effective_http_proxy_port(&mut self) -> Option<i64> {
        let tun = self
            .active_session
            .as_ref()
            .is_some_and(|session| session.tun_mode)
            || self.state.settings.tun_mode;
        if tun {
            return None;
        }
        let (_, http_port) = self.get_effective_proxy_ports();
        (http_port > 0).then_some(http_port)
    }

    /// Python `get_effective_http_proxy_url`: the live local HTTP proxy URL,
    /// including optional auth.
    pub fn get_effective_http_proxy_url(&mut self) -> Option<String> {
        let port = self.get_effective_http_proxy_port()?;
        let settings = self.state.settings.clone();
        let username = settings.proxy_auth_username.trim();
        let password = settings.proxy_auth_password.as_str();
        let credentials = if settings.proxy_auth_enabled
            && !username.is_empty()
            && !password.is_empty()
        {
            format!(
                "{}:{}@",
                urlencode(username.as_bytes()),
                urlencode(password.as_bytes())
            )
        } else {
            String::new()
        };
        Some(format!("http://{credentials}{PROXY_HOST}:{port}"))
    }
}

/// Python `urllib.parse.quote(value, safe="")`.
pub(crate) fn urlencode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl AppController {
    /// Python `set_discord_proxy_enabled`.
    pub async fn set_discord_proxy_enabled(&mut self, enabled: bool) {
        if enabled && !is_process_elevated() {
            self.emit_status(
                "warning",
                "Discord Voice через WinDivert недоступен без прав администратора.",
            );
            return;
        }
        let tun_active = self.state.settings.tun_mode
            || self
                .active_session
                .as_ref()
                .is_some_and(|session| session.tun_mode);
        if enabled && tun_active {
            self.emit_status(
                "warning",
                "Discord Voice недоступен при включенном TUN: трафик Discord уже идет через VPN.",
            );
            return;
        }
        if enabled && self.state.settings.block_vpn_conflicts {
            let settings = self.state.settings.clone();
            let ports: HashSet<u16> = [
                u16::try_from(settings.local_socks_port).unwrap_or(0),
                u16::try_from(settings.local_http_port).unwrap_or(0),
                DEFAULT_DISCORD_SOCKS_PORT,
            ]
            .into_iter()
            .collect();
            let snapshot = scan_network_conflicts(
                &ports,
                Some(&self.owned_core_process_pids()),
                Some(&self.owned_core_executable_paths()),
            );
            let apps = snapshot
                .get("apps")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let unknown_client = snapshot
                .get("unknown_client")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if !apps.is_empty() || unknown_client {
                let name = if !apps.is_empty() {
                    apps.iter()
                        .take(4)
                        .filter_map(|value| value.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    "другой VPN/прокси-клиент".to_owned()
                };
                self.emit_status(
                    "error",
                    &format!(
                        "Нельзя включить Discord Voice одновременно с другим VPN/прокси-клиентом: {name}. Отключите или закройте его."
                    ),
                );
                return;
            }
        }
        if self.state.settings.discord_proxy_enabled == enabled {
            return;
        }
        self.state.settings.discord_proxy_enabled = enabled;
        self.emit(AppEvent::SettingsChanged(self.state.settings.clone()));
        self.schedule_save();
        if self.connected || self.runtime.desired_connected {
            self.request_transition("discord proxy changed");
        }
        if !enabled {
            let result = self.discord_proxy.disable();
            self.log(&format!("[discord-proxy] disable: {}", result.message));
            self.emit_status(
                if result.ok { "success" } else { "warning" },
                &result.message,
            );
        } else if !self.connected && !self.runtime.desired_connected {
            self.apply_discord_proxy().await;
        }
    }

    /// Python `apply_discord_proxy`.
    pub async fn apply_discord_proxy(&mut self) {
        if !self.state.settings.discord_proxy_enabled {
            return;
        }
        let tun_active = self.state.settings.tun_mode
            || self
                .active_session
                .as_ref()
                .is_some_and(|session| session.tun_mode);
        if tun_active {
            let result = self.discord_proxy.disable();
            self.log(&format!(
                "[discord-proxy] disabled while TUN is active: {}",
                result.message
            ));
            return;
        }
        if !self.connected && !self.runtime.desired_connected {
            self.emit_status(
                "warning",
                "Сначала запустите прокси Lumen, потом включите Discord Voice через прокси",
            );
            return;
        }
        let result = self
            .discord_proxy
            .enable(DEFAULT_DISCORD_SOCKS_PORT as i64)
            .await;
        self.log(&format!("[discord-proxy] enable: {}", result.message));
        self.emit_status(
            if result.ok { "success" } else { "error" },
            &result.message,
        );
    }

    /// InternalEvent::DiscordProxyApply and delayed single-shots reuse the
    /// same queue slot.
    pub(crate) fn schedule_discord_proxy_apply(&mut self) {
        self.schedule_delayed(Duration::ZERO, DelayedAction::DiscordProxy);
    }
}
