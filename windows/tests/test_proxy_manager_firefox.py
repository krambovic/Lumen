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
    monkeypatch.setattr(manager, "_persist_backup", lambda _values: None)
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


def _firefox_profile(monkeypatch, tmp_path, *, running: bool):
    manager = FirefoxProxyManager()
    manager._backup_file = tmp_path / "firefox-backup.json"
    profile = tmp_path / "profile"
    profile.mkdir()
    (profile / "prefs.js").write_text('user_pref("browser.theme", "old");\n')
    monkeypatch.setattr(manager, "_find_profiles", lambda: [profile])
    state = {"running": running}
    monkeypatch.setattr(manager, "_browser_running", lambda: state["running"])
    manager.enable(http_port=10809, socks_port=10808)
    return manager, profile, state


def _firefox_rewrite(profile: Path) -> None:
    # Firefox drops markers and prefs equal to their defaults on exit.
    (profile / "prefs.js").write_text(
        "// Mozilla User Preferences\n"
        'user_pref("browser.theme", "old");\n'
        'user_pref("network.proxy.http", "127.0.0.1");\n'
        'user_pref("network.proxy.http_port", 10808);\n'
        'user_pref("network.proxy.socks", "127.0.0.1");\n'
        'user_pref("network.proxy.socks_port", 10808);\n'
        'user_pref("network.proxy.type", 1);\n'
    )


def test_firefox_restore_handles_prefs_rewritten_by_browser(monkeypatch, tmp_path) -> None:
    manager, profile, _state = _firefox_profile(monkeypatch, tmp_path, running=False)
    _firefox_rewrite(profile)

    manager.disable()

    prefs = (profile / "prefs.js").read_text()
    assert "network.proxy" not in prefs and '"browser.theme", "old"' in prefs
    assert not (profile / "user.js").exists()
    assert not manager._backup_file.exists()


def test_firefox_restore_is_pinned_while_browser_runs(monkeypatch, tmp_path) -> None:
    manager, profile, state = _firefox_profile(monkeypatch, tmp_path, running=True)

    manager.disable()

    user_js = (profile / "user.js").read_text()
    assert 'user_pref("network.proxy.type", 5);' in user_js
    assert "Lumen system proxy begin" not in user_js
    assert manager._backup_file.exists()

    _firefox_rewrite(profile)  # the browser exits with the old proxy in memory
    state["running"] = False
    manager.disable()

    assert "network.proxy" not in (profile / "prefs.js").read_text()
    assert not (profile / "user.js").exists()
    assert not manager._backup_file.exists()


def test_running_firefox_restore_does_not_pin_over_another_proxy(monkeypatch, tmp_path):
    manager, profile, _state = _firefox_profile(monkeypatch, tmp_path, running=True)
    for name in ("user.js", "prefs.js"):
        target = profile / name
        target.write_text(target.read_text().replace('"127.0.0.1"', '"other.proxy"'))
    manager.disable()
    text = (profile / "user.js").read_text()
    assert '"other.proxy"' in text
    assert "Lumen proxy restore begin" not in text
