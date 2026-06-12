from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any
import uuid

from .constants import ROUTING_RULE, STATE_SCHEMA_VERSION


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


@dataclass(slots=True)
class Node:
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    name: str = ""
    scheme: str = ""
    server: str = ""
    port: int = 0
    link: str = ""
    outbound: dict[str, Any] = field(default_factory=dict)
    group: str = "Default"
    tags: list[str] = field(default_factory=list)
    ping_ms: int | None = None
    last_used_at: str | None = None
    created_at: str = field(default_factory=utc_now_iso)
    country_code: str = ""
    speed_mbps: float | None = None
    is_alive: bool | None = None
    ping_history: list[tuple[str, int | None]] = field(default_factory=list)
    speed_history: list[tuple[str, float | None]] = field(default_factory=list)
    sort_order: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "scheme": self.scheme,
            "server": self.server,
            "port": self.port,
            "link": self.link,
            "outbound": self.outbound,
            "group": self.group,
            "tags": list(self.tags),
            "ping_ms": self.ping_ms,
            "last_used_at": self.last_used_at,
            "created_at": self.created_at,
            "country_code": self.country_code,
            "speed_mbps": self.speed_mbps,
            "is_alive": self.is_alive,
            "ping_history": self.ping_history,
            "speed_history": self.speed_history,
            "sort_order": self.sort_order,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "Node":
        return Node(
            id=str(data.get("id") or uuid.uuid4()),
            name=str(data.get("name") or ""),
            scheme=str(data.get("scheme") or ""),
            server=str(data.get("server") or ""),
            port=int(data.get("port") or 0),
            link=str(data.get("link") or ""),
            outbound=dict(data.get("outbound") or {}),
            group=str(data.get("group") or "Default"),
            tags=list(data.get("tags") or []),
            ping_ms=data.get("ping_ms"),
            last_used_at=data.get("last_used_at"),
            created_at=str(data.get("created_at") or utc_now_iso()),
            country_code=str(data.get("country_code") or ""),
            speed_mbps=data.get("speed_mbps"),
            is_alive=data.get("is_alive"),
            ping_history=data.get("ping_history", []),
            speed_history=data.get("speed_history", []),
            sort_order=int(data.get("sort_order", 0)),
        )


@dataclass(slots=True)
class RoutingSettings:
    mode: str = ROUTING_RULE
    bypass_lan: bool = True
    direct_domains: list[str] = field(default_factory=list)
    proxy_domains: list[str] = field(default_factory=list)
    block_domains: list[str] = field(default_factory=list)
    dns_mode: str = "system"  # system | builtin
    dns_bootstrap_server: str = "1.1.1.1"  # DNS for direct traffic
    dns_bootstrap_type: str = "udp"        # udp | tcp | tls | https
    dns_proxy_server: str = "8.8.8.8"     # DNS for proxy traffic
    dns_proxy_type: str = "https"          # tcp | tls | https
    process_rules: list[dict[str, str]] = field(default_factory=list)  # [{"process": "chrome.exe", "action": "direct|proxy|block"}]
    process_preset_routes: dict[str, str] = field(default_factory=dict)  # {"telegram": "proxy", "windows_system": "direct"}
    service_routes: dict[str, str] = field(default_factory=dict)  # {"youtube": "proxy", "steam": "direct", ...}
    tun_default_outbound: str = "direct"  # "proxy" | "direct"

    def to_dict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "bypass_lan": self.bypass_lan,
            "direct_domains": list(self.direct_domains),
            "proxy_domains": list(self.proxy_domains),
            "block_domains": list(self.block_domains),
            "dns_mode": self.dns_mode,
            "dns_bootstrap_server": self.dns_bootstrap_server,
            "dns_bootstrap_type": self.dns_bootstrap_type,
            "dns_proxy_server": self.dns_proxy_server,
            "dns_proxy_type": self.dns_proxy_type,
            "process_rules": list(self.process_rules),
            "process_preset_routes": dict(self.process_preset_routes),
            "service_routes": dict(self.service_routes),
            "tun_default_outbound": self.tun_default_outbound,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "RoutingSettings":
        return RoutingSettings(
            mode=str(data.get("mode") or ROUTING_RULE),
            bypass_lan=bool(data.get("bypass_lan", True)),
            direct_domains=list(data.get("direct_domains") or []),
            proxy_domains=list(data.get("proxy_domains") or []),
            block_domains=list(data.get("block_domains") or []),
            dns_mode=str(data.get("dns_mode") or "system"),
            dns_bootstrap_server=str(data.get("dns_bootstrap_server") or "1.1.1.1"),
            dns_bootstrap_type=str(data.get("dns_bootstrap_type") or "udp"),
            dns_proxy_server=str(data.get("dns_proxy_server") or "8.8.8.8"),
            dns_proxy_type=str(data.get("dns_proxy_type") or "https"),
            process_rules=list(data.get("process_rules") or []),
            process_preset_routes=dict(data.get("process_preset_routes") or {}),
            service_routes=dict(data.get("service_routes") or {}),
            tun_default_outbound=str(data.get("tun_default_outbound") or "direct"),
        )


