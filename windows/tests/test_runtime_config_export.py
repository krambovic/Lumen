from __future__ import annotations

import json
from types import SimpleNamespace

from xray_fluent.app_controller import AppController
from xray_fluent.models import Node


def test_native_singbox_proxy_exports_proxy_runtime_json() -> None:
    node = Node(
        scheme="hysteria2",
        server="example.com",
        port=443,
        outbound={"protocol": "hysteria2"},
    )
    calls: list[bool] = []

    class _Controller:
        selected_node = node
        state = SimpleNamespace(settings=SimpleNamespace(tun_mode=False))

        @staticmethod
        def _get_node_by_id(_node_id: str):
            return node

        @staticmethod
        def is_singbox_editor_mode(_settings) -> bool:
            return False

        @staticmethod
        def _plan_runtime_singbox(_node, *, tun_mode: bool):
            calls.append(tun_mode)
            return SimpleNamespace(singbox_config={"inbounds": [{"type": "mixed"}]})

        @staticmethod
        def uses_xray_raw_config():
            raise AssertionError("native sing-box node must not enter the Xray export path")

    payload = AppController.export_runtime_config_json(_Controller())

    assert calls == [False]
    assert json.loads(payload or "{}") == {"inbounds": [{"type": "mixed"}]}
