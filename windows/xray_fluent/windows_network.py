from __future__ import annotations

import ctypes
from dataclasses import dataclass
import os
import re
import socket
from ctypes import wintypes


_ERROR_BUFFER_OVERFLOW = 111
_GAA_FLAG_INCLUDE_GATEWAYS = 0x0080
_IF_OPER_STATUS_UP = 1
_IF_TYPE_SOFTWARE_LOOPBACK = 24
_IF_TYPE_TUNNEL = 131
_TUN_NAME_RE = re.compile(r"(?i)lumen|xftun|singbox|wintun|(?:^|[^a-z])tun(?:[^a-z]|$)")


@dataclass(frozen=True, slots=True)
class WindowsAdapterSnapshot:
    alias: str
    interface_index: int
    gateway: str = ""
    dns_servers: tuple[str, ...] = ()
    metric: int = 0
    is_up: bool = True
    if_type: int = 0
    description: str = ""

    @property
    def looks_like_tun(self) -> bool:
        name = f"{self.alias} {self.description}"
        return bool(_TUN_NAME_RE.search(name))


@dataclass(frozen=True, slots=True)
class WindowsNativeRouteContext:
    interface_alias: str
    interface_index: int
    next_hop: str
    dns_servers: tuple[str, ...]
    tun_active: bool


class _SocketAddress(ctypes.Structure):
    _fields_ = [
        ("lp_sockaddr", ctypes.c_void_p),
        ("sockaddr_length", ctypes.c_int),
    ]


class _AdapterDnsAddress(ctypes.Structure):
    pass


class _AdapterGatewayAddress(ctypes.Structure):
    pass


class _AdapterAddresses(ctypes.Structure):
    pass


_AdapterDnsAddress._fields_ = [
    ("length", wintypes.ULONG),
    ("reserved", wintypes.DWORD),
    ("next", ctypes.POINTER(_AdapterDnsAddress)),
    ("address", _SocketAddress),
]

_AdapterGatewayAddress._fields_ = [
    ("length", wintypes.ULONG),
    ("reserved", wintypes.DWORD),
    ("next", ctypes.POINTER(_AdapterGatewayAddress)),
    ("address", _SocketAddress),
]

# Fields through ipv4_metric from IP_ADAPTER_ADDRESSES_LH.  The structure is
# variable-sized, but these offsets are stable on all supported Windows 10/11
# versions and are enough for direct-route selection.
_AdapterAddresses._fields_ = [
    ("length", wintypes.ULONG),
    ("if_index", wintypes.DWORD),
    ("next", ctypes.POINTER(_AdapterAddresses)),
    ("adapter_name", ctypes.c_char_p),
    ("first_unicast_address", ctypes.c_void_p),
    ("first_anycast_address", ctypes.c_void_p),
    ("first_multicast_address", ctypes.c_void_p),
    ("first_dns_server_address", ctypes.POINTER(_AdapterDnsAddress)),
    ("dns_suffix", ctypes.c_wchar_p),
    ("description", ctypes.c_wchar_p),
    ("friendly_name", ctypes.c_wchar_p),
    ("physical_address", ctypes.c_ubyte * 8),
    ("physical_address_length", wintypes.DWORD),
    ("flags", wintypes.DWORD),
    ("mtu", wintypes.DWORD),
    ("if_type", wintypes.DWORD),
    ("oper_status", ctypes.c_int),
    ("ipv6_if_index", wintypes.DWORD),
    ("zone_indices", wintypes.DWORD * 16),
    ("first_prefix", ctypes.c_void_p),
    ("transmit_link_speed", ctypes.c_ulonglong),
    ("receive_link_speed", ctypes.c_ulonglong),
    ("first_wins_server_address", ctypes.c_void_p),
    ("first_gateway_address", ctypes.POINTER(_AdapterGatewayAddress)),
    ("ipv4_metric", wintypes.ULONG),
    ("ipv6_metric", wintypes.ULONG),
]


class _SockaddrIn(ctypes.Structure):
    _fields_ = [
        ("family", ctypes.c_ushort),
        ("port", ctypes.c_ushort),
        ("address", ctypes.c_ubyte * 4),
        ("zero", ctypes.c_ubyte * 8),
    ]


