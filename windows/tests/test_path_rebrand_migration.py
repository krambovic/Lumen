from __future__ import annotations

from pathlib import Path

from xray_fluent import path_utils


def test_existing_core_path_in_legacy_install_is_migrated(tmp_path: Path, monkeypatch) -> None:
    base_dir = tmp_path / "Lumen"
    default_path = base_dir / "core" / "xray.exe"
    legacy_path = tmp_path / "Lumen KVN" / "core" / "xray.exe"
    default_path.parent.mkdir(parents=True)
    default_path.write_bytes(b"new-core")
    legacy_path.parent.mkdir(parents=True)
    legacy_path.write_bytes(b"old-core")
    monkeypatch.setattr(path_utils, "BASE_DIR", base_dir)

    normalized = path_utils.normalize_configured_path(
        legacy_path,
        default_path=default_path,
        migrate_default_location=True,
    )

    assert normalized == str(Path("core") / "xray.exe")


def test_existing_custom_core_path_is_preserved(tmp_path: Path, monkeypatch) -> None:
    base_dir = tmp_path / "Lumen"
    default_path = base_dir / "core" / "sing-box.exe"
    custom_path = tmp_path / "CustomCore" / "core" / "sing-box.exe"
    custom_path.parent.mkdir(parents=True)
    custom_path.write_bytes(b"custom-core")
    monkeypatch.setattr(path_utils, "BASE_DIR", base_dir)

    normalized = path_utils.normalize_configured_path(
        custom_path,
        default_path=default_path,
        migrate_default_location=True,
    )

    assert normalized == str(custom_path)


def test_source_checkout_falls_back_to_repository_level_default_core(
    tmp_path: Path, monkeypatch
) -> None:
    base_dir = tmp_path / "windows"
    default_path = base_dir / "core" / "xray.exe"
    repository_core = tmp_path / "core" / "xray.exe"
    repository_core.parent.mkdir(parents=True)
    repository_core.write_bytes(b"source-core")
    monkeypatch.setattr(path_utils, "BASE_DIR", base_dir)
    monkeypatch.setattr(path_utils.sys, "frozen", False, raising=False)

    resolved = path_utils.resolve_configured_path(
        "core/xray.exe",
        default_path=default_path,
        use_default_if_empty=True,
        migrate_default_location=True,
    )

    assert resolved == repository_core.resolve()


def test_missing_custom_core_does_not_use_source_checkout_fallback(
    tmp_path: Path, monkeypatch
) -> None:
    base_dir = tmp_path / "windows"
    default_path = base_dir / "core" / "xray.exe"
    repository_core = tmp_path / "core" / "xray.exe"
    repository_core.parent.mkdir(parents=True)
    repository_core.write_bytes(b"source-core")
    custom_path = tmp_path / "custom" / "xray.exe"
    monkeypatch.setattr(path_utils, "BASE_DIR", base_dir)
    monkeypatch.setattr(path_utils.sys, "frozen", False, raising=False)

    resolved = path_utils.resolve_configured_path(
        custom_path,
        default_path=default_path,
        migrate_default_location=True,
    )

    assert resolved == custom_path.resolve()
