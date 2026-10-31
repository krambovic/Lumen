"""Curated "What's new" notes shown by the welcome flow after an update.

Titles and texts are Russian source strings; QML translates them through
``I18n.t`` so every note needs matching keys in ``locales/*.json``.  Add an
entry for every released ``APP_VERSION`` (tests enforce this).
"""

from __future__ import annotations

import re
from typing import Any

# Saved profiles written before Lumen tracked the welcome flow have no
# ``onboarding_version``.  They are upgrades, never fresh installs.
LEGACY_PROFILE_VERSION = "0"

_VERSION_RE = re.compile(r"^v?(\d+(?:\.\d+){0,3})(?:[-+].*)?$")

# glyph: Segoe Fluent Icons code point rendered next to the note.
RELEASE_NOTES: dict[str, tuple[dict[str, str], ...]] = {
    "2.1.3": (
        {
            "glyph": "\uEC05",
            "title": "Подписки и автовыбор",
            "text": "Импорт из буфера распознаёт подписки; исправлены группы автовыбора и переключение серверов.",
        },
        {
            "glyph": "\uE9D9",
            "title": "Задержка без хэндшейков",
            "text": "HTTP GET по умолчанию; задержка на панели измеряется на прогретом соединении.",
        },
        {
            "glyph": "\uE895",
            "title": "Обновлённые компоненты",
            "text": "sing-box extended 2.7.3 с исправлениями Lumen и droute 2.2.1 в комплекте и обновлениях.",
        },
        {
            "glyph": "\uE790",
            "title": "Стабильность и оформление",
            "text": "Исправлены ложные конфликты ядер и подсказки; новая светлая иконка и обновления только для Desktop.",
        },
    ),
    "2.1.2": (
        {
            "glyph": "",
            "title": "Смена ядра в TUN",
            "text": "TUN снова передаёт данные после переключения между Xray и sing-box extended.",
        },
    ),
    "2.1.1": (
        {
            "glyph": "",
            "title": "Выбор основного ядра",
            "text": "Auto, Xray или sing-box extended в Настройках → Сеть; на Xray работает и TUN.",
        },
        {
            "glyph": "",
            "title": "Быстрый запуск TUN",
            "text": "TUN включается за секунду, а не за 10–20 секунд.",
        },
        {
            "glyph": "",
            "title": "Плавный интерфейс",
            "text": "Без подвисаний при запуске и остановке Zapret и TUN, изменении размера окна и раскрытии боковой панели.",
        },
    ),
    "2.1.0": (
        {
            "glyph": "\uEC05",
            "title": "Новая вкладка «Серверы»",
            "text": "Выбор подписки и группы в одном месте, инспектор сервера и подписки, все действия — в меню по правому клику.",
        },
        {
            "glyph": "\uE80F",
            "title": "Новая панель управления",
            "text": "Подключение, серверы, задержка и трафик приложений — на одном экране.",
        },
        {
            "glyph": "\uE790",
            "title": "Новые иконки",
            "text": "Обновлены иконки Маршрутов, Zapret и Логов; Логи переехали вниз.",
        },
        {
            "glyph": "\uE943",
            "title": "Конфиги — в настройках",
            "text": "Редактор конфигов теперь открывается кнопкой в Настройках → Данные.",
        },
    ),
    "2.0.0": (
        {
            "glyph": "\uE753",
            "title": "Бесплатный WARP в один клик",
            "text": "Lumen сам получает профили Cloudflare WARP — AmneziaWG и MASQUE — и добавляет их в группу «WARP». Без ботов и ручного копирования ключей.",
        },
        {
            "glyph": "\uE945",
            "title": "Zapret для отдельного сервера",
            "text": "Обход DPI можно включить для конкретного сервера: стратегия применяется только к его адресу и не мешает остальному трафику.",
        },
        {
            "glyph": "\uE71B",
            "title": "Цепочки прокси",
            "text": "Подключайтесь к серверу через промежуточный прокси или выпускайте трафик через дополнительный выходной узел.",
        },
        {
            "glyph": "\uE8BC",
            "title": "Быстрая настройка при первом запуске",
            "text": "Новый мастер: язык, тема, регион, маршрутизация, серверы, обновления, автозапуск и диагностика — за пару минут. После обновлений Lumen коротко расскажет, что изменилось.",
        },
    ),
}

FALLBACK_NOTE: dict[str, str] = {
    "glyph": "\uE895",
    "title": "Исправления и улучшения",
    "text": "Lumen стал стабильнее и быстрее. Подробный список изменений — на странице релиза на GitHub.",
}


def version_key(value: Any) -> tuple[int, ...]:
    """Numeric release key; empty for unknown/invalid values."""
    match = _VERSION_RE.match(str(value or "").strip())
    if not match:
        return ()
    parts = [int(part) for part in match.group(1).split(".")]
    while len(parts) < 3:
        parts.append(0)
    return tuple(parts)


def normalize_seen_version(value: Any) -> str:
    """Keep a saved onboarding version only when it is a plausible release."""
    text = str(value or "").strip().lstrip("v")
    if not text:
        return ""
    return text if version_key(text) else LEGACY_PROFILE_VERSION


def is_upgrade(seen_version: str, app_version: str) -> bool:
    """True when the profile last completed the flow on an older release."""
    seen = str(seen_version or "").strip()
    if not seen:
        return False  # fresh install: nothing to compare with
    current = version_key(app_version)
    previous = version_key(seen)
    if not current:
        return False
    promoted_to_stable = (
        previous == current
        and "-" in seen.split("+", 1)[0]
        and "-" not in str(app_version).split("+", 1)[0]
    )
    return not previous or previous < current or promoted_to_stable


def notes_between(seen_version: str, app_version: str, *, limit: int = 3) -> list[dict[str, Any]]:
    """Release groups newer than ``seen_version`` up to ``app_version`` (newest first)."""
    current_text = str(app_version or "").strip().lstrip("v")
    current = version_key(current_text)
    previous = version_key(seen_version) or (0, 0, 0)
    groups: list[dict[str, Any]] = []
    for version in sorted(RELEASE_NOTES, key=version_key, reverse=True):
        key = version_key(version)
        if key and key <= current and (previous < key or is_upgrade(seen_version, version)):
            groups.append({"version": version, "items": [dict(item) for item in RELEASE_NOTES[version]]})
    if current and (not groups or version_key(groups[0]["version"]) != current):
        groups.insert(0, {"version": current_text, "items": [dict(FALLBACK_NOTE)]})
    return groups[: max(1, int(limit))]


def release_note_strings() -> set[str]:
    """Every translatable string used by the notes (for catalog tests)."""
    strings = {FALLBACK_NOTE["title"], FALLBACK_NOTE["text"]}
    for items in RELEASE_NOTES.values():
        for item in items:
            strings.add(item["title"])
            strings.add(item["text"])
    return strings
