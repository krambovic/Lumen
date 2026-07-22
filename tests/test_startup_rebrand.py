from __future__ import annotations

from pathlib import Path

from xray_fluent import app_updater
from xray_fluent import startup


def test_legacy_registry_cleanup_covers_all_app_owned_identities() -> None:
    assert r"Software\Classes\lumen-kvn" in startup.LEGACY_PROTOCOL_KEYS
    assert r"Software\Classes\AppUserModelId\Lumen.LumenKVN" in startup.LEGACY_PROTOCOL_KEYS
    assert r"Software\Classes\Applications\LumenKVN.exe" in startup.LEGACY_PROTOCOL_KEYS
    assert r"Software\Microsoft\Windows\CurrentVersion\App Paths\LumenKVN.exe" in startup.LEGACY_PROTOCOL_KEYS
    assert (
        r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\Lumen.LumenKVN"
        in startup.LEGACY_PROTOCOL_KEYS
    )


def test_legacy_shell_shortcuts_and_start_menu_groups_are_removed(
    tmp_path: Path,
    monkeypatch,
) -> None:
    appdata = tmp_path / "AppData" / "Roaming"
    program_data = tmp_path / "ProgramData"
    user_profile = tmp_path / "User"
    public_profile = tmp_path / "Public"
    monkeypatch.setenv("APPDATA", str(appdata))
    monkeypatch.setenv("ProgramData", str(program_data))
    monkeypatch.setenv("USERPROFILE", str(user_profile))
    monkeypatch.setenv("PUBLIC", str(public_profile))

    old_group = appdata / "Microsoft" / "Windows" / "Start Menu" / "Programs" / "Lumen KVN"
    old_group.mkdir(parents=True)
    (old_group / "Lumen KVN.lnk").write_bytes(b"shortcut")
    old_startup = (
        program_data
        / "Microsoft"
        / "Windows"
        / "Start Menu"
        / "Programs"
        / "Startup"
        / "LumenKVN.lnk"
    )
    old_startup.parent.mkdir(parents=True)
    old_startup.write_bytes(b"shortcut")
    old_desktop = user_profile / "Desktop" / "lumen-kvn.lnk"
    old_desktop.parent.mkdir(parents=True)
    old_desktop.write_bytes(b"shortcut")

    startup._cleanup_legacy_shell_entries()

    assert not old_group.exists()
    assert not old_startup.exists()
    assert not old_desktop.exists()


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
