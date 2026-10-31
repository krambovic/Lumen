from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import json
import re
import time
from typing import TYPE_CHECKING, Any

from .clash_api import ClashApiError, SingboxClashApiClient
from .runtime_planner import SingboxRuntimePlan, singbox_node_source_signature, singbox_node_tag

if TYPE_CHECKING:
    from ...app_controller import AppController
    from ...models import Node


_HOT_SWITCH_PRUNE_ATTEMPTS = 5
_CONFIG_ITEM_ERROR = re.compile(r"\b(outbounds|endpoints)\[(\d+)\]")
_SELECTOR_ENFORCE_ATTEMPTS = 4
_SELECTOR_ENFORCE_DELAY = 0.25


@dataclass(slots=True)
class SingboxStartResult:
    plan: SingboxRuntimePlan
    session_label: str


def start_runtime(
    controller: AppController,
    node: Node | None,
    *,
    prev_active_core: str,
    tun_mode: bool,
) -> SingboxStartResult | None:
    controller._active_core = "singbox"
    plan_started = time.monotonic()
    try:
        plan = controller._plan_runtime_singbox(node, tun_mode=tun_mode)
    except (ValueError, RuntimeError) as exc:
        controller._active_core = prev_active_core
        controller._set_connection_status("error", str(exc), level="error")
        return None

    log_domain = "tun" if tun_mode else "proxy"
    controller._log(f"[{log_domain}] runtime plan ready in {int((time.monotonic() - plan_started) * 1000)} ms")

    session_label = plan.source_path.name
    if plan.used_selected_node and node is not None:
        session_label = f"{plan.source_path.name} / {node.name}"
    start_message = (
        f"Запуск VPN: {session_label} (sing-box + xray sidecar)..."
        if plan.is_hybrid
        else f"Запуск {'VPN' if tun_mode else 'прокси'}: {session_label}..."
    )
    controller._set_connection_status("starting", start_message, level="info")
    controller._log(f"[{log_domain}] sing-box planner outcome: {plan.outcome} from {plan.source_path}")
    route = plan.singbox_config.get("route") if isinstance(plan.singbox_config, dict) else {}
    dns = plan.singbox_config.get("dns") if isinstance(plan.singbox_config, dict) else {}
    route_final = route.get("final") if isinstance(route, dict) else ""
    dns_final = dns.get("final") if isinstance(dns, dict) else ""
    controller._log(f"[{log_domain}] routing final={route_final or '--'} dns_final={dns_final or '--'}")
    for line in _runtime_summary_lines(plan.singbox_config):
        controller._log(f"[{log_domain}] {line}")
    if plan.used_selected_node and node is not None:
        if plan.is_hybrid:
            controller._log(
                f"[{log_domain}] outbound tag 'proxy' replaced with local xray relay for unsupported node: {node.name}"
            )
        else:
            controller._log(f"[{log_domain}] outbound tag 'proxy' replaced from selected node: {node.name}")

    validation_started = time.monotonic()
    plan, valid, validation_output = validate_runtime_plan(controller, plan, node, tun_mode=tun_mode)
    controller._log(f"[{log_domain}] core config check completed in {int((time.monotonic() - validation_started) * 1000)} ms")
    if not valid:
        controller._active_core = prev_active_core
        controller._set_connection_status(
            "error",
            f"sing-box не принимает новый конфиг: {validation_output}",
            level="error",
        )
        return None
    if plan.xray_sidecar is not None:
        valid, validation_output = controller.xray.validate_config(
            controller.state.settings.xray_path,
            plan.xray_sidecar.config,
        )
        if not valid:
            controller._active_core = prev_active_core
            controller._set_connection_status(
                "error",
                f"Xray sidecar не принимает новый конфиг: {validation_output}",
                level="error",
            )
            return None

    if not controller._start_singbox_runtime_plan(plan):
        controller._set_connection_status(
            "error",
            (
                "Не удалось запустить sing-box hybrid runtime. Смотрите причину в последних строках лога sing-box."
                if plan.is_hybrid
                else (
                    "Не удалось запустить sing-box TUN runtime. Смотрите причину в последних строках лога sing-box."
                    if tun_mode
                    else "Не удалось запустить sing-box proxy runtime. Смотрите причину в последних строках лога sing-box."
                )
            ),
            level="error",
        )
        controller._active_core = prev_active_core
        return None

    if not tun_mode:
        settings = controller.state.settings
        try:
            if settings.enable_system_proxy:
                controller.proxy.enable(
                    int(settings.local_http_port),
                    int(settings.local_socks_port),
                    bypass_lan=controller._system_proxy_bypass_lan(),
                    configure_firefox=bool(settings.firefox_proxy_integration),
                )
            else:
                controller.proxy.disable(restore_previous=True)
        except Exception as exc:
            controller.singbox.stop(expected=True)
            if controller.xray.is_running:
                controller.xray.stop()
            controller._active_core = prev_active_core
            controller._set_connection_status(
                "error",
                f"Не удалось применить системный прокси для sing-box: {exc}",
                level="error",
            )
            return None

    return SingboxStartResult(plan=plan, session_label=session_label)


