from __future__ import annotations

from pathlib import Path
import filecmp
import hashlib
import os
import shutil
import sys

PORTABLE_MARKERS = ("portable.txt", "portable")
LEGACY_APP_NAMES = ("Lumen KVN", "lumen-kvn", "LumenKVN", "Lumen_KVN")
NAME_MIGRATION_MARKER = ".lumen_name_migration_v1"
MIGRATION_BACKUP_SUFFIX = ".pre_lumen_migration"


def is_writable(path: Path) -> bool:
    try:
        path.mkdir(parents=True, exist_ok=True)
        probe = path / ".write_test"
        probe.write_text("ok", encoding="utf-8")
        probe.unlink()
        return True
    except Exception:
        return False


def _backup_conflicting_state(source: Path, target: Path) -> None:
    for name in ("state.enc", "traffic_history.json", "install_id"):
        source_file = source / name
        target_file = target / name
        backup_file = target / f"{name}{MIGRATION_BACKUP_SUFFIX}"
        if not source_file.is_file() or not target_file.is_file() or backup_file.exists():
            continue
        shutil.copy2(target_file, backup_file)


def _migration_source_id(source: Path) -> str:
    normalized = str(source.resolve()).replace("\\", "/").casefold()
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


def _copied_tree_matches(source: Path, target: Path) -> bool:
    for source_file in source.rglob("*"):
        if not source_file.is_file():
            continue
        target_file = target / source_file.relative_to(source)
        if not target_file.is_file() or not filecmp.cmp(source_file, target_file, shallow=False):
            return False
    return True


def _remove_migrated_source(source: Path) -> None:
    shutil.rmtree(source)
    try:
        source.parent.rmdir()
    except OSError:
        pass


def _migrate_legacy_data(source: Path, target: Path) -> bool:
    """Move legacy user data once, preferring and preserving the pre-rename state."""
    try:
        if source.resolve() == target.resolve() or not source.is_dir():
            return False
        marker = target / NAME_MIGRATION_MARKER
        if marker.is_file():
            if marker.read_text(encoding="utf-8").strip() == _migration_source_id(source):
                _remove_migrated_source(source)
                return True
            return False
        target.mkdir(parents=True, exist_ok=True)
        _backup_conflicting_state(source, target)
        shutil.copytree(source, target, dirs_exist_ok=True)
        if not _copied_tree_matches(source, target):
            return False
        marker.write_text(_migration_source_id(source), encoding="utf-8")
        _remove_migrated_source(source)
        return True
    except Exception:
        return False


def _migrate_first_legacy_data(candidates: tuple[Path, ...], target: Path) -> None:
    marker = target / NAME_MIGRATION_MARKER
    if marker.is_file():
        try:
            marker_value = marker.read_text(encoding="utf-8").strip()
            is_current_marker = len(marker_value) == 64 and all(
                char in "0123456789abcdef" for char in marker_value.lower()
            )
            if not is_current_marker:
                matched_source = next(
                    (
                        candidate
                        for candidate in candidates
                        if marker_value and Path(marker_value).resolve() == candidate.resolve()
                    ),
                    None,
                )
                if matched_source is not None:
                    marker.write_text(_migration_source_id(matched_source), encoding="utf-8")
                elif not any(candidate.is_dir() for candidate in candidates):
                    marker.write_text("complete", encoding="utf-8")
        except Exception:
            pass
        for legacy_dir in candidates:
            if _migrate_legacy_data(legacy_dir, target):
                break
        return
    for legacy_dir in candidates:
        if _migrate_legacy_data(legacy_dir, target):
            break


def user_data_dir(app_name: str) -> Path:
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
    if base:
        target_dir = Path(base) / app_name / "data"
        _migrate_first_legacy_data(
            tuple(Path(base) / legacy_name / "data" for legacy_name in LEGACY_APP_NAMES),
            target_dir,
        )
        return target_dir

    target_dir = Path.home() / f".{app_name}" / "data"
    _migrate_first_legacy_data(
        tuple(Path.home() / f".{legacy_name}" / "data" for legacy_name in LEGACY_APP_NAMES),
        target_dir,
    )
    return target_dir


def _portable_marker_present(base_dir: Path) -> bool:
    for name in PORTABLE_MARKERS:
        try:
            if (base_dir / name).is_file():
                return True
        except Exception:
            pass
    return False


def resolve_data_dir(base_dir: Path, install_data_dir: Path, app_name: str) -> Path:
    if not getattr(sys, "frozen", False):
        return install_data_dir
    if _portable_marker_present(base_dir) and is_writable(install_data_dir):
        _migrate_first_legacy_data(
            tuple(base_dir.parent / legacy_folder / "data" for legacy_folder in LEGACY_APP_NAMES),
            install_data_dir,
        )
        return install_data_dir
    return user_data_dir(app_name)


def seed_user_data(src: Path, dst: Path) -> None:
    if src == dst or not src.exists():
        return
    try:
        dst.mkdir(parents=True, exist_ok=True)
    except Exception:
        return
    for sub in ("templates", "configs"):
        source = src / sub
        target = dst / sub
        if source.is_dir() and not target.exists():
            try:
                shutil.copytree(source, target)
            except Exception:
                pass
    for name in ("state.enc", "traffic_history.json"):
        source = src / name
        target = dst / name
        if not source.is_file():
            continue
        if not target.exists():
            try:
                shutil.copy2(source, target)
            except Exception:
                continue
        if target.exists():
            try:
                source.unlink()
            except Exception:
                pass


_install_id_cache: str | None = None


def get_install_id() -> str:
    """Stable anonymous client id to correlate diagnostics."""
    global _install_id_cache
    if _install_id_cache:
        return _install_id_cache

    from .constants import DATA_DIR

    path = DATA_DIR / "install_id"
    try:
        value = path.read_text(encoding="utf-8").strip()
    except Exception:
        value = ""
    if not value:
        import uuid

        value = uuid.uuid4().hex
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value, encoding="utf-8")
        except Exception:
            pass
    _install_id_cache = value
    return value
