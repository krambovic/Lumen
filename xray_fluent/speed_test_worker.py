"""Worker for measuring download speed through proxy nodes."""

from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from copy import deepcopy
from dataclasses import dataclass
import json
import socket
import subprocess
import tempfile
import time
from pathlib import Path
from urllib.request import ProxyHandler, Request

from PyQt6.QtCore import QThread, pyqtSignal

from .constants import (
    PROXY_HOST,
    SPEED_TEST_BATCH_SIZE,
    SPEED_TEST_DEFAULT_URL,
    SPEED_TEST_MAX_SAMPLE_BYTES,
    SPEED_TEST_MAX_WORKERS,
    SPEED_TEST_MIN_SAMPLE_SECONDS,
    SPEED_TEST_ROUNDS,
    SPEED_TEST_STARTUP_TIMEOUT,
    SPEED_TEST_TIMEOUT,
    SPEED_TEST_URLS_BY_COUNTRY,
)
from .http_utils import build_opener
from .models import Node, RoutingSettings


@dataclass(frozen=True)
class _SpeedTestTarget:
    node: Node
    http_port: int


def _get_speed_url(country_code: str) -> str:
    return SPEED_TEST_URLS_BY_COUNTRY.get(country_code.lower(), SPEED_TEST_DEFAULT_URL)


