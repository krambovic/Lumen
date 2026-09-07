from __future__ import annotations

import math
import time
from typing import TYPE_CHECKING

from ..active_profile_health import HEALTH_MAX_AGE_SEC

if TYPE_CHECKING:
    from ..app_controller import AppController
    from ..models import Node

AUTO_SWITCH_HIGH_TICKS_REQUIRED = 10
AUTO_SWITCH_IDLE_BPS = 1024.0
AUTO_SWITCH_HEALTH_GRACE_SEC = 12.0  # compatibility; missing TCP latency is no longer a timer
AUTO_SWITCH_HEALTH_FAILURES_REQUIRED = 3


def reset_health_tracking(controller: AppController) -> None:
    """Also call from the controller's connection/hot-switch reset path."""
    controller._health_down_since = 0.0
    controller._health_failure_count = 0
    controller._health_last_sample_at = 0.0
    controller._health_traffic_seen_at = 0.0
    controller._health_profile_id = ""


def _positive(value: float) -> bool:
    try:
        return math.isfinite(float(value)) and float(value) > 0
    except (TypeError, ValueError, OverflowError):
        return False


def _check_profile_health(
    controller: AppController, now: float, down_bps: float, up_bps: float,
    status: str | None, checked_at: float | None, profile_id: str | None,
) -> bool:
    selected_id = str(controller.state.selected_node_id or "")
    if getattr(controller, "_health_profile_id", "") != selected_id:
        reset_health_tracking(controller)
        controller._health_profile_id = selected_id
    # Traffic is stronger evidence than a probe failure. Do this FIRST, even
    # if latency is None or the active transport is UDP-native / TUN.
    if _positive(down_bps) or _positive(up_bps):
        controller._health_failure_count = 0
        controller._health_down_since = 0.0
        controller._health_traffic_seen_at = now
        return False
    try:
        sampled = float(checked_at) if checked_at is not None else 0.0
    except (TypeError, ValueError, OverflowError):
        sampled = 0.0
    fresh = math.isfinite(sampled) and sampled > 0 and 0 <= now - sampled <= HEALTH_MAX_AGE_SEC
    state = str(status or "UNKNOWN").upper()
    if (not fresh or not selected_id or str(profile_id or "") != selected_id
            or state not in {"HEALTHY", "FAILED"}
            or sampled <= getattr(controller, "_health_traffic_seen_at", 0.0)):
        controller._health_down_since = 0.0
        controller._health_failure_count = 0
        return False
    # Metrics repeat the last health sample between HTTP probes. Count a failed
    # probe once, not once per live-metrics tick. Out-of-order results are ignored.
    if sampled <= getattr(controller, "_health_last_sample_at", 0.0):
        return False
    previous_sample = getattr(controller, "_health_last_sample_at", 0.0)
    if sampled - previous_sample > HEALTH_MAX_AGE_SEC or controller._health_down_since == 0.0:
        # A long silent gap (suspend, delayed delivery) is not a consecutive
        # failure streak. Respect the legacy controller reset marker too.
        controller._health_failure_count = 0
        controller._health_down_since = 0.0
    controller._health_last_sample_at = sampled
    if state == "HEALTHY":
        controller._health_down_since = 0.0
        controller._health_failure_count = 0
        return False
    controller._health_failure_count = getattr(controller, "_health_failure_count", 0) + 1
    if controller._health_down_since == 0.0:
        controller._health_down_since = now
    if controller._health_failure_count < AUTO_SWITCH_HEALTH_FAILURES_REQUIRED:
        return False
    if now - controller._auto_switch_last_switch < controller.state.settings.auto_switch_cooldown_sec:
        return False
    return _perform_auto_switch(
        controller, now,
        "[auto-switch] active profile HTTP health failed repeatedly → switching",
        "auto-switch: profile HTTP health",
    )


