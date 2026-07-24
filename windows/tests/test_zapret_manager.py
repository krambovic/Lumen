from __future__ import annotations

import os
import sys

from PyQt6.QtCore import QCoreApplication

from xray_fluent import zapret_manager


def test_list_presets_cache_tracks_added_files(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(zapret_manager, "PRESETS_DIR", tmp_path)
    zapret_manager.ZapretManager.invalidate_preset_cache()

    (tmp_path / "first.txt").write_text("--hostlist list.txt\n", encoding="utf-8")

    assert zapret_manager.ZapretManager.list_presets() == ["first"]

    (tmp_path / "_hidden.txt").write_text("--skip\n", encoding="utf-8")
    (tmp_path / "second.txt").write_text("--filter-tcp=443\n", encoding="utf-8")

    assert zapret_manager.ZapretManager.list_presets() == ["first", "second"]


def test_parse_preset_args_cache_invalidates_after_file_change(tmp_path) -> None:
    preset = tmp_path / "preset.txt"
    zapret_manager.ZapretManager.invalidate_preset_cache()

    preset.write_text("--hostlist list.txt\n# ignored\n", encoding="utf-8")
    assert zapret_manager.ZapretManager._parse_preset_args(preset) == ["--hostlist list.txt"]

    stat = preset.stat()
    preset.write_text("--filter-tcp=443\n--dpi-desync=fake\n", encoding="utf-8")
    os.utime(preset, ns=(stat.st_atime_ns + 1_000_000_000, stat.st_mtime_ns + 1_000_000_000))

    assert zapret_manager.ZapretManager._parse_preset_args(preset) == [
        "--filter-tcp=443",
        "--dpi-desync=fake",
    ]


def test_missing_ipset_base_is_created_from_ipset_all(tmp_path, monkeypatch) -> None:
    zapret_root = tmp_path / "zapret"
    lists = zapret_root / "lists"
    lists.mkdir(parents=True)
    (lists / "ipset-all.txt").write_text("1.1.1.0/24\n", encoding="utf-8")
    monkeypatch.setattr(zapret_manager, "ZAPRET_DIR", zapret_root)

    missing = zapret_manager.ZapretManager._missing_referenced_files(["--ipset=lists/ipset-base.txt"])

    assert missing == []
    assert (lists / "ipset-base.txt").read_text(encoding="utf-8") == "1.1.1.0/24\n"


def test_ipset_registration_error_is_not_windivert_conflict() -> None:
    assert not zapret_manager.ZapretManager._looks_like_windivert_conflict(
        1,
        ["failed to register ipset 'lists/ipset-base.txt'"],
    )


def test_released_qprocess_is_scheduled_for_deletion() -> None:
    app = QCoreApplication.instance() or QCoreApplication(sys.argv)

    class _Process:
        deleted = False

        def deleteLater(self) -> None:
            self.deleted = True

    manager = zapret_manager.ZapretManager()
    process = _Process()
    manager._process = process

    manager._release_process(process)

    assert manager._process is None
    assert process.deleted is True
    assert app is QCoreApplication.instance()
