from __future__ import annotations

import re
import socket
import os
import threading
import time
from urllib.request import Request, ProxyHandler
from .http_utils import build_opener
from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait

from PyQt6.QtCore import QThread, pyqtSignal

from .constants import ICMP_PING_TIMEOUT_MS
from .models import Node
from .probe_capabilities import endpoint_method
from .network_route_context import get_windows_default_route_context
from .route_leases import acquire_route, release_route
from .direct_http import _resolve_a_direct
from .subprocess_utils import CREATE_NO_WINDOW, result_output_text, run_text_pumped


_MAX_PING_WORKERS = 16

_ICMP_TIME_RE = re.compile(r"[=<]\s*(\d+(?:[.,]\d+)?)\s*ms", re.IGNORECASE)


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


def udp_ping(host: str, port: int, timeout: float = 2.0) -> int | None:
    """Measure a UDP endpoint without assuming a TCP listener.

    Hysteria/Hysteria2 use QUIC over UDP.  A tiny datagram is enough to make
    the OS perform the endpoint reachability check; when the peer answers (or
    reports an ICMP error) we return the elapsed time.  Some servers silently
    drop unknown datagrams, in which case the caller can use ICMP as fallback.
    """
    if not host or not port:
        return None
    start = time.perf_counter()
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.settimeout(timeout)
            sock.connect((host, port))
            sock.send(b"\x00")
            try:
                sock.recv(1)
            except socket.timeout:
                return None
        return int((time.perf_counter() - start) * 1000.0)
    except OSError:
        return None


def hysteria_ping(host: str, port: int, timeout: float = 2.0) -> int | None:
    """Ping a Hysteria endpoint using its UDP transport.

    UDP probes are intentionally followed by ICMP: Hysteria servers commonly
    ignore non-QUIC datagrams, while ICMP still provides a useful endpoint
    latency measurement without starting sing-box or a proxy session.
    """
    measured = udp_ping(host, port, timeout)
    if measured is not None:
        return measured
    return icmp_ping(host, int(timeout * 1000))


def http_get_ping(host: str, port: int, timeout: float = 3.0) -> int | None:
    """Measure an endpoint with a direct HTTP(S) GET, without a proxy core."""
    if not host or not port:
        return None
    authority = "[" + host.strip("[]") + "]" if ":" in host else host
    opener = build_opener(ProxyHandler({}))
    for scheme in ("https", "http"):
        started = time.perf_counter()
        try:
            request = Request(f"{scheme}://{authority}:{int(port)}/", method="GET")
            with opener.open(request, timeout=timeout) as response:
                response.read(1)
            return int((time.perf_counter() - started) * 1000.0)
        except Exception:
            continue
    return None


def endpoint_ping(host: str, port: int, protocol: str, method: str, timeout: float) -> int | None:
    """Best-effort transport ping for protocols without an Xray test adapter."""
    if method == "real":
        return None  # A real probe requires an authenticated proxy core.
    if method == "icmp":
        return icmp_ping(host, int(timeout * 1000))
    if method == "http":
        return http_get_ping(host, port, timeout)
    if protocol in {"hysteria", "hysteria2", "hy", "hy2", "tuic", "awg", "wireguard", "warp", "masque"}:
        return hysteria_ping(host, port, timeout)
    return tcp_ping(host, port, timeout)


