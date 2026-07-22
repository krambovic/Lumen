from pathlib import Path

from xray_fluent.models import AppSettings
from xray_fluent.proxy_manager import FirefoxProxyManager, ProxyManager


class _FailingFirefoxProxy:
    def __init__(self) -> None:
        self.enable_calls = 0
        self.disable_calls = 0

    def enable(self, **_kwargs) -> None:
        self.enable_calls += 1
        raise PermissionError(13, "Permission denied", "user.js")

    def disable(self) -> None:
        self.disable_calls += 1
        raise PermissionError(13, "Permission denied", "user.js")


def _isolated_proxy_manager(monkeypatch) -> tuple[ProxyManager, _FailingFirefoxProxy]:
    manager = ProxyManager()
    firefox = _FailingFirefoxProxy()
    manager._firefox_proxy = firefox
    manager._backup = {}
    monkeypatch.setattr(manager, "_write_settings", lambda _values: None)
    monkeypatch.setattr(manager, "_set_wininet_connection_proxy", lambda *_args: True)
    return manager, firefox


def test_firefox_integration_is_disabled_by_default() -> None:
    settings = AppSettings()
    assert settings.firefox_proxy_integration is False
    assert AppSettings.from_dict({}).firefox_proxy_integration is False
    assert settings.to_dict()["firefox_proxy_integration"] is False


def test_firefox_permission_error_does_not_fail_system_proxy(monkeypatch) -> None:
    manager, firefox = _isolated_proxy_manager(monkeypatch)

    manager.enable(10809, 10808, configure_firefox=True)

    assert firefox.enable_calls == 1


def test_disabled_firefox_integration_ignores_cleanup_permission_error(monkeypatch) -> None:
    manager, firefox = _isolated_proxy_manager(monkeypatch)

    manager.enable(10809, 10808, configure_firefox=False)

    assert firefox.disable_calls == 1


def test_firefox_manager_skips_locked_profile_and_continues(monkeypatch) -> None:
    manager = FirefoxProxyManager()
    locked = Path("locked.default-release")
    writable = Path("writable.default-release")
    monkeypatch.setattr(manager, "_find_profiles", lambda: [locked, writable])
    monkeypatch.setattr(
        manager,
        "_load_backup",
        lambda: {
            str(locked): {"user.js": "", "prefs.js": ""},
            str(writable): {"user.js": "", "prefs.js": ""},
        },
    )
    monkeypatch.setattr(manager, "_save_backup", lambda _backup: None)
    attempted: list[Path] = []

    def write_profile(profile: Path, **_kwargs) -> None:
        attempted.append(profile)
        if profile == locked:
            raise PermissionError(13, "Permission denied", str(profile / "user.js"))

    monkeypatch.setattr(manager, "_write_profile_prefs", write_profile)

    manager.enable(http_port=10809, socks_port=10808)

    assert attempted == [locked, writable]
