from __future__ import annotations

from typing import TYPE_CHECKING, Any

from ..engines.singbox.config_builder import build_singbox_outbound
from .node_runtime_service import proxy_core_for_node

if TYPE_CHECKING:
    from ..models import AppSettings, Node


_XHTTP_MODES = {"", "auto", "packet-up", "stream-up", "stream-one"}


def validate_server_preflight(node: Node | None, settings: AppSettings) -> str | None:
    if node is None:
        return None
    outbound = node.outbound if isinstance(node.outbound, dict) else {}
    protocol = str(outbound.get("protocol") or node.scheme or "").strip().lower()
    stream = outbound.get("streamSettings") if isinstance(outbound.get("streamSettings"), dict) else {}
    tun_singbox = bool(settings.tun_mode and settings.tun_engine == "singbox")

    network = str(stream.get("network") or "").strip().lower()
    if network == "xhttp":
        problem = _validate_xhttp(stream)
        if problem:
            return problem

    if str(stream.get("security") or "").strip().lower() == "reality":
        problem = _validate_reality(stream)
        if problem:
            return problem

    if protocol in {"warp", "wireguard", "awg"}:
        problem = _validate_warp_awg(outbound)
        if problem:
            return problem

    if protocol in {"hysteria", "hysteria2"}:
        native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else outbound
        problem = _validate_hysteria(native)
        if problem:
            return problem

    if protocol == "mieru":
        native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else outbound
        problem = _validate_mieru(native)
        if problem:
            return problem

    if protocol == "masque":
        native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else outbound
        problem = _validate_masque(native)
        if problem:
            return problem

    if protocol == "singbox_config":
        full_config = outbound.get("singbox_config")
        if not isinstance(full_config, dict):
            return "sing-box-extended provider config РїРѕРІСЂРµР¶РґС‘РЅ: РѕС‚СЃСѓС‚СЃС‚РІСѓРµС‚ РїРѕР»РЅС‹Р№ JSON РєРѕРЅС„РёРі."
        if not isinstance(full_config.get("outbounds"), list):
            return "sing-box-extended provider config РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ outbounds."
        return None

    if tun_singbox or proxy_core_for_node(node) == "singbox":
        try:
            build_singbox_outbound(node, tag="proxy")
        except ValueError as exc:
            if protocol in {"vless", "vmess", "trojan", "shadowsocks", "socks", "http"}:
                return None
            return str(exc)

    return None


def _validate_xhttp(stream: dict[str, Any]) -> str | None:
    settings = stream.get("xhttpSettings") if isinstance(stream.get("xhttpSettings"), dict) else {}
    mode = str(settings.get("mode") or "").strip()
    if mode not in _XHTTP_MODES:
        return f"XHTTP mode `{mode}` не поддерживается sing-box extended. Используйте auto, packet-up, stream-up или stream-one."
    extra = settings.get("extra")
    if extra is not None and not isinstance(extra, dict):
        return "XHTTP extra должен быть JSON-объектом."
    return None


def _validate_reality(stream: dict[str, Any]) -> str | None:
    reality = stream.get("realitySettings") if isinstance(stream.get("realitySettings"), dict) else {}
    if not str(reality.get("serverName") or "").strip():
        return "Reality сервер без SNI/serverName не может быть запущен корректно."
    if not str(reality.get("publicKey") or "").strip():
        return "Reality сервер без publicKey не может быть запущен корректно."
    return None


def _validate_warp_awg(outbound: dict[str, Any]) -> str | None:
    native = outbound.get("singbox") if isinstance(outbound.get("singbox"), dict) else outbound
    if not isinstance(native, dict):
        return "WARP/AWG конфиг повреждён: отсутствует sing-box endpoint."
    peers = native.get("peers")
    if isinstance(peers, list):
        if not peers:
            return "WARP/AWG конфиг без peers не может быть запущен."
        peer = peers[0] if isinstance(peers[0], dict) else {}
        if not str(peer.get("address") or "").strip():
            return "WARP/AWG peer без endpoint address не может быть запущен."
    return None


def _validate_hysteria(outbound: dict[str, Any]) -> str | None:
    if not str(outbound.get("server") or "").strip():
        return "Hysteria/Hysteria2 сервер без адреса не может быть запущен."
    if not int(outbound.get("server_port") or outbound.get("port") or 0):
        return "Hysteria/Hysteria2 сервер без порта не может быть запущен."
    return None


def _validate_mieru(outbound: dict[str, Any]) -> str | None:
    if not str(outbound.get("server") or "").strip():
        return "Mieru СЃРµСЂРІРµСЂ Р±РµР· Р°РґСЂРµСЃР° РЅРµ РјРѕР¶РµС‚ Р±С‹С‚СЊ Р·Р°РїСѓС‰РµРЅ."
    has_port = bool(int(outbound.get("server_port") or 0))
    has_ranges = isinstance(outbound.get("server_ports"), list) and bool(outbound.get("server_ports"))
    if not has_port and not has_ranges:
        return "Mieru СЃРµСЂРІРµСЂ РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ server_port РёР»Рё server_ports."
    if str(outbound.get("transport") or "").strip().upper() not in {"TCP", "UDP"}:
        return "Mieru transport РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ TCP РёР»Рё UDP."
    if not str(outbound.get("username") or "").strip() or not str(outbound.get("password") or "").strip():
        return "Mieru СЃРµСЂРІРµСЂ РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ username Рё password."
    return None


def _validate_masque(outbound: dict[str, Any]) -> str | None:
    profile = outbound.get("profile")
    if profile is not None and not isinstance(profile, dict):
        return "MASQUE profile РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ JSON-РѕР±СЉРµРєС‚РѕРј."
    if outbound.get("allowed_ips") is not None and not isinstance(outbound.get("allowed_ips"), list):
        return "MASQUE allowed_ips РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ СЃРїРёСЃРєРѕРј."
    return None
