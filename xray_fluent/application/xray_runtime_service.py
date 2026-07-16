from __future__ import annotations

import hashlib
import json
from copy import deepcopy
from typing import TYPE_CHECKING, Any

from ..constants import DEFAULT_DISCORD_SOCKS_PORT, PROXY_HOST
from ..multiplex import apply_xray_multiplex
from ..routing_runtime import apply_xray_gui_routing
from ..xray_inbounds import ensure_xray_mixed_proxy_inbound
from ..xray_fragments import apply_xray_final_fragment, apply_xray_outbound_fragment
from .connection_service import find_free_api_port
from .node_runtime_service import is_native_singbox_only_node, native_singbox_only_message
from .runtime_introspection import extract_xray_runtime_ports
from .runtime_security import clamp_xray_local_inbounds
from .session_state import XrayRuntimeConfig

if TYPE_CHECKING:
    from ..app_controller import AppController
    from ..models import Node


APP_METRICS_API_TAG = "__app_metrics_api"
APP_METRICS_API_INBOUND_TAG = "__app_metrics_api_in"
APP_DISCORD_PROXY_INBOUND_TAG = "discord-socks-in"


def inspect_active_xray_config(controller: AppController) -> tuple:
    path, text = controller.load_active_xray_config_text()
    text_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    has_proxy_outbound = False
    socks_port = 0
    http_port = 0
    api_port = 0
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        payload = None
    if payload is not None:
        ensure_xray_metrics_contract(controller, payload, allocate_port=False)
        has_proxy_outbound = controller._config_has_proxy_outbound(payload)
        socks_port, http_port, api_port = extract_xray_runtime_ports(payload)
    return path, text_hash, has_proxy_outbound, socks_port, http_port, api_port


