"""HTTP transport that bypasses application proxies and Windows TUN routes.

This is the primary path for one-shot subscription helpers: it resolves each
destination through DNS reachable over the physical gateway, installs temporary
/32 routes for DNS and HTTP targets, and uses an urllib opener with all
system/environment proxies disabled. If Windows cannot prepare a route, the
packaged helper can still rely on its permanent sing-box process-direct rule.
"""

from __future__ import annotations

import errno
import http.client
import ipaddress
import os
import secrets
import socket
import struct
import sys
import urllib.request
from functools import partial

from .http_utils import get_ssl_context
from .network_route_context import get_windows_default_route_context
from .subprocess_utils import CREATE_NO_WINDOW, run_text_pumped


_FALLBACK_DNS_SERVERS = ("1.1.1.1", "8.8.8.8", "9.9.9.9")


class DirectNetworkUnavailable(RuntimeError):
    """Raised rather than silently leaking a direct-only request into TUN."""


def _is_ipv4(value: str) -> bool:
    try:
        return ipaddress.ip_address(value).version == 4
    except ValueError:
        return False


def _skip_dns_name(data: bytes, offset: int) -> int:
    while offset < len(data):
        length = data[offset]
        if length == 0:
            return offset + 1
        if length & 0xC0 == 0xC0:
            return offset + 2
        offset += length + 1
    raise ValueError("invalid DNS name")


def _resolve_a_direct(host: str, dns_server: str, timeout: float = 2.5) -> str:
    transaction_id = secrets.randbits(16)
    labels = [part for part in host.rstrip(".").split(".") if part]
    try:
        qname = b"".join(bytes([len(label)]) + label.encode("idna") for label in labels) + b"\x00"
    except (UnicodeError, ValueError):
        return ""
    packet = struct.pack(">HHHHHH", transaction_id, 0x0100, 1, 0, 0, 0)
    packet += qname + struct.pack(">HH", 1, 1)
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.settimeout(timeout)
            sock.sendto(packet, (dns_server, 53))
            data, _ = sock.recvfrom(4096)
    except OSError:
        return ""
    try:
        if len(data) < 12 or struct.unpack(">H", data[:2])[0] != transaction_id:
            return ""
        question_count, answer_count = struct.unpack(">HH", data[4:8])
        offset = 12
        for _ in range(question_count):
            offset = _skip_dns_name(data, offset) + 4
        for _ in range(answer_count):
            offset = _skip_dns_name(data, offset)
            record_type, record_class, _ttl, data_length = struct.unpack(">HHIH", data[offset:offset + 10])
            offset += 10
            if record_type == 1 and record_class == 1 and data_length == 4:
                return socket.inet_ntoa(data[offset:offset + 4])
            offset += data_length
    except (OSError, ValueError, struct.error):
        return ""
    return ""


