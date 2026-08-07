from __future__ import annotations

import ctypes
import ctypes.wintypes
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

from xray_fluent import (
    diagnostics_uploader,
    discord_proxy_manager,
    win_proc_monitor,
    zapret_manager,
)
from xray_fluent.constants import DIAGNOSTICS_SECRET
from xray_fluent.diagnostics import _sanitize_state
from xray_fluent.discord_proxy_manager import DiscordInstall
from xray_fluent.proxy_manager import ProxyManager


class _NoopFirefoxProxy:
    def enable(self, **_kwargs) -> None:
        return None

    def disable(self) -> None:
        return None


def _proxy_manager(tmp_path: Path, monkeypatch, registry: dict) -> tuple[ProxyManager, list[dict]]:
    manager = ProxyManager()
    manager._backup_file = tmp_path / "system_proxy_backup.json"
    manager._firefox_proxy = _NoopFirefoxProxy()
    written: list[dict] = []
    monkeypatch.setattr(manager, "_read_settings", lambda: dict(registry))
    monkeypatch.setattr(manager, "_write_settings", lambda values: written.append(dict(values)))
    monkeypatch.setattr(manager, "_set_wininet_connection_proxy", lambda *_args: True)
    return manager, written


# ── proxy_manager: crash-recovery backup ────────────────────────


def test_enable_keeps_the_persisted_backup_after_an_abnormal_exit(tmp_path, monkeypatch) -> None:
    stale_registry = {
        "ProxyEnable": 1,
        "ProxyServer": "127.0.0.1:10808",
        "ProxyOverride": "",
        "AutoConfigURL": "",
    }
    manager, _written = _proxy_manager(tmp_path, monkeypatch, stale_registry)
    original = {
        "ProxyEnable": 0,
        "ProxyServer": "",
        "ProxyOverride": "",
        "AutoConfigURL": "http://corp/proxy.pac",
    }
    manager._backup_file.write_text(json.dumps(original), encoding="utf-8")

    manager.enable(10809, 10808)

    assert manager._backup == original
    assert json.loads(manager._backup_file.read_text(encoding="utf-8")) == original


def test_enable_never_snapshots_lumens_own_proxy_endpoint(tmp_path, monkeypatch) -> None:
    stale_registry = {
        "ProxyEnable": 1,
        "ProxyServer": "http=127.0.0.1:10808;https=127.0.0.1:10808",
        "ProxyOverride": "<local>",
        "AutoConfigURL": "",
    }
    manager, _written = _proxy_manager(tmp_path, monkeypatch, stale_registry)

    manager.enable(10809, 10808)

    assert manager._backup["ProxyEnable"] == 0
    assert manager._backup["ProxyServer"] == ""


def test_enable_snapshots_a_foreign_proxy_unchanged(tmp_path, monkeypatch) -> None:
    registry = {
        "ProxyEnable": 1,
        "ProxyServer": "proxy.corp.local:3128",
        "ProxyOverride": "<local>",
        "AutoConfigURL": "",
    }
    manager, _written = _proxy_manager(tmp_path, monkeypatch, registry)

    manager.enable(10809, 10808)

    assert manager._backup == registry


def test_reconcile_stale_state_restores_the_persisted_backup(tmp_path, monkeypatch) -> None:
    stale_registry = {
        "ProxyEnable": 1,
        "ProxyServer": "127.0.0.1:10808",
        "ProxyOverride": "",
        "AutoConfigURL": "",
    }
    manager, written = _proxy_manager(tmp_path, monkeypatch, stale_registry)
    original = {
        "ProxyEnable": 0,
        "ProxyServer": "",
        "ProxyOverride": "",
        "AutoConfigURL": "http://corp/proxy.pac",
    }
    manager._backup_file.write_text(json.dumps(original), encoding="utf-8")

    assert manager.reconcile_stale_state() is True
    assert written == [original]
    assert not manager._backup_file.exists()


def test_reconcile_stale_state_is_a_no_op_for_untouched_settings(tmp_path, monkeypatch) -> None:
    registry = {
        "ProxyEnable": 1,
        "ProxyServer": "proxy.corp.local:3128",
        "ProxyOverride": "",
        "AutoConfigURL": "",
    }
    manager, written = _proxy_manager(tmp_path, monkeypatch, registry)

    assert manager.reconcile_stale_state() is False
    assert written == []


# ── diagnostics: exported state ─────────────────────────────────


