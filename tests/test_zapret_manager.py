from __future__ import annotations

import os

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