def check_auto_switch(
    controller: AppController, down_bps: float, latency_ms: int | None = None, *,
    up_bps: float = 0.0, health_status: str | None = None,
    health_checked_at: float | None = None, health_profile_id: str | None = None,
) -> None:
    """Legacy positional signature is retained; latency is display-only.

    Health switching requires 3 distinct, fresh, active-profile HTTP failures
    plus cooldown. UNKNOWN/unsupported/stale/missing probes never fail a node.
    The separate opt-in sustained-download speed-drop policy is unchanged.
    """
    settings = controller.state.settings
    if not settings.auto_switch_enabled or not controller.connected or controller._switching or controller._reconnecting:
        reset_health_tracking(controller)
        return
    if len(controller.state.nodes) < 2 or controller._auto_switch_exhausted:
        return
    now = time.monotonic()
    if _check_profile_health(controller, now, down_bps, up_bps, health_status, health_checked_at, health_profile_id):
        return
    threshold_bps = settings.auto_switch_threshold_kbps * 1024.0
    if down_bps >= threshold_bps:
        controller._auto_switch_high_ticks += 1
        if controller._auto_switch_high_ticks >= AUTO_SWITCH_HIGH_TICKS_REQUIRED:
            controller._auto_switch_active_download = True
        controller._auto_switch_low_since = 0.0
        return
    if not controller._auto_switch_active_download:
        controller._auto_switch_high_ticks = 0
        return
    if down_bps < AUTO_SWITCH_IDLE_BPS:
        controller._auto_switch_low_since = 0.0
        controller._auto_switch_high_ticks = 0
        controller._auto_switch_active_download = False
        return
    controller._auto_switch_high_ticks = 0
    if controller._auto_switch_low_since == 0.0:
        controller._auto_switch_low_since = now
        return
    low_duration = now - controller._auto_switch_low_since
    if low_duration < settings.auto_switch_delay_sec:
        return
    if now - controller._auto_switch_last_switch < settings.auto_switch_cooldown_sec:
        return
    _perform_auto_switch(
        controller, now,
        f"[auto-switch] speed {down_bps / 1024:.0f} KB/s < {settings.auto_switch_threshold_kbps} KB/s "
        f"for {low_duration:.0f}s → switching",
        "auto-switch: speed drop",
    )


def get_next_node_for_auto_switch(controller: AppController) -> Node | None:
    current_id = controller.state.selected_node_id
    nodes = controller.state.nodes
    if not nodes:
        return None
    current_node = next((node for node in nodes if node.id == current_id), None)
    current_group = (current_node.group or "Default") if current_node is not None else ""
    scoped_nodes = [node for node in nodes if not current_group or (node.group or "Default") == current_group]
    if len(scoped_nodes) < 2:
        return None
    candidates = [
        node for node in scoped_nodes
        if node.id != current_id and node.is_alive is True and node.speed_mbps is not None and node.speed_mbps > 0
    ]
    if candidates:
        return max(candidates, key=lambda node: node.speed_mbps)
    candidates = [node for node in scoped_nodes if node.id != current_id and node.is_alive is True]
    if candidates:
        return min(candidates, key=lambda node: node.ping_ms if node.ping_ms is not None else float("inf"))
    current_idx: int | None = None
    for idx, node in enumerate(scoped_nodes):
        if node.id == current_id:
            current_idx = idx
            break
    if current_idx is None:
        return scoped_nodes[0] if scoped_nodes else None
    next_idx = (current_idx + 1) % len(scoped_nodes)
    return None if scoped_nodes[next_idx].id == current_id else scoped_nodes[next_idx]


def _perform_auto_switch(controller: AppController, now: float, log_message: str, reason: str) -> bool:
    max_attempts = max(1, len(controller.state.nodes) - 1)
    if controller._auto_switch_cycle_attempts >= max_attempts:
        controller._auto_switch_exhausted = True
        controller._auto_switch_low_since = 0.0
        reset_health_tracking(controller)
        controller._auto_switch_active_download = False
        controller.status.emit("warning", "Автопереключение остановлено: все серверы уже проверены")
        controller._log("[auto-switch] exhausted all nodes for current session")
        return False
    next_node = get_next_node_for_auto_switch(controller)
    if not next_node:
        return False
    controller._auto_switch_low_since = 0.0
    reset_health_tracking(controller)
    controller._auto_switch_last_switch = now
    controller._auto_switch_active_download = False
    controller._auto_switch_cycle_attempts += 1
    controller._auto_switch_transitioning = True
    controller._log(log_message)
    controller.auto_switch_triggered.emit(next_node.name)
    controller.state.selected_node_id = next_node.id
    controller.selection_changed.emit(next_node)
    controller.save()
    controller._desired_connected = True
    controller._request_transition(reason)
    return True