def _ipv4_from_socket_address(value: _SocketAddress) -> str:
    if not value.lp_sockaddr or value.sockaddr_length < ctypes.sizeof(_SockaddrIn):
        return ""
    sockaddr = ctypes.cast(value.lp_sockaddr, ctypes.POINTER(_SockaddrIn)).contents
    if int(sockaddr.family) != socket.AF_INET:
        return ""
    try:
        return socket.inet_ntop(socket.AF_INET, bytes(sockaddr.address))
    except (OSError, ValueError):
        return ""


def _linked_ipv4_addresses(pointer, *, limit: int = 32) -> tuple[str, ...]:
    result: list[str] = []
    current = pointer
    for _ in range(limit):
        if not current:
            break
        item = current.contents
        address = _ipv4_from_socket_address(item.address)
        if address and address not in result:
            result.append(address)
        current = item.next
    return tuple(result)


def _query_adapter_snapshots() -> list[WindowsAdapterSnapshot]:
    if os.name != "nt":
        return []
    iphlpapi = ctypes.WinDLL("iphlpapi.dll", use_last_error=True)
    get_adapters_addresses = iphlpapi.GetAdaptersAddresses
    get_adapters_addresses.argtypes = [
        wintypes.ULONG,
        wintypes.ULONG,
        ctypes.c_void_p,
        ctypes.POINTER(_AdapterAddresses),
        ctypes.POINTER(wintypes.ULONG),
    ]
    get_adapters_addresses.restype = wintypes.ULONG

    size = wintypes.ULONG(16 * 1024)
    buffer = ctypes.create_string_buffer(size.value)
    for _ in range(3):
        result = int(
            get_adapters_addresses(
                socket.AF_INET,
                _GAA_FLAG_INCLUDE_GATEWAYS,
                None,
                ctypes.cast(buffer, ctypes.POINTER(_AdapterAddresses)),
                ctypes.byref(size),
            )
        )
        if result != _ERROR_BUFFER_OVERFLOW:
            break
        buffer = ctypes.create_string_buffer(size.value)
    if result != 0:
        return []

    snapshots: list[WindowsAdapterSnapshot] = []
    current = ctypes.cast(buffer, ctypes.POINTER(_AdapterAddresses))
    for _ in range(256):
        if not current:
            break
        item = current.contents
        gateways = _linked_ipv4_addresses(item.first_gateway_address)
        dns_servers = _linked_ipv4_addresses(item.first_dns_server_address)
        snapshots.append(
            WindowsAdapterSnapshot(
                alias=str(item.friendly_name or "").strip(),
                description=str(item.description or "").strip(),
                interface_index=int(item.if_index),
                gateway=gateways[0] if gateways else "",
                dns_servers=dns_servers,
                metric=int(item.ipv4_metric),
                is_up=int(item.oper_status) == _IF_OPER_STATUS_UP,
                if_type=int(item.if_type),
            )
        )
        current = item.next
    return snapshots


def select_windows_route_context(
    adapters: list[WindowsAdapterSnapshot],
) -> WindowsNativeRouteContext | None:
    active = [item for item in adapters if item.is_up and item.interface_index > 0]
    tun_active = any(item.looks_like_tun for item in active)
    candidates = [
        item
        for item in active
        if item.gateway
        and not item.looks_like_tun
        and item.if_type not in {_IF_TYPE_SOFTWARE_LOOPBACK, _IF_TYPE_TUNNEL}
    ]
    if not candidates:
        return None
    physical = min(
        candidates,
        key=lambda item: (
            item.metric if item.metric > 0 else 2**31,
            item.interface_index,
        ),
    )
    return WindowsNativeRouteContext(
        interface_alias=physical.alias or physical.description,
        interface_index=physical.interface_index,
        next_hop=physical.gateway,
        dns_servers=physical.dns_servers,
        tun_active=tun_active,
    )


def query_windows_route_context() -> WindowsNativeRouteContext | None:
    try:
        return select_windows_route_context(_query_adapter_snapshots())
    except (AttributeError, OSError, TypeError, ValueError):
        return None
