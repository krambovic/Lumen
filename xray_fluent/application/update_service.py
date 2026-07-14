from __future__ import annotations

from typing import TYPE_CHECKING

from ..constants import PROXY_HOST
from ..engines.xray import XrayCoreUpdateResult, XrayCoreUpdateWorker
from ..qthread_utils import bind_thread_reference

if TYPE_CHECKING:
    from ..app_controller import AppController


def _controller_proxy_url(controller: AppController) -> str | None:
    if not controller.connected:
        return None
    try:
        port = controller.get_effective_http_proxy_port()
    except Exception:
        return None
    return f"http://{PROXY_HOST}:{int(port)}" if port else None


def _start_xray_worker(controller: AppController, *, apply_update: bool) -> None:
    worker = XrayCoreUpdateWorker(
        controller.state.settings.xray_path,
        controller.state.settings.xray_release_channel,
        controller.state.settings.xray_update_feed_url,
        apply_update=apply_update,
        proxy_url=controller._xray_update_proxy_url,
    )
    controller._xray_update_worker = worker
    bind_thread_reference(controller, "_xray_update_worker", worker)
    worker.progress.connect(controller.xray_update_progress.emit)
    worker.done.connect(controller._on_xray_update_worker_done)
    worker.request_disconnect.connect(controller._on_update_disconnect_request)
    worker.start()


def run_xray_core_update(controller: AppController, apply_update: bool, silent: bool = False) -> None:
    if controller._shutting_down:
        return
    if controller._xray_update_worker and controller._xray_update_worker.isRunning():
        if not silent:
            controller.status.emit("info", "Обновление Xray уже выполняется")
        return

    if apply_update and controller.connected:
        # Check first through the live proxy and disconnect only when a verified
        # newer release is actually ready to install.
        controller._xray_update_apply_requested = True
        controller._reconnect_after_xray_update = False
        worker_apply = False
    else:
        controller._reconnect_after_xray_update = False
        controller._xray_update_apply_requested = False
        worker_apply = apply_update

    controller._xray_update_silent = silent
    controller._xray_update_proxy_url = _controller_proxy_url(controller)
    _start_xray_worker(controller, apply_update=worker_apply)

    if not silent:
        message = "Обновление Xray..." if apply_update else "Проверка обновлений Xray..."
        controller.status.emit("info", message)


def on_xray_update_worker_done(controller: AppController, result: XrayCoreUpdateResult) -> None:
    controller.xray_update_result.emit(result)

    if controller._shutting_down:
        controller._xray_update_apply_requested = False
        controller._reconnect_after_xray_update = False
        controller._xray_update_silent = False
        return

    # First pass of a user-requested apply was check-only. Now that we know the
    # real state, either install (dropping the connection only at this point) or
    # leave everything untouched when already up to date / on error.
    if controller._xray_update_apply_requested:
        controller._xray_update_apply_requested = False
        silent = controller._xray_update_silent
        if result.status == "available":
            if not silent:
                controller.status.emit("info", "Обновление Xray...")
            controller._reconnect_after_xray_update = False
            controller._xray_update_silent = silent
            _start_xray_worker(controller, apply_update=True)
            return
        if result.status == "error":
            if not silent:
                controller.status.emit("error", result.message)
            else:
                controller._log(f"[core-update] error: {result.message}")
        else:  # up_to_date — connection stays intact, nothing else to do
            if not silent:
                controller.status.emit("info", result.message)
            else:
                controller._log(f"[core-update] {result.message}")
        controller._xray_update_silent = False
        return

    if result.status == "error":
        if not controller._xray_update_silent:
            controller.status.emit("error", result.message)
        else:
            controller._log(f"[core-update] error: {result.message}")
    elif result.status == "updated":
        if not controller._xray_update_silent:
            controller.status.emit("success", result.message)
        controller._log(f"[core-update] {result.message}")
    elif result.status == "available":
        if not controller._xray_update_silent:
            controller.status.emit("warning", result.message)
        else:
            controller._log(f"[core-update] {result.message}")
    elif result.status == "up_to_date":
        if not controller._xray_update_silent:
            controller.status.emit("info", result.message)
        else:
            controller._log(f"[core-update] {result.message}")

    if controller._reconnect_after_xray_update:
        controller._reconnect_after_xray_update = False
        controller._desired_connected = True
        controller._request_transition("core update reconnect")

    controller._xray_update_silent = False