class WindowsDirectRoute:
    """Maintain temporary physical-gateway routes for one HTTP session."""

    def __init__(self) -> None:
        self._enabled = os.name == "nt"
        self._interface_index = 0
        self._gateway = ""
        self._bypass_required = False
        self._dns_servers: tuple[str, ...] = ()
        self._added_routes: set[str] = set()
        self._resolved: dict[str, str] = {}

    def __enter__(self) -> "WindowsDirectRoute":
        if not self._enabled:
            return self
        context = get_windows_default_route_context()
        if context is None:
            # Route discovery may briefly fail while Windows updates adapters
            # or while PowerShell is busy. Retry once instead of failing the
            # whole subscription update from a transient cached result.
            context = get_windows_default_route_context(force_refresh=True)
        if context is None:
            raise DirectNetworkUnavailable(
                "Не удалось определить физический интернет-интерфейс для прямой загрузки"
            )
        self._bypass_required = bool(context.tun_active)
        if not self._bypass_required:
            return self
        if (
            not context.is_physical
            or context.interface_index <= 0
            or not _is_ipv4(context.next_hop)
        ):
            raise DirectNetworkUnavailable(
                "Не удалось обойти TUN: физический интернет-интерфейс недоступен"
            )
        self._interface_index = context.interface_index
        self._gateway = context.next_hop
        candidates = tuple(context.dns_servers) + _FALLBACK_DNS_SERVERS
        self._dns_servers = tuple(dict.fromkeys(item for item in candidates if _is_ipv4(item)))
        return self

    def _has_exact_route(self, ip: str) -> bool:
        if not self._enabled or not self._bypass_required:
            return True
        script = (
            f"$route = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '{ip}/32' "
            "-ErrorAction SilentlyContinue | Where-Object { "
            f"$_.InterfaceIndex -eq {self._interface_index} -and $_.NextHop -eq '{self._gateway}' "
            "} | Select-Object -First 1; if ($route) { exit 0 } else { exit 1 }"
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

    def _ensure_route(self, ip: str) -> bool:
        if not self._enabled or not self._bypass_required:
            return True
        if ip in self._added_routes or self._has_exact_route(ip):
            return True
        try:
            result = run_text_pumped(
                [
                    "route", "add", ip, "mask", "255.255.255.255", self._gateway,
                    "metric", "1", "if", str(self._interface_index),
                ],
                timeout=4,
                creationflags=CREATE_NO_WINDOW,
            )
        except Exception:
            return False
        if result.returncode != 0:
            return self._has_exact_route(ip)
        self._added_routes.add(ip)
        return True

    def resolve(self, host: str) -> str:
        host = str(host or "").strip().strip("[]")
        if not host:
            raise DirectNetworkUnavailable("Пустой адрес сервера")
        if not self._enabled or not self._bypass_required:
            return host
        if host in self._resolved:
            return self._resolved[host]
        if _is_ipv4(host):
            if ipaddress.ip_address(host).is_loopback:
                self._resolved[host] = host
                return host
            if not self._ensure_route(host):
                raise DirectNetworkUnavailable(f"Не удалось создать прямой маршрут к {host}")
            self._resolved[host] = host
            return host

        for dns_server in self._dns_servers:
            if not self._ensure_route(dns_server):
                continue
            ip = _resolve_a_direct(host, dns_server)
            if not ip or ip.startswith(("0.", "127.", "169.254.", "198.18.", "198.19.")):
                continue
            if not self._ensure_route(ip):
                continue
            self._resolved[host] = ip
            return ip
        raise DirectNetworkUnavailable(
            f"Не удалось напрямую определить адрес сервера: {host}"
        )

    def __exit__(self, *_exc) -> None:
        if self._enabled and self._bypass_required:
            for ip in tuple(self._added_routes):
                try:
                    run_text_pumped(
                        ["route", "delete", ip, "mask", "255.255.255.255", self._gateway],
                        timeout=4,
                        creationflags=CREATE_NO_WINDOW,
                    )
                except Exception:
                    pass
        self._added_routes.clear()
        self._resolved.clear()


def _connect_direct(connection: http.client.HTTPConnection, resolver) -> None:
    sys.audit("http.client.connect", connection, connection.host, connection.port)
    target = resolver(connection.host)
    connection.sock = connection._create_connection(  # noqa: SLF001 - mirrors stdlib connect
        (target, connection.port), connection.timeout, connection.source_address
    )
    try:
        connection.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    except OSError as exc:
        if exc.errno != errno.ENOPROTOOPT:
            raise
    if connection._tunnel_host:  # noqa: SLF001
        connection._tunnel()  # noqa: SLF001


class _DirectHTTPConnection(http.client.HTTPConnection):
    def __init__(self, host, *, resolver, **kwargs):
        self._direct_resolver = resolver
        super().__init__(host, **kwargs)

    def connect(self) -> None:
        _connect_direct(self, self._direct_resolver)


class _DirectHTTPSConnection(http.client.HTTPSConnection):
    def __init__(self, host, *, resolver, **kwargs):
        self._direct_resolver = resolver
        super().__init__(host, **kwargs)

    def connect(self) -> None:
        _connect_direct(self, self._direct_resolver)
        server_hostname = self._tunnel_host or self.host  # noqa: SLF001
        self.sock = self._context.wrap_socket(self.sock, server_hostname=server_hostname)  # noqa: SLF001


class _DirectHTTPHandler(urllib.request.HTTPHandler):
    def __init__(self, route: WindowsDirectRoute) -> None:
        super().__init__()
        self._route = route

    def http_open(self, request):
        factory = partial(_DirectHTTPConnection, resolver=self._route.resolve)
        return self.do_open(factory, request)


class _DirectHTTPSHandler(urllib.request.HTTPSHandler):
    def __init__(self, route: WindowsDirectRoute) -> None:
        super().__init__(context=get_ssl_context())
        self._route = route

    def https_open(self, request):
        factory = partial(_DirectHTTPSConnection, resolver=self._route.resolve)
        return self.do_open(factory, request, context=self._context)


class DirectUrlOpener:
    """Context-managed urllib opener whose traffic is guaranteed direct."""

    def __init__(self) -> None:
        self._route = WindowsDirectRoute()
        self._opener = None

    def __enter__(self):
        self._route.__enter__()
        self._opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}),
            _DirectHTTPHandler(self._route),
            _DirectHTTPSHandler(self._route),
        )
        return self._opener

    def __exit__(self, *exc) -> None:
        try:
            if self._opener is not None:
                self._opener.close()
        finally:
            self._route.__exit__(*exc)
