import pytest

from xray_fluent import app_updater


def release(tag, *, prerelease=False, assets=None, **extra):
    return dict(tag_name=tag, prerelease=prerelease, draft=False,
                assets=assets if assets is not None else [
                    {"name": "Lumen-Setup-windows-x64.exe"},
                    {"name": "Lumen-portable-windows-x64.zip"},
                ], **extra)


@pytest.mark.parametrize("portable", [False, True])
def test_android_latest_falls_back_to_newest_compatible_desktop(monkeypatch, portable):
    monkeypatch.setattr(app_updater, "is_portable", lambda: portable)
    android = release("ANDROID-v99.0.0")
    desktop = release("v2.0.0")
    releases = [android, release("v3.0.0", assets=[{"name": "Lumen.apk"}]),
                release("v1.0.0"), desktop, release("v4.0.0-pre.1", prerelease=True)]
    checker = app_updater.UpdateChecker()
    monkeypatch.setattr(checker, "_fetch_json", lambda url:
                        android if url == app_updater.GITHUB_API else releases)
    assert checker._pick_target() is desktop


@pytest.mark.parametrize("portable", [False, True])
def test_release_must_have_asset_for_current_install_type(monkeypatch, portable):
    monkeypatch.setattr(app_updater, "is_portable", lambda: portable)
    incompatible = release("v3.0.0", assets=[{
        "name": "Lumen-Setup.exe" if portable else "Lumen-portable-windows-x64.zip"}])
    desktop = release("v2.0.0")
    checker = app_updater.UpdateChecker()
    monkeypatch.setattr(checker, "_fetch_json", lambda url:
                        incompatible if url == app_updater.GITHUB_API else [incompatible, desktop])
    assert checker._pick_target() is desktop


def test_beta_filters_android_and_compares_semver_reusing_release_list(monkeypatch):
    monkeypatch.setattr(app_updater, "is_portable", lambda: False)
    android = release("android-v99.0.0")
    beta = release("v2.0.0-pre.10", prerelease=True)
    releases = [android, release("v9.0.0-android", prerelease=True),
                release("v2.0.0-pre.9", prerelease=True), beta, release("v1.0.0")]
    calls = []
    checker = app_updater.UpdateChecker(channel="beta")
    def fetch(url):
        calls.append(url)
        return android if url == app_updater.GITHUB_API else releases
    monkeypatch.setattr(checker, "_fetch_json", fetch)
    assert checker._pick_target() is beta
    assert calls.count(app_updater.RELEASES_API) == 1


def test_search_continues_past_full_page_of_android_releases(monkeypatch):
    monkeypatch.setattr(app_updater, "is_portable", lambda: False)
    android = release("android-v99.0.0")
    desktop = release("v2.0.0")
    pages = {app_updater.GITHUB_API: android,
             app_updater.RELEASES_API: [android] * 100,
             app_updater.RELEASES_API + "&page=2": [desktop]}
    checker = app_updater.UpdateChecker()
    monkeypatch.setattr(checker, "_fetch_json", pages.__getitem__)
    assert checker._pick_target() is desktop


def test_only_android_releases_emit_no_update_or_error(monkeypatch):
    monkeypatch.setattr(app_updater, "is_portable", lambda: False)
    android = release("android-v99.0.0")
    checker = app_updater.UpdateChecker()
    monkeypatch.setattr(checker, "_fetch_json", lambda url:
                        android if url == app_updater.GITHUB_API else [android])
    results, errors = [], []
    checker.result.connect(results.append)
    checker.error.connect(errors.append)
    checker.run()
    assert results == [None]
    assert errors == []
