from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.app_controller import AppController
from xray_fluent.application.signature_service import transition_signature
from xray_fluent.core_resource_updater import (
    IRAN_RULES_LATEST_API,
    LOYALSOLDIER_LATEST_API,
    RUNETFREEDOM_LATEST_API,
    _resolve_geodata_release,
)
from xray_fluent.models import AppSettings, RoutingSettings


def _release(*asset_names: str) -> dict:
    return {
        "tag_name": "test-release",
        "assets": [
            {
                "name": name,
                "browser_download_url": f"https://example.invalid/{name}",
                "digest": "sha256:" + ("a" * 64),
            }
            for name in asset_names
        ],
    }


def test_regional_profile_defaults_to_russia_and_is_normalized() -> None:
    assert AppSettings().regional_preset == "russia"
    assert AppSettings.from_dict({"regional_preset": "china"}).regional_preset == "china"
    assert AppSettings.from_dict({"regional_preset": "unknown"}).regional_preset == "russia"


def test_geodata_release_source_follows_region(monkeypatch) -> None:
    seen: list[str] = []

    def request(url: str, **_kwargs):
        seen.append(url)
        if url == RUNETFREEDOM_LATEST_API:
            return _release("geoip.dat", "geosite.dat", "sing-box.zip")
        return _release("geoip.dat", "geosite.dat")

    monkeypatch.setattr("xray_fluent.core_resource_updater._request_json", request)

    _resolve_geodata_release(region="russia")
    _resolve_geodata_release(region="china")
    _resolve_geodata_release(region="iran")

    assert seen == [RUNETFREEDOM_LATEST_API, LOYALSOLDIER_LATEST_API, IRAN_RULES_LATEST_API]


def test_regional_profile_commit_updates_settings_and_routing_with_one_restart() -> None:
    settings_events = []
    routing_events = []
    transitions = []
    saves = []
    controller = SimpleNamespace(
        state=SimpleNamespace(
            settings=AppSettings(regional_preset="russia"),
            routing=RoutingSettings(preset_id="blocked", mode="rule"),
        ),
        settings_changed=SimpleNamespace(emit=settings_events.append),
        routing_changed=SimpleNamespace(emit=routing_events.append),
        schedule_save=lambda: saves.append(True),
        _routing_signature=lambda routing: repr(routing),
        _log=lambda _message: None,
        connected=True,
        _desired_connected=True,
        _request_transition=transitions.append,
    )

    changed = AppController._commit_regional_preset(
        controller,
        "china",
        restart_runtime=True,
    )

    assert changed
    assert controller.state.settings.regional_preset == "china"
    assert controller.state.routing.preset_id == "blocked_cn"
    assert settings_events == [controller.state.settings]
    assert routing_events == [controller.state.routing]
    assert saves == [True]
    assert transitions == ["regional routing profile changed"]


def test_region_changes_runtime_signature_even_when_global_preset_stays_the_same() -> None:
    controller = SimpleNamespace(
        selected_node=None,
        is_singbox_editor_mode=lambda _settings: False,
        uses_xray_raw_config=lambda _settings: False,
    )
    routing = RoutingSettings(preset_id="global", mode="global")
    russia = AppSettings(regional_preset="russia")
    china = AppSettings(regional_preset="china")

    assert transition_signature(controller, settings=russia, routing=routing) != transition_signature(
        controller,
        settings=china,
        routing=routing,
    )
