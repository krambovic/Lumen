from __future__ import annotations

from pathlib import Path

from xray_fluent import data_paths


def test_user_data_migration_repairs_already_created_target(tmp_path: Path, monkeypatch) -> None:
    legacy = tmp_path / "Lumen KVN" / "data"
    target = tmp_path / "Lumen" / "data"
    legacy.mkdir(parents=True)
    target.mkdir(parents=True)
    (legacy / "state.enc").write_bytes(b"real-old-state")
    (legacy / "install_id").write_text("existing-install", encoding="utf-8")
    (target / "state.enc").write_bytes(b"fresh-broken-state")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))

    result = data_paths.user_data_dir("Lumen")

    assert result == target
    assert (target / "state.enc").read_bytes() == b"real-old-state"
    assert (target / "state.enc.pre_lumen_migration").read_bytes() == b"fresh-broken-state"
    assert (target / "install_id").read_text(encoding="utf-8") == "existing-install"
    assert (target / data_paths.NAME_MIGRATION_MARKER).is_file()
    assert not legacy.exists()
    assert "lumen kvn" not in (target / data_paths.NAME_MIGRATION_MARKER).read_text(encoding="utf-8").casefold()


def test_user_data_migration_runs_only_once(tmp_path: Path, monkeypatch) -> None:
    legacy = tmp_path / "Lumen KVN" / "data"
    legacy.mkdir(parents=True)
    (legacy / "state.enc").write_bytes(b"original")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))

    target = data_paths.user_data_dir("Lumen")
    (target / "state.enc").write_bytes(b"new-lumen-state")

    data_paths.user_data_dir("Lumen")

    assert (target / "state.enc").read_bytes() == b"new-lumen-state"
    assert not legacy.exists()


def test_portable_data_migrates_from_adjacent_legacy_folder(tmp_path: Path, monkeypatch) -> None:
    base_dir = tmp_path / "Lumen"
    install_data = base_dir / "data"
    legacy_data = tmp_path / "Lumen KVN" / "data"
    base_dir.mkdir()
    legacy_data.mkdir(parents=True)
    (base_dir / "portable").write_text("", encoding="utf-8")
    (legacy_data / "state.enc").write_bytes(b"portable-state")
    monkeypatch.setattr(data_paths.sys, "frozen", True, raising=False)

    result = data_paths.resolve_data_dir(base_dir, install_data, "Lumen")

    assert result == install_data
    assert (install_data / "state.enc").read_bytes() == b"portable-state"
    assert not legacy_data.exists()


def test_failed_legacy_cleanup_is_retried_without_overwriting_new_state(tmp_path: Path, monkeypatch) -> None:
    legacy = tmp_path / "Lumen KVN" / "data"
    legacy.mkdir(parents=True)
    (legacy / "state.enc").write_bytes(b"legacy-state")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))

    real_rmtree = data_paths.shutil.rmtree
    attempts = 0

    def fail_cleanup_once(path: Path) -> None:
        nonlocal attempts
        attempts += 1
        if attempts == 1:
            raise PermissionError("still in use")
        real_rmtree(path)

    monkeypatch.setattr(data_paths.shutil, "rmtree", fail_cleanup_once)
    target = data_paths.user_data_dir("Lumen")
    assert legacy.exists()
    assert (target / data_paths.NAME_MIGRATION_MARKER).is_file()

    (target / "state.enc").write_bytes(b"new-state")
    data_paths.user_data_dir("Lumen")

    assert not legacy.exists()
    assert (target / "state.enc").read_bytes() == b"new-state"


def test_pre_release_path_marker_is_sanitized_and_cleanup_finishes(tmp_path: Path, monkeypatch) -> None:
    legacy = tmp_path / "Lumen KVN" / "data"
    target = tmp_path / "Lumen" / "data"
    legacy.mkdir(parents=True)
    target.mkdir(parents=True)
    (legacy / "state.enc").write_bytes(b"legacy-state")
    (target / "state.enc").write_bytes(b"legacy-state")
    marker = target / data_paths.NAME_MIGRATION_MARKER
    marker.write_text(str(legacy), encoding="utf-8")
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))

    data_paths.user_data_dir("Lumen")

    assert not legacy.exists()
    marker_value = marker.read_text(encoding="utf-8").strip()
    assert len(marker_value) == 64
    assert "lumen" not in marker_value
