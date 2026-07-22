from __future__ import annotations

from pathlib import Path

from xray_fluent import app_updater
from xray_fluent import startup


def test_legacy_bridge_uses_canonical_executable_for_startup(tmp_path: Path, monkeypatch) -> None:
    legacy = tmp_path / "LumenKVN.exe"
    canonical = tmp_path / "Lumen.exe"
    legacy.write_bytes(b"bridge")
    canonical.write_bytes(b"app")
    monkeypatch.setattr(startup.sys, "frozen", True, raising=False)
    monkeypatch.setattr(startup.sys, "executable", str(legacy))

    command = startup.build_startup_command()

    assert str(canonical) in command
    assert str(legacy) not in command


def test_installed_update_moves_legacy_install_to_renamed_sibling(tmp_path: Path, monkeypatch) -> None:
    legacy_dir = tmp_path / "Lumen KVN"
    monkeypatch.setattr(app_updater, "is_portable", lambda: False)
    monkeypatch.setattr(app_updater, "_registered_install_dir", lambda: legacy_dir)

    assert app_updater._target_app_dir(legacy_dir) == tmp_path / "Lumen"


def test_registered_legacy_install_is_not_reused(tmp_path: Path, monkeypatch) -> None:
    current_dir = tmp_path / "downloaded-copy"
    legacy_dir = tmp_path / "LumenKVN"
    monkeypatch.setattr(app_updater, "is_portable", lambda: False)
    monkeypatch.setattr(app_updater, "_registered_install_dir", lambda: legacy_dir)

    assert app_updater._target_app_dir(current_dir) == tmp_path / "Lumen"


def test_portable_update_keeps_user_selected_directory(tmp_path: Path, monkeypatch) -> None:
    current_dir = tmp_path / "My portable VPN"
    monkeypatch.setattr(app_updater, "is_portable", lambda: True)

    assert app_updater._target_app_dir(current_dir) == current_dir


def test_portable_update_renames_only_known_legacy_directory(tmp_path: Path, monkeypatch) -> None:
    current_dir = tmp_path / "LumenKVN"
    monkeypatch.setattr(app_updater, "is_portable", lambda: True)

    assert app_updater._target_app_dir(current_dir) == tmp_path / "Lumen"


def test_portable_update_does_not_overwrite_existing_renamed_directory(
    tmp_path: Path,
    monkeypatch,
) -> None:
    current_dir = tmp_path / "Lumen KVN"
    (tmp_path / "Lumen").mkdir()
    monkeypatch.setattr(app_updater, "is_portable", lambda: True)

    assert app_updater._target_app_dir(current_dir) == current_dir