@dataclass(slots=True)
class SecuritySettings:
    enabled: bool = False
    password_hash: str = ""
    salt: str = ""
    auto_lock_minutes: int = 15

    def to_dict(self) -> dict[str, Any]:
        return {
            "enabled": self.enabled,
            "password_hash": self.password_hash,
            "salt": self.salt,
            "auto_lock_minutes": self.auto_lock_minutes,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "SecuritySettings":
        return SecuritySettings(
            enabled=bool(data.get("enabled", False)),
            password_hash=str(data.get("password_hash") or ""),
            salt=str(data.get("salt") or ""),
            auto_lock_minutes=int(data.get("auto_lock_minutes") or 15),
        )


@dataclass(slots=True)
class AppSettings:
    theme: str = "system"  # system | light | dark
    accent_color: str = "#0078D4"
    interface_mode: str = "full"  # compact | full
    auto_connect_last: bool = True
    start_minimized: bool = False
    enable_system_proxy: bool = True
    system_proxy_bypass_lan: bool = True
    launch_on_startup: bool = False
    always_run_as_admin: bool = False
    reconnect_on_network_change: bool = True
    xray_path: str = ""
    log_level: str = "warning"
    check_updates: bool = True
    allow_updates: bool = True
    release_channel: str = "stable"  # stable | beta | nightly
    update_feed_url: str = ""
    xray_release_channel: str = "beta"  # stable | beta | nightly
    xray_update_feed_url: str = ""
    xray_auto_update: bool = False
    enable_xray_fragment: bool = False
    enable_final_fragment: bool = True
    discord_proxy_enabled: bool = False
    tun_mode: bool = False
    tun_engine: str = "singbox"  # "singbox" | "xray" | "tun2socks"
    xray_config_file: str = ""
    xray_template_file: str = ""
    singbox_path: str = ""
    singbox_config_file: str = ""
    singbox_template_file: str = ""
    window_width: int = 1000
    window_height: int = 720
    window_x: int = -1
    window_y: int = -1
    zapret_preset: str = ""
    zapret_autostart: bool = False
    auto_switch_enabled: bool = True
    auto_switch_threshold_kbps: int = 50
    auto_switch_delay_sec: int = 30
    auto_switch_cooldown_sec: int = 60
    # Не подключаться автоматически к только что импортированному серверу.
    auto_connect_on_import: bool = False
    # Способ измерения ping: "tcping" | "icmp" | "real" (реальная задержка HTTP).
    ping_method: str = "tcping"
    # Пользовательский URL/размер для теста скорости (пусто = значение по умолчанию).
    speed_test_url: str = ""
    # Число одновременных проверок при тесте (0 = значение по умолчанию).
    speed_test_concurrency: int = 0
    # Интервал авто-обновления подписок в минутах (0 = выключено).
    subscription_auto_update_minutes: int = 240

    def __post_init__(self) -> None:
        self.tun_engine = _normalize_tun_engine(self.tun_engine)

    def to_dict(self) -> dict[str, Any]:
        return {
            "theme": self.theme,
            "accent_color": self.accent_color,
            "interface_mode": self.interface_mode,
            "auto_connect_last": self.auto_connect_last,
            "start_minimized": self.start_minimized,
            "enable_system_proxy": self.enable_system_proxy,
            "system_proxy_bypass_lan": self.system_proxy_bypass_lan,
            "launch_on_startup": self.launch_on_startup,
            "always_run_as_admin": self.always_run_as_admin,
            "reconnect_on_network_change": self.reconnect_on_network_change,
            "xray_path": self.xray_path,
            "log_level": self.log_level,
            "check_updates": self.check_updates,
            "allow_updates": self.allow_updates,
            "release_channel": self.release_channel,
            "update_feed_url": self.update_feed_url,
            "xray_release_channel": self.xray_release_channel,
            "xray_update_feed_url": self.xray_update_feed_url,
            "xray_auto_update": self.xray_auto_update,
            "enable_xray_fragment": self.enable_xray_fragment,
            "enable_final_fragment": self.enable_final_fragment,
            "discord_proxy_enabled": self.discord_proxy_enabled,
            "tun_mode": self.tun_mode,
            "tun_engine": self.tun_engine,
            "xray_config_file": self.xray_config_file,
            "xray_template_file": self.xray_template_file,
            "singbox_path": self.singbox_path,
            "singbox_config_file": self.singbox_config_file,
            "singbox_template_file": self.singbox_template_file,
            "window_width": self.window_width,
            "window_height": self.window_height,
            "window_x": self.window_x,
            "window_y": self.window_y,
            "zapret_preset": self.zapret_preset,
            "zapret_autostart": self.zapret_autostart,
            "auto_switch_enabled": self.auto_switch_enabled,
            "auto_switch_threshold_kbps": self.auto_switch_threshold_kbps,
            "auto_switch_delay_sec": self.auto_switch_delay_sec,
            "auto_switch_cooldown_sec": self.auto_switch_cooldown_sec,
            "auto_connect_on_import": self.auto_connect_on_import,
            "ping_method": self.ping_method,
            "speed_test_url": self.speed_test_url,
            "speed_test_concurrency": self.speed_test_concurrency,
            "subscription_auto_update_minutes": self.subscription_auto_update_minutes,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "AppSettings":
        xray_release_channel = str(data.get("xray_release_channel") or "beta")
        if xray_release_channel == "stable":
            xray_release_channel = "beta"
        return AppSettings(
            theme=str(data.get("theme") or "system"),
            accent_color=str(data.get("accent_color") or "#0078D4"),
            interface_mode=str(data.get("interface_mode") or "full"),
            auto_connect_last=bool(data.get("auto_connect_last", True)),
            start_minimized=bool(data.get("start_minimized", False)),
            enable_system_proxy=bool(data.get("enable_system_proxy", True)),
            system_proxy_bypass_lan=bool(data.get("system_proxy_bypass_lan", True)),
            launch_on_startup=bool(data.get("launch_on_startup", False)),
            always_run_as_admin=bool(data.get("always_run_as_admin", False)),
            reconnect_on_network_change=bool(data.get("reconnect_on_network_change", True)),
            xray_path=str(data.get("xray_path") or ""),
            log_level=str(data.get("log_level") or "warning"),
            check_updates=bool(data.get("check_updates", True)),
            allow_updates=bool(data.get("allow_updates", True)),
            release_channel=str(data.get("release_channel") or "stable"),
            update_feed_url=str(data.get("update_feed_url") or ""),
            xray_release_channel=xray_release_channel,
            xray_update_feed_url=str(data.get("xray_update_feed_url") or ""),
            xray_auto_update=bool(data.get("xray_auto_update", False)),
            enable_xray_fragment=bool(data.get("enable_xray_fragment", False)),
            enable_final_fragment=bool(data.get("enable_final_fragment", True)),
            discord_proxy_enabled=bool(data.get("discord_proxy_enabled", False)),
            tun_mode=bool(data.get("tun_mode", False)),
            tun_engine=_normalize_tun_engine(data.get("tun_engine")),
            xray_config_file=str(data.get("xray_config_file") or ""),
            xray_template_file=str(data.get("xray_template_file") or ""),
            singbox_path=str(data.get("singbox_path") or ""),
            singbox_config_file=str(data.get("singbox_config_file") or ""),
            singbox_template_file=str(data.get("singbox_template_file") or ""),
            window_width=int(data.get("window_width") or 1000),
            window_height=int(data.get("window_height") or 720),
            window_x=int(data.get("window_x", -1)),
            window_y=int(data.get("window_y", -1)),
            zapret_preset=str(data.get("zapret_preset") or ""),
            zapret_autostart=bool(data.get("zapret_autostart", False)),
            auto_switch_enabled=bool(data.get("auto_switch_enabled", True)),
            auto_switch_threshold_kbps=int(data.get("auto_switch_threshold_kbps") or 50),
            auto_switch_delay_sec=int(data.get("auto_switch_delay_sec") or 30),
            auto_switch_cooldown_sec=int(data.get("auto_switch_cooldown_sec") or 60),
            auto_connect_on_import=bool(data.get("auto_connect_on_import", False)),
            ping_method=str(data.get("ping_method") or "tcping"),
            speed_test_url=str(data.get("speed_test_url") or ""),
            speed_test_concurrency=int(data.get("speed_test_concurrency") or 0),
            subscription_auto_update_minutes=int(data.get("subscription_auto_update_minutes") or 240),
        )


@dataclass(slots=True)
class AppState:
    schema_version: int = STATE_SCHEMA_VERSION
    selected_node_id: str | None = None
    nodes: list[Node] = field(default_factory=list)
    routing: RoutingSettings = field(default_factory=RoutingSettings)
    settings: AppSettings = field(default_factory=AppSettings)
    security: SecuritySettings = field(default_factory=SecuritySettings)
    # Импортированные подписки: [{"url", "name", "group", "updated_at", "node_count"}].
    subscriptions: list[dict[str, Any]] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "selected_node_id": self.selected_node_id,
            "nodes": [node.to_dict() for node in self.nodes],
            "routing": self.routing.to_dict(),
            "settings": self.settings.to_dict(),
            "security": self.security.to_dict(),
            "subscriptions": [dict(item) for item in self.subscriptions],
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "AppState":
        nodes_raw = data.get("nodes") or []
        nodes = [Node.from_dict(item) for item in nodes_raw if isinstance(item, dict)]
        return AppState(
            schema_version=int(data.get("schema_version") or STATE_SCHEMA_VERSION),
            selected_node_id=data.get("selected_node_id"),
            nodes=nodes,
            routing=RoutingSettings.from_dict(dict(data.get("routing") or {})),
            settings=AppSettings.from_dict(dict(data.get("settings") or {})),
            security=SecuritySettings.from_dict(dict(data.get("security") or {})),
            subscriptions=[dict(item) for item in (data.get("subscriptions") or []) if isinstance(item, dict)],
        )


def _normalize_tun_engine(value: Any) -> str:
    engine = str(value or "").strip().lower()
    if engine == "tun2socks":
        return "tun2socks"
    # v2rayN-style TUN is sing-box based. Older Lumen KVN builds stored
    # "xray" here, which could produce unstable DNS/routing on Windows.
    return "singbox"