def test_sanitize_state_drops_the_security_block_and_hashed_secrets() -> None:
    safe = _sanitize_state(
        {
            "security": {"password_hash": "aGFzaA==", "salt": "c2FsdA==", "enabled": True},
            "settings": {"subscription_token": "abc", "proxy_allow_lan": False},
            "nodes": [{"name": "n1", "uuid": "u", "publicKey": "pk"}],
        }
    )

    assert "security" not in safe
    assert safe["settings"]["subscription_token"] == "***"
    assert safe["settings"]["proxy_allow_lan"] is False
    assert safe["nodes"] == [{"name": "n1"}]
    assert "aGFzaA==" not in json.dumps(safe)
    assert "c2FsdA==" not in json.dumps(safe)


def test_diagnostics_ingest_is_not_falsely_authenticated() -> None:
    # A secret shipped inside every client authenticates nothing, so neither the
    # constant nor the signing helper may come back.
    assert DIAGNOSTICS_SECRET == ""
    assert not hasattr(diagnostics_uploader, "_sign_headers")
    source = Path(diagnostics_uploader.__file__).read_text(encoding="utf-8")
    assert "X-Diag-Signature" not in source


# ── discord: privilege boundary and payload integrity ───────────


def _discord_install(tmp_path: Path) -> DiscordInstall:
    root = tmp_path / "Discord"
    app_dir = root / "app-1.0.0"
    app_dir.mkdir(parents=True)
    return DiscordInstall("stable", root, app_dir, app_dir / "Discord.exe", "Discord.exe")


def test_discord_is_relaunched_through_the_shell_not_with_the_admin_token(tmp_path, monkeypatch) -> None:
    launched: dict[str, object] = {}
    monkeypatch.setattr(
        discord_proxy_manager,
        "subprocess",
        SimpleNamespace(
            DEVNULL=-3,
            Popen=lambda command, **kwargs: launched.update(command=command, kwargs=kwargs),
        ),
    )

    discord_proxy_manager._launch_discord(_discord_install(tmp_path))

    assert launched["command"][0] == "explorer.exe"
    assert launched["command"][1].endswith("Discord.exe")


def test_discord_wait_loops_pump_the_qt_event_loop(tmp_path, monkeypatch) -> None:
    slept: list[float] = []
    monkeypatch.setattr(discord_proxy_manager, "sleep_with_events", slept.append)
    monkeypatch.setattr(discord_proxy_manager, "_process_pids_for_install", lambda _install: [1])

    assert discord_proxy_manager._wait_for_discord_exit(_discord_install(tmp_path), timeout_sec=0.1) is False
    assert slept


def _write_droute_bundle(directory: Path, *, version: str = "2.0.0", extra: dict[str, bytes] | None = None) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    payload = b"MZ" + b"\0" * 2048
    (directory / "droute.exe").write_bytes(payload)
    (directory / "version.txt").write_text(version + "\n", encoding="utf-8")
    for name, data in (extra or {}).items():
        (directory / name).write_bytes(data)
    (directory / "SHA256SUMS.txt").write_text(
        f"{hashlib.sha256(payload).hexdigest()}  droute.exe\n", encoding="utf-8"
    )


def test_empty_droute_checksum_list_is_rejected(tmp_path) -> None:
    bundle = tmp_path / "bundle"
    _write_droute_bundle(bundle)
    (bundle / "SHA256SUMS.txt").write_text("", encoding="utf-8")

    assert discord_proxy_manager.get_bundled_droute_version(bundle) == ""


def test_install_skips_bundle_files_missing_from_the_checksum_list(tmp_path) -> None:
    bundle = tmp_path / "bundle"
    target = tmp_path / "staged"
    _write_droute_bundle(bundle, extra={"evil.dll": b"payload"})

    assert discord_proxy_manager.install_bundled_droute(target, bundle_dir=bundle) == "2.0.0"
    assert (target / "droute.exe").is_file()
    assert (target / "version.txt").is_file()
    assert not (target / "evil.dll").exists()


def test_ensure_droute_bundle_executes_the_copy_in_the_install_tree(tmp_path, monkeypatch) -> None:
    bundle = tmp_path / "bundle"
    staged = tmp_path / "staged"
    _write_droute_bundle(bundle)
    staged.mkdir()
    (staged / "droute.exe").write_bytes(b"tampered" * 512)
    (staged / "version.txt").write_text("99.0.0\n", encoding="utf-8")

    monkeypatch.setattr(discord_proxy_manager, "DROUTE_BUNDLED_DIR", bundle)
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_BUNDLED_EXE", bundle / "droute.exe")
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_DIR", staged)
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_EXE", staged / "droute.exe")
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_VERSION_FILE", staged / "version.txt")
    monkeypatch.setattr(discord_proxy_manager, "DROUTE_NOTICE", staged / "README.droute.txt")

    assert discord_proxy_manager.ensure_droute_bundle() == bundle / "droute.exe"


# ── zapret: processes and driver services Lumen owns ────────────


