from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

from xray_fluent.application import profile_service
from xray_fluent.application.profile_service import (
    sync_singbox_routing_from_config,
)
from xray_fluent.models import RoutingSettings
from xray_fluent.routing_runtime import apply_singbox_gui_routing


class _ProfileController:
    def __init__(self, routing: RoutingSettings | None = None) -> None:
        self.connected = False
        self._desired_connected = False
        self._active_core = ""
        self.state = SimpleNamespace(routing=routing or RoutingSettings())
        self.cached: list[tuple[Path, str]] = []
        self.transitions: list[str] = []
        self.routing_updates: list[tuple[RoutingSettings, bool]] = []
        self.active_singbox_path: Path | None = None

    def _cache_singbox_document_state(self, path: Path, text: str) -> None:
        self.cached.append((path, text))

    def _request_transition(self, reason: str) -> None:
        self.transitions.append(reason)

    def update_routing(self, routing: RoutingSettings, *, restart_runtime: bool = True) -> None:
        self.state.routing = routing
        self.routing_updates.append((routing, restart_runtime))

    @staticmethod
    def validate_json_text(_text: str) -> tuple[bool, str]:
        return True, "JSON корректен"

    @staticmethod
    def is_singbox_editor_mode() -> bool:
        return False

    def _set_active_singbox_config_path(self, path: Path) -> Path:
        self.active_singbox_path = path
        return path


def test_gui_singbox_final_is_mirrored_into_runtime_routing_state() -> None:
    controller = _ProfileController(
        RoutingSettings(
            mode="rule",
            preset_id="custom-user-preset",
            tun_default_outbound="proxy",
        )
    )

    changed = sync_singbox_routing_from_config(
        controller,
        '{"route":{"rules":[],"final":"direct"}}',
    )

    assert changed is True
    assert controller.state.routing.mode == "rule"
    assert controller.state.routing.preset_id == "custom-user-preset"
    assert controller.state.routing.tun_default_outbound == "direct"
    assert controller.routing_updates[-1][1] is False


def test_gui_singbox_unknown_final_does_not_change_routing_state() -> None:
    controller = _ProfileController(RoutingSettings(tun_default_outbound="proxy"))

    changed = sync_singbox_routing_from_config(
        controller,
        '{"route":{"final":"custom-selector"}}',
    )

    assert changed is False
    assert controller.routing_updates == []


def test_saving_in_gui_persists_text_and_honors_route_final(tmp_path: Path, monkeypatch) -> None:
    active = tmp_path / "singbox.json"
    controller = _ProfileController(
        RoutingSettings(
            mode="global",
            preset_id="global",
            tun_default_outbound="proxy",
        )
    )
    monkeypatch.setattr(
        profile_service,
        "ensure_active_config",
        lambda _controller, _engine, _path=None, sync_template=False: active,
    )
    text = '{"route":{"rules":[],"final":"direct"}}'

    saved_path = profile_service.save_config_text(controller, "singbox", text)

    assert saved_path == active
    assert active.read_text(encoding="utf-8") == text
    assert controller.active_singbox_path == active
    assert controller.cached[-1] == (active, text)
    assert controller.state.routing.mode == "rule"
    assert controller.state.routing.preset_id == "global"
    assert controller.state.routing.tun_default_outbound == "direct"

    runtime = {"route": {"rules": [], "final": "proxy"}}
    apply_singbox_gui_routing(runtime, controller.state.routing)
    assert runtime["route"]["final"] == "direct"


def test_saving_unrelated_json_edit_does_not_override_newer_routing_choice(
    tmp_path: Path,
    monkeypatch,
) -> None:
    active = tmp_path / "singbox.json"
    controller = _ProfileController(
        RoutingSettings(mode="rule", tun_default_outbound="direct")
    )
    monkeypatch.setattr(
        profile_service,
        "ensure_active_config",
        lambda _controller, _engine, _path=None, sync_template=False: active,
    )
    previous = '{"log":{"level":"info"},"route":{"final":"proxy"}}'
    updated = '{"log":{"level":"warning"},"route":{"final":"proxy"}}'

    profile_service.save_config_text(
        controller,
        "singbox",
        updated,
        previous_text=previous,
    )

    assert active.read_text(encoding="utf-8") == updated
    assert controller.state.routing.tun_default_outbound == "direct"
    assert controller.routing_updates == []


def test_applying_json_explicitly_honors_unchanged_route_final(tmp_path: Path, monkeypatch) -> None:
    active = tmp_path / "singbox.json"
    controller = _ProfileController(
        RoutingSettings(mode="rule", tun_default_outbound="proxy")
    )
    monkeypatch.setattr(
        profile_service,
        "ensure_active_config",
        lambda _controller, _engine, _path=None, sync_template=False: active,
    )
    text = '{"route":{"final":"direct"}}'

    ok, saved_path, _message = profile_service.apply_singbox_config_text(
        controller,
        text,
        previous_text=text,
    )

    assert ok is True
    assert saved_path == active
    assert controller.state.routing.tun_default_outbound == "direct"
