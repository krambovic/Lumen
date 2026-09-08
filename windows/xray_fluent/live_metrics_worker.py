from __future__ import annotations

import threading
import time
from collections.abc import Mapping
from typing import Any

from PyQt6.QtCore import QThread, pyqtSignal

from .active_profile_health import HealthSample, probe_active_profile
from .metrics_api import ClashApiClient, MetricsApiError
from .process_traffic_collector import (
    ProcessTrafficSnapshot, collect_process_stats, connection_tracking_epoch,
)
from .win_proc_monitor import get_proxy_connections
from .xray_stats_client import XrayStatsClient

_MAX_REASONABLE_BYTES_PER_SEC = 256 * 1024 ** 2
_MAX_PLAUSIBLE_CONN_BYTES = 1 * 1024 ** 5


def _metrics_idle_delay(interval: float, wall_elapsed: float, cpu_elapsed: float) -> float:
    # A slow machine must not run telemetry back-to-back forever. Native/JSON
    # work that consumes CPU earns a rest; network wait is not counted as CPU.
    return max(0.05, interval - wall_elapsed, min(30.0, max(0.0, cpu_elapsed) * 4.0))


class LiveMetricsWorker(QThread):
    metrics = pyqtSignal(object)

    def __init__(
        self, xray_path: str, api_port: int, ping_host: str = "", ping_port: int = 0,
        interval_ms: int = 1000, ping_interval_sec: float = 3.0, mode: str = "xray",
        clash_api_port: int = 19090, clash_api_secret: str = "", socks_port: int = 10808,
        http_port: int = 10809, xray_inbound_tags: list[str] | None = None,
        *, active_profile_id: str = "", active_outbound_tag: str = "proxy",
        health_proxy_url: str = "", process_stats_enabled: bool = True,
        process_stats_interval_sec: float = 4.0,
    ):
        """Existing positional arguments remain valid; ping endpoint is legacy.

        Health is HTTP via a verified active Clash outbound (including UDP/TUN),
        or via health_proxy_url ONLY when its listener is guaranteed to route
        the active profile. Missing capability is UNKNOWN, never TCP failure.
        """
        super().__init__()
        self._xray_path, self._api_port = xray_path, api_port
        self._ping_host, self._ping_port = ping_host, ping_port  # compatibility; never dialed
        self._interval_ms = max(250, interval_ms)
        self._ping_interval_sec = max(1.0, ping_interval_sec)  # HTTP health cadence
        self._mode = mode
        self._clash_api_port, self._clash_api_secret = clash_api_port, str(clash_api_secret or "")
        self._socks_port, self._http_port = socks_port, http_port
        self._stopped = False
        self._stop_event = threading.Event()
        self._config_lock = threading.Lock()
        self._health_generation = 0
        self._active_profile_id = str(active_profile_id)
        self._active_outbound_tag = str(active_outbound_tag)
        self._health_proxy_url = str(health_proxy_url)
        self._last_health = HealthSample(profile_id=self._active_profile_id)
        self._last_ping_ms: int | None = None
        self._process_stats_enabled = threading.Event()
        self.set_process_stats_enabled(process_stats_enabled)
        self._process_stats_interval_sec = max(1.0, float(process_stats_interval_sec))
        self._process_stats_prev_ts = 0.0
        self._tracking_epoch = connection_tracking_epoch()
        self._xray_inbound_tags = tuple(dict.fromkeys(str(tag).strip() for tag in xray_inbound_tags or [] if str(tag).strip()))
        self._stats_client = XrayStatsClient(api_port)
        self._clash_client = ClashApiClient(clash_api_port, self._clash_api_secret)
        self._connections_document: Mapping[str, Any] | None = None
        self._outbound_graph: Mapping[str, Any] | None = None
        self._traffic_reason = "not sampled"

    def set_process_stats_enabled(self, enabled: bool) -> None:
        """Hide expensive process details without stopping traffic/health polling."""
        if enabled:
            self._process_stats_enabled.set()
        else:
            self._process_stats_enabled.clear()

    def set_active_profile(self, profile_id: str, *, outbound_tag: str = "proxy", health_proxy_url: str = "") -> None:
        """Invalidate old health when hot-switching without restarting this worker."""
        with self._config_lock:
            self._health_generation += 1
            self._active_profile_id = str(profile_id)
            self._active_outbound_tag = str(outbound_tag)
            self._health_proxy_url = str(health_proxy_url)
            self._last_health = HealthSample(profile_id=self._active_profile_id)

    def stop(self) -> None:
        self._stopped = True
        self._stop_event.set()
        self.requestInterruption()
        self._stats_client.close()
        self._clash_client.close()

    cancel = stop

    def run(self) -> None:
        prev_uplink = prev_downlink = None
        prev_ts: float | None = None
        last_process_ts = float("-inf")
        proxy_prev_bytes: dict[str, tuple[int, int]] = {}
        proxy_total_bytes: dict[str, tuple[int, int]] = {}
        try:
            while not self._stopped:
                tick_started = time.monotonic()
                cpu_started = time.thread_time()
                uplink, downlink = self._query_inbound_totals()
                sampled_at = time.monotonic()
                available = uplink is not None and downlink is not None
                up_bps = down_bps = 0.0
                if available:
                    if prev_ts is not None and prev_uplink is not None and prev_downlink is not None:
                        dt = max(0.001, sampled_at - prev_ts)
                        up_bps = max(0.0, (uplink - prev_uplink) / dt)
                        down_bps = max(0.0, (downlink - prev_downlink) / dt)
                    prev_uplink, prev_downlink, prev_ts = uplink, downlink, sampled_at
                if self._stopped:
                    break
                with self._config_lock:
                    profile_id, tag, proxy_url = self._active_profile_id, self._active_outbound_tag, self._health_proxy_url
                    last_health = self._last_health
                    generation = self._health_generation
                if sampled_at - last_health.checked_at >= self._ping_interval_sec:
                    if self._mode == "singbox":
                        try:
                            self._outbound_graph = self._clash_client.get("/proxies", timeout=0.8)
                        except MetricsApiError:
                            self._outbound_graph = None
                    if self._stopped:
                        break
                    sample = probe_active_profile(
                        profile_id=profile_id, outbound_tag=tag, proxy_url=proxy_url,
                        clash_client=self._clash_client if self._mode == "singbox" else None,
                        outbound_graph=self._outbound_graph,
                    )
                    with self._config_lock:
                        if generation == self._health_generation and profile_id == self._active_profile_id:
                            self._last_health = sample
                if self._stopped:
                    break
                process_stats = None
                if self._process_stats_enabled.is_set() and sampled_at - last_process_ts >= self._process_stats_interval_sec:
                    last_process_ts = sampled_at
                    try:
                        if self._mode == "singbox" and self._connections_document is not None:
                            process_stats = collect_process_stats(
                                self._clash_api_port, clash_api_secret=self._clash_api_secret,
                                connections_document=self._connections_document,
                                outbound_graph=self._outbound_graph, tracking_epoch=self._tracking_epoch,
                            )
                        elif self._mode == "xray":
                            process_stats = self._collect_proxy_process_stats(proxy_prev_bytes, proxy_total_bytes)
                    except Exception:
                        process_stats = None
                if self._stopped:
                    break
                with self._config_lock:
                    health_payload = self._last_health.payload()
                self._last_ping_ms = health_payload["latency_ms"]
                self.metrics.emit({
                    "down_bps": down_bps, "up_bps": up_bps, "process_stats": process_stats,
                    "traffic_available": available, "traffic_reason": self._traffic_reason,
                    "traffic_checked_at": sampled_at if available else 0.0,
                    "upload_total": uplink, "download_total": downlink, **health_payload,
                })
                delay = _metrics_idle_delay(
                    self._interval_ms / 1000.0, time.monotonic() - tick_started,
                    time.thread_time() - cpu_started,
                )
                self._stop_event.wait(delay)
        finally:
            self._stats_client.close()
            self._clash_client.close()

    def _collect_proxy_process_stats(
        self, prev_bytes: dict[str, tuple[int, int]], total_bytes: dict[str, tuple[int, int]],
    ) -> list[ProcessTrafficSnapshot] | None:
        """Validated EStats deltas for sockets to this app's local listeners only.

        First samples establish baselines. Closed-connection aggregate drops and
        implausible counters cannot manufacture session traffic. This retains
        the existing proxy-mode best-effort accounting, never network totals.
        """
        try:
            proxy_procs = get_proxy_connections(self._socks_port, self._http_port)
        except Exception:
            return None
        if not proxy_procs:
            return None
        now = time.monotonic()
        dt = max(0.5, now - self._process_stats_prev_ts) if self._process_stats_prev_ts > 0 else 2.0
        self._process_stats_prev_ts = now
        max_delta = int(_MAX_REASONABLE_BYTES_PER_SEC * dt)
        result: list[ProcessTrafficSnapshot] = []
        for process in proxy_procs:
            has_previous = process.exe in prev_bytes
            prev_in, prev_out = prev_bytes.get(process.exe, (0, 0))
            total_in, total_out = total_bytes.get(process.exe, (0, 0))
            current_in = process.bytes_in if 0 <= process.bytes_in <= _MAX_PLAUSIBLE_CONN_BYTES else prev_in
            current_out = process.bytes_out if 0 <= process.bytes_out <= _MAX_PLAUSIBLE_CONN_BYTES else prev_out
            raw_in, raw_out = max(0, current_in - prev_in), max(0, current_out - prev_out)
            delta_in = raw_in if has_previous and raw_in <= max_delta else 0
            delta_out = raw_out if has_previous and raw_out <= max_delta else 0
            total_in += delta_in
            total_out += delta_out
            total_bytes[process.exe] = total_in, total_out
            prev_bytes[process.exe] = current_in, current_out
            result.append(ProcessTrafficSnapshot(
                exe=process.exe, upload=total_out, download=total_in, connections=process.connections,
                # A local proxy socket does not prove the core used a VPN exit.
                route="unknown", unknown_bytes=total_in + total_out,
                down_speed=delta_in / dt, up_speed=delta_out / dt,
            ))
        result.sort(key=lambda item: item.upload + item.download, reverse=True)
        return result

    def _query_inbound_totals(self) -> tuple[int | None, int | None]:
        if self._mode == "singbox":
            return self._query_clash_api_totals()
        if self._mode == "xray":
            return self._query_xray_stats()
        self._traffic_reason = "metrics unsupported for active core"
        return None, None

    def _query_clash_api_totals(self) -> tuple[int | None, int | None]:
        self._connections_document = None
        try:
            data = self._clash_client.get("/connections", timeout=0.8)
        except MetricsApiError as exc:
            self._traffic_reason = str(exc)
            return None, None
        self._connections_document = data
        upload, download = data.get("uploadTotal"), data.get("downloadTotal")
        if any(
            isinstance(value, bool) or not isinstance(value, int) or value < 0
            for value in (upload, download)
        ):
            self._traffic_reason = "Clash API traffic totals missing or invalid"
            return None, None
        self._traffic_reason = ""
        return upload, download

    def _query_xray_stats(self) -> tuple[int | None, int | None]:
        if not self._xray_inbound_tags:
            self._traffic_reason = "active Xray inbound tags unavailable"
            return None, None
        stats = self._stats_client.query()
        if stats is None:
            self._traffic_reason = self._stats_client.reason
            return None, None
        uplink_keys = [f"inbound>>>{tag}>>>traffic>>>uplink" for tag in self._xray_inbound_tags]
        downlink_keys = [f"inbound>>>{tag}>>>traffic>>>downlink" for tag in self._xray_inbound_tags]
        if any(key not in stats for key in uplink_keys + downlink_keys):
            self._traffic_reason = "active Xray inbound counters unavailable"
            return None, None
        self._traffic_reason = ""
        return sum(stats[key] for key in uplink_keys), sum(stats[key] for key in downlink_keys)
