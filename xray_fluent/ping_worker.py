from __future__ import annotations

import socket
import os
import time
from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait

from PyQt6.QtCore import QThread, pyqtSignal

from .models import Node
from .subprocess_utils import CREATE_NO_WINDOW, result_output_text, run_text_pumped


_MAX_PING_WORKERS = 16


def tcp_ping(host: str, port: int, timeout: float = 2.0) -> int | None:
    if not host or not port:
        return None
    start = time.perf_counter()
    try:
        with socket.create_connection((host, port), timeout=timeout):
            elapsed = (time.perf_counter() - start) * 1000.0
            return int(elapsed)
    except OSError:
        return None


def _is_ipv4_address(value: str) -> bool:
    try:
        socket.inet_aton(value)
    except OSError:
        return False
    return value.count(".") == 3


def _resolve_ipv4(host: str) -> str:
    if _is_ipv4_address(host):
        return host
    try:
        infos = socket.getaddrinfo(host, None, socket.AF_INET, socket.SOCK_STREAM)
    except OSError:
        return ""
    for info in infos:
        sockaddr = info[4]
        if sockaddr:
            ip = str(sockaddr[0])
            if _is_ipv4_address(ip):
                return ip
    return ""


def _detect_direct_gateway() -> str:
    if os.name != "nt":
        return ""
    script = (
        "$routes = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue "
        "| Where-Object { $_.NextHop -and $_.NextHop -ne '0.0.0.0' } "
        "| Sort-Object RouteMetric, InterfaceMetric; "
        "$route = $routes | Where-Object { "
        "$alias = [string]$_.InterfaceAlias; "
        "$alias -notmatch '(?i)bebra|xftun|wintun|tun' "
        "} | Select-Object -First 1; "
        "if (-not $route) { $route = $routes | Select-Object -First 1 }; "
        "if (-not $route) { exit 1 }; "
        "$route.NextHop"
    )
    try:
        result = run_text_pumped(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            timeout=6,
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        return ""
    if result.returncode != 0:
        return ""
    gateway = result_output_text(result).strip().splitlines()
    return gateway[0].strip() if gateway else ""


def _looks_like_tun_gateway(gateway: str) -> bool:
    return gateway.startswith(("172.19.", "198.18.", "198.19."))


class _WindowsPingBypass:
    def __init__(self, nodes: list[Node], enabled: bool):
        self._nodes = nodes
        self._enabled = bool(enabled and os.name == "nt")
        self._gateway = ""
        self._added_ips: set[str] = set()

    def __enter__(self):
        if not self._enabled:
            return self
        self._gateway = _detect_direct_gateway()
        if not self._gateway or _looks_like_tun_gateway(self._gateway):
            return self

        ips: set[str] = set()
        for node in self._nodes:
            ip = _resolve_ipv4(str(node.server or "").strip())
            if ip and not ip.startswith(("127.", "0.", "169.254.")):
                ips.add(ip)

        for ip in ips:
            try:
                result = run_text_pumped(
                    ["route", "add", ip, "mask", "255.255.255.255", self._gateway, "metric", "1"],
                    timeout=4,
                    creationflags=CREATE_NO_WINDOW,
                )
            except Exception:
                continue
            if result.returncode == 0:
                self._added_ips.add(ip)
        return self

    def __exit__(self, *_exc) -> None:
        for ip in self._added_ips:
            try:
                run_text_pumped(
                    ["route", "delete", ip, "mask", "255.255.255.255", self._gateway],
                    timeout=4,
                    creationflags=CREATE_NO_WINDOW,
                )
            except Exception:
                pass
        self._added_ips.clear()


class PingWorker(QThread):
    result = pyqtSignal(str, object)
    progress = pyqtSignal(int, int)  # current, total
    completed = pyqtSignal()

    def __init__(self, nodes: list[Node], timeout: float = 2.0, *, bypass_tun: bool = False):
        super().__init__()
        self._nodes = nodes
        self._timeout = timeout
        self._bypass_tun = bypass_tun
        self._cancelled = False

    def cancel(self) -> None:
        self._cancelled = True

    def run(self) -> None:
        total = len(self._nodes)
        if total == 0:
            self.completed.emit()
            return

        max_workers = min(_MAX_PING_WORKERS, total)
        executor = ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="ping")
        pending: dict[Future[int | None], str] = {}
        iterator = iter(self._nodes)
        completed = 0

        bypass = _WindowsPingBypass(self._nodes, self._bypass_tun)
        try:
            bypass.__enter__()
            for _ in range(max_workers):
                node = next(iterator, None)
                if node is None:
                    break
                future = executor.submit(tcp_ping, node.server, node.port, self._timeout)
                pending[future] = node.id

            while pending and not self._cancelled:
                done, _ = wait(tuple(pending), timeout=0.1, return_when=FIRST_COMPLETED)
                if not done:
                    continue

                for future in done:
                    node_id = pending.pop(future)
                    try:
                        ms = future.result()
                    except Exception:
                        ms = None

                    completed += 1
                    self.result.emit(node_id, ms)
                    self.progress.emit(completed, total)

                    if self._cancelled:
                        break

                    next_node = next(iterator, None)
                    if next_node is not None:
                        next_future = executor.submit(tcp_ping, next_node.server, next_node.port, self._timeout)
                        pending[next_future] = next_node.id

            if self._cancelled:
                for future in pending:
                    future.cancel()
        finally:
            bypass.__exit__(None, None, None)
            executor.shutdown(wait=False, cancel_futures=True)

        self.completed.emit()
