from __future__ import annotations

from xray_fluent.windows_network import (
    WindowsAdapterSnapshot,
    select_windows_route_context,
)


def test_select_windows_route_context_ignores_tun_and_keeps_physical_dns() -> None:
    adapters = [
        WindowsAdapterSnapshot(
            alias="singbox_tun",
            description="Wintun Userspace Tunnel",
            interface_index=8,
            gateway="172.18.0.1",
            metric=1,
        ),
        WindowsAdapterSnapshot(
            alias="Wi-Fi",
            interface_index=11,
            gateway="192.168.1.1",
            dns_servers=("9.9.9.9",),
            metric=35,
        ),
        WindowsAdapterSnapshot(
            alias="Ethernet",
            interface_index=15,
            gateway="192.168.100.1",
            dns_servers=("1.1.1.1", "8.8.8.8"),
            metric=20,
        ),
    ]

    context = select_windows_route_context(adapters)

    assert context is not None
    assert context.interface_alias == "Ethernet"
    assert context.interface_index == 15
    assert context.next_hop == "192.168.100.1"
    assert context.dns_servers == ("1.1.1.1", "8.8.8.8")
    assert context.tun_active is True


def test_select_windows_route_context_returns_none_without_physical_gateway() -> None:
    adapters = [
        WindowsAdapterSnapshot(
            alias="Lumen TUN",
            interface_index=8,
            gateway="172.18.0.1",
            metric=1,
        ),
        WindowsAdapterSnapshot(
            alias="Ethernet",
            interface_index=15,
            metric=20,
        ),
    ]

    assert select_windows_route_context(adapters) is None
