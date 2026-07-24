from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import connection_service


class _Controller:
    def __init__(self) -> None:
        self._connecting = False
        self._kill_switch_engaged = False
        self._reconnecting = False
        self.locked = False
        self.xray = SimpleNamespace(_proc=None)
        self.singbox = SimpleNamespace(_proc=None)
        self.messages: list[tuple[str, str, str | None]] = []

    def _set_connection_status(self, phase: str, message: str, level: str | None = None) -> None:
        self.messages.append((phase, message, level))


def test_known_client_blocks_connection_with_product_name(monkeypatch) -> None:
    controller = _Controller()
    monkeypatch.setattr(
        connection_service,
        "scan_network_conflicts",
        lambda *_args, **_kwargs: {"apps": ["v2rayN"], "ports": [], "unknown_client": False},
    )

    assert connection_service.connect_selected(controller) is False
    assert "v2rayN" in controller.messages[-1][1]
    assert "xray.exe" not in controller.messages[-1][1]


def test_unknown_client_blocks_connection_without_core_name(monkeypatch) -> None:
    controller = _Controller()
    monkeypatch.setattr(
        connection_service,
        "scan_network_conflicts",
        lambda *_args, **_kwargs: {"apps": [], "ports": [], "unknown_client": True},
    )

    assert connection_service.connect_selected(controller) is False
    assert "другой VPN/прокси-клиент" in controller.messages[-1][1]
    assert "xray.exe" not in controller.messages[-1][1]
