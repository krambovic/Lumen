from __future__ import annotations

from datetime import datetime, timezone
from typing import TYPE_CHECKING

from ..connectivity_test import ConnectivityTestWorker
from ..constants import DEFAULT_HTTP_PORT, XRAY_PATH_DEFAULT
from ..path_utils import resolve_configured_path
from ..ping_worker import PingWorker
from ..qthread_utils import bind_thread_reference, retain_thread_until_finished
from ..speed_test_worker import SpeedTestWorker

if TYPE_CHECKING:
    from ..app_controller import AppController
    from ..models import Node


def _clear_ping_measurements(controller: AppController, nodes: list[Node]) -> None:
    changed = False
    for node in nodes:
        if node.ping_ms is None and node.is_alive is None:
            continue
        node.ping_ms = None
        node.is_alive = None
        controller.ping_updated.emit(node.id, None)
        changed = True
    if changed:
        controller.schedule_save()


def _clear_speed_measurements(controller: AppController, nodes: list[Node]) -> None:
    changed = False
    for node in nodes:
        if node.speed_mbps is None and node.is_alive is None:
            continue
        node.speed_mbps = None
        node.is_alive = None
        controller.speed_updated.emit(node.id, None, False)
        controller.speed_progress_updated.emit(node.id, 0)
        changed = True
    if changed:
        controller.schedule_save()


def ping_nodes(
    controller: AppController,
    node_ids: set[str] | None = None,
    method: str | None = None,
) -> None:
    nodes = controller.state.nodes
    if node_ids:
        nodes = [node for node in nodes if node.id in node_ids]
    if not nodes:
        return

    if controller._ping_worker and controller._ping_worker.isRunning():
        previous = controller._ping_worker
        previous.cancel()
        retain_thread_until_finished(
            controller,
            controller._retired_workers,
            previous,
            delete_worker=False,
        )
        controller._ping_worker = None

    resolved_method = (method or controller.state.settings.ping_method or "tcping").strip().lower()
    if resolved_method not in ("tcping", "icmp", "real"):
        resolved_method = "tcping"

    controller._ping_total = len(nodes)
    controller._ping_completed = 0
    controller._ping_node_map = {node.id: node for node in nodes}
    _clear_ping_measurements(controller, nodes)
    controller.bulk_task_progress.emit("ping", 0, controller._ping_total, False)

    if resolved_method == "real":
        # Реальная задержка измеряется через временный xray-прокси (как в v2rayN).
        resolved = resolve_configured_path(
            controller.state.settings.xray_path,
            default_path=XRAY_PATH_DEFAULT,
            use_default_if_empty=True,
            migrate_default_location=True,
        )
        xray_path = str(resolved) if resolved else controller.state.settings.xray_path
        controller._log("[ping] Измеряю реальную задержку через временные прокси")
        real_active_session = controller._active_session
        real_bypass_tun = bool(
            controller.connected
            and real_active_session is not None
            and real_active_session.tun_mode
        )
        worker = SpeedTestWorker(
            nodes,
            xray_path=xray_path,
            routing=controller.state.routing,
            mode="ping",
            concurrency=controller.state.settings.speed_test_concurrency,
            bypass_tun=real_bypass_tun,
        )
        controller._ping_worker = worker
        bind_thread_reference(controller, "_ping_worker", worker)
        worker.ping_result.connect(controller._on_ping_result)
        worker.progress.connect(controller._on_ping_progress)
        worker.completed.connect(controller._on_ping_complete)
        worker.start()
        return

    active_session = controller._active_session
    bypass_tun = bool(controller.connected and active_session is not None and active_session.tun_mode)
    if bypass_tun:
        controller._log("[ping] TUN включен: проверяю серверы через временные прямые маршруты")
    controller._ping_worker = PingWorker(nodes, bypass_tun=bypass_tun, method=resolved_method)
    bind_thread_reference(controller, "_ping_worker", controller._ping_worker)
    controller._ping_worker.result.connect(controller._on_ping_result)
    controller._ping_worker.progress.connect(controller._on_ping_progress)
    controller._ping_worker.completed.connect(controller._on_ping_complete)
    controller._ping_worker.start()


def speed_test_nodes(controller: AppController, node_ids: set[str] | None = None) -> bool:
    nodes = controller.state.nodes
    if node_ids:
        nodes = [node for node in nodes if node.id in node_ids]
    if not nodes:
        return False

    if controller._speed_worker and controller._speed_worker.isRunning():
        controller.status.emit("info", "Тест скорости уже выполняется. Остановите его перед новым запуском.")
        return False

    resolved = resolve_configured_path(
        controller.state.settings.xray_path,
        default_path=XRAY_PATH_DEFAULT,
        use_default_if_empty=True,
        migrate_default_location=True,
    )
    xray_path = str(resolved) if resolved else controller.state.settings.xray_path

    controller._speed_total = len(nodes)
    controller._speed_completed = 0
    controller._speed_node_map = {node.id: node for node in nodes}
    _clear_speed_measurements(controller, nodes)
    controller.bulk_task_progress.emit("speed", 0, controller._speed_total, False)
    active_session = controller._active_session
    bypass_tun = bool(controller.connected and active_session is not None and active_session.tun_mode)
    if bypass_tun:
        controller._log("[speed] TUN включен: тестирую через временные прямые маршруты")
    controller._speed_worker = SpeedTestWorker(
        nodes,
        xray_path=xray_path,
        routing=controller.state.routing,
        test_url=controller.state.settings.speed_test_url,
        concurrency=controller.state.settings.speed_test_concurrency,
        bypass_tun=bypass_tun,
    )
    bind_thread_reference(controller, "_speed_worker", controller._speed_worker)
    controller._speed_worker.result.connect(controller._on_speed_result)
    controller._speed_worker.progress.connect(controller._on_speed_progress)
    controller._speed_worker.node_progress.connect(controller._on_speed_node_progress)
    controller._speed_worker.completed.connect(controller._on_speed_complete)
    controller._speed_worker.start()
    return True