def ensure_xray_metrics_contract(
    controller: AppController,
    payload: dict[str, Any],
    *,
    allocate_port: bool,
) -> tuple[int, tuple[str, ...]]:
    stats = payload.get("stats")
    if not isinstance(stats, dict):
        payload["stats"] = {}

    policy = controller._ensure_dict(payload, "policy")
    system_policy = controller._ensure_dict(policy, "system")
    system_policy["statsInboundUplink"] = True
    system_policy["statsInboundDownlink"] = True
    system_policy["statsOutboundUplink"] = True
    system_policy["statsOutboundDownlink"] = True

    outbounds = controller._ensure_list(payload, "outbounds")
    api = controller._ensure_dict(payload, "api")
    existing_api_tag = str(api.get("tag") or "").strip()
    api_tag = APP_METRICS_API_TAG
    if existing_api_tag:
        for outbound in outbounds:
            if not isinstance(outbound, dict):
                continue
            if str(outbound.get("tag") or "").strip() != existing_api_tag:
                continue
            protocol = str(outbound.get("protocol") or "").strip().lower()
            if protocol in {"freedom", "loopback"}:
                api_tag = existing_api_tag
            break
    api["tag"] = api_tag
    services = api.get("services")
    normalized_services = [str(item) for item in services] if isinstance(services, list) else []
    if "StatsService" not in normalized_services:
        normalized_services.append("StatsService")
    api["services"] = normalized_services

    inbounds = controller._ensure_list(payload, "inbounds")
    existing_ports = controller._collect_xray_inbound_ports(payload)

    preferred_api_port = 0
    for inbound in inbounds:
        if not isinstance(inbound, dict):
            continue
        if str(inbound.get("tag") or "") != APP_METRICS_API_INBOUND_TAG:
            continue
        try:
            preferred_api_port = int(inbound.get("port") or 0)
        except (TypeError, ValueError):
            preferred_api_port = 0
        if preferred_api_port > 0:
            existing_ports.discard(preferred_api_port)
        break

    if preferred_api_port > 0:
        api_port = preferred_api_port
    elif allocate_port:
        try:
            api_port = find_free_api_port(excluded=existing_ports)
        except RuntimeError as exc:
            raise ValueError("Не удалось выделить локальный порт для Xray metrics API.") from exc
    else:
        api_port = 0

    metrics_inbound = {
        "tag": APP_METRICS_API_INBOUND_TAG,
        "listen": PROXY_HOST,
        "port": api_port,
        "protocol": "dokodemo-door",
        "settings": {"address": PROXY_HOST},
    }
    controller._replace_or_append_tagged(inbounds, APP_METRICS_API_INBOUND_TAG, metrics_inbound)

    discord_proxy_enabled = bool(controller.state.settings.discord_proxy_enabled)
    if discord_proxy_enabled and (
        int(DEFAULT_DISCORD_SOCKS_PORT) not in existing_ports or any(
            isinstance(inbound, dict) and str(inbound.get("tag") or "") == APP_DISCORD_PROXY_INBOUND_TAG
            for inbound in inbounds
        )
    ):
        controller._replace_or_append_tagged(
            inbounds,
            APP_DISCORD_PROXY_INBOUND_TAG,
            {
                "tag": APP_DISCORD_PROXY_INBOUND_TAG,
                "listen": PROXY_HOST,
                "port": int(DEFAULT_DISCORD_SOCKS_PORT),
                "protocol": "socks",
                "settings": {
                    "auth": "noauth",
                    "udp": True,
                },
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls"],
                    "routeOnly": False,
                },
            },
        )

    has_api_outbound = any(
        isinstance(outbound, dict) and str(outbound.get("tag") or "") == api_tag
        for outbound in outbounds
    )
    if not has_api_outbound:
        outbounds.append({"tag": api_tag, "protocol": "freedom", "settings": {}})

    user_inbound_tags: list[str] = []
    for index, inbound in enumerate(inbounds):
        if not isinstance(inbound, dict):
            continue
        tag = str(inbound.get("tag") or "").strip()
        if tag == APP_METRICS_API_INBOUND_TAG:
            continue
        if not tag:
            tag = f"__app_user_inbound_{index}"
            inbound["tag"] = tag
        if tag not in user_inbound_tags:
            user_inbound_tags.append(tag)

    routing = controller._ensure_dict(payload, "routing")
    rules = controller._ensure_list(routing, "rules")
    metrics_rule = {
        "type": "field",
        "inboundTag": [APP_METRICS_API_INBOUND_TAG],
        "outboundTag": api_tag,
    }
    discord_proxy_rule = {
        "type": "field",
        "inboundTag": [APP_DISCORD_PROXY_INBOUND_TAG],
        "outboundTag": "proxy",
    }
    replaced = False
    for index, rule in enumerate(rules):
        if not isinstance(rule, dict):
            continue
        inbound_tags = rule.get("inboundTag")
        if isinstance(inbound_tags, list) and APP_METRICS_API_INBOUND_TAG in [str(item) for item in inbound_tags]:
            rules[index] = metrics_rule
            replaced = True
            break
    if not replaced:
        rules.insert(0, metrics_rule)
    if discord_proxy_enabled:
        replaced = False
        for index, rule in enumerate(rules):
            if not isinstance(rule, dict):
                continue
            inbound_tags = rule.get("inboundTag")
            if isinstance(inbound_tags, list) and APP_DISCORD_PROXY_INBOUND_TAG in [str(item) for item in inbound_tags]:
                rules[index] = discord_proxy_rule
                replaced = True
                break
        if not replaced:
            rules.insert(1 if rules and rules[0] == metrics_rule else 0, discord_proxy_rule)

    allow_lan = bool(getattr(controller.state.settings, "proxy_allow_lan", False))
    changed_local_listens = clamp_xray_local_inbounds(payload, allow_lan=allow_lan)
    if changed_local_listens:
        controller._log(f"[xray] local inbounds locked to loopback ({changed_local_listens} change(s))")

    return api_port, tuple(user_inbound_tags)