def test_kill_orphaned_does_not_taskkill_by_image_name(monkeypatch) -> None:
    commands: list[list[str]] = []
    monkeypatch.setattr(zapret_manager, "kill_processes_by_path", lambda *_args, **_kwargs: False)
    monkeypatch.setattr(
        zapret_manager,
        "run_text_pumped",
        lambda command, **_kwargs: commands.append(command),
    )

    assert zapret_manager.ZapretManager._kill_orphaned(timeout=0.1, settle_delay=0) == []
    assert commands == []


class _FakeRegistryKey:
    def __init__(self, image_path: str | None = None) -> None:
        self.image_path = image_path

    def __enter__(self) -> "_FakeRegistryKey":
        return self

    def __exit__(self, *_args: object) -> bool:
        return False


class _FakeWinreg:
    HKEY_LOCAL_MACHINE = 0

    def __init__(self, services: dict[str, str]) -> None:
        self.services = services

    def OpenKey(self, root, sub_key, *_args):
        if isinstance(root, int):
            return _FakeRegistryKey()
        return _FakeRegistryKey(self.services[sub_key])

    def EnumKey(self, _key, index: int) -> str:
        names = list(self.services)
        if index >= len(names):
            raise OSError(259, "no more data")
        return names[index]

    def QueryValueEx(self, key: _FakeRegistryKey, _name: str):
        if key.image_path is None:
            raise OSError(2, "not found")
        return key.image_path, 1


def test_windivert_cleanup_only_targets_lumens_own_driver_service(tmp_path, monkeypatch) -> None:
    zapret_root = tmp_path / "zapret"
    (zapret_root / "exe").mkdir(parents=True)
    monkeypatch.setattr(zapret_manager, "ZAPRET_DIR", zapret_root)
    monkeypatch.setattr(
        zapret_manager,
        "winreg",
        _FakeWinreg(
            {
                "WinDivert14": r"\??\C:\GoodbyeDPI\x86_64\WinDivert64.sys",
                "LumenMonkey": rf"\??\{zapret_root / 'exe' / 'Monkey64.sys'}",
                "Spooler": r"C:\Windows\System32\spoolsv.exe",
            }
        ),
    )
    zapret_manager.ZapretManager._windivert_services_cache = None

    try:
        assert zapret_manager.ZapretManager._find_windivert_services() == ["LumenMonkey"]
    finally:
        zapret_manager.ZapretManager._windivert_services_cache = None


# ── zapret: user presets survive the portable updater ───────────


def _isolate_presets(tmp_path: Path, monkeypatch) -> tuple[Path, Path]:
    builtin = tmp_path / "install" / "zapret" / "presets"
    user = tmp_path / "data" / "zapret" / "presets"
    builtin.mkdir(parents=True)
    monkeypatch.setattr(zapret_manager, "PRESETS_DIR", builtin)
    monkeypatch.setattr(zapret_manager, "USER_PRESETS_DIR", user)
    zapret_manager.ZapretManager.invalidate_preset_cache()
    return builtin, user


def test_saved_presets_are_written_outside_the_install_tree(tmp_path, monkeypatch) -> None:
    builtin, user = _isolate_presets(tmp_path, monkeypatch)

    info = zapret_manager.ZapretManager.save_preset("MyISP", "--filter-tcp=443\n", "tuned")

    assert info.file_path == user / "MyISP.txt"
    assert not (builtin / "MyISP.txt").exists()
    assert zapret_manager.ZapretManager.list_presets() == ["MyISP"]


def test_builtin_and_user_presets_are_merged_with_the_user_copy_winning(tmp_path, monkeypatch) -> None:
    builtin, user = _isolate_presets(tmp_path, monkeypatch)
    (builtin / "Default.txt").write_text("--builtin\n", encoding="utf-8")
    (builtin / "Gaming.txt").write_text("--gaming\n", encoding="utf-8")

    zapret_manager.ZapretManager.save_preset("Default", "--edited\n")

    assert zapret_manager.ZapretManager.list_presets() == ["Default", "Gaming"]
    assert zapret_manager.ZapretManager.preset_path("Default") == user / "Default.txt"
    assert "--edited" in zapret_manager.ZapretManager.read_preset("Default")
    assert (builtin / "Default.txt").read_text(encoding="utf-8") == "--builtin\n"


def test_imported_presets_land_in_the_user_directory(tmp_path, monkeypatch) -> None:
    builtin, user = _isolate_presets(tmp_path, monkeypatch)
    (builtin / "Shared.txt").write_text("--builtin\n", encoding="utf-8")
    source = tmp_path / "Shared.txt"
    source.write_text("--imported\n", encoding="utf-8")

    info = zapret_manager.ZapretManager.import_preset(source)

    assert info is not None
    assert info.file_path == user / "Shared (1).txt"


