from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
import locale
from typing import Any
import uuid

from .constants import ROUTING_RULE, STATE_SCHEMA_VERSION


DEFAULT_WINDOW_WIDTH = 1280
DEFAULT_WINDOW_HEIGHT = 720
_LEGACY_WINDOW_DEFAULT = (1024, 768)


def _normalize_window_size(width: Any, height: Any) -> tuple[int, int]:
    try:
        w = int(width or DEFAULT_WINDOW_WIDTH)
    except (TypeError, ValueError):
        w = DEFAULT_WINDOW_WIDTH
    try:
        h = int(height or DEFAULT_WINDOW_HEIGHT)
    except (TypeError, ValueError):
        h = DEFAULT_WINDOW_HEIGHT
    if (w, h) == _LEGACY_WINDOW_DEFAULT or w < 640 or h < 360:
        return DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT
    return w, h


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
    preset_id: str = "blocked"  # global | blocked | except_ru | custom
    bypass_lan: bool = True
    direct_domains: list[str] = field(default_factory=list)
    proxy_domains: list[str] = field(default_factory=list)
    block_domains: list[str] = field(default_factory=list)
    dns_mode: str = "system"  # system | builtin
    dns_bootstrap_server: str = "8.8.8.8"  # DNS for direct traffic
    dns_bootstrap_type: str = "udp"        # udp | tcp | tls | https
    dns_bootstrap_strategy: str = "prefer_ipv4"
    dns_proxy_server: str = "8.8.8.8"     # DNS for proxy traffic
    dns_proxy_type: str = "https"          # tcp | tls | https
    dns_proxy_strategy: str = "prefer_ipv4"
    dns_fake_enabled: bool = False
    dns_hijack_enabled: bool = True
    tun_route_exclude_address: list[str] = field(default_factory=list)
    process_rules: list[dict[str, str]] = field(default_factory=list)  # [{"process": "chrome.exe", "action": "direct|proxy|block"}]
    process_preset_routes: dict[str, str] = field(default_factory=dict)  # {"telegram": "proxy", "windows_system": "direct"}
    service_routes: dict[str, str] = field(default_factory=dict)  # {"youtube": "proxy", "steam": "direct", ...}
    tun_default_outbound: str = "direct"  # "proxy" | "direct"

    def to_dict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "preset_id": self.preset_id,
            "bypass_lan": self.bypass_lan,
            "direct_domains": list(self.direct_domains),
            "proxy_domains": list(self.proxy_domains),
            "block_domains": list(self.block_domains),
            "dns_mode": self.dns_mode,
            "dns_bootstrap_server": self.dns_bootstrap_server,
            "dns_bootstrap_type": self.dns_bootstrap_type,
            "dns_bootstrap_strategy": self.dns_bootstrap_strategy,
            "dns_proxy_server": self.dns_proxy_server,
            "dns_proxy_type": self.dns_proxy_type,
            "dns_proxy_strategy": self.dns_proxy_strategy,
            "dns_fake_enabled": self.dns_fake_enabled,
            "dns_hijack_enabled": self.dns_hijack_enabled,
            "tun_route_exclude_address": list(self.tun_route_exclude_address),
            "process_rules": list(self.process_rules),
            "process_preset_routes": dict(self.process_preset_routes),
            "service_routes": dict(self.service_routes),
            "tun_default_outbound": self.tun_default_outbound,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "RoutingSettings":
        preset_id = str(data.get("preset_id") or "").strip().lower()
        if not preset_id:
            direct = {str(item).strip().lower() for item in (data.get("direct_domains") or [])}
            proxy = {str(item).strip().lower() for item in (data.get("proxy_domains") or [])}
            if {"geosite:category-ru", "geoip:ru"} & direct:
                preset_id = "except_ru"
            elif {
                "geosite:ru-blocked",
                "geosite:category-media-ru-blocked",
                "geoip:ru-blocked",
                "geoip:ru-blocked-community",
            } & proxy:
                preset_id = "blocked"
            elif str(data.get("mode") or ROUTING_RULE) == "global":
                preset_id = "global"
            else:
                preset_id = "custom"
        return RoutingSettings(
            mode=str(data.get("mode") or ROUTING_RULE),
            preset_id=preset_id,
            bypass_lan=bool(data.get("bypass_lan", True)),
            direct_domains=list(data.get("direct_domains") or []),
            proxy_domains=list(data.get("proxy_domains") or []),
            block_domains=list(data.get("block_domains") or []),
            dns_mode=str(data.get("dns_mode") or "system"),
            dns_bootstrap_server=str(data.get("dns_bootstrap_server") or "8.8.8.8"),
            dns_bootstrap_type=str(data.get("dns_bootstrap_type") or "udp"),
            dns_bootstrap_strategy=str(data.get("dns_bootstrap_strategy") or "prefer_ipv4"),
            dns_proxy_server=str(data.get("dns_proxy_server") or "8.8.8.8"),
            dns_proxy_type=str(data.get("dns_proxy_type") or "https"),
            dns_proxy_strategy=str(data.get("dns_proxy_strategy") or "prefer_ipv4"),
            dns_fake_enabled=bool(data.get("dns_fake_enabled", False)),
            dns_hijack_enabled=bool(data.get("dns_hijack_enabled", True)),
            tun_route_exclude_address=[
                str(item).strip()
                for item in (data.get("tun_route_exclude_address") or [])
                if str(item).strip()
            ],
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
    language: str = field(default_factory=lambda: _detect_system_language())  # ru | en
    accent_color: str = "#0078D4"
    interface_mode: str = "full"  # compact | full
    auto_connect_last: bool = True
    start_minimized: bool = False
    enable_system_proxy: bool = True
    system_proxy_bypass_lan: bool = True
    launch_on_startup: bool = False
    launch_in_tray_on_startup: bool = True
    always_run_as_admin: bool = False
    reconnect_on_network_change: bool = True
    prefer_ipv6: bool = False
    # Kill-switch: при неожиданном обрыве VPN не выпускать трафик напрямую (fail-closed).
    kill_switch: bool = False
    # Проверять обновления ядер и geoip/geosite при запуске и уведомлять.
    resource_update_check: bool = False
    xray_path: str = ""
    log_level: str = "warning"
    check_updates: bool = True
    allow_updates: bool = True
    app_auto_update: bool = False
    release_channel: str = "stable"  # stable | beta | nightly
    update_feed_url: str = ""
    xray_release_channel: str = "beta"  # stable | beta | nightly
    xray_update_feed_url: str = ""
    xray_auto_update: bool = False
    enable_xray_fragment: bool = False
    enable_final_fragment: bool = False
    fragment_packets: str = "tlshello"
    fragment_length: str = "50-100"
    fragment_delay: str = "10-20"
    tail_fragment_enabled: bool = False
    multiplex_enabled: bool = False
    multiplex_concurrency: int = 8
    discord_proxy_enabled: bool = False
    tun_mode: bool = False
    tun_engine: str = "singbox"
    xray_config_file: str = ""
    xray_template_file: str = ""
    singbox_path: str = ""
    singbox_config_file: str = ""
    singbox_template_file: str = ""
    window_width: int = DEFAULT_WINDOW_WIDTH
    window_height: int = DEFAULT_WINDOW_HEIGHT
    window_x: int = -1
    window_y: int = -1
    zapret_preset: str = ""
    zapret_autostart: bool = False
    auto_switch_enabled: bool = False
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
    # ── Внешний вид 2.0 (Appearance Studio) ──
    ui_density: str = "comfortable"      # comfortable | compact | spacious
    ui_corner_radius: int = 8            # базовый радиус скругления, px
    ui_font_family: str = ""             # "" = системный шрифт по умолчанию
    ui_font_scale: int = 100             # масштаб шрифта, %
    ui_backdrop: str = "mica"            # mica | acrylic | solid
    ui_transparency_strength: int = 50   # 0..100, higher = more transparent
    ui_theme_preset: str = "default"     # пресет палитры
    ui_animations: bool = True           # глобальный тумблер анимаций
    ui_base_tint: str = ""               # "" = выкл; иначе #RRGGBB — свой базовый тон окна (финальный, приглушённый)
    ui_base_tint_src: str = ""           # "#RRGGBB|mute" — состояние пикера (база + приглушение)
    ui_wallpaper: str = ""               # путь к файлу обоев; "" = выкл
    ui_wallpaper_opacity: int = 50       # непрозрачность обоев, %
    ui_wallpaper_blur: int = 10          # размытие обоев, 0..100
    ui_wallpaper_brightness: int = 50    # яркость обоев, 0..100 (100 = оригинал)
    diagnostics_upload_enabled: bool = True
    proxy_allow_lan: bool = False
    tun_strict_route: bool = False
    tun_stack: str = "mixed"
    tun_mtu: int = 9000
    tun_endpoint_independent_nat: bool = False
    tun_block_quic: bool = True
    local_socks_port: int = 10808
    local_http_port: int = 10809
    sniff_route_only: bool = False

    def __post_init__(self) -> None:
        self.tun_engine = _normalize_tun_engine(self.tun_engine)
        self.tun_stack = _normalize_tun_stack(self.tun_stack)
        self.local_socks_port = _normalize_local_port(self.local_socks_port, 10808)
        self.local_http_port = _normalize_local_port(self.local_http_port, 10809)
        if self.local_http_port == self.local_socks_port:
            self.local_http_port = _normalize_local_port(self.local_socks_port + 1, 10809)
        self.tun_mtu = _clamp_tun_mtu(self.tun_mtu)

    def to_dict(self) -> dict[str, Any]:
        return {
            "theme": self.theme,
            "language": self.language,
            "accent_color": self.accent_color,
            "interface_mode": self.interface_mode,
            "auto_connect_last": self.auto_connect_last,
            "start_minimized": self.start_minimized,
            "enable_system_proxy": self.enable_system_proxy,
            "system_proxy_bypass_lan": self.system_proxy_bypass_lan,
            "launch_on_startup": self.launch_on_startup,
            "launch_in_tray_on_startup": self.launch_in_tray_on_startup,
            "always_run_as_admin": self.always_run_as_admin,
            "reconnect_on_network_change": self.reconnect_on_network_change,
            "prefer_ipv6": self.prefer_ipv6,
            "kill_switch": self.kill_switch,
            "resource_update_check": self.resource_update_check,
            "xray_path": self.xray_path,
            "log_level": self.log_level,
            "check_updates": self.check_updates,
            "allow_updates": self.allow_updates,
            "app_auto_update": self.app_auto_update,
            "release_channel": self.release_channel,
            "update_feed_url": self.update_feed_url,
            "xray_release_channel": self.xray_release_channel,
            "xray_update_feed_url": self.xray_update_feed_url,
            "xray_auto_update": self.xray_auto_update,
            "enable_xray_fragment": self.enable_xray_fragment,
            "enable_final_fragment": self.enable_final_fragment,
            "fragment_packets": self.fragment_packets,
            "fragment_length": self.fragment_length,
            "fragment_delay": self.fragment_delay,
            "tail_fragment_enabled": self.tail_fragment_enabled,
            "multiplex_enabled": self.multiplex_enabled,
            "multiplex_concurrency": self.multiplex_concurrency,
            "discord_proxy_enabled": self.discord_proxy_enabled,
            "tun_mode": self.tun_mode,
            "tun_engine": self.tun_engine,
            "proxy_allow_lan": self.proxy_allow_lan,
            "tun_strict_route": self.tun_strict_route,
            "tun_stack": self.tun_stack,
            "tun_mtu": self.tun_mtu,
            "tun_endpoint_independent_nat": self.tun_endpoint_independent_nat,
            "tun_block_quic": self.tun_block_quic,
            "local_socks_port": self.local_socks_port,
            "local_http_port": self.local_http_port,
            "sniff_route_only": self.sniff_route_only,
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
            "ui_density": self.ui_density,
            "ui_corner_radius": self.ui_corner_radius,
            "ui_font_family": self.ui_font_family,
            "ui_font_scale": self.ui_font_scale,
            "ui_backdrop": self.ui_backdrop,
            "ui_transparency_strength": self.ui_transparency_strength,
            "ui_theme_preset": self.ui_theme_preset,
            "ui_animations": self.ui_animations,
            "ui_base_tint": self.ui_base_tint,
            "ui_base_tint_src": self.ui_base_tint_src,
            "ui_wallpaper": self.ui_wallpaper,
            "ui_wallpaper_opacity": self.ui_wallpaper_opacity,
            "ui_wallpaper_blur": self.ui_wallpaper_blur,
            "ui_wallpaper_brightness": self.ui_wallpaper_brightness,
            "diagnostics_upload_enabled": self.diagnostics_upload_enabled,
        }

    @staticmethod
    def from_dict(data: dict[str, Any]) -> "AppSettings":
        xray_release_channel = str(data.get("xray_release_channel") or "beta")
        if xray_release_channel == "stable":
            xray_release_channel = "beta"
        window_width, window_height = _normalize_window_size(
            data.get("window_width"),
            data.get("window_height"),
        )
        return AppSettings(
            theme=str(data.get("theme") or "system"),
            language=_normalize_language(data.get("language")),
            accent_color=str(data.get("accent_color") or "#0078D4"),
            interface_mode=str(data.get("interface_mode") or "full"),
            auto_connect_last=bool(data.get("auto_connect_last", True)),
            start_minimized=bool(data.get("start_minimized", False)),
            enable_system_proxy=bool(data.get("enable_system_proxy", True)),
            system_proxy_bypass_lan=bool(data.get("system_proxy_bypass_lan", True)),
            launch_on_startup=bool(data.get("launch_on_startup", False)),
            launch_in_tray_on_startup=bool(data.get("launch_in_tray_on_startup", True)),
            always_run_as_admin=bool(data.get("always_run_as_admin", False)),
            reconnect_on_network_change=bool(data.get("reconnect_on_network_change", True)),
            prefer_ipv6=bool(data.get("prefer_ipv6", False)),
            kill_switch=bool(data.get("kill_switch", False)),
            resource_update_check=bool(data.get("resource_update_check", False)),
            xray_path=str(data.get("xray_path") or ""),
            log_level=str(data.get("log_level") or "warning"),
            check_updates=bool(data.get("check_updates", True)),
            allow_updates=bool(data.get("allow_updates", True)),
            app_auto_update=bool(data.get("app_auto_update", False)),
            release_channel=str(data.get("release_channel") or "stable"),
            update_feed_url=str(data.get("update_feed_url") or ""),
            xray_release_channel=xray_release_channel,
            xray_update_feed_url=str(data.get("xray_update_feed_url") or ""),
            xray_auto_update=bool(data.get("xray_auto_update", False)),
            enable_xray_fragment=bool(data.get("enable_xray_fragment", False)),
            enable_final_fragment=bool(data.get("enable_final_fragment", False)),
            fragment_packets=str(data.get("fragment_packets") or "tlshello"),
            fragment_length=str(data.get("fragment_length") or "50-100"),
            fragment_delay=str(data.get("fragment_delay") or "10-20"),
            tail_fragment_enabled=bool(data.get("tail_fragment_enabled", False)),
            multiplex_enabled=bool(data.get("multiplex_enabled", False)),
            multiplex_concurrency=int(data.get("multiplex_concurrency") or 8),
            discord_proxy_enabled=bool(data.get("discord_proxy_enabled", False)),
            tun_mode=bool(data.get("tun_mode", False)),
            tun_engine=_normalize_tun_engine(data.get("tun_engine")),
            proxy_allow_lan=bool(data.get("proxy_allow_lan", False)),
            tun_strict_route=bool(data.get("tun_strict_route", False)),
            tun_stack=_normalize_tun_stack(data.get("tun_stack")),
            tun_mtu=_clamp_tun_mtu(data.get("tun_mtu")),
            tun_endpoint_independent_nat=bool(data.get("tun_endpoint_independent_nat", False)),
            tun_block_quic=bool(data.get("tun_block_quic", True)),
            local_socks_port=_normalize_local_port(data.get("local_socks_port", 10808), 10808),
            local_http_port=_normalize_local_port(data.get("local_http_port", 10809), 10809),
            sniff_route_only=bool(data.get("sniff_route_only", False)),
            xray_config_file=str(data.get("xray_config_file") or ""),
            xray_template_file=str(data.get("xray_template_file") or ""),
            singbox_path=str(data.get("singbox_path") or ""),
            singbox_config_file=str(data.get("singbox_config_file") or ""),
            singbox_template_file=str(data.get("singbox_template_file") or ""),
            window_width=window_width,
            window_height=window_height,
            window_x=int(data.get("window_x", -1)),
            window_y=int(data.get("window_y", -1)),
            zapret_preset=str(data.get("zapret_preset") or ""),
            zapret_autostart=bool(data.get("zapret_autostart", False)),
            auto_switch_enabled=bool(data.get("auto_switch_enabled", False)),
            auto_switch_threshold_kbps=int(data.get("auto_switch_threshold_kbps") or 50),
            auto_switch_delay_sec=int(data.get("auto_switch_delay_sec") or 30),
            auto_switch_cooldown_sec=int(data.get("auto_switch_cooldown_sec") or 60),
            auto_connect_on_import=bool(data.get("auto_connect_on_import", False)),
            ping_method=str(data.get("ping_method") or "tcping"),
            speed_test_url=str(data.get("speed_test_url") or ""),
            speed_test_concurrency=int(data.get("speed_test_concurrency") or 0),
            subscription_auto_update_minutes=int(data.get("subscription_auto_update_minutes") if data.get("subscription_auto_update_minutes") is not None else 240),
            ui_density=str(data.get("ui_density") or "comfortable"),
            ui_corner_radius=int(data.get("ui_corner_radius") if data.get("ui_corner_radius") is not None else 8),
            ui_font_family=str(data.get("ui_font_family") or ""),
            ui_font_scale=int(data.get("ui_font_scale") or 100),
            ui_backdrop=str(data.get("ui_backdrop") or "mica"),
            ui_transparency_strength=int(data.get("ui_transparency_strength") if data.get("ui_transparency_strength") is not None else 50),
            ui_theme_preset=str(data.get("ui_theme_preset") or "default"),
            ui_animations=bool(data.get("ui_animations", True)),
            ui_base_tint=str(data.get("ui_base_tint") or ""),
            ui_base_tint_src=str(data.get("ui_base_tint_src") or ""),
            ui_wallpaper=str(data.get("ui_wallpaper") or ""),
            ui_wallpaper_opacity=int(data.get("ui_wallpaper_opacity") if data.get("ui_wallpaper_opacity") is not None else 50),
            ui_wallpaper_blur=int(data.get("ui_wallpaper_blur") if data.get("ui_wallpaper_blur") is not None else 10),
            ui_wallpaper_brightness=int(data.get("ui_wallpaper_brightness") if data.get("ui_wallpaper_brightness") is not None else 50),
            diagnostics_upload_enabled=bool(data.get("diagnostics_upload_enabled", True)),
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
    # Ручные группы без подписки. Нужны, чтобы пустая группа была видна в фильтре.
    manual_groups: list[str] = field(default_factory=list)
    # Кастомные пресеты маршрутизации: [{"id", "name", "routing": {...}}].
    routing_presets: list[dict[str, Any]] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "selected_node_id": self.selected_node_id,
            "nodes": [node.to_dict() for node in self.nodes],
            "routing": self.routing.to_dict(),
            "settings": self.settings.to_dict(),
            "security": self.security.to_dict(),
            "subscriptions": [dict(item) for item in self.subscriptions],
            "manual_groups": list(self.manual_groups),
            "routing_presets": [dict(item) for item in self.routing_presets],
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
            manual_groups=[
                str(item).strip()
                for item in (data.get("manual_groups") or [])
                if str(item).strip()
            ],
            routing_presets=[dict(item) for item in (data.get("routing_presets") or []) if isinstance(item, dict)],
        )


def _normalize_tun_stack(value: Any) -> str:
    stack = str(value or "").strip().lower()
    return stack if stack in {"system", "gvisor", "mixed"} else "mixed"


def _clamp_tun_mtu(value: Any) -> int:
    try:
        mtu = int(value)
    except (TypeError, ValueError):
        return 9000
    return max(1280, min(mtu, 65535))


def _normalize_local_port(value: Any, default: int) -> int:
    try:
        port = int(value)
    except (TypeError, ValueError):
        return default
    if port < 1025 or port > 65535 or port == 10818:  # 10818 is reserved for droute Discord Voice
        return default
    return port


def _normalize_tun_engine(value: Any) -> str:
    # Lumen KVN TUN is sing-box-extended based. Migrate older saved engines to
    # the only supported engine.
    return "singbox"


def _normalize_language(value: Any) -> str:
    language = str(value or "").strip().lower()
    return language if language in {"ru", "en"} else _detect_system_language()


def _detect_system_language() -> str:
    try:
        language = (locale.getlocale()[0] or "").lower()
    except Exception:
        language = ""
    return "ru" if language.startswith("ru") else "en"