def build_runtime_xray_config(controller: AppController, node: Node | None = None) -> XrayRuntimeConfig:
    source_path, text = controller.load_active_xray_config_text()
    node_outbound = node.outbound if node is not None and isinstance(node.outbound, dict) else {}
    full_xray_config = (
        node_outbound.get("xray_config")
        if str(node_outbound.get("protocol") or "").strip().lower() == "xray_config"
        else None
    )
    using_full_node_config = isinstance(full_xray_config, dict)
    if using_full_node_config:
        payload = deepcopy(full_xray_config)
    else:
        try:
            payload = json.loads(text)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{source_path.name}: {controller._format_json_error_message(text, exc)}") from exc

    if not isinstance(payload, dict):
        raise ValueError("Корень xray config должен быть JSON-объектом.")

    settings = controller.state.settings
    route_only = bool(getattr(settings, "sniff_route_only", False))
    patched_inbounds = ensure_xray_mixed_proxy_inbound(
        payload,
        socks_port=int(getattr(settings, "local_socks_port", 10808)),
        http_port=int(getattr(settings, "local_http_port", 10809)),
        route_only=route_only,
    )
    if patched_inbounds:
        controller._log(f"[xray] local proxy inbound normalized for mixed mode ({patched_inbounds} change(s))")

    api_port, inbound_tags = controller._ensure_xray_metrics_contract(payload, allocate_port=True)

    outbounds = payload.get("outbounds")
    has_proxy_outbound = bool(using_full_node_config and isinstance(outbounds, list) and outbounds)
    used_selected_node = using_full_node_config
    if isinstance(outbounds, list) and not using_full_node_config:
        for index, outbound in enumerate(outbounds):
            if not isinstance(outbound, dict) or outbound.get("tag") != "proxy":
                continue
            has_proxy_outbound = True
            if node is None:
                raise ValueError("В конфиге есть outbound tag `proxy`. Выберите сервер для запуска xray.")
            if is_native_singbox_only_node(node):
                raise ValueError(native_singbox_only_message(node))
            problem = controller._prepare_node_for_runtime(node)
            if problem:
                raise ValueError(problem)
            proxy_outbound = deepcopy(node.outbound)
            proxy_outbound["tag"] = "proxy"
            apply_xray_multiplex(
                proxy_outbound,
                enabled=controller.state.settings.multiplex_enabled,
                concurrency=controller.state.settings.multiplex_concurrency,
            )
            outbounds[index] = proxy_outbound
            used_selected_node = True
            break

    # A full AUTO profile owns its routing/observatory contract. Replacing the
    # rules here would silently remove balancerTag=auto and disable leastPing.
    if not using_full_node_config:
        apply_xray_gui_routing(payload, controller._runtime_routing(), controller.state.settings)
    if not using_full_node_config and controller.state.settings.enable_xray_fragment:
        patched = apply_xray_outbound_fragment(
            payload,
            packets=controller.state.settings.fragment_packets,
            length=controller.state.settings.fragment_length,
            delay=controller.state.settings.fragment_delay,
            tail_fragment=controller.state.settings.tail_fragment_enabled,
        )
        if patched:
            controller._log(f"[xray] outbound fragment enabled for {patched} outbound(s)")
    if not using_full_node_config and controller.state.settings.enable_final_fragment:
        patched = apply_xray_final_fragment(
            payload,
            packets=controller.state.settings.fragment_packets,
            length=controller.state.settings.fragment_length,
            delay=controller.state.settings.fragment_delay,
            tail_fragment=controller.state.settings.tail_fragment_enabled,
        )
        if patched:
            controller._log(f"[xray] final TLS fragment enabled for {patched} proxy outbound(s)")

    socks_port, http_port, _ = extract_xray_runtime_ports(payload)
    ping_host, ping_port = controller._infer_xray_ping_target(payload, node if used_selected_node else None)
    return XrayRuntimeConfig(
        config=payload,
        source_path=source_path,
        has_proxy_outbound=has_proxy_outbound,
        used_selected_node=used_selected_node,
        socks_port=socks_port,
        http_port=http_port,
        api_port=api_port,
        inbound_tags=inbound_tags,
        ping_host=ping_host,
        ping_port=ping_port,
    )
