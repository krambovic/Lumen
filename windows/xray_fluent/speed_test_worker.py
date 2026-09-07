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
from urllib.request import Request

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
from .models import Node, RoutingSettings
from .ping_worker import _WindowsPingBypass
from .probe_capabilities import probe_backend, node_protocol
from .native_test_config import build_native_test_config
from .strict_proxy import proxy_opener
from .secret_scrubber import scrub_text
from .xray_fragments import apply_xray_final_fragment


@dataclass(frozen=True)
class _SpeedTestTarget:
    node: Node
    http_port: int


def _resolve_speed_test_concurrency(node_count: int, configured: int) -> int:
    total = max(1, int(node_count or 0))
    requested = int(configured or 0)
    if requested > 0:
        return min(requested, total)
    return min(max(1, SPEED_TEST_MIXED_CONCURRENCY), total)


class SpeedTestWorker(QThread):
    """Tests nodes like v2rayN Mixedtest: ping first, then one speed download."""

    result = pyqtSignal(str, object, bool)   # node_id, speed_mbps (float|None), is_alive
    ping_result = pyqtSignal(str, object)    # node_id, delay_ms (int|None) - режим ping
    progress = pyqtSignal(int, int)          # current, total
    node_progress = pyqtSignal(str, int)     # node_id, percent 0..100
    completed = pyqtSignal()
    failure = pyqtSignal(str, str)
    tests_profile = True

    def __init__(
        self,
        nodes: list[Node],
        xray_path: str,
        routing: RoutingSettings | None = None,
        timeout: float = SPEED_TEST_TIMEOUT,
        *,
        mode: str = "speed",
        test_url: str = "",
        concurrency: int = 0,
        bypass_tun: bool = False,
        singbox_path: str = "",
    ):
        super().__init__()
        self.setObjectName("lumen-speed-test" if mode == "speed" else "lumen-real-ping")
        self._nodes = list(nodes)
        self._xray_path = xray_path
        from .constants import SINGBOX_PATH_DEFAULT
        self._singbox_path = singbox_path or str(SINGBOX_PATH_DEFAULT)
        self._routing = routing or RoutingSettings()
        self._timeout = timeout
        self._mode = mode if mode in ("speed", "ping") else "speed"
        self._test_url = (test_url or "").strip() or SPEED_TEST_DEFAULT_URL
        self._concurrency = int(concurrency or 0)
        self._bypass_tun = bool(bypass_tun)
        self._cancelled = False
        self._completed_nodes = 0
        self._processes: set[subprocess.Popen] = set()
        self._process_lock = threading.Lock()
        self._unavailable_nodes: set[str] = set()
        self._responses: list[object] = []
        self._response_lock = threading.Lock()

    def cancel(self) -> None:
        # Called from the GUI thread: only signal and release, never wait —
        # run()'s finally does the kill escalation on the worker thread.
        self._cancelled = True
        with self._response_lock:
            responses = list(self._responses)
        for response in responses:
            try:
                response.close()
            except Exception:
                pass
        self._terminate_all_processes(wait=False)

    def _terminate_all_processes(self, *, wait: bool = True) -> None:
        with self._process_lock:
            processes = list(self._processes)
        for proc in processes:
            if proc.poll() is None:
                try:
                    proc.terminate()
                except Exception:
                    pass
        if not wait:
            return
        deadline = time.monotonic() + 1.0
        while time.monotonic() < deadline and any(proc.poll() is None for proc in processes):
            time.sleep(0.05)
        for proc in processes:
            if proc.poll() is None:
                try:
                    proc.kill()
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
            for node in self._nodes:
                self.node_progress.emit(node.id, 0)

            max_workers = _resolve_speed_test_concurrency(len(self._nodes), self._concurrency)
            with _WindowsPingBypass(self._bypass_targets(), self._bypass_tun, cancelled=lambda: self._cancelled) as bypass:
                self._bypass = bypass
                executor = ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="speed-test")
                pending: set[Future[tuple[Node, float | None, bool]]] = set()
                iterator = iter(self._nodes)
                exhausted = False

                def fill_pending_slots() -> None:
                    nonlocal exhausted
                    while len(pending) < max_workers and not exhausted and not self._cancelled:
                        node = next(iterator, None)
                        if node is None:
                            exhausted = True
                            break
                        pending.add(executor.submit(self._test_node, node))

                try:
                    fill_pending_slots()
                    while (pending or not exhausted) and not self._cancelled:
                        if not pending:
                            fill_pending_slots()
                            if not pending:
                                break
                        done, _ = wait(pending, timeout=0.2, return_when=FIRST_COMPLETED)
                        if not done:
                            continue
                        for future in done:
                            pending.discard(future)
                            if self._cancelled:
                                break
                            try:
                                node, speed, alive = future.result()
                            except Exception:
                                continue
                            self._emit_node_result(node, speed, alive, total)
                            fill_pending_slots()
                finally:
                    if self._cancelled:
                        for future in pending:
                            future.cancel()
                    executor.shutdown(wait=True, cancel_futures=True)
        finally:
            self._terminate_all_processes()
            self.completed.emit()

    def _emit_node_result(self, node: Node, value: float | None, alive: bool, total: int) -> None:
        self._completed_nodes += 1
        self.node_progress.emit(node.id, 100)
        if self._mode == "ping":
            delay = int(value) if (value is not None and value >= 0) else None
            self.ping_result.emit(node.id, delay)
        else:
            self.result.emit(node.id, value, alive)
        self.progress.emit(self._completed_nodes, total)

    def _bypass_targets(self) -> list[Node]:
        targets = []
        for node in self._nodes:
            for host in self._profile_hosts(node):
                target = deepcopy(node)
                target.server = host
                targets.append(target)
        return targets

    @staticmethod
    def _profile_hosts(node: Node) -> list[str]:
        hosts = set()
        def walk(value):
            if isinstance(value, dict):
                for key in ("server", "address"):
                    host = value.get(key)
                    if isinstance(host, str) and host and "/" not in host:
                        hosts.add(host)
                for child in value.values():
                    walk(child)
            elif isinstance(value, list):
                for child in value:
                    walk(child)
        walk(node.outbound)
        if node.server and node_protocol(node) not in {"xray_config", "singbox_config"}:
            hosts.add(node.server)
        return sorted(hosts)

    def _pin_native_targets(self, config: dict) -> None:
        bypass = getattr(self, "_bypass", None)
        if bypass is None:
            return
        for item in [*config.get("outbounds", []), *config.get("endpoints", [])]:
            if not isinstance(item, dict):
                continue
            host = str(item.get("server") or "")
            ip = bypass.direct_ip(host) if host else ""
            if ip and ip != host:
                tls = item.get("tls")
                if isinstance(tls, dict) and tls.get("enabled"):
                    tls.setdefault("server_name", host)
                transport = item.get("transport")
                if isinstance(transport, dict):
                    if transport.get("type") == "ws":
                        transport.setdefault("headers", {}).setdefault("Host", host)
                    elif transport.get("type") in {"http", "httpupgrade", "xhttp"}:
                        transport.setdefault("host", host)
                item["server"] = ip
            for peer in item.get("peers", []):
                if isinstance(peer, dict):
                    host = str(peer.get("address") or peer.get("server") or "")
                    ip = bypass.direct_ip(host) if host else ""
                    if ip:
                        peer["address" if "address" in peer else "server"] = ip

    @property
    def unavailable_nodes(self) -> frozenset[str]:
        with self._process_lock:
            return frozenset(self._unavailable_nodes)

    def _unavailable(self, node: Node, reason: str):
        with self._process_lock:
            self._unavailable_nodes.add(node.id)
        self.failure.emit(node.id, scrub_text(reason))
        return node, None, False

    def _test_node(self, node: Node) -> tuple[Node, float | None, bool]:
        if self._cancelled:
            return node, None, False
        backend = probe_backend(node)
        executable = self._singbox_path if backend == "singbox" else self._xray_path
        if not backend or not Path(executable).is_file():
            return self._unavailable(node, "Совместимое ядро для теста не найдено")

        bypass = getattr(self, "_bypass", None)
        if bypass is not None and any(not bypass.prepare_host(host) for host in self._profile_hosts(node)):
            return self._unavailable(node, "Не удалось подготовить прямой маршрут для изолированного теста")
        reservation: socket.socket | None = None
        tmp = None
        proc = None

        try:
            port, reservation = self._reserve_port()
            target = _SpeedTestTarget(node=node, http_port=port)
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
                [executable, "run", "-c", tmp.name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=0x08000000,  # CREATE_NO_WINDOW
            )
            self._register_process(proc)

            if not self._wait_for_ready(proc, target):
                return self._unavailable(node, "Временное ядро не запустилось: профиль не проверен")

            delay_ms = self._real_ping(target)
            if self._cancelled:
                return node, None, False
            if delay_ms < 0:
                return node, None, False

            if self._mode == "ping":
                # Режим реальной задержки: только ping, без загрузки.
                return node, float(delay_ms), True

            self.node_progress.emit(node.id, 35)
            speed = self._measure_speed(target)
            return node, speed, bool(speed and speed > 0)

        except Exception as exc:
            return self._unavailable(node, str(exc))

        finally:
            if reservation is not None:
                self._close_reserved_ports([reservation])
            if proc is not None:
                # Keep the PID visible as Lumen-owned until the temporary core
                # has really exited. Otherwise conflict scanning can briefly
                # report our own test process as a foreign Xray instance.
                try:
                    self._stop_process(proc)
                finally:
                    self._unregister_process(proc)
            if tmp:
                try:
                    Path(tmp.name).unlink(missing_ok=True)
                except Exception:
                    pass

    @staticmethod
    def _node_protocol(node: Node) -> str:
        outbound = node.outbound if isinstance(node.outbound, dict) else {}
        return str(outbound.get("protocol") or outbound.get("type") or node.scheme or "").strip().lower()

    @classmethod
    def _uses_direct_ping_fallback(cls, node: Node) -> bool:
        # Compatibility shim: a real test never falls back to endpoint ping.
        return False

    def _apply_direct_ip_to_outbound(self, outbound: dict, host: str) -> None:
        # While TUN is up, point the temp xray outbound at the server's real
        # IP (resolved directly, bypassing fake-ip) so its connection follows
        # the temporary direct host route instead of being tunneled. TLS SNI /
        # host live in streamSettings and are left untouched.
        bypass = getattr(self, "_bypass", None)
        if bypass is None:
            return
        host = str(host or "").strip()
        ip = bypass.direct_ip(host)
        if not ip or ip == host:
            return
        stream = outbound.get("streamSettings")
        if isinstance(stream, dict):
            security = str(stream.get("security") or "")
            if security in {"tls", "reality"}:
                stream.setdefault(security + "Settings", {}).setdefault("serverName", host)
            network = str(stream.get("network") or "tcp")
            if network == "ws":
                stream.setdefault("wsSettings", {}).setdefault("headers", {}).setdefault("Host", host)
            elif network in {"http", "h2", "xhttp", "httpupgrade"}:
                key = "httpSettings" if network == "h2" else network + "Settings"
                stream.setdefault(key, {}).setdefault("host", host)
        settings = outbound.get("settings")
        if not isinstance(settings, dict):
            return
        for key in ("vnext", "servers"):
            entries = settings.get(key)
            if not isinstance(entries, list):
                continue
            for entry in entries:
                if isinstance(entry, dict) and entry.get("address"):
                    entry["address"] = ip

    def _build_config(self, target: _SpeedTestTarget) -> dict:
        if probe_backend(target.node) == "singbox":
            config = build_native_test_config(target.node, target.http_port)
            self._pin_native_targets(config)
            return config
        inbound_tag = "speed-http"
        outbound_tag = "speed-proxy"
        stored_outbound = target.node.outbound if isinstance(target.node.outbound, dict) else {}
        full_config = stored_outbound.get("xray_config")
        if str(stored_outbound.get("protocol") or "").strip().lower() == "xray_config" and isinstance(full_config, dict):
            return self._build_auto_config(target, full_config, inbound_tag)
        proxy_outbound = deepcopy(target.node.outbound)
        proxy_outbound["tag"] = outbound_tag
        self._apply_direct_ip_to_outbound(proxy_outbound, target.node.server)

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
                        "routeOnly": False,
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

    def _build_auto_config(self, target: _SpeedTestTarget, full_config: dict, inbound_tag: str) -> dict:
        """Build a temporary HTTP inbound while preserving AUTO balancer/observer."""
        config = deepcopy(full_config)
        config["log"] = {"loglevel": "none"}
        config["inbounds"] = [
            {
                "tag": inbound_tag,
                "listen": PROXY_HOST,
                "port": int(target.http_port),
                "protocol": "http",
                "settings": {},
                "sniffing": {
                    "enabled": True,
                    "destOverride": ["http", "tls"],
                    "routeOnly": False,
                },
            }
        ]
        config.pop("api", None)
        config.pop("stats", None)
        config.pop("policy", None)

        routing = config.get("routing")
        if not isinstance(routing, dict):
            routing = {}
            config["routing"] = routing
        balancer_tag = ""
        rules = routing.get("rules") if isinstance(routing.get("rules"), list) else []
        for rule in rules:
            if isinstance(rule, dict) and str(rule.get("balancerTag") or "").strip():
                balancer_tag = str(rule["balancerTag"]).strip()
                break
        if not balancer_tag:
            balancers = routing.get("balancers") if isinstance(routing.get("balancers"), list) else []
            for balancer in balancers:
                if isinstance(balancer, dict) and str(balancer.get("tag") or "").strip():
                    balancer_tag = str(balancer["tag"]).strip()
                    break
        outbounds = config.get("outbounds", [])
        by_tag = {str(o.get("tag") or ""): o for o in outbounds if isinstance(o, dict)}
        if balancer_tag:
            balancer = next((b for b in routing.get("balancers", []) if isinstance(b, dict) and b.get("tag") == balancer_tag), None)
            if not balancer:
                raise ValueError("Missing AUTO balancer")
            selectors = balancer.get("selector", [])
            candidates = [o for tag, o in by_tag.items() if any(tag.startswith(str(prefix)) for prefix in selectors)]
            fallback = str(balancer.get("fallbackTag") or "")
            if fallback:
                candidates.append(by_tag.get(fallback, {}))
            if not candidates or any(str(o.get("protocol") or "") in {"", "freedom", "blackhole", "dns"} for o in candidates):
                raise ValueError("AUTO test cannot bypass the profile through direct/unknown members")
            target_rule = {"balancerTag": balancer_tag}
        else:
            first = next((o for o in outbounds if isinstance(o, dict)), {})
            if str(first.get("protocol") or "") in {"", "freedom", "blackhole", "dns"} or not first.get("tag"):
                raise ValueError("Full profile default is not a testable proxy")
            target_rule = {"outboundTag": first["tag"]}
        routing["rules"] = [{"type": "field", "inboundTag": [inbound_tag], **target_rule}]

        outbounds = config.get("outbounds") if isinstance(config.get("outbounds"), list) else []
        for outbound in outbounds:
            if not isinstance(outbound, dict):
                continue
            host = self._xray_outbound_host(outbound)
            self._apply_direct_ip_to_outbound(outbound, host)
        return config

    @staticmethod
    def _xray_outbound_host(outbound: dict) -> str:
        settings = outbound.get("settings") if isinstance(outbound.get("settings"), dict) else {}
        for key in ("vnext", "servers"):
            entries = settings.get(key)
            if isinstance(entries, list) and entries and isinstance(entries[0], dict):
                return str(entries[0].get("address") or entries[0].get("server") or "").strip()
        return str(settings.get("address") or settings.get("server") or outbound.get("address") or "").strip()

    def _real_ping(self, target: _SpeedTestTarget) -> int:
        if self._cancelled:
            return -1
        opener = self._build_proxy_opener(target.http_port)
        req = Request(SPEED_TEST_PING_URL, headers={"User-Agent": "Lumen/SpeedTest"})
        self.node_progress.emit(target.node.id, 20)
        started = time.perf_counter()
        try:
            with opener.open(req, timeout=min(self._timeout, SPEED_TEST_PING_TIMEOUT)) as resp:
                self._register_response(resp)
                try:
                    resp.read(16)
                finally:
                    self._unregister_response(resp)
            elapsed_ms = int((time.perf_counter() - started) * 1000)
            if elapsed_ms > SPEED_TEST_MAX_PING_MS:
                return -1
            self.node_progress.emit(target.node.id, 30)
            return elapsed_ms
        except Exception:
            return -1

    def _measure_speed(self, target: _SpeedTestTarget) -> float | None:
        opener = self._build_proxy_opener(target.http_port)
        req = Request(self._test_url, headers={"User-Agent": "Lumen/SpeedTest"})

        try:
            started = time.perf_counter()
            last_update = started
            total_bytes = 0
            window_bytes = 0
            max_speed = 0.0

            idle_timeout = min(self._timeout, SPEED_TEST_DOWNLOAD_IDLE_TIMEOUT)
            with opener.open(req, timeout=idle_timeout) as resp:
                self._register_response(resp)
                try:
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
                finally:
                    self._unregister_response(resp)

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
        return proxy_opener("http://" + PROXY_HOST + ":" + str(http_port))

    def _register_process(self, proc: subprocess.Popen) -> None:
        with self._process_lock:
            self._processes.add(proc)

    def _unregister_process(self, proc: subprocess.Popen) -> None:
        with self._process_lock:
            self._processes.discard(proc)

    def _register_response(self, response: object) -> None:
        with self._response_lock:
            self._responses.append(response)

    def _unregister_response(self, response: object) -> None:
        with self._response_lock:
            self._responses = [item for item in self._responses if item is not response]

    @staticmethod
    def _stop_process(proc: subprocess.Popen) -> None:
        if proc.poll() is not None:
            return
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            try:
                proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass

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
