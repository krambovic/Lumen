from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from .runtime_planner import SingboxRuntimePlan

if TYPE_CHECKING:
    from ...app_controller import AppController
    from ...models import Node


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
    try:
        plan = controller._plan_runtime_singbox(node, tun_mode=tun_mode)
    except ValueError as exc:
        controller._active_core = prev_active_core
        controller._set_connection_status("error", str(exc), level="error")
        return None

    session_label = plan.source_path.name
    if plan.used_selected_node and node is not None:
        session_label = f"{plan.source_path.name} / {node.name}"
    start_message = (
        f"Запуск VPN: {session_label} (sing-box + xray sidecar)..."
        if plan.is_hybrid
        else f"Запуск {'VPN' if tun_mode else 'прокси'}: {session_label}..."
    )
    controller._set_connection_status("starting", start_message, level="info")
    log_domain = "tun" if tun_mode else "proxy"
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
    return start_runtime(controller, node, prev_active_core=prev_active_core, tun_mode=True)


def start_proxy(
    controller: AppController,
    node: Node | None,
    *,
    prev_active_core: str,
) -> SingboxStartResult | None:
    return start_runtime(controller, node, prev_active_core=prev_active_core, tun_mode=False)


def restart_runtime(controller: AppController, reason: str) -> bool:
    node = controller.selected_node
    controller._switching = True
    try:
        controller._log(f"[tun-hot-swap] {reason}")
        try:
            plan = controller._plan_runtime_singbox(node)
        except ValueError as exc:
            controller._set_connection_status("error", str(exc), level="error")
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
        controller._log(f"[tun-hot-swap] routing final={route_final or '--'} dns_final={dns_final or '--'}")
        for line in _runtime_summary_lines(plan.singbox_config):
            controller._log(f"[tun-hot-swap] {line}")
        controller._metrics_request.emit(False)

        if controller.singbox.is_running and not controller.singbox.stop():
            controller._set_connection_status("error", "Не удалось остановить предыдущий процесс sing-box", level="error")
            return False
        if controller.xray.is_running and not controller.xray.stop():
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

        session_node = node if plan.used_selected_node else None
        if session_node is not None:
            session_node.last_used_at = datetime.now(timezone.utc).isoformat()

        ping_host, ping_port = controller._infer_singbox_ping_target(plan.singbox_config, session_node)
        controller._capture_active_session(
            session_node,
            tun=True,
            core="singbox",
            api_port=0,
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
            f"Переключено: {session_label}" + (" (TUN, xray sidecar)" if plan.is_hybrid else " (TUN)"),
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
