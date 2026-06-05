"""Worker for v2rayN-style speed testing through temporary xray cores."""

from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from copy import deepcopy
from dataclasses import dataclass
import json
import socket
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from urllib.request import ProxyHandler, Request

from PyQt6.QtCore import QThread, pyqtSignal

from .constants import (
    PROXY_HOST,
    SPEED_TEST_DEFAULT_URL,
    SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT,
    SPEED_TEST_MAX_PING_MS,
    SPEED_TEST_MIN_BYTES_AFTER_GRACE,
    SPEED_TEST_MIN_MBPS_AFTER_GRACE,
    SPEED_TEST_MIXED_CONCURRENCY,
    SPEED_TEST_PING_URL,
    SPEED_TEST_PING_TIMEOUT,
    SPEED_TEST_SLOW_GRACE_SECONDS,
    SPEED_TEST_STARTUP_TIMEOUT,
    SPEED_TEST_TIMEOUT,
)
from .http_utils import build_opener
from .models import Node, RoutingSettings
from .xray_fragments import apply_xray_final_fragment


@dataclass(frozen=True)
class _SpeedTestTarget:
    node: Node
    http_port: int


class SpeedTestWorker(QThread):
    """Tests nodes like v2rayN Mixedtest: ping first, then one speed download."""

    result = pyqtSignal(str, object, bool)   # node_id, speed_mbps (float|None), is_alive
    progress = pyqtSignal(int, int)          # current, total
    node_progress = pyqtSignal(str, int)     # node_id, percent 0..100
    completed = pyqtSignal()

    def __init__(
        self,
        nodes: list[Node],
        xray_path: str,
        routing: RoutingSettings | None = None,
        timeout: float = SPEED_TEST_TIMEOUT,
    ):
        super().__init__()
        self._nodes = list(nodes)
        self._xray_path = xray_path
        self._routing = routing or RoutingSettings()
        self._timeout = timeout
        self._cancelled = False
        self._completed_nodes = 0
        self._processes: set[subprocess.Popen] = set()
        self._process_lock = threading.Lock()

    def cancel(self) -> None:
        self._cancelled = True
        self._terminate_all_processes()

    def _terminate_all_processes(self) -> None:
        with self._process_lock:
            processes = list(self._processes)
        for proc in processes:
            if proc.poll() is None:
                try:
                    proc.terminate()
                except Exception:
                    pass

    @property
    def completed_nodes(self) -> int:
        return self._completed_nodes

    @property
    def was_cancelled(self) -> bool:
        return self._cancelled

    def run(self) -> None:
        total = len(self._nodes)
        self._completed_nodes = 0
        try:
            if not Path(self._xray_path).is_file():
                for node in self._nodes:
                    if self._cancelled:
                        break
                    self._emit_node_result(node, None, False, total)
                return

            for node in self._nodes:
                self.node_progress.emit(node.id, 0)

            max_workers = min(max(1, SPEED_TEST_MIXED_CONCURRENCY), max(1, len(self._nodes)))
            with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="speed-test") as executor:
                pending: set[Future[tuple[Node, float | None, bool]]] = {
                    executor.submit(self._test_node, node)
                    for node in self._nodes
                }

                while pending and not self._cancelled:
                    done, pending = wait(pending, timeout=0.2, return_when=FIRST_COMPLETED)
                    for future in done:
                        if self._cancelled:
                            break
                        try:
                            node, speed, alive = future.result()
                        except Exception:
                            continue
                        self._emit_node_result(node, speed, alive, total)

                if self._cancelled:
                    for future in pending:
                        future.cancel()
        finally:
            self._terminate_all_processes()
            self.completed.emit()

    def _emit_node_result(self, node: Node, speed: float | None, alive: bool, total: int) -> None:
        self._completed_nodes += 1
        self.node_progress.emit(node.id, 100)
        self.result.emit(node.id, speed, alive)
        self.progress.emit(self._completed_nodes, total)

    def _test_node(self, node: Node) -> tuple[Node, float | None, bool]:
        port, reservation = self._reserve_port()
        target = _SpeedTestTarget(node=node, http_port=port)
        tmp = None
        proc = None

        try:
            config = self._build_config(target)
            tmp = tempfile.NamedTemporaryFile(
                mode="w",
                suffix=".json",
                prefix="xray_speed_",
                delete=False,
                encoding="utf-8",
            )
            json.dump(config, tmp, ensure_ascii=True)
            tmp.close()

            self._close_reserved_ports([reservation])
            reservation = None

            proc = subprocess.Popen(
                [self._xray_path, "run", "-c", tmp.name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=0x08000000,  # CREATE_NO_WINDOW
            )
            self._register_process(proc)

            if not self._wait_for_ready(proc, target):
                return node, None, False

            delay_ms = self._real_ping(target)
            if self._cancelled:
                return node, None, False
            if delay_ms <= 0:
                return node, None, False

            self.node_progress.emit(node.id, 35)
            speed = self._measure_speed(target)
            return node, speed, bool(speed and speed > 0)

        except Exception:
            return node, None, False
        finally:
            if reservation is not None:
                self._close_reserved_ports([reservation])
            if proc is not None:
                self._unregister_process(proc)
                self._stop_process(proc)
            if tmp:
                try:
                    Path(tmp.name).unlink(missing_ok=True)
                except Exception:
                    pass

    def _build_config(self, target: _SpeedTestTarget) -> dict:
        inbound_tag = "speed-http"
        outbound_tag = "speed-proxy"
        proxy_outbound = deepcopy(target.node.outbound)
        proxy_outbound["tag"] = outbound_tag

        config = {
            "log": {"loglevel": "none"},
            "inbounds": [
                {
                    "tag": inbound_tag,
                    "listen": PROXY_HOST,
                    "port": int(target.http_port),
                    "protocol": "http",
                    "settings": {},
                    "sniffing": {
                        "enabled": True,
                        "destOverride": ["http", "tls"],
                        "routeOnly": True,
                    },
                }
            ],
            "outbounds": [
                proxy_outbound,
                {"tag": "direct", "protocol": "freedom", "settings": {}},
                {"tag": "block", "protocol": "blackhole", "settings": {}},
            ],
            "routing": {
                "domainStrategy": "AsIs",
                "rules": [
                    {
                        "type": "field",
                        "inboundTag": [inbound_tag],
                        "outboundTag": outbound_tag,
                    }
                ],
            },
        }
        apply_xray_final_fragment(config, tag_prefix=outbound_tag)
        return config

    def _real_ping(self, target: _SpeedTestTarget) -> int:
        opener = self._build_proxy_opener(target.http_port)
        req = Request(SPEED_TEST_PING_URL, headers={"User-Agent": "BebraVPN/SpeedTest"})
        self.node_progress.emit(target.node.id, 20)
        started = time.perf_counter()
        try:
            with opener.open(req, timeout=min(self._timeout, SPEED_TEST_PING_TIMEOUT)) as resp:
                resp.read(16)
            elapsed_ms = int((time.perf_counter() - started) * 1000)
            if elapsed_ms > SPEED_TEST_MAX_PING_MS:
                return -1
            self.node_progress.emit(target.node.id, 30)
            return max(1, elapsed_ms)
        except Exception:
            return -1

    def _measure_speed(self, target: _SpeedTestTarget) -> float | None:
        opener = self._build_proxy_opener(target.http_port)
        req = Request(SPEED_TEST_DEFAULT_URL, headers={"User-Agent": "BebraVPN/SpeedTest"})

        try:
            started = time.perf_counter()
            last_update = started
            total_bytes = 0
            window_bytes = 0
            max_speed = 0.0

            idle_timeout = min(self._timeout, SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT)
            with opener.open(req, timeout=idle_timeout) as resp:
                while not self._cancelled:
                    chunk = resp.read(64 * 1024)
                    now = time.perf_counter()
                    if not chunk:
                        break

                    total_bytes += len(chunk)
                    window_bytes += len(chunk)
                    elapsed = now - started
                    if elapsed >= self._timeout:
                        break
                    if (
                        elapsed >= SPEED_TEST_SLOW_GRACE_SECONDS
                        and total_bytes < SPEED_TEST_MIN_BYTES_AFTER_GRACE
                    ):
                        return None

                    window_elapsed = now - last_update
                    if window_elapsed >= 1.0:
                        speed = (window_bytes / (1000 * 1000)) / max(window_elapsed, 0.001)
                        max_speed = max(max_speed, speed)
                        if (
                            elapsed >= SPEED_TEST_SLOW_GRACE_SECONDS
                            and max_speed < SPEED_TEST_MIN_MBPS_AFTER_GRACE
                        ):
                            return None
                        window_bytes = 0
                        last_update = now
                        percent = 35 + int(60 * min(1.0, elapsed / max(self._timeout, 0.1)))
                        self.node_progress.emit(target.node.id, max(35, min(95, percent)))

            elapsed_total = time.perf_counter() - started
            if window_bytes > 0:
                speed = (window_bytes / (1000 * 1000)) / max(time.perf_counter() - last_update, 0.001)
                max_speed = max(max_speed, speed)

            if total_bytes <= 0 or elapsed_total <= 0:
                return None

            if max_speed <= 0:
                max_speed = (total_bytes / (1000 * 1000)) / elapsed_total

            self.node_progress.emit(target.node.id, 95)
            return round(max_speed, 1)

        except Exception:
            return None

    def _wait_for_ready(self, proc: subprocess.Popen, target: _SpeedTestTarget) -> bool:
        deadline = time.perf_counter() + SPEED_TEST_STARTUP_TIMEOUT
        while time.perf_counter() < deadline:
            if self._cancelled:
                return False
            if proc.poll() is not None:
                return False
            if self._is_port_ready(target.http_port):
                self.node_progress.emit(target.node.id, 10)
                return True
            time.sleep(0.05)
        return False

    @staticmethod
    def _build_proxy_opener(http_port: int):
        proxy_url = f"http://{PROXY_HOST}:{http_port}"
        return build_opener(ProxyHandler({"http": proxy_url, "https": proxy_url}))

    def _register_process(self, proc: subprocess.Popen) -> None:
        with self._process_lock:
            self._processes.add(proc)

    def _unregister_process(self, proc: subprocess.Popen) -> None:
        with self._process_lock:
            self._processes.discard(proc)

    @staticmethod
    def _stop_process(proc: subprocess.Popen) -> None:
        if proc.poll() is not None:
            return
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()

    def _reserve_port(self) -> tuple[int, socket.socket]:
        sockets: list[socket.socket] = []
        try:
            ports, sockets = self._reserve_ports(1)
            return ports[0], sockets[0]
        except Exception:
            self._close_reserved_ports(sockets)
            raise

    def _reserve_ports(self, count: int) -> tuple[list[int], list[socket.socket]]:
        sockets: list[socket.socket] = []
        ports: list[int] = []
        try:
            for _ in range(count):
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.bind((PROXY_HOST, 0))
                sockets.append(sock)
                ports.append(int(sock.getsockname()[1]))
        except Exception:
            self._close_reserved_ports(sockets)
            raise
        return ports, sockets

    @staticmethod
    def _close_reserved_ports(sockets: list[socket.socket]) -> None:
        while sockets:
            sock = sockets.pop()
            try:
                sock.close()
            except Exception:
                pass

    @staticmethod
    def _is_port_ready(port: int) -> bool:
        try:
            with socket.create_connection((PROXY_HOST, port), timeout=0.1):
                return True
        except OSError:
            return False