def start_tun(
    controller: AppController,
    node: Node | None,
    *,
    prev_active_core: str,
) -> SingboxStartResult | None:
    from ...windows_network import wait_for_adapters_gone

    # After a core switch Xray's tunnel may still hold the default route.
    if not wait_for_adapters_gone(("lumen_xray",), timeout=3.0):
        controller._log("[tun] Xray TUN adapter is still up; starting sing-box TUN anyway")
    return start_runtime(controller, node, prev_active_core=prev_active_core, tun_mode=True)


def start_proxy(
    controller: AppController,
    node: Node | None,
    *,
    prev_active_core: str,
) -> SingboxStartResult | None:
    return start_runtime(controller, node, prev_active_core=prev_active_core, tun_mode=False)


def try_hot_switch_selector(controller: AppController, reason: str) -> bool:
    """Switch a native sing-box selector without restarting its runtime."""
    session = controller._active_session
    node = controller.selected_node
    if (
        session is None
        or node is None
        or session.active_core != "singbox"
        or getattr(session, "node_id", None) == node.id
        or getattr(session, "hybrid", False)
        or not session.clash_api_selector
        or not session.clash_api_secret
    ):
        return False

    settings = getattr(getattr(controller, "state", None), "settings", None)
    if settings is not None and bool(getattr(settings, "tun_mode", session.tun_mode)) != bool(session.tun_mode):
        return False

    node_signatures = dict(getattr(session, "clash_api_node_signatures", ()) or ())
    if not node_signatures:
        return False
    if node_signatures.get(node.id) != singbox_node_source_signature(node):
        controller._log("[clash-api] selected node is not present in the active runtime mapping")
        return False

    target = singbox_node_tag(node.id)
    client = SingboxClashApiClient(secret=session.clash_api_secret)
    try:
        result = client.switch_selector(session.clash_api_selector, target)
    except ClashApiError as exc:
        controller._log(f"[clash-api] selector switch unavailable; using restart fallback: {exc}")
        return False

    controller._log(
        f"[clash-api] switched selector={result.selector} "
        f"{result.previous} -> {result.current}; reason={reason}"
    )
    controller._metrics_request.emit(False)
    node.last_used_at = datetime.now(timezone.utc).isoformat()
    ping_host, ping_port = controller._infer_singbox_ping_target(
        {"outbounds": [{"tag": "proxy", "type": "selector"}]},
        node,
    )
    controller._capture_active_session(
        node,
        tun=bool(session.tun_mode),
        core=session.active_core,
        api_port=session.api_port,
        clash_api_secret=session.clash_api_secret,
        clash_api_selector=session.clash_api_selector,
        clash_api_node_signatures=session.clash_api_node_signatures,
        health_proxy_url=getattr(session, "health_proxy_url", ""),
        hybrid=False,
        socks_port=session.socks_port,
        http_port=session.http_port,
        xray_inbound_tags=session.xray_inbound_tags,
        sidecar_relay_port=0,
        protect_ss_port=0,
        protect_ss_password="",
        ping_host=ping_host,
        ping_port=ping_port,
    )
    label = node.name or node.server
    mode_label = "TUN" if session.tun_mode else "прокси"
    controller._set_connection_status("running", f"Переключено: {label} ({mode_label})", level="success")
    controller.save()
    controller._metrics_request.emit(True)
    return True


