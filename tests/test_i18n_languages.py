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
    ("language", "expected"),
    [
        (
            "en",
            "sing-box extended 1.13.14-extended-2.5.2 is available. "
            "A compatible core will be installed together with a Lumen update.",
        ),
        (
            "fa",
            "نسخه 1.13.14-extended-2.5.2 از sing-box extended در دسترس است. "
            "هستهٔ سازگار همراه با به‌روزرسانی Lumen نصب خواهد شد.",
        ),
        (
            "zh",
            "sing-box extended 1.13.14-extended-2.5.2 可用。"
            "兼容内核将随 Lumen 更新一起安装。",
        ),
    ],
)
def test_dynamic_singbox_update_message_is_fully_translated(
    language: str,
    expected: str,
) -> None:
    message = (
        "Доступен sing-box extended 1.13.14-extended-2.5.2. "
        "Совместимое ядро устанавливается вместе с обновлением Lumen"
    )

    assert i18n.translate_dynamic(message, i18n.CATALOGS[language]) == expected


@pytest.mark.parametrize("language", ["en", "fa", "zh"])
def test_dynamic_update_messages_do_not_leave_russian_status_text(language: str) -> None:
    catalog = i18n.CATALOGS[language]
    messages = (
        "droute актуален (1.2.3, встроенный)",
        "geoip.dat и geosite.dat обновлены до 20260718",
        "Доступно обновление Xray до 26.7.19",
        "Доступен переход Xray на 26.7.18",
        "Xray core актуален (26.7.18)",
        "архив содержит sing-box 1.13.14, ожидалась версия 1.13.15",
    )

    for message in messages:
        translated = i18n.translate_dynamic(message, catalog)
        assert translated is not None
        assert translated != message
        assert not any("\u0400" <= char <= "\u04ff" for char in translated)


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
