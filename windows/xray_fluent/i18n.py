from __future__ import annotations

import json
import re
from pathlib import Path

_PLACEHOLDER = re.compile(r"\{(\w+)\}")
_LOCALES_DIR = Path(__file__).resolve().parent / "locales"

SOURCE_LANGUAGE = "ru"
_FALLBACK_NAMES = {"ru": "Русский", "en": "English"}

CATALOGS: dict[str, dict[str, str]] = {}
LANGUAGE_NAMES: dict[str, str] = {}


def _load_catalogs() -> None:
    CATALOGS.clear()
    LANGUAGE_NAMES.clear()
    LANGUAGE_NAMES[SOURCE_LANGUAGE] = _FALLBACK_NAMES[SOURCE_LANGUAGE]
    if not _LOCALES_DIR.is_dir():
        return
    for path in sorted(_LOCALES_DIR.glob("*.json")):
        code = path.stem.lower()
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if not isinstance(data, dict):
            continue
        name = data.get("__name__")
        CATALOGS[code] = {
            key: value
            for key, value in data.items()
            if isinstance(key, str) and not key.startswith("__")
        }
        LANGUAGE_NAMES[code] = name if isinstance(name, str) and name else _FALLBACK_NAMES.get(code, code.upper())


_load_catalogs()

_language = "en" if "en" in CATALOGS else SOURCE_LANGUAGE


def available_languages() -> list[str]:
    return [SOURCE_LANGUAGE] + sorted(CATALOGS)


def language_name(code: str) -> str:
    return LANGUAGE_NAMES.get(code, code.upper())


def set_language(lang: str) -> str:
    global _language
    candidate = (lang or "").lower()
    if candidate in available_languages():
        _language = candidate
    elif "en" in CATALOGS:
        _language = "en"
    else:
        _language = SOURCE_LANGUAGE
    return _language


def get_language() -> str:
    return _language


def active_map() -> dict:
    if _language == SOURCE_LANGUAGE:
        return {}
    return CATALOGS.get(_language, {})


def translate(_key: str, params: dict | None = None) -> str:
    text = active_map().get(_key, _key)
    if params:
        def _sub(match):
            name = match.group(1)
            return str(params[name]) if name in params else match.group(0)
        text = _PLACEHOLDER.sub(_sub, text)
    return text


def translate_dynamic(message: str, catalog: dict[str, str] | None = None) -> str | None:
    """Translate a rendered message by matching catalog keys with placeholders.

    Backend workers report complete Russian messages such as
    ``sing-box актуален (1.2.3)``.  Regular key lookup cannot translate those
    once the version has already been inserted, so match them against catalog
    templates such as ``sing-box актуален ({version})``.
    """
    translations = active_map() if catalog is None else catalog
    if not message or not translations:
        return None
    exact = translations.get(message)
    if exact is not None:
        return exact

    templates = sorted(
        (source for source in translations if _PLACEHOLDER.search(source)),
        key=len,
        reverse=True,
    )
    for source in templates:
        parts = _PLACEHOLDER.split(source)
        field_names: list[str] = []
        pattern_parts: list[str] = []
        for index, part in enumerate(parts):
            if index % 2:
                field_index = len(field_names)
                field_names.append(part)
                pattern_parts.append(f"(?P<_field_{field_index}>.*?)")
            else:
                pattern_parts.append(re.escape(part))
        pattern = "".join(pattern_parts)
        match = re.fullmatch(pattern, message, flags=re.DOTALL)
        if match is None:
            continue
        params = {
            name: match.group(f"_field_{index}")
            for index, name in enumerate(field_names)
        }
        translated = translations[source]
        return _PLACEHOLDER.sub(
            lambda placeholder: params.get(placeholder.group(1), placeholder.group(0)),
            translated,
        )
    return None


def tr(_key: str, **params) -> str:
    return translate(_key, params)
