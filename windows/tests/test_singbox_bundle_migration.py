from __future__ import annotations

from pathlib import Path

import xray_fluent.path_utils as path_utils
import xray_fluent.storage as storage_module
from xray_fluent.models import AppState
from xray_fluent.storage import StateStorage


def test_legacy_bundled_singbox_paths_migrate_to_current_bundle(
    tmp_path: Path, monkeypatch
) -> None:
    base_dir = tmp_path / "Lumen"
    old_path = base_dir / "core" / "sing-box.exe"
    previous_versioned_path = (
        base_dir / "core" / "sing-box-extended-1.14.1-2.7.2" / "sing-box.exe"
    )
    new_path = base_dir / "core" / "sing-box-extended" / "sing-box.exe"
    old_path.parent.mkdir(parents=True)
    old_path.write_bytes(b"old core")
    previous_versioned_path.parent.mkdir(parents=True)
    previous_versioned_path.write_bytes(b"previous versioned core")
    new_path.parent.mkdir(parents=True)
    new_path.write_bytes(b"new core")

    monkeypatch.setattr(storage_module, "BASE_DIR", base_dir)
    monkeypatch.setattr(storage_module, "SINGBOX_PATH_DEFAULT", new_path)
    monkeypatch.setattr(path_utils, "BASE_DIR", base_dir)
    monkeypatch.setattr(path_utils.sys, "frozen", False, raising=False)

    storage = StateStorage.__new__(StateStorage)
    for legacy_path in (
        Path("core") / "sing-box.exe",
        old_path,
        Path("core") / "sing-box-extended-1.14.1-2.7.2" / "sing-box.exe",
        previous_versioned_path,
    ):
        state = AppState()
        state.settings.singbox_path = str(legacy_path)

        normalized = storage._normalize_state_paths(state)

        assert normalized.settings.singbox_path == str(
            Path("core") / "sing-box-extended" / "sing-box.exe"
        )