def validate_runtime_plan(
    controller: AppController,
    plan: SingboxRuntimePlan,
    node: Node | None,
    *,
    tun_mode: bool,
) -> tuple[SingboxRuntimePlan, bool, str]:
    """Validate a plan; never let another stored node break the selected one.

    The hot-switch selector embeds every stored node, so one malformed import
    would make sing-box reject the whole runtime.  Drop the rejected member
    and retry, then fall back to a plan with only the selected node.
    """
    settings = controller.state.settings
    valid, output = controller.singbox.validate_config(settings.singbox_path, plan.singbox_config)
    if valid or not plan.clash_api_selector:
        return plan, valid, output
    for _ in range(_HOT_SWITCH_PRUNE_ATTEMPTS):
        pruned = _prune_rejected_selector_member(plan, output)
        if not pruned:
            break
        controller._log(f"[hot-switch] sing-box rejected server {pruned}; it is excluded from fast switching")
        valid, output = controller.singbox.validate_config(settings.singbox_path, plan.singbox_config)
        if valid:
            return plan, valid, output
    controller._log("[hot-switch] falling back to a runtime with only the selected server")
    try:
        fallback = controller._plan_runtime_singbox(node, tun_mode=tun_mode, hot_switch=False)
    except (ValueError, RuntimeError):
        return plan, valid, output
    fallback_valid, fallback_output = controller.singbox.validate_config(settings.singbox_path, fallback.singbox_config)
    return fallback, fallback_valid, fallback_output


def _prune_rejected_selector_member(plan: SingboxRuntimePlan, output: str) -> str:
    match = _CONFIG_ITEM_ERROR.search(str(output or ""))
    config = plan.singbox_config
    if match is None or not isinstance(config, dict):
        return ""
    items = config.get(match.group(1))
    index = int(match.group(2))
    if not isinstance(items, list) or index >= len(items) or not isinstance(items[index], dict):
        return ""
    tag = str(items[index].get("tag") or "")
    selector = next(
        (
            item
            for item in config.get("outbounds") or ()
            if isinstance(item, dict) and item.get("type") == "selector" and item.get("tag") == plan.clash_api_selector
        ),
        None,
    )
    if selector is None or not tag or tag == selector.get("default"):
        return ""
    members = [member for member in selector.get("outbounds") or () if member != tag]
    if len(members) == len(selector.get("outbounds") or ()):
        return ""
    # Only the definition and the selector membership may mention the tag.
    if json.dumps(config, ensure_ascii=False).count(json.dumps(tag, ensure_ascii=False)) > 2:
        return ""
    selector["outbounds"] = members
    del items[index]
    plan.clash_api_node_signatures = tuple(
        (node_id, signature)
        for node_id, signature in plan.clash_api_node_signatures
        if singbox_node_tag(node_id) != tag
    )
    return tag


def enforce_selector_default(controller: AppController, plan: SingboxRuntimePlan) -> bool:
    """Point Lumen's hot-switch selector at the planned node after start.

    sing-box restores the last selector choice from cache_file and ignores
    `default` when that tag still exists, so a restart or a fresh connect
    would silently keep the previous server.
    """
    selector = plan.clash_api_selector
    if not selector or not plan.clash_api_secret:
        return True
    target = next(
        (
            str(item.get("default") or "")
            for item in plan.singbox_config.get("outbounds") or ()
            if isinstance(item, dict) and item.get("type") == "selector" and item.get("tag") == selector
        ),
        "",
    )
    if not target:
        return True
    client = SingboxClashApiClient(secret=plan.clash_api_secret)
    error: ClashApiError | None = None
    for attempt in range(_SELECTOR_ENFORCE_ATTEMPTS):
        if attempt:
            time.sleep(_SELECTOR_ENFORCE_DELAY)
        try:
            result = client.switch_selector(selector, target)
        except ClashApiError as exc:
            error = exc
            continue
        if result.changed:
            controller._log(f"[clash-api] restored selector={selector} {result.previous} -> {result.current} (cached choice ignored)")
        return True
    controller._log(f"[clash-api] cannot confirm selected server in sing-box selector: {error}")
    return False


