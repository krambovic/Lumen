from copy import deepcopy
import json
import pytest
from xray_fluent.engines.singbox import manager as mod

ERROR = "start inbound/tun[tun-in]: configure tun interface: (create adapter: Cannot create a file when that file already exists. | open existing adapter: Element not found.)"

class FakeProcess:
    stdout = None
    pid = 12345
    def __init__(self, code=None): self.returncode = code
    def poll(self): return self.returncode
    def terminate(self): self.returncode = 0
    def kill(self): self.returncode = 0
    def wait(self, *args, **kwargs): return self.returncode

@pytest.fixture
def setup_manager(monkeypatch, tmp_path):
    exe = tmp_path / "sing-box.exe"
    exe.write_bytes(b"mock executable - never launched")
    config_path = tmp_path / "runtime.json"
    monkeypatch.setattr(mod, "RUNTIME_DIR", tmp_path)
    monkeypatch.setattr(mod, "SINGBOX_CONFIG_FILE", config_path)
    monkeypatch.setattr(mod, "run_text_pumped", lambda *a, **k: pytest.fail("No adapter/PowerShell mutation allowed"))
    monkeypatch.setattr(mod, "sleep_with_events", lambda *_: None)
    manager = mod.SingBoxManager()
    monkeypatch.setattr(manager, "_read_output", lambda proc: None)
    monkeypatch.setattr(manager, "_wait_clash_api_port_released", lambda *a, **k: None)
    return manager, exe, config_path

def config(alias="LumenTun"):
    return {"inbounds": [{"type": "tun", "tag": "tun-in", "interface_name": alias}], "outbounds": [{"type": "direct"}]}

def test_adapter_collision_uses_a_new_alias_and_keeps_user_profile(setup_manager, monkeypatch):
    manager, exe, path = setup_manager
    original = config()
    before = deepcopy(original)
    attempts = []
    def spawn(*args, **kwargs):
        attempts.append(json.loads(path.read_text(encoding="utf-8")))
        return FakeProcess(1 if len(attempts) == 1 else None)
    monkeypatch.setattr(mod.subprocess, "Popen", spawn)
    def ready(proc, alias):
        if proc.poll() is not None:
            manager._last_output_lines.append(ERROR)
            return False
        return True
    monkeypatch.setattr(manager, "_wait_until_tun_ready", ready)
    assert manager.start(str(exe), original, prevalidated=True)
    assert len(attempts) == 2
    old, new = [entry["inbounds"][0]["interface_name"] for entry in attempts]
    assert old != new and new.startswith(mod.SINGBOX_TUN_INTERFACE_NAME + "-")
    assert manager._tun_interface_name == new
    assert original == before
    assert manager.stop(fast=True)

def test_persistent_adapter_collision_has_a_finite_retry_budget(setup_manager, monkeypatch):
    manager, exe, path = setup_manager
    aliases = []
    def spawn(*args, **kwargs):
        aliases.append(json.loads(path.read_text(encoding="utf-8"))["inbounds"][0]["interface_name"])
        return FakeProcess(1)
    monkeypatch.setattr(mod.subprocess, "Popen", spawn)
    def ready(proc, alias):
        manager._last_output_lines.append(ERROR)
        return False
    monkeypatch.setattr(manager, "_wait_until_tun_ready", ready)
    assert not manager.start(str(exe), config(), prevalidated=True)
    assert len(aliases) == 3
    assert len(set(aliases)) == 3

def test_unknown_adapters_are_never_purged_by_alias_or_wintun_prefix(setup_manager):
    manager, _, _ = setup_manager
    manager.cleanup_orphaned_tun_adapters(interface_name="other-vpn")
    manager._purge_stale_wintun_devices(interface_name="tun0")

def test_missing_tun_alias_is_written_consistently(setup_manager, monkeypatch):
    manager, exe, path = setup_manager
    payload = config()
    payload["inbounds"][0].pop("interface_name")
    monkeypatch.setattr(mod.subprocess, "Popen", lambda *a, **k: FakeProcess())
    seen = []
    monkeypatch.setattr(manager, "_wait_until_tun_ready", lambda proc, alias: seen.append(alias) or True)
    assert manager.start(str(exe), payload, prevalidated=True)
    assert json.loads(path.read_text(encoding="utf-8"))["inbounds"][0]["interface_name"] == seen[0]
    assert "interface_name" not in payload["inbounds"][0]
    assert manager.stop(fast=True)
