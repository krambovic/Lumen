from __future__ import annotations

from pathlib import Path
import zipfile

import pytest

import build_qml


def test_repository_droute_bundle_is_complete() -> None:
    assert build_qml._validate_droute_bundle() == "2.0.0"


def test_droute_bundle_validation_rejects_missing_payload(tmp_path: Path) -> None:
    (tmp_path / "version.txt").write_text("2.0.0\n", encoding="utf-8")

    with pytest.raises(RuntimeError, match="incomplete"):
        build_qml._validate_droute_bundle(tmp_path)


def test_subscription_fetcher_is_copied_into_packaged_app(tmp_path: Path) -> None:
    main_executable = tmp_path / "Lumen.exe"
    main_executable.write_bytes(b"pyinstaller-launcher")

    helper = build_qml._install_subscription_fetcher(tmp_path)

    assert helper == tmp_path / "lumen-subscription-fetcher.exe"
    assert helper.read_bytes() == b"pyinstaller-launcher"


def test_legacy_update_bridge_is_copied_into_packaged_app(tmp_path: Path) -> None:
    main_executable = tmp_path / "Lumen.exe"
    main_executable.write_bytes(b"new-lumen-binary")

    bridge = build_qml._install_legacy_update_bridge(tmp_path)

    assert bridge == tmp_path / "LumenKVN.exe"
    assert bridge.read_bytes() == b"new-lumen-binary"


def test_portable_zip_has_flat_root_for_legacy_updater(tmp_path: Path, monkeypatch) -> None:
    app_dir = tmp_path / "Lumen"
    app_dir.mkdir()
    (app_dir / "Lumen.exe").write_bytes(b"binary")
    (app_dir / "data").mkdir()
    (app_dir / "data" / "version.txt").write_text("1", encoding="utf-8")
    archive = tmp_path / "portable.zip"
    monkeypatch.setattr(build_qml, "APP_DIR", app_dir)

    build_qml._pack_zip(archive)

    with zipfile.ZipFile(archive) as zf:
        names = set(zf.namelist())
    assert "Lumen.exe" in names
    assert "data/version.txt" in names
    assert not any(name.startswith("Lumen/") for name in names)


def test_installer_carries_one_launch_bridge_for_legacy_updater() -> None:
    installer = build_qml.INNO_SCRIPT.read_text(encoding="utf-8")

    assert "UsePreviousAppDir=no" in installer
    assert 'Excludes: "zapret\\exe\\*.sys,portable,LumenKVN.exe"' in installer
    assert 'Type: files; Name: "{app}\\LumenKVN.exe"' in installer
    assert 'Type: files; Name: "{app}\\assets\\LumenKVN.ico"' in installer
    assert "WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LumenKVN_is1" in installer
    assert "procedure UseCanonicalInstallDir;" in installer
    assert "function PrepareToInstall(var NeedsRestart: Boolean): String;" in installer
    assert "Notifications\\Settings\\Lumen.LumenKVN" in installer
    assert "Classes\\Applications\\LumenKVN.exe" in installer
    assert "HKLM\\Software\\Classes\\lumen-kvn" in installer


def test_installer_registers_lumen_deep_link_protocol() -> None:
    installer = build_qml.INNO_SCRIPT.read_text(encoding="utf-8")

    assert '[Registry]' in installer
    assert 'Subkey: "Software\\Classes\\lumen"' in installer
    assert 'ValueName: "URL Protocol"' in installer
    assert '"""{app}\\Lumen.exe"" ""%1"""' in installer


def test_only_current_brand_assets_are_tracked() -> None:
    assert (build_qml.ASSETS_DIR / "Lumen.ico").is_file()
    assert (build_qml.ASSETS_DIR / "Lumen.png").is_file()
    assert not (build_qml.ASSETS_DIR / "LumenKVN.ico").exists()
    assert not (build_qml.ASSETS_DIR / "LumenKVN.png").exists()