def restart_runtime(controller: AppController, reason: str) -> bool:
    node = controller.selected_node
    tun_mode = bool(controller.state.settings.tun_mode)
    controller._switching = True
    try:
        log_prefix = "tun-hot-swap" if tun_mode else "proxy-hot-swap"
        controller._log(f"[{log_prefix}] {reason}")
        try:
            plan = controller._plan_runtime_singbox(node, tun_mode=tun_mode)
        except (ValueError, RuntimeError) as exc:
            controller._set_connection_status("error", str(exc), level="error")
            return False

        plan, valid, validation_output = validate_runtime_plan(controller, plan, node, tun_mode=tun_mode)
        if not valid:
            controller._set_connection_status(
                "error",
                f"sing-box не принимает новый конфиг: {validation_output}",
                level="error",
            )
            return False
        if plan.xray_sidecar is not None:
            valid, validation_output = controller.xray.validate_config(
                controller.state.settings.xray_path,
                plan.xray_sidecar.config,
            )
            if not valid:
                controller._set_connection_status(
                    "error",
                    f"Xray sidecar не принимает новый конфиг: {validation_output}",
                    level="error",
                )
                return False

        session_label = plan.source_path.name
        if plan.used_selected_node and node is not None:
            session_label = f"{plan.source_path.name} / {node.name}"
        start_message = (
            f"Переключение на {session_label} (sing-box + xray sidecar)..."
            if plan.is_hybrid
            else f"Переключение на {session_label}..."
        )
        controller._set_connection_status("starting", start_message, level="info")
        route = plan.singbox_config.get("route") if isinstance(plan.singbox_config, dict) else {}
        dns = plan.singbox_config.get("dns") if isinstance(plan.singbox_config, dict) else {}
        route_final = route.get("final") if isinstance(route, dict) else ""
        dns_final = dns.get("final") if isinstance(dns, dict) else ""
        controller._log(f"[{log_prefix}] routing final={route_final or '--'} dns_final={dns_final or '--'}")
        for line in _runtime_summary_lines(plan.singbox_config):
            controller._log(f"[{log_prefix}] {line}")
        controller._metrics_request.emit(False)

        if controller.singbox.is_running and not controller.singbox.stop(fast=True):
            controller._set_connection_status("error", "Не удалось остановить предыдущий процесс sing-box", level="error")
            return False
        if controller.xray.is_running and not controller.xray.stop(fast=True):
            controller._set_connection_status("error", "Не удалось остановить предыдущий процесс Xray sidecar", level="error")
            return False

        controller._xray_api_port = 0
        controller._protect_ss_port = 0
        controller._protect_ss_password = ""
        if not controller._start_singbox_runtime_plan(plan):
            controller._set_connection_status(
                "error",
                (
                    "Не удалось перезапустить sing-box hybrid runtime. Смотрите причину в последних строках лога sing-box."
                    if plan.is_hybrid
                    else "Не удалось перезапустить sing-box runtime. Смотрите причину в последних строках лога sing-box."
                ),
                level="error",
            )
            controller._handle_unexpected_disconnect()
            return False

        if not tun_mode:
            settings = controller.state.settings
            try:
                if settings.enable_system_proxy:
                    controller.proxy.enable(
                        int(settings.local_http_port),
                        int(settings.local_socks_port),
                        bypass_lan=controller._system_proxy_bypass_lan(),
                        configure_firefox=bool(settings.firefox_proxy_integration),
                    )
                else:
                    controller.proxy.disable(restore_previous=True)
            except Exception as exc:
                controller._set_connection_status(
                    "error",
                    f"Не удалось применить системный прокси: {exc}",
                    level="error",
                )
                controller._handle_unexpected_disconnect()
                return False

        session_node = node if plan.used_selected_node else None
        if session_node is not None:
            session_node.last_used_at = datetime.now(timezone.utc).isoformat()

        ping_host, ping_port = controller._infer_singbox_ping_target(plan.singbox_config, session_node)
        controller._capture_active_session(
            session_node,
            tun=tun_mode,
            core="singbox",
            api_port=0,
            clash_api_secret=plan.clash_api_secret,
            clash_api_selector=plan.clash_api_selector,
            clash_api_node_signatures=plan.clash_api_node_signatures,
            health_proxy_url=plan.health_proxy_url,
            hybrid=plan.is_hybrid,
            xray_inbound_tags=(),
            sidecar_relay_port=plan.xray_sidecar.relay_port if plan.xray_sidecar else 0,
            protect_ss_port=controller._protect_ss_port,
            protect_ss_password=controller._protect_ss_password,
            ping_host=ping_host,
            ping_port=ping_port,
        )
        controller._set_connection_status(
            "running",
            f"Переключено: {session_label}"
            + (
                " (TUN, xray sidecar)"
                if plan.is_hybrid and tun_mode
                else " (TUN)" if tun_mode else " (прокси)"
            ),
            level="success",
        )
        controller.save()
        return True
    finally:
        controller._switching = False
        _, controller.connected = controller._refresh_connected_state()
        controller.connection_changed.emit(controller.connected)
        controller._metrics_request.emit(controller.connected)


