from __future__ import annotations

from types import SimpleNamespace

import pytest

from xray_fluent.app_controller import AppController
from xray_fluent.application.node_runtime_service import proxy_core_for_node
from xray_fluent.models import Node


def _node(protocol: str) -> Node:
    return Node(
        scheme=protocol,
        server="example.com",
        port=443,
        outbound={"protocol": protocol},
    )


@pytest.mark.parametrize(
    "protocol",
    ["hysteria", "hysteria2", "tuic", "awg", "warp", "wireguard", "mieru", "masque", "singbox_config"],
)
def test_native_singbox_protocols_select_singbox_proxy_core(protocol: str) -> None:
    assert proxy_core_for_node(_node(protocol)) == "singbox"


@pytest.mark.parametrize(
    "protocol",
    ["vless", "vmess", "trojan", "shadowsocks", "socks", "http"],
)
def test_xray_protocols_select_xray_proxy_core(protocol: str) -> None:
    assert proxy_core_for_node(_node(protocol)) == "xray"


def test_insecure_tls_node_selects_singbox_proxy_core() -> None:
    node = _node("trojan")
    node.outbound["streamSettings"] = {
        "security": "tls",
        "tlsSettings": {"serverName": "example.com", "allowInsecure": True},
    }

    assert proxy_core_for_node(node) == "singbox"


def test_xray_proxy_hot_swap_is_rejected_when_selected_node_requires_singbox() -> None:
    controller = SimpleNamespace(
        selected_node=_node("hysteria2"),
        state=SimpleNamespace(settings=SimpleNamespace(tun_mode=False)),
        _inspect_active_xray_config=lambda: (_ for _ in ()).throw(
            AssertionError("Xray config must not be inspected for a sing-box target")
        ),
    )

    assert AppController._can_proxy_hot_swap(controller, SimpleNamespace()) is False
