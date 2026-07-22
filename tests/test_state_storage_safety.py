from __future__ import annotations

import threading
import time
import json

import pytest
from PyQt6.QtCore import QCoreApplication

from xray_fluent.app_controller import AppController
from xray_fluent.models import AppState, Node
from xray_fluent.security import decrypt_with_passphrase, encrypt_with_passphrase, is_passphrase_encrypted
from xray_fluent.storage import StateLoadError, StateStorage
import xray_fluent.storage as storage_module


def test_empty_existing_state_is_quarantined_instead_of_reset(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    state_file.write_text("", encoding="utf-8")
    storage = StateStorage(state_file)

    with pytest.raises(StateLoadError):
        storage.load()

    assert not state_file.exists()
    assert list(tmp_path.glob("state.json.corrupt-*"))


def test_invalid_state_is_quarantined_instead_of_reset(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    state_file.write_text('{"nodes": [', encoding="utf-8")
    storage = StateStorage(state_file)

    with pytest.raises(StateLoadError):
        storage.load()

    assert not state_file.exists()
    assert list(tmp_path.glob("state.json.corrupt-*"))


def test_corrupt_primary_is_restored_from_rotating_backup(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    storage.save(AppState(nodes=[Node(id="first", name="First")]))
    storage.save(AppState(nodes=[Node(id="second", name="Second")]))
    state_file.write_text('{"nodes": [', encoding="utf-8")

    restored = storage.load()

    assert [node.id for node in restored.nodes] == ["first"]
    assert json.loads(state_file.read_text(encoding="utf-8"))["nodes"][0]["id"] == "first"
    assert list(tmp_path.glob("state.json.corrupt-*"))


def test_complete_temporary_state_recovers_interrupted_replace(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    state_file.write_text("", encoding="utf-8")
    temp_file = tmp_path / ".state.json.tmp"
    temp_file.write_text(
        json.dumps(AppState(nodes=[Node(id="temporary", name="Temporary")]).to_dict()),
        encoding="utf-8",
    )

    restored = storage.load()

    assert [node.id for node in restored.nodes] == ["temporary"]
    assert state_file.is_file()


def test_legacy_absolute_profile_paths_are_rewritten_in_state_and_backups(
    tmp_path,
    monkeypatch,
) -> None:
    data_dir = tmp_path / "Lumen" / "data"
    legacy_data_dir = tmp_path / "Lumen KVN" / "data"
    path_map = {
        "XRAY_CONFIGS_DIR": data_dir / "configs" / "xray",
        "XRAY_TEMPLATES_DIR": data_dir / "templates" / "xray",
        "SINGBOX_CONFIGS_DIR": data_dir / "configs" / "sing-box",
        "SINGBOX_TEMPLATES_DIR": data_dir / "templates" / "sing-box",
    }
    monkeypatch.setattr(storage_module, "DATA_DIR", data_dir)
    monkeypatch.setattr(storage_module, "CONFIGS_DIR", data_dir / "configs")
    monkeypatch.setattr(storage_module, "RUNTIME_DIR", data_dir / "runtime")
    monkeypatch.setattr(storage_module, "LOG_DIR", data_dir / "logs")
    for name, directory in path_map.items():
        monkeypatch.setattr(storage_module, name, directory)
        directory.mkdir(parents=True, exist_ok=True)

    state = AppState()
    settings = state.settings
    references = {
        "xray_config_file": ("configs", "xray", "custom.json"),
        "xray_template_file": ("templates", "xray", "template.json"),
        "singbox_config_file": ("configs", "sing-box", "custom.json"),
        "singbox_template_file": ("templates", "sing-box", "template.json"),
    }
    for field, parts in references.items():
        target = data_dir.joinpath(*parts)
        target.write_text("{}", encoding="utf-8")
        setattr(settings, field, str(legacy_data_dir.joinpath(*parts)))

    state_file = data_dir / "state.enc"
    state_file.parent.mkdir(parents=True, exist_ok=True)
    legacy_payload = json.dumps(state.to_dict())
    state_file.write_text(legacy_payload, encoding="utf-8")
    backup = state_file.with_name("state.enc.bak1")
    backup.write_text(legacy_payload, encoding="utf-8")

    loaded = StateStorage(state_file).load()

    assert loaded.settings.xray_config_file == "custom.json"
    assert loaded.settings.xray_template_file == "template.json"
    assert loaded.settings.singbox_config_file == "custom.json"
    assert loaded.settings.singbox_template_file == "template.json"
    assert "Lumen KVN" not in state_file.read_text(encoding="utf-8")
    assert "Lumen KVN" not in backup.read_text(encoding="utf-8")


def test_passphrase_encrypted_state_keeps_encryption_during_path_migration(
    tmp_path,
    monkeypatch,
) -> None:
    data_dir = tmp_path / "Lumen" / "data"
    configs_dir = data_dir / "configs" / "xray"
    configs_dir.mkdir(parents=True)
    (configs_dir / "custom.json").write_text("{}", encoding="utf-8")
    monkeypatch.setattr(storage_module, "DATA_DIR", data_dir)
    monkeypatch.setattr(storage_module, "CONFIGS_DIR", data_dir / "configs")
    monkeypatch.setattr(storage_module, "RUNTIME_DIR", data_dir / "runtime")
    monkeypatch.setattr(storage_module, "LOG_DIR", data_dir / "logs")
    monkeypatch.setattr(storage_module, "XRAY_CONFIGS_DIR", configs_dir)
    monkeypatch.setattr(storage_module, "XRAY_TEMPLATES_DIR", data_dir / "templates" / "xray")
    monkeypatch.setattr(storage_module, "SINGBOX_CONFIGS_DIR", data_dir / "configs" / "sing-box")
    monkeypatch.setattr(storage_module, "SINGBOX_TEMPLATES_DIR", data_dir / "templates" / "sing-box")

    state = AppState()
    state.settings.xray_config_file = str(
        tmp_path / "LumenKVN" / "data" / "configs" / "xray" / "custom.json"
    )
    passphrase = "migration-secret"
    state_file = data_dir / "state.enc"
    state_file.write_text(
        encrypt_with_passphrase(json.dumps(state.to_dict()).encode("utf-8"), passphrase),
        encoding="utf-8",
    )
    storage = StateStorage(state_file)
    storage.passphrase = passphrase

    loaded = storage.load()
    rewritten = state_file.read_text(encoding="utf-8")

    assert loaded.settings.xray_config_file == "custom.json"
    assert is_passphrase_encrypted(rewritten)
    decrypted = decrypt_with_passphrase(rewritten, passphrase).decode("utf-8")
    assert "LumenKVN" not in decrypted


class _SlowStorage:
    def __init__(self) -> None:
        self.saved = []

    def save(self, state: AppState) -> None:
        time.sleep(0.3)
        self.saved.append(state)


def test_app_controller_save_uses_background_writer() -> None:
    QCoreApplication.instance() or QCoreApplication([])
    controller = AppController()
    storage = _SlowStorage()
    controller.storage = storage
    controller.state = AppState(
        nodes=[
            Node(id=str(index), name=f"node-{index}", server=f"{index}.example.com")
            for index in range(300)
        ]
    )

    started = time.monotonic()
    controller.save()
    elapsed = time.monotonic() - started

    try:
        assert elapsed < 0.2
        controller._flush_state_saves(timeout=2.0)
        assert len(storage.saved) == 1
    finally:
        controller._save_executor_shutdown = True
        controller._save_executor.shutdown(wait=True, cancel_futures=False)


def test_worker_thread_save_does_not_wait_for_gui_event_loop() -> None:
    QCoreApplication.instance() or QCoreApplication([])
    controller = AppController()
    storage = _SlowStorage()
    controller.storage = storage
    controller.state = AppState(
        nodes=[
            Node(id=str(index), name=f"node-{index}", server=f"{index}.example.com")
            for index in range(300)
        ]
    )
    worker = threading.Thread(target=controller.save)

    try:
        worker.start()
        worker.join(timeout=0.5)
        assert not worker.is_alive()
        controller._flush_state_saves(timeout=2.0)
        assert len(storage.saved) == 1
    finally:
        worker.join(timeout=1.0)
        controller._save_executor_shutdown = True
        controller._save_executor.shutdown(wait=True, cancel_futures=False)
