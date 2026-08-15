from __future__ import annotations

import json
from pathlib import Path

from xray_fluent import startup, traffic_history
from xray_fluent.traffic_history import TrafficHistoryStorage


def _use_temp_history(monkeypatch, tmp_path: Path) -> Path:
    target = tmp_path / "traffic_history.json"
    monkeypatch.setattr(traffic_history, "TRAFFIC_HISTORY_FILE", target)
    return target


def test_history_is_written_through_a_temporary_file(monkeypatch, tmp_path: Path) -> None:
    target = _use_temp_history(monkeypatch, tmp_path)
    replaced: list[tuple[str, str]] = []
    original_replace = traffic_history.os.replace

    def _record(src, dst):
        replaced.append((Path(src).name, Path(dst).name))
        original_replace(src, dst)

    monkeypatch.setattr(traffic_history.os, "replace", _record)

    storage = TrafficHistoryStorage()
    storage.start_session("node", "proxy")

    assert replaced == [(f".{target.name}.tmp", target.name)]
    assert json.loads(target.read_text(encoding="utf-8"))["sessions"]
    assert not list(tmp_path.glob(".*tmp"))


def test_corrupt_history_is_kept_aside_instead_of_being_overwritten(monkeypatch, tmp_path: Path) -> None:
    target = _use_temp_history(monkeypatch, tmp_path)
    target.write_text('{"sessions": [{"id": "half', encoding="utf-8")

    storage = TrafficHistoryStorage()
    storage.start_session("node", "proxy")

    backups = list(tmp_path.glob(f"{target.name}.corrupt-*"))
    assert len(backups) == 1
    assert backups[0].read_text(encoding="utf-8") == '{"sessions": [{"id": "half'
    assert json.loads(target.read_text(encoding="utf-8"))["sessions"]


def test_legacy_migration_runs_only_once_per_process(monkeypatch) -> None:
    monkeypatch.setattr(startup.sys, "platform", "win32")
    monkeypatch.setattr(startup, "_legacy_cleanup_done", False)
    shell_cleanups: list[int] = []
    task_deletes: list[list[str]] = []

    class _Winreg:
        HKEY_CURRENT_USER = 0
        KEY_READ = 0
        KEY_SET_VALUE = 0

        @staticmethod
        def OpenKey(*_args, **_kwargs):
            raise OSError("no such key")

    monkeypatch.setattr(startup, "winreg", _Winreg)
    monkeypatch.setattr(startup, "_delete_registry_startup", lambda _name: None)
    monkeypatch.setattr(startup, "_delete_startup_approved", lambda _name: None)
    monkeypatch.setattr(startup, "_delete_registry_tree", lambda _root, _path: None)
    monkeypatch.setattr(startup, "_cleanup_legacy_shell_entries", lambda: shell_cleanups.append(1))
    monkeypatch.setattr(
        startup.subprocess,
        "run",
        lambda command, **_kwargs: task_deletes.append(command),
    )

    startup.cleanup_legacy_system_entries()
    startup.cleanup_legacy_system_entries()
    startup.cleanup_legacy_system_entries()

    assert shell_cleanups == [1]
    assert len(task_deletes) == len(startup.LEGACY_APP_NAMES)


def test_legacy_task_deletion_cannot_hang_forever(monkeypatch) -> None:
    monkeypatch.setattr(startup.sys, "platform", "win32")
    monkeypatch.setattr(startup, "_legacy_cleanup_done", False)
    timeouts: list[object] = []

    class _Winreg:
        HKEY_CURRENT_USER = 0
        KEY_READ = 0
        KEY_SET_VALUE = 0

        @staticmethod
        def OpenKey(*_args, **_kwargs):
            raise OSError("no such key")

    monkeypatch.setattr(startup, "winreg", _Winreg)
    monkeypatch.setattr(startup, "_delete_registry_startup", lambda _name: None)
    monkeypatch.setattr(startup, "_delete_startup_approved", lambda _name: None)
    monkeypatch.setattr(startup, "_delete_registry_tree", lambda _root, _path: None)
    monkeypatch.setattr(startup, "_cleanup_legacy_shell_entries", lambda: None)
    monkeypatch.setattr(
        startup.subprocess,
        "run",
        lambda _command, **kwargs: timeouts.append(kwargs.get("timeout")),
    )

    startup.cleanup_legacy_system_entries()

    assert timeouts and all(value == startup.SCHTASKS_TIMEOUT for value in timeouts)


def test_disabled_run_entry_does_not_hide_an_active_elevated_task(monkeypatch) -> None:
    monkeypatch.setattr(startup.sys, "platform", "win32")
    monkeypatch.setattr(startup, "_legacy_cleanup_done", True)
    monkeypatch.setattr(startup, "_registry_startup_exists", lambda _name: True)
    monkeypatch.setattr(startup, "_startup_task_exists", lambda: True)

    class _Key:
        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return None

    class _Winreg:
        HKEY_CURRENT_USER = 0
        KEY_READ = 0

        @staticmethod
        def OpenKey(*_args, **_kwargs):
            return _Key()

        @staticmethod
        def QueryValueEx(*_args):
            return bytes([0x03]) + bytes(11), 0

    monkeypatch.setattr(startup, "winreg", _Winreg)

    assert startup.get_startup_state(startup.TASK_NAME) == startup.STARTUP_STATE_ENABLED
