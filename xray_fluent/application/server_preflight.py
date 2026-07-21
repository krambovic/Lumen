from __future__ import annotations

from typing import TYPE_CHECKING, Any

from ..engines.singbox.config_builder import build_singbox_outbound
from .node_runtime_service import proxy_core_for_node

if TYPE_CHECKING:
    from ..models import AppSettings, Node


_XHTTP_MODES = {"", "auto", "packet-up", "stream-up", "stream-one"}
_XRAY_OBSERVATORY_STRATEGIES = {"leastping", "leastload"}


def validate_server_preflight(node: Node | None, settings: AppSettings) -> str | None:
    return None
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    protocol = str(outbound.get("protocol") or node.scheme or "").strip().lower()
    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    tun_singbox = bool(settings.tun_mode)

    if str(stream.get("network") or "").strip().lower() == "xhttp":
        problem = _validate_xhttp(stream)
        if problem:
            return problem

    if str(stream.get("security") or "").strip().lower() == "reality":
        problem = _validate_reality(stream)
        if problem:
            return problem

    validators = {
        "warp": _validate_warp_awg,
        "wireguard": _validate_warp_awg,
        "awg": _validate_warp_awg,
        "hysteria": _validate_hysteria,
        "hysteria2": _validate_hysteria,
        "mieru": _validate_mieru,
        "masque": _validate_masque,
    }
    validator = validators.get(protocol)
    if validator is not None:
        native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else outbound
        problem = validator(native)
        if problem:
            return problem

    if protocol == "singbox_config":
        full_config = outbound.get("singbox_config")
        if not isinstance(full_config, dict):
            return "Конфигурация провайдера sing-box extended повреждена: отсутствует полный JSON-конфиг."
        if not isinstance(full_config.get("outbounds"), list):
            return "Конфигурация провайдера sing-box extended должна содержать список outbounds."
        return None

    if protocol == "xray_config":
        full_config = outbound.get("xray_config")
        if not isinstance(full_config, dict):
            return "AUTO-профиль Xray повреждён: отсутствует полный JSON-конфиг."
        if not isinstance(full_config.get("outbounds"), list):
            return "AUTO-профиль Xray должен содержать список outbounds."
        routing = full_config.get("routing")
        if not isinstance(routing, dict) or not isinstance(routing.get("balancers"), list):
            return "AUTO-профиль Xray не содержит routing.balancers."
        balancers = routing["balancers"]
        has_observer = isinstance(full_config.get("observatory"), dict) or isinstance(
            full_config.get("burstObservatory"), dict
        )
        if _xray_balancers_require_observatory(balancers) and not has_observer:
            return (
                "AUTO-профиль Xray использует leastPing/leastLoad, но не содержит "
                "observatory или burstObservatory."
            )
        return None

    if tun_singbox or proxy_core_for_node(node) == "singbox":
        try:
            build_singbox_outbound(node, tag="proxy")
        except ValueError as exc:
            if protocol in {"vless", "vmess", "trojan", "shadowsocks", "socks", "http"}:
                return None
            return str(exc)
    return None


def _xray_balancers_require_observatory(balancers: list[Any]) -> bool:
    for balancer in balancers:
        if not isinstance(balancer, dict):
            continue
        strategy = balancer.get("strategy")
        if not isinstance(strategy, dict):
            continue
        strategy_type = str(strategy.get("type") or "").strip().lower()
        if strategy_type in _XRAY_OBSERVATORY_STRATEGIES:
            return True
    return False


def _validate_xhttp(stream: dict[str, Any]) -> str | None:
    settings = stream.get("xhttpSettings") if isinstance(stream.get("xhttpSettings"), dict) else {}
    mode = str(settings.get("mode") or "").strip()
    if mode not in _XHTTP_MODES:
        return f"Режим XHTTP `{mode}` не поддерживается sing-box extended. Используйте auto, packet-up, stream-up или stream-one."
    extra = settings.get("extra")
    if extra is not None and not isinstance(extra, dict):
        return "Поле XHTTP extra должно быть JSON-объектом."
    return None


def _validate_reality(stream: dict[str, Any]) -> str | None:
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    if not str(reality.get("serverName") or "").strip():
        return "Сервер Reality без SNI/serverName не может быть запущен корректно."
    if not str(reality.get("publicKey") or "").strip():
        return "Сервер Reality без publicKey не может быть запущен корректно."
    return None


def _validate_warp_awg(outbound: dict[str, Any]) -> str | None:
    if not isinstance(outbound, dict):
        return "Конфигурация WARP/AWG повреждена: отсутствует endpoint sing-box."
    peers = outbound.get("peers")
    if isinstance(peers, list):
        if not peers:
            return "Конфигурация WARP/AWG без peers не может быть запущена."
        peer = peers[0] if isinstance(peers[0], dict) else {}
        if not str(peer.get("address") or peer.get("server") or "").strip():
            return "Peer WARP/AWG без адреса endpoint не может быть запущен."
    return None


def _validate_hysteria(outbound: dict[str, Any]) -> str | None:
    if not str(outbound.get("server") or "").strip():
        return "Сервер Hysteria/Hysteria2 без адреса не может быть запущен."
    if not int(outbound.get("server_port") or outbound.get("port") or 0):
        return "Сервер Hysteria/Hysteria2 без порта не может быть запущен."
    return None


def _validate_mieru(outbound: dict[str, Any]) -> str | None:
    if not str(outbound.get("server") or "").strip():
        return "Сервер Mieru без адреса не может быть запущен."
    has_port = bool(int(outbound.get("server_port") or 0))
    has_ranges = isinstance(outbound.get("server_ports"), list) and bool(outbound.get("server_ports"))
    if not has_port and not has_ranges:
        return "Сервер Mieru должен содержать server_port или server_ports."
    if str(outbound.get("transport") or "").strip().upper() not in {"TCP", "UDP"}:
        return "Транспорт Mieru должен быть TCP или UDP."
    if not str(outbound.get("username") or "").strip() or not str(outbound.get("password") or "").strip():
        return "Сервер Mieru должен содержать username и password."
    return None


def _validate_masque(outbound: dict[str, Any]) -> str | None:
    profile = outbound.get("profile")
    if profile is not None and not isinstance(profile, dict):
        return "Профиль MASQUE должен быть JSON-объектом."
    if outbound.get("allowed_ips") is not None and not isinstance(outbound.get("allowed_ips"), list):
        return "Поле MASQUE allowed_ips должно быть списком."
    return None
