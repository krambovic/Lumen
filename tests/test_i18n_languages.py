from __future__ import annotations

import pytest

from xray_fluent import i18n, models


def test_persian_and_chinese_catalogs_are_available() -> None:
    languages = i18n.available_languages()

    assert languages == ["ru", "en", "fa", "zh"]
    assert i18n.language_name("fa") == "فارسی"
    assert i18n.language_name("zh") == "简体中文"


@pytest.mark.parametrize("language", ["fa", "zh"])
def test_new_language_catalog_can_be_activated(language: str) -> None:
    previous = i18n.get_language()
    try:
        assert i18n.set_language(language) == language
        assert i18n.active_map()
        assert i18n.translate("Настройки") != "Настройки"
    finally:
        i18n.set_language(previous)


@pytest.mark.parametrize(
    ("system_locale", "expected"),
    [
        ("fa_IR", "fa"),
        ("zh_CN", "zh"),
        ("zh_TW", "zh"),
        ("ru_RU", "ru"),
        ("de_DE", "en"),
    ],
)
def test_system_language_detection(
    monkeypatch: pytest.MonkeyPatch,
    system_locale: str,
    expected: str,
) -> None:
    monkeypatch.setattr(models.locale, "getlocale", lambda: (system_locale, "UTF-8"))

    assert models._detect_system_language() == expected


@pytest.mark.parametrize("language", ["ru", "en", "fa", "zh"])
def test_app_settings_preserve_supported_language(language: str) -> None:
    assert models.AppSettings.from_dict({"language": language}).language == language