def _runtime_summary_lines(config: dict[str, Any]) -> list[str]:
    lines: list[str] = []
    if not isinstance(config, dict):
        return lines

    dns = config.get("dns")
    if isinstance(dns, dict):
        servers = dns.get("servers") if isinstance(dns.get("servers"), list) else []
        server_tags = [
            str(server.get("tag") or "")
            for server in servers
            if isinstance(server, dict) and str(server.get("tag") or "")
        ]
        fake_enabled = any(
            isinstance(server, dict)
            and str(server.get("tag") or "") == "fake-dns"
            and str(server.get("type") or "") == "fakeip"
            for server in servers
        )
        lines.append(
            "dns servers="
            + ",".join(server_tags)
            + f" final={dns.get('final') or '--'} fake={'on' if fake_enabled else 'off'}"
        )
        rules = dns.get("rules") if isinstance(dns.get("rules"), list) else []
        fake_rules = sum(
            1
            for rule in rules
            if isinstance(rule, dict) and str(rule.get("server") or "") == "fake-dns"
        )
        if fake_enabled:
            lines.append(f"dns fake rules={fake_rules} total_dns_rules={len(rules)}")

    proxy = _find_tagged(config.get("outbounds"), "proxy") or _find_tagged(config.get("endpoints"), "proxy")
    if proxy:
        server = str(proxy.get("server") or "")
        if not server and str(proxy.get("type") or "") in {"wireguard", "warp"}:
            peers = proxy.get("peers") if isinstance(proxy.get("peers"), list) else []
            first_peer = next((peer for peer in peers if isinstance(peer, dict)), {})
            server = str(first_peer.get("address") or "")
        if not server and str(proxy.get("type") or "") in {"openvpn", "openvpn-client"}:
            servers = proxy.get("servers") if isinstance(proxy.get("servers"), list) else []
            first_server = next((item for item in servers if isinstance(item, dict)), {})
            server = str(first_server.get("server") or "")
        tls = proxy.get("tls") if isinstance(proxy.get("tls"), dict) else {}
        server_name = str(tls.get("server_name") or "")
        resolver = proxy.get("domain_resolver")
        resolver_text = resolver if isinstance(resolver, str) else ""
        lines.append(
            f"proxy type={proxy.get('type') or '--'} server={server or '--'}"
            + (f" sni={server_name}" if server_name else "")
            + (f" resolver={resolver_text}" if resolver_text else "")
        )

    tun = next(
        (
            inbound
            for inbound in config.get("inbounds") or []
            if isinstance(inbound, dict) and str(inbound.get("type") or "") == "tun"
        ),
        None,
    )
    if isinstance(tun, dict):
        excludes = tun.get("route_exclude_address")
        excludes_list = [str(item) for item in excludes] if isinstance(excludes, list) else []
        preview = ",".join(excludes_list[:4])
        if len(excludes_list) > 4:
            preview += ",..."
        lines.append(
            f"tun interface={tun.get('interface_name') or '--'} stack={tun.get('stack') or '--'}"
            f" route_exclude={len(excludes_list)}"
            + (f" [{preview}]" if preview else "")
        )
    return lines


def _find_tagged(items: Any, tag: str) -> dict[str, Any] | None:
    if not isinstance(items, list):
        return None
    for item in items:
        if isinstance(item, dict) and str(item.get("tag") or "") == tag:
            return item
    return None