class SpeedTestWorker(QThread):
    """Tests download speed for proxy nodes using batched temporary xray configs."""

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
        self._current_proc: subprocess.Popen | None = None

    def cancel(self) -> None:
        self._cancelled = True
        proc = self._current_proc
        if proc and proc.poll() is None:
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

            for start in range(0, total, max(1, SPEED_TEST_BATCH_SIZE)):
                if self._cancelled:
                    break
                batch = self._nodes[start:start + max(1, SPEED_TEST_BATCH_SIZE)]
                self._test_batch_with_fallback(batch, total)
        finally:
            self.completed.emit()

    def _test_batch_with_fallback(self, nodes: list[Node], total: int) -> None:
        if not nodes or self._cancelled:
            return

        results = self._test_batch(nodes)
        if results is None:
            if self._cancelled:
                return
            if len(nodes) == 1:
                self._emit_node_result(nodes[0], None, False, total)
                return
            midpoint = len(nodes) // 2
            self._test_batch_with_fallback(nodes[:midpoint], total)
            self._test_batch_with_fallback(nodes[midpoint:], total)
            return

        for node, speed, alive in results:
            if self._cancelled:
                return
            self._emit_node_result(node, speed, alive, total)

    def _emit_node_result(self, node: Node, speed: float | None, alive: bool, total: int) -> None:
        self._completed_nodes += 1
        self.node_progress.emit(node.id, 100)
        self.result.emit(node.id, speed, alive)
        self.progress.emit(self._completed_nodes, total)

    def _test_batch(self, nodes: list[Node]) -> list[tuple[Node, float | None, bool]] | None:
        ports, reservations = self._reserve_ports(len(nodes))
        targets = [_SpeedTestTarget(node=node, http_port=port) for node, port in zip(nodes, ports)]
        tmp = None
        proc = None

        try:
            config = self._build_batch_config(targets)
            tmp = tempfile.NamedTemporaryFile(
                mode="w",
                suffix=".json",
                prefix="xray_speed_batch_",
                delete=False,
                encoding="utf-8",
            )
            json.dump(config, tmp, ensure_ascii=True)
            tmp.close()

            self._close_reserved_ports(reservations)
            reservations = []

            proc = subprocess.Popen(
                [self._xray_path, "run", "-c", tmp.name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=0x08000000,  # CREATE_NO_WINDOW
            )
            self._current_proc = proc

            if not self._wait_for_batch_ready(proc, targets):
                return None

            return self._measure_batch(targets)

        except Exception:
            if self._cancelled:
                return []
            return None
        finally:
            self._current_proc = None
            self._close_reserved_ports(reservations)
            if proc and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
            if tmp:
                try:
                    Path(tmp.name).unlink(missing_ok=True)
                except Exception:
                    pass

    def _build_batch_config(self, targets: list[_SpeedTestTarget]) -> dict:
        inbounds: list[dict] = []
        outbounds: list[dict] = [
            {
                "tag": "direct",
                "protocol": "freedom",
                "settings": {},
            },
            {
                "tag": "block",
                "protocol": "blackhole",
                "settings": {},
            },
        ]
        rules: list[dict] = []

        for index, target in enumerate(targets):
            suffix = f"{index}-{target.node.id[:8]}"
            inbound_tag = f"speed-http-{suffix}"
            outbound_tag = f"speed-proxy-{suffix}"
            proxy_outbound = deepcopy(target.node.outbound)
            proxy_outbound["tag"] = outbound_tag

            inbounds.append(
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
            )
            outbounds.append(proxy_outbound)
            rules.append(
                {
                    "type": "field",
                    "inboundTag": [inbound_tag],
                    "outboundTag": outbound_tag,
                }
            )

        return {
            "log": {"loglevel": "none"},
            "inbounds": inbounds,
            "outbounds": outbounds,
            "routing": {
                "domainStrategy": "AsIs",
                "rules": rules,
            },
        }

    def _measure_batch(self, targets: list[_SpeedTestTarget]) -> list[tuple[Node, float | None, bool]]:
        results: dict[str, tuple[Node, float | None, bool]] = {}
        max_workers = min(max(1, SPEED_TEST_MAX_WORKERS), len(targets))
        with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="speed-test") as executor:
            pending: set[Future[tuple[Node, float | None, bool]]] = {
                executor.submit(self._test_target, target)
                for target in targets
            }

            while pending and not self._cancelled:
                done, pending = wait(pending, timeout=0.2, return_when=FIRST_COMPLETED)
                for future in done:
                    try:
                        node, speed, alive = future.result()
                    except Exception:
                        continue
                    results[node.id] = (node, speed, alive)

            if self._cancelled:
                for future in pending:
                    future.cancel()

        return [
            results.get(target.node.id, (target.node, None, False))
            for target in targets
        ]

    def _test_target(self, target: _SpeedTestTarget) -> tuple[Node, float | None, bool]:
        url = _get_speed_url(target.node.country_code)
        rounds = max(1, SPEED_TEST_ROUNDS)
        results: list[float] = []

        for round_index in range(rounds):
            if self._cancelled:
                break
            speed = self._measure_speed(url, target.node.id, target.http_port, round_index, rounds)
            if speed is not None and speed > 0:
                results.append(speed)

        if not results or self._cancelled:
            return target.node, None, False

        if len(results) > 1:
            results.sort()
            results = results[1:]
        return target.node, round(sum(results) / len(results), 2), True

    def _measure_speed(
        self,
        url: str,
        node_id: str,
        http_port: int,
        round_index: int,
        total_rounds: int,
    ) -> float | None:
        proxy_url = f"http://{PROXY_HOST}:{http_port}"
        handler = ProxyHandler({"http": proxy_url, "https": proxy_url})
        opener = build_opener(handler)
        req = Request(url, headers={"User-Agent": "BebraVPN/SpeedTest"})

        try:
            start = time.perf_counter()
            total_bytes = 0
            percent_start = 20 + int(70 * round_index / max(total_rounds, 1))
            percent_end = 20 + int(70 * (round_index + 1) / max(total_rounds, 1))
            with opener.open(req, timeout=self._timeout) as resp:
                length_header = resp.headers.get("Content-Length") or ""
                try:
                    total_length = int(length_header)
                except (TypeError, ValueError):
                    total_length = 0

                while True:
                    chunk = resp.read(64 * 1024)
                    if not chunk:
                        break
                    total_bytes += len(chunk)
                    elapsed = time.perf_counter() - start
                    if self._cancelled:
                        return None

                    if total_length > 0:
                        fraction = min(1.0, total_bytes / total_length)
                    else:
                        fraction = min(1.0, elapsed / max(self._timeout, 0.1))

                    percent = percent_start + int((percent_end - percent_start) * fraction)
                    self.node_progress.emit(node_id, max(percent_start, min(percent_end, percent)))

                    if elapsed > self._timeout:
                        break
                    if elapsed >= SPEED_TEST_MIN_SAMPLE_SECONDS and total_bytes >= SPEED_TEST_MAX_SAMPLE_BYTES:
                        break

            elapsed = time.perf_counter() - start
            if elapsed <= 0 or total_bytes <= 0:
                return None

            self.node_progress.emit(node_id, percent_end)
            speed_mbps = (total_bytes / (1024 * 1024)) / elapsed
            return round(speed_mbps, 2)

        except Exception:
            return None

    def _wait_for_batch_ready(self, proc: subprocess.Popen, targets: list[_SpeedTestTarget]) -> bool:
        deadline = time.perf_counter() + SPEED_TEST_STARTUP_TIMEOUT
        ready_ports: set[int] = set()

        while time.perf_counter() < deadline:
            if self._cancelled:
                return False
            if proc.poll() is not None:
                return False

            for target in targets:
                if target.http_port in ready_ports:
                    continue
                if self._is_port_ready(target.http_port):
                    ready_ports.add(target.http_port)
                    self.node_progress.emit(target.node.id, 15)

            if len(ready_ports) == len(targets):
                return True
            time.sleep(0.05)

        return False

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