def icmp_ping(host: str, timeout_ms: int = ICMP_PING_TIMEOUT_MS) -> int | None:
    """Системный ICMP ping (один пакет). Возвращает задержку в мс или None."""
    host = str(host or "").strip()
    if not host:
        return None
    timeout_ms = max(200, int(timeout_ms or ICMP_PING_TIMEOUT_MS))
    if os.name == "nt":
        command = ["ping", "-n", "1", "-w", str(timeout_ms), host]
    else:
        timeout_sec = max(1, int(round(timeout_ms / 1000.0)))
        command = ["ping", "-c", "1", "-W", str(timeout_sec), host]
    try:
        result = run_text_pumped(
            command,
            timeout=max(2.0, timeout_ms / 1000.0 + 1.0),
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        return None
    if result.returncode != 0:
        return None
    text = result_output_text(result)
    match = _ICMP_TIME_RE.search(text)
    if not match:
        return None
    try:
        return int(round(float(match.group(1).replace(",", "."))))
    except ValueError:
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
        "$alias -notmatch '(?i)lumen|xftun|wintun|tun' "
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
    return gateway.startswith(("172.18.", "172.19.", "198.18.", "198.19."))


def _has_direct_host_route(ip: str, gateway: str, interface_index: int = 0) -> bool:
    if os.name != "nt" or not ip or not gateway:
        return False
    script = (
        f"$routes = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '{ip}/32' -ErrorAction SilentlyContinue; "
        f"$route = $routes | Where-Object {{ $_.NextHop -eq '{gateway}' -and ({interface_index} -eq 0 -or $_.InterfaceIndex -eq {interface_index}) }} | Select-Object -First 1; "
        "if ($route) { exit 0 } else { exit 1 }"
    )
    try:
        result = run_text_pumped(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
            timeout=3,
            creationflags=CREATE_NO_WINDOW,
        )
    except Exception:
        return False
    return result.returncode == 0


_DIRECT_DNS_SERVERS = ("8.8.8.8", "1.1.1.1", "9.9.9.9")


def _looks_like_fake_ip(ip: str) -> bool:
    # sing-box fake-ip pool (198.18.0.0/15) and the TUN subnet must never be
    # used as a real ping target or host route destination.
    return ip.startswith(("198.18.", "198.19.", "172.18.", "172.19."))


def _skip_dns_name(data: bytes, idx: int) -> int:
    while True:
        length = data[idx]
        if length == 0:
            return idx + 1
        if length & 0xC0 == 0xC0:
            return idx + 2
        idx += length + 1


def _direct_dns_resolve_a(domain: str, dns_server: str, timeout: float = 1.5) -> str:
    return _resolve_a_direct(domain, dns_server, timeout)


_DNS_CACHE: dict[tuple, tuple[float, str]] = {}
_DNS_CACHE_LOCK = threading.Lock()


class _WindowsPingBypass:
    def __init__(self, nodes: list[Node], enabled: bool, *, cancelled=None, dns_servers=None):
        self._nodes = nodes
        self._enabled = bool(enabled and os.name == "nt")
        self._gateway = ""
        self._interface_index = 0
        self._cancelled = cancelled or (lambda: False)
        self._requested_dns = dns_servers
        self._dns_servers = ()
        self._prepare_lock = threading.Lock()
        self._preparing: dict[str, threading.Event] = {}
        self._added_ips: set[str] = set()
        self._dns_routes: set[str] = set()
        self._covered_ips: set[str] = set()
        self._host_ips: dict[str, str] = {}

    def _route_add(self, ip: str) -> bool:
        if self._cancelled():
            return False
        try:
            result = run_text_pumped(
                ["route", "add", ip, "mask", "255.255.255.255", self._gateway, "metric", "1"] + (["if", str(self._interface_index)] if self._interface_index else []),
                timeout=4,
                creationflags=CREATE_NO_WINDOW,
            )
        except Exception:
            return False
        return result.returncode == 0

    def _acquire_route(self, ip: str) -> bool:
        return acquire_route(
            (self._gateway, ip, self._interface_index),
            lambda: self._route_add(ip),
            lambda: _has_direct_host_route(ip, self._gateway, self._interface_index),
            lambda: self._route_delete(ip),
        )

    def _release_route(self, ip: str) -> None:
        release_route((self._gateway, ip, self._interface_index))

    def _route_delete(self, ip: str) -> None:
        try:
            run_text_pumped(
                ["route", "delete", ip, "mask", "255.255.255.255", self._gateway] + (["if", str(self._interface_index)] if self._interface_index else []),
                timeout=4,
                creationflags=CREATE_NO_WINDOW,
            )
        except Exception:
            pass

    def _resolve_real_ip(self, host: str) -> str:
        if _is_ipv4_address(host):
            return host
        key = (self._gateway, self._interface_index, host.lower(), self._dns_servers)
        now = time.monotonic()
        with _DNS_CACHE_LOCK:
            cached = _DNS_CACHE.get(key)
            if cached and cached[0] > now:
                return cached[1]
        ip = ""
        for dns_server in self._dns_servers:
            if self._cancelled():
                break
            with self._prepare_lock:
                if dns_server not in self._added_ips:
                    if not self._acquire_route(dns_server):
                        continue
                    self._added_ips.add(dns_server)
                    self._dns_routes.add(dns_server)
            ip = _direct_dns_resolve_a(host, dns_server, timeout=1.0)
            if ip and not _looks_like_fake_ip(ip):
                break
            ip = ""
        with _DNS_CACHE_LOCK:
            if len(_DNS_CACHE) >= 1024:
                _DNS_CACHE.pop(next(iter(_DNS_CACHE)))
            _DNS_CACHE[key] = (time.monotonic() + (60 if ip else 5), ip)
        return ip

    def __enter__(self):
        if not self._enabled or self._cancelled():
            return self
        context = get_windows_default_route_context()
        if context is None or not context.is_physical or self._cancelled():
            return self
        self._gateway = context.next_hop
        self._interface_index = context.interface_index
        if not _is_ipv4_address(self._gateway) or _looks_like_tun_gateway(self._gateway):
            self._gateway = ""
            return self
        # Bootstrap uses only the physical adapter DNS (or explicit config),
        # never hard-coded public fallback resolvers or TUN/system DNS.
        candidates = context.dns_servers if self._requested_dns is None else self._requested_dns
        self._dns_servers = tuple(dict.fromkeys(ip for ip in candidates if _is_ipv4_address(ip)))[:3]
        return self

    def prepare_host(self, host: str) -> bool:
        if not self._enabled:
            return not self._cancelled()
        host = str(host or "").strip().strip("[]")
        if not host or not self._gateway or self._cancelled():
            return False
        # The IPv4 bypass must fail closed for IPv6 instead of silently using TUN.
        if ":" in host:
            return False
        with self._prepare_lock:
            event = self._preparing.get(host)
            owner = event is None
            if owner:
                event = threading.Event()
                self._preparing[host] = event
        if not owner:
            while not event.wait(0.1):
                if self._cancelled():
                    return False
            return self.can_ping_direct(host)
        try:
            ip = self._resolve_real_ip(host)
            if not ip or self._cancelled() or ip.startswith(("127.", "0.", "169.254.")):
                return False
            with self._prepare_lock:
                if ip not in self._added_ips and not self._acquire_route(ip):
                    return False
                self._added_ips.add(ip)
                self._covered_ips.add(ip)
                self._host_ips[host] = ip
            return True
        finally:
            event.set()

    def direct_ip(self, host: str) -> str:
        host = str(host or "").strip()
        if not self._enabled:
            return host
        return self._host_ips.get(host, "")

    def can_ping_direct(self, host: str) -> bool:
        if not self._enabled:
            return True
        ip = self._host_ips.get(str(host or "").strip())
        return bool(ip and ip in self._covered_ips)

    def __exit__(self, *_exc) -> None:
        for ip in self._added_ips:
            self._release_route(ip)
        self._added_ips.clear()
        self._dns_routes.clear()
        self._covered_ips.clear()
        self._host_ips.clear()


class PingWorker(QThread):
    result = pyqtSignal(str, object)
    progress = pyqtSignal(int, int)  # current, total
    completed = pyqtSignal()

    def __init__(
        self,
        nodes: list[Node],
        timeout: float = 2.0,
        *,
        bypass_tun: bool = False,
        method: str = "tcping",
    ):
        super().__init__()
        self.setObjectName("lumen-ping-worker")
        self._nodes = nodes
        self._timeout = timeout
        self._bypass_tun = bypass_tun
        self._method = method if method in ("tcping", "icmp", "http", "real") else "tcping"
        self._cancelled = False

    def _measure(self, node: Node) -> int | None:
        if self._cancelled:
            return None
        target = node.server
        bypass = getattr(self, "_bypass", None)
        if bypass is not None:
            if not bypass.prepare_host(node.server):
                return None
            target = bypass.direct_ip(node.server) or node.server
        if self._method in {"http", "real"}:
            # These methods require SpeedTestWorker and a full core profile.
            return None
        actual = endpoint_method(node, self._method)
        if actual == "icmp":
            return icmp_ping(target, int(self._timeout * 1000))
        return tcp_ping(target, node.port, self._timeout)

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
        exhausted = False
        completed = 0

        bypass = _WindowsPingBypass(self._nodes, self._bypass_tun, cancelled=lambda: self._cancelled)
        self._bypass = bypass

        def submit_node(node: Node) -> None:
            nonlocal completed
            future = executor.submit(self._measure, node)
            pending[future] = node.id

        def fill_pending_slots() -> None:
            nonlocal exhausted
            while len(pending) < max_workers and not exhausted and not self._cancelled:
                node = next(iterator, None)
                if node is None:
                    exhausted = True
                    break
                submit_node(node)

        try:
            bypass.__enter__()
            fill_pending_slots()

            while (pending or not exhausted) and not self._cancelled:
                if not pending:
                    fill_pending_slots()
                    if not pending:
                        break
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

                    fill_pending_slots()

            if self._cancelled:
                for future in pending:
                    future.cancel()
        finally:
            # No measurement may outlive its direct host route.
            executor.shutdown(wait=True, cancel_futures=True)
            bypass.__exit__(None, None, None)

        self.completed.emit()
