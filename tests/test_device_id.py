from __future__ import annotations

import uuid
from pathlib import Path

from xray_fluent import device_id


def _reset_cache() -> None:
    device_id._cached_hwid = None


def test_hwid_is_valid_uuid_and_stable(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setattr(device_id, "_HWID_FILE", tmp_path / "device.id")
    _reset_cache()

    first = device_id.get_device_hwid()
    uuid.UUID(first)  # raises if not a valid UUID

    # Cached in-memory: same value without re-reading.
    assert device_id.get_device_hwid() == first

    # Persisted to disk and reused after a cache reset.
    assert (tmp_path / "device.id").read_text(encoding="utf-8").strip() == first
    _reset_cache()
    assert device_id.get_device_hwid() == first


def test_invalid_stored_hwid_is_regenerated(tmp_path: Path, monkeypatch) -> None:
    hwid_file = tmp_path / "device.id"
    hwid_file.write_text("not-a-uuid", encoding="utf-8")
    monkeypatch.setattr(device_id, "_HWID_FILE", hwid_file)
    _reset_cache()

    value = device_id.get_device_hwid()
    uuid.UUID(value)
    assert value != "not-a-uuid"
    assert hwid_file.read_text(encoding="utf-8").strip() == value