def test_delete_preset_removes_both_copies(tmp_path, monkeypatch) -> None:
    builtin, user = _isolate_presets(tmp_path, monkeypatch)
    (builtin / "Default.txt").write_text("--builtin\n", encoding="utf-8")
    zapret_manager.ZapretManager.save_preset("Default", "--edited\n")

    assert zapret_manager.ZapretManager.delete_preset("Default") is True
    assert zapret_manager.ZapretManager.list_presets() == []
    assert not (builtin / "Default.txt").exists()
    assert not (user / "Default.txt").exists()


# ── win_proc_monitor: recycled PIDs and connection tuples ───────


class _FakeKernel32:
    def __init__(self, state: dict) -> None:
        self.state = state
        self.name_queries = 0

    def OpenProcess(self, _access, _inherit, pid):
        return pid if self.state.get("alive", True) else 0

    def QueryFullProcessImageNameW(self, _handle, _flags, buffer, _size):
        self.name_queries += 1
        buffer.value = self.state["exe"]
        return 1

    def CloseHandle(self, _handle) -> int:
        return 1


def test_pid_cache_is_keyed_on_the_process_creation_time(monkeypatch) -> None:
    state = {"created": 111, "exe": r"C:\chrome\chrome.exe"}
    kernel = _FakeKernel32(state)
    monkeypatch.setattr(win_proc_monitor, "_kernel32", kernel)
    monkeypatch.setattr(win_proc_monitor, "_process_creation_time", lambda _handle: state["created"])
    win_proc_monitor.clear_pid_cache()

    assert win_proc_monitor._pid_to_exe(4820) == "chrome.exe"
    assert win_proc_monitor._pid_to_exe(4820) == "chrome.exe"
    assert kernel.name_queries == 1

    state.update(created=222, exe=r"C:\steam\steam.exe")
    assert win_proc_monitor._pid_to_exe(4820) == "steam.exe"
    assert kernel.name_queries == 2

    win_proc_monitor.clear_pid_cache()


def _tcp_row(local_port: int, pid: int) -> win_proc_monitor._MIB_TCPROW_OWNER_PID:
    return win_proc_monitor._MIB_TCPROW_OWNER_PID(
        dwState=5,
        dwLocalAddr=0x0100007F,
        dwLocalPort=win_proc_monitor._ntohs(local_port),
        dwRemoteAddr=0x0100007F,
        dwRemotePort=win_proc_monitor._ntohs(10808),
        dwOwningPid=pid,
    )


class _FakeIphlpapi:
    def __init__(self) -> None:
        self.rows: list = []
        self.estats_enabled: list = []

    def _blob(self):
        class _Table(ctypes.Structure):
            _fields_ = [
                ("dwNumEntries", ctypes.wintypes.DWORD),
                ("table", win_proc_monitor._MIB_TCPROW_OWNER_PID * max(1, len(self.rows))),
            ]

        table = _Table()
        table.dwNumEntries = len(self.rows)
        for index, row in enumerate(self.rows):
            table.table[index] = row
        return table

    def GetExtendedTcpTable(self, buffer, size_ref, _order, _af, _class, _reserved):
        table = self._blob()
        if buffer is None:
            size_ref._obj.value = ctypes.sizeof(table)
            return 122
        ctypes.memmove(buffer, ctypes.byref(table), ctypes.sizeof(table))
        return 0

    def SetPerTcpConnectionEStats(self, row_ref, *_args):
        row = row_ref._obj
        self.estats_enabled.append((row.dwLocalAddr, row.dwLocalPort, row.dwRemoteAddr, row.dwRemotePort))
        return 0

    def GetPerTcpConnectionEStats(self, *_args):
        return 1


def test_estats_tracking_is_rebuilt_from_the_live_tcp_table(monkeypatch) -> None:
    fake = _FakeIphlpapi()
    monkeypatch.setattr(win_proc_monitor, "_iphlpapi", fake)
    monkeypatch.setattr(win_proc_monitor, "_pid_to_exe", lambda _pid: "steam.exe")
    win_proc_monitor.clear_pid_cache()

    fake.rows = [_tcp_row(50000, 4820)]
    win_proc_monitor.get_proxy_connections()
    assert len(fake.estats_enabled) == 1

    fake.rows = [_tcp_row(50001, 4821)]
    win_proc_monitor.get_proxy_connections()
    assert len(fake.estats_enabled) == 2
    assert len(win_proc_monitor._estats_enabled) == 1

    # 50000 was closed and recycled — eStats must be enabled for it again.
    fake.rows = [_tcp_row(50000, 4822)]
    win_proc_monitor.get_proxy_connections()
    assert len(fake.estats_enabled) == 3

    win_proc_monitor.clear_pid_cache()
