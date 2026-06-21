from xray_fluent.app_updater import AppUpdate, should_auto_install
from xray_fluent.models import AppSettings


def _update(*, downgrade: bool = False) -> AppUpdate:
    return AppUpdate(
        version="2.0.0",
        tag="v2.0.0",
        download_url="https://example.test/setup.exe",
        size=1,
        notes="",
        is_downgrade=downgrade,
    )


def test_app_auto_update_setting_round_trip() -> None:
    settings = AppSettings(app_auto_update=True)
    restored = AppSettings.from_dict(settings.to_dict())
    assert restored.app_auto_update is True


def test_app_auto_update_defaults_to_disabled() -> None:
    assert AppSettings.from_dict({}).app_auto_update is False


def test_auto_install_requires_permission_and_never_downgrades() -> None:
    assert should_auto_install(_update(), enabled=True, allow_updates=True)
    assert not should_auto_install(_update(), enabled=False, allow_updates=True)
    assert not should_auto_install(_update(), enabled=True, allow_updates=False)
    assert not should_auto_install(_update(downgrade=True), enabled=True, allow_updates=True)
