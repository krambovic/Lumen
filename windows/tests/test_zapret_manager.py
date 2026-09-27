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


def test_missing_preset_lists_are_reported_without_creating_placeholders(tmp_path, monkeypatch) -> None:
    zapret_root = tmp_path / "zapret"
    lists = zapret_root / "lists"
    lists.mkdir(parents=True)
    existing = lists / "custom.txt"
    existing.write_text("example.org\n", encoding="utf-8")
    monkeypatch.setattr(zapret_manager, "ZAPRET_DIR", zapret_root)

    args = [
        "--hostlist=lists/cloudflare.txt",
        "--ipset-exclude=lists/ipset-akamai.txt",
        "--hostlist=lists/custom.txt",
    ]
    missing = zapret_manager.ZapretManager._missing_referenced_files(args)

    assert missing == [lists / "cloudflare.txt", lists / "ipset-akamai.txt"]
    assert not (lists / "cloudflare.txt").exists()
    assert not (lists / "ipset-akamai.txt").exists()
    assert existing.read_text(encoding="utf-8") == "example.org\n"


def test_missing_preset_list_paths_outside_lists_are_not_created(tmp_path, monkeypatch) -> None:
    zapret_root = tmp_path / "zapret"
    (zapret_root / "lists").mkdir(parents=True)
    monkeypatch.setattr(zapret_manager, "ZAPRET_DIR", zapret_root)

    missing = zapret_manager.ZapretManager._missing_referenced_files(
        ["--hostlist=../outside.txt", "--ipset=lists/nested/inside.txt"]
    )

    assert len(missing) == 2
    assert not (tmp_path / "outside.txt").exists()
    assert not (zapret_root / "lists" / "nested").exists()


def test_missing_bundled_blob_is_detected_before_start(tmp_path, monkeypatch) -> None:
    zapret_root = tmp_path / "zapret"
    (zapret_root / "bin").mkdir(parents=True)
    monkeypatch.setattr(zapret_manager, "ZAPRET_DIR", zapret_root)

    missing = zapret_manager.ZapretManager._missing_referenced_files(
        ["--blob=tls:@bin/missing.bin", "--lua-init=@lua/missing.lua"]
    )

    assert missing == [zapret_root / "bin" / "missing.bin", zapret_root / "lua" / "missing.lua"]


def test_every_bundled_preset_has_its_referenced_resources() -> None:
    missing = {}
    for preset in zapret_manager.PRESETS_DIR.glob("*.txt"):
        args = zapret_manager.ZapretManager._parse_preset_args(preset)
        paths = zapret_manager.ZapretManager._referenced_zapret_files(args)
        absent = [
            str(path.relative_to(zapret_manager.ZAPRET_DIR))
            for path in paths
            if not path.is_file()
            and not (
                path.name == "ipset-base.txt"
                and path.parent.name == "lists"
                and (path.parent / "ipset-all.txt").is_file()
            )
        ]
        if absent:
            missing[preset.name] = sorted(set(absent))

    assert not missing


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
