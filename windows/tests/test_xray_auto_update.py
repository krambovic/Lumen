from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application import update_service
from xray_fluent.engines.xray.core_updater import XrayCoreUpdateResult
from xray_fluent.models import AppSettings


class _Signal:
    def __init__(self) -> None:
        self.values = []

    def emit(self, *values) -> None:
        self.values.append(values)


def _controller(*, connected: bool) -> SimpleNamespace:
    return SimpleNamespace(
        connected=connected,
        _shutting_down=False,
        _xray_update_worker=None,
        _xray_update_apply_requested=False,
        _reconnect_after_xray_update=False,
        _xray_update_silent=False,
        _xray_update_proxy_url=None,
        state=SimpleNamespace(
            settings=SimpleNamespace(
                xray_path="xray.exe",
                xray_release_channel="beta",
                xray_update_feed_url="",
            )
        ),
        status=_Signal(),
        xray_update_result=_Signal(),
        _log=lambda _message: None,
        get_effective_http_proxy_port=lambda: 10809,
    )


def test_silent_auto_update_checks_while_connected(monkeypatch) -> None:
    controller = _controller(connected=True)
    starts = []
    monkeypatch.setattr(
        update_service,
        "_start_xray_worker",
        lambda _controller, *, apply_update: starts.append(apply_update),
    )

    update_service.run_xray_core_update(controller, apply_update=True, silent=True)

    assert starts == [False]
    assert controller._xray_update_apply_requested is True
    assert controller._xray_update_proxy_url == "http://127.0.0.1:10809"


def test_silent_available_result_starts_install_without_toast(monkeypatch) -> None:
    controller = _controller(connected=True)
    controller._xray_update_apply_requested = True
    controller._xray_update_silent = True
    starts = []
    monkeypatch.setattr(
        update_service,
        "_start_xray_worker",
        lambda _controller, *, apply_update: starts.append(apply_update),
    )
    result = XrayCoreUpdateResult(
        status="available",
        message="available",
        channel="beta",
        current_version="26.7.11",
        latest_version="26.7.12-beta.1",
    )

    update_service.on_xray_update_worker_done(controller, result)

    assert starts == [True]
    assert controller.status.values == []
    assert controller._xray_update_silent is True


def test_xray_release_channel_defaults_to_beta_but_preserves_stable() -> None:
    assert AppSettings.from_dict({}).xray_release_channel == "beta"
    assert AppSettings.from_dict({"xray_release_channel": "stable"}).xray_release_channel == "stable"