def cancel_speed_test(controller: AppController) -> bool:
    worker = controller._speed_worker
    if worker is None or not worker.isRunning():
        controller.status.emit("info", "Тест скорости сейчас не выполняется")
        return False
    worker.cancel()
    controller.status.emit("info", "Останавливаю тест скорости...")
    return True


def test_connectivity(controller: AppController, url: str | None = None) -> None:
    target = (url or "https://www.gstatic.com/generate_204").strip()
    if not target:
        target = "https://www.gstatic.com/generate_204"

    if controller._connectivity_worker and controller._connectivity_worker.isRunning():
        controller.status.emit("info", "Тест подключения уже выполняется")
        return

    http_port = controller.get_effective_http_proxy_port() or int(controller.state.settings.local_http_port)
    controller._connectivity_worker = ConnectivityTestWorker(http_port, target, tun_mode=controller.state.settings.tun_mode)
    bind_thread_reference(controller, "_connectivity_worker", controller._connectivity_worker)
    controller._connectivity_worker.result.connect(controller._on_connectivity_result)
    controller._connectivity_worker.start()


def on_ping_result(controller: AppController, node_id: str, ping_ms: int | None) -> None:
    if controller.sender() is not controller._ping_worker:
        return
    node = getattr(controller, "_ping_node_map", {}).get(node_id)
    if node is not None:
        node.ping_ms = ping_ms
        node.is_alive = ping_ms is not None
        ts = datetime.now(timezone.utc).isoformat()
        node.ping_history.append((ts, ping_ms))
        if len(node.ping_history) > 50:
            node.ping_history = node.ping_history[-50:]
    controller.ping_updated.emit(node_id, ping_ms)


def on_ping_progress(controller: AppController, current: int, total: int) -> None:
    if controller.sender() is not controller._ping_worker:
        return
    controller._ping_completed = current
    controller.bulk_task_progress.emit("ping", current, total, False)


def on_ping_complete(controller: AppController) -> None:
    if controller.sender() is not controller._ping_worker:
        return
    controller.bulk_task_progress.emit("ping", controller._ping_completed, controller._ping_total, True)
    controller._ping_node_map = {}
    controller.save()


def on_speed_result(controller: AppController, node_id: str, speed_mbps: float | None, is_alive: bool) -> None:
    if controller.sender() is not controller._speed_worker:
        return
    node = getattr(controller, "_speed_node_map", {}).get(node_id)
    if node is not None:
        node.speed_mbps = speed_mbps
        node.is_alive = is_alive
        ts = datetime.now(timezone.utc).isoformat()
        node.speed_history.append((ts, speed_mbps))
        if len(node.speed_history) > 50:
            node.speed_history = node.speed_history[-50:]
        controller.schedule_save()
    controller.speed_updated.emit(node_id, speed_mbps, is_alive)


def on_speed_progress(controller: AppController, current: int, total: int) -> None:
    if controller.sender() is not controller._speed_worker:
        return
    controller._speed_completed = current
    controller.bulk_task_progress.emit("speed", current, total, False)


def on_speed_node_progress(controller: AppController, node_id: str, percent: int) -> None:
    if controller.sender() is not controller._speed_worker:
        return
    controller.speed_progress_updated.emit(node_id, max(0, min(100, int(percent))))


def on_speed_complete(controller: AppController) -> None:
    if controller.sender() is not controller._speed_worker:
        return
    worker = controller._speed_worker
    cancelled = bool(worker.was_cancelled) if worker is not None else False
    completed = worker.completed_nodes if worker is not None else controller._speed_completed
    controller._speed_completed = completed
    if cancelled:
        controller.speed_test_cancelled.emit(completed, controller._speed_total)
    controller.bulk_task_progress.emit("speed", completed, controller._speed_total, True)
    controller._speed_node_map = {}
    controller.save()
    if cancelled:
        controller.status.emit("info", f"Тест скорости остановлен ({completed}/{controller._speed_total})")
    else:
        controller.status.emit("success", "Тест скорости завершён")


def on_connectivity_result(controller: AppController, ok: bool, message: str, elapsed_ms: int | None) -> None:
    if controller.sender() is not controller._connectivity_worker:
        return
    if ok and elapsed_ms is not None:
        text = f"Подключение в порядке: {elapsed_ms} мс"
        controller.status.emit("success", text)
        controller._log(f"[test] {message} ({elapsed_ms} ms)")
    else:
        controller.status.emit("warning", "Тест подключения не пройден")
        controller._log(f"[test] {message}")
    controller.connectivity_test_done.emit(ok, message, elapsed_ms)
