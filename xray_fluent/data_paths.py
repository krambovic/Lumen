from __future__ import annotations

from pathlib import Path
import os
import shutil
import sys

PORTABLE_MARKERS = ("portable.txt", "portable")


def is_writable(path: Path) -> bool:
    try:
        path.mkdir(parents=True, exist_ok=True)
        probe = path / ".write_test"
        probe.write_text("ok", encoding="utf-8")
        probe.unlink()
        return True
    except Exception:
        return False


def user_data_dir(app_name: str) -> Path:
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
    if base:
        return Path(base) / app_name / "data"
    return Path.home() / f".{app_name}" / "data"


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
