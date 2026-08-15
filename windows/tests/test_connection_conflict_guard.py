from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import connection_service
from xray_fluent.models import AppSettings
from xray_fluent.qml_app.bridge.app_bridge import AppBridge


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

    def owned_core_process_pids(self) -> set[int]:
        return set()

    def owned_core_executable_paths(self) -> set[str]:
        return set()


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


def test_conflict_guard_defaults_on_and_round_trips() -> None:
    settings = AppSettings.from_dict({})
    assert settings.block_vpn_conflicts is True
    restored = AppSettings.from_dict(AppSettings(block_vpn_conflicts=False).to_dict())
    assert restored.block_vpn_conflicts is False


def test_conflict_guard_can_be_disabled_without_scanning() -> None:
    controller = _Controller()
    controller.state = SimpleNamespace(
        settings=SimpleNamespace(block_vpn_conflicts=False)
    )
    assert connection_service.should_block_vpn_conflicts(controller) is False


def test_network_mode_request_bypasses_conflict_dialog_when_disabled(monkeypatch) -> None:
    bridge = AppBridge.__new__(AppBridge)
    bridge.controller = SimpleNamespace(
        state=SimpleNamespace(
            settings=SimpleNamespace(block_vpn_conflicts=False)
        )
    )

    def fail_if_scanned(*_args, **_kwargs):
        raise AssertionError("conflict scan must be skipped when the guard is disabled")

    monkeypatch.setattr("xray_fluent.qml_app.bridge.app_bridge.scan_network_conflicts", fail_if_scanned)
    assert AppBridge._request_network_mode(bridge, "tun") is True


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
