from __future__ import annotations

from pathlib import Path

from .constants import BASE_DIR


LEGACY_APP_DIR_NAMES = {"lumen kvn", "lumenkvn", "lumen-kvn", "lumen_kvn"}


def _clean_path_value(path_value: str | Path | None) -> str:
    return str(path_value or "").strip()


def _base_relative(path: Path) -> Path | None:
    try:
        return path.resolve(strict=False).relative_to(BASE_DIR.resolve(strict=False))
    except ValueError:
        return None


def _looks_like_default_location(path: Path, default_path: Path) -> bool:
    default_relative = _base_relative(default_path) or default_path
    path_parts = tuple(part.casefold() for part in path.parts)
    default_parts = tuple(part.casefold() for part in default_relative.parts)
    if len(path_parts) < len(default_parts):
        return False
    return path_parts[-len(default_parts):] == default_parts


def _is_legacy_default_location(path: Path, default_path: Path) -> bool:
    default_relative = _base_relative(default_path) or default_path
    relative_parts = tuple(default_relative.parts)
    if not relative_parts or len(path.parts) <= len(relative_parts):
        return False
    install_root = path.parents[len(relative_parts) - 1]
    if install_root.name.casefold() in LEGACY_APP_DIR_NAMES:
        return True
    try:
        return install_root.resolve(strict=False) == Path("C:/Program").resolve(strict=False)
    except OSError:
        return False


def normalize_path_for_storage(path_value: str | Path | None) -> str:
    text = _clean_path_value(path_value)
    if not text:
        return ""

    path = Path(text)
    if not path.is_absolute():
        return str(path)

    relative = _base_relative(path)
    if relative is not None:
        return str(relative)
    return str(path)


def normalize_configured_path(
    path_value: str | Path | None,
    *,
    default_path: Path | None = None,
    use_default_if_empty: bool = False,
    migrate_default_location: bool = False,
) -> str:
    text = _clean_path_value(path_value)
    if not text:
        if use_default_if_empty and default_path is not None:
            return normalize_path_for_storage(default_path)
        return ""

    path = Path(text)
    if (
        default_path is not None
        and migrate_default_location
        and path.is_absolute()
        and _looks_like_default_location(path, default_path)
        and (not path.exists() or _is_legacy_default_location(path, default_path))
    ):
        return normalize_path_for_storage(default_path)

    return normalize_path_for_storage(path)


def resolve_configured_path(
    path_value: str | Path | None,
    *,
    default_path: Path | None = None,
    use_default_if_empty: bool = False,
    migrate_default_location: bool = False,
) -> Path | None:
    normalized = normalize_configured_path(
        path_value,
        default_path=default_path,
        use_default_if_empty=use_default_if_empty,
        migrate_default_location=migrate_default_location,
    )
    if not normalized:
        return None

    path = Path(normalized)
    if path.is_absolute():
        return path.resolve(strict=False)
    return (BASE_DIR / path).resolve(strict=False)
