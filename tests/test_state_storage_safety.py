from __future__ import annotations

import threading
import time
import json

import pytest
from PyQt6.QtCore import QCoreApplication

from xray_fluent.app_controller import AppController
from xray_fluent.models import AppState, Node
from xray_fluent.storage import StateLoadError, StateStorage


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
