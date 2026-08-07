from __future__ import annotations

import json

from xray_fluent.models import AppSettings, AppState, Node
from xray_fluent.security import decode_encrypted, encode_encrypted, is_passphrase_encrypted
from xray_fluent.storage import StateStorage


def test_dpapi_state_file_stays_encrypted_after_save(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    payload = storage._serialize_state(AppState(nodes=[Node(id="first", name="First")]))
    state_file.write_text(encode_encrypted(payload.encode("utf-8")), encoding="utf-8")

    state = storage.load()
    state.nodes.append(Node(id="second", name="Second"))
    storage.save(state)

    saved = state_file.read_text(encoding="utf-8")
    assert not saved.lstrip().startswith("{")
    restored = json.loads(decode_encrypted(saved).decode("utf-8"))
    assert [node["id"] for node in restored["nodes"]] == ["first", "second"]
    backup = state_file.with_name("state.json.bak1")
    assert not backup.read_text(encoding="utf-8").lstrip().startswith("{")


def test_plain_state_file_is_not_encrypted_by_save(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    storage.save(AppState(nodes=[Node(id="first", name="First")]))

    storage.load()
    storage.save(AppState(nodes=[Node(id="second", name="Second")]))

    assert json.loads(state_file.read_text(encoding="utf-8"))["nodes"][0]["id"] == "second"


def test_proxy_auth_password_upgrades_plain_windows_state_to_dpapi(tmp_path, monkeypatch) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    monkeypatch.setattr("xray_fluent.storage.sys.platform", "win32")
    state = AppState(
        settings=AppSettings(
            proxy_auth_enabled=True,
            proxy_auth_username="alice",
            proxy_auth_password="secret",
        )
    )

    storage.save(state)

    saved = state_file.read_text(encoding="utf-8")
    assert not saved.lstrip().startswith("{")
    restored = json.loads(decode_encrypted(saved).decode("utf-8"))
    assert restored["settings"]["proxy_auth_password"] == "secret"


def test_passphrase_wins_over_the_stored_encoding(tmp_path) -> None:
    state_file = tmp_path / "state.json"
    storage = StateStorage(state_file)
    payload = storage._serialize_state(AppState(nodes=[Node(id="first", name="First")]))
    state_file.write_text(encode_encrypted(payload.encode("utf-8")), encoding="utf-8")

    state = storage.load()
    storage.passphrase = "secret"
    storage.save(state)

    assert is_passphrase_encrypted(state_file.read_text(encoding="utf-8"))
