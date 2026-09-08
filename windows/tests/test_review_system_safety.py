from copy import deepcopy
import json
from types import SimpleNamespace as NS
import threading
import pytest
from PyQt6 import sip
from PyQt6.QtCore import QThread
from xray_fluent.app_controller import AppController
from xray_fluent.models import AppSettings
from xray_fluent.proxy_manager import ProxyManager, FirefoxProxyManager
from xray_fluent import proxy_manager as proxy_module, qthread_utils


def proxy_fixture(monkeypatch, tmp_path):
    manager = ProxyManager()
    manager._backup_file = tmp_path / "proxy.json"
    original = {"ProxyEnable": 0, "ProxyServer": "127.0.0.1:8888", "ProxyOverride": "company.invalid",
                "AutoConfigURL": "https://pac.invalid/proxy.pac"}
    current, flags, native, writes = deepcopy(original), [13], [], []
    monkeypatch.setattr(proxy_module, "_process_creation_time", lambda pid: 12345)
    monkeypatch.setattr(manager, "_read_settings", lambda: dict(current))
    monkeypatch.setattr(manager, "_query_connection_flags", lambda: flags[0])
    def write(values):
        writes.append(dict(values))
        current.update({key: values[key] for key in original if key in values})
    def apply(connection, server, bypass, enabled, **kwargs):
        native.append((connection, server, bypass, enabled, kwargs))
        flags[0] = kwargs.get("flags", 3 if enabled else 1)
        return True
    monkeypatch.setattr(manager, "_write_settings", write)
    monkeypatch.setattr(manager, "_set_connection_proxy", apply)
    monkeypatch.setattr(manager, "_refresh_system_proxy", lambda: None)
    monkeypatch.setattr(manager, "_enumerate_ras_entries", lambda: pytest.fail("RAS must remain untouched"))
    manager._firefox_proxy = NS(disable=lambda: None)
    return manager, original, current, flags, native, writes


def test_proxy_restores_static_pac_and_wpad_not_just_disabled(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    manager.enable(10809, 10808)
    saved = json.loads(manager._backup_file.read_text())
    assert saved["original"]["ProxyServer"] == "127.0.0.1:8888"
    assert saved["original"]["WinInetFlags"] == 13
    assert manager.disable() is True
    assert current == original and flags[0] == 13
    assert native[-1][4] == {"flags": 13, "auto_config_url": original["AutoConfigURL"]}
    assert not manager._backup_file.exists()


def test_external_proxy_change_is_not_overwritten(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    manager.enable(10809, 10808)
    current["ProxyServer"] = "external.invalid:5000"
    before = deepcopy(current)
    native.clear()
    writes.clear()
    assert manager.disable() is False
    assert current == before and not native and not writes
    assert manager._backup_file.exists()


def test_legacy_proxy_backup_does_not_grant_ownership(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    manager._backup_file.write_text(json.dumps(original))
    assert manager.reconcile_stale_state() is False
    with pytest.raises(RuntimeError):
        manager.enable(10809, 10808)
    assert not native and not writes and current == original
    assert json.loads(manager._backup_file.read_text()) == original


def test_live_other_instance_is_not_recovered(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    manager.enable(10809, 10808)
    receipt = json.loads(manager._backup_file.read_text())
    receipt["owner"] = [987654321, 777]
    manager._backup_file.write_text(json.dumps(receipt))
    manager._backup = manager._applied = manager._owner = None
    monkeypatch.setattr(proxy_module, "_process_creation_time", lambda pid: 777)
    writes.clear()
    native.clear()
    assert manager.reconcile_stale_state() is False
    assert not native and not writes


def test_proxy_backup_failure_prevents_every_system_write(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    def fail(values):
        raise OSError("disk full")
    monkeypatch.setattr(manager, "_persist_backup", fail)
    with pytest.raises(OSError):
        manager.enable(10809, 10808)
    assert current == original and not writes and not native
    assert manager._backup is manager._applied is manager._owner is None


def test_proxy_without_complete_wininet_snapshot_is_not_enabled(monkeypatch, tmp_path):
    manager, original, current, flags, native, writes = proxy_fixture(monkeypatch, tmp_path)
    monkeypatch.setattr(manager, "_query_connection_flags", lambda: None)
    with pytest.raises(RuntimeError):
        manager.enable(10809, 10808)
    assert not writes and not native


def firefox_fixture(monkeypatch, tmp_path):
    manager = FirefoxProxyManager()
    manager._backup_file = tmp_path / "firefox-backup.json"
    profile = tmp_path / "profile"
    profile.mkdir()
    for name in ("user.js", "prefs.js"):
        (profile / name).write_text('user_pref("browser.theme", "old");\nuser_pref("network.proxy.type", 5);\n')
    monkeypatch.setattr(manager, "_find_profiles", lambda: [profile])
    manager.enable(http_port=10809, socks_port=10808)
    return manager, profile


def test_firefox_restore_preserves_new_unrelated_preferences(monkeypatch, tmp_path):
    manager, profile = firefox_fixture(monkeypatch, tmp_path)
    for name in ("user.js", "prefs.js"):
        path = profile / name
        path.write_text(path.read_text().replace('"old"', '"new"') + 'user_pref("browser.new", true);\n')
    manager.disable()
    for name in ("user.js", "prefs.js"):
        text = (profile / name).read_text()
        assert '"new"' in text and '"browser.new", true' in text
        assert '"network.proxy.type", 5' in text and '"network.proxy.type", 1' not in text
        assert "Lumen system proxy" not in text
    assert not manager._backup_file.exists()


def test_firefox_does_not_replace_an_external_proxy_choice(monkeypatch, tmp_path):
    manager, profile = firefox_fixture(monkeypatch, tmp_path)
    path = profile / "prefs.js"
    changed = path.read_text().replace('"network.proxy.type", 1', '"network.proxy.type", 2')
    path.write_text(changed)
    manager.disable()
    assert path.read_text() == changed


def test_deleted_thread_wrapper_is_already_stopped():
    worker = QThread()
    sip.delete(worker)
    assert qthread_utils.stop_and_wait_for_thread(worker, timeout=0)
    assert not qthread_utils.is_thread_pending(worker)


def preferences_controller():
    jobs, statuses, results = [], [], []
    c = NS(_shutting_down=False, state=NS(settings=AppSettings()),
           _windows_pref_lock=threading.Lock(), _startup_io_lock=threading.Lock(),
           _windows_pref_pending=None, _windows_pref_running=False,
           _start_background_task=lambda target, name: jobs.append(target),
           status=NS(emit=lambda *args: statuses.append(args)),
           _admin_setting_applied=NS(emit=lambda *args: results.append(args)))
    c._drain_windows_preferences = lambda: AppController._drain_windows_preferences(c)
    return c, jobs, statuses, results


def test_windows_preferences_are_coalesced_off_the_caller(monkeypatch):
    import xray_fluent.app_controller as module
    applied = []
    monkeypatch.setattr(module, "set_startup_enabled", lambda name, launch, command: applied.append((launch, command)))
    monkeypatch.setattr(module, "build_startup_command", lambda **kw: str(kw["in_tray"]))
    monkeypatch.setattr(module, "set_always_run_as_admin", lambda value: applied.append(value))
    monkeypatch.setattr(module, "is_process_elevated", lambda: False)
    c, jobs, statuses, results = preferences_controller()
    c.state.settings.launch_on_startup = True
    AppController._queue_windows_preferences(c, startup=True, admin=False)
    c.state.settings.always_run_as_admin = True
    c.state.settings.launch_in_tray_on_startup = False
    AppController._queue_windows_preferences(c, startup=False, admin=True)
    assert len(jobs) == 1 and not applied
    jobs[0]()
    assert applied == [(True, "False"), True]
    assert results == [(True, False, "")] and not c._windows_pref_running


def test_a_new_preference_wins_over_in_flight_startup_io(monkeypatch):
    import xray_fluent.app_controller as module
    c, jobs, statuses, results = preferences_controller()
    applied = []
    def setter(name, enabled, command):
        applied.append(enabled)
        if enabled:
            c.state.settings.launch_on_startup = False
            AppController._queue_windows_preferences(c, startup=True, admin=False)
    monkeypatch.setattr(module, "set_startup_enabled", setter)
    monkeypatch.setattr(module, "build_startup_command", lambda **kw: "command")
    c.state.settings.launch_on_startup = True
    AppController._queue_windows_preferences(c, startup=True, admin=False)
    jobs[0]()
    assert applied == [True, False] and len(jobs) == 1


def test_periodic_startup_observer_cannot_disable_a_new_registration(monkeypatch):
    import xray_fluent.app_controller as module
    c, jobs, statuses, results = preferences_controller()
    c._startup_settings_generation = 1
    c.state.settings.launch_on_startup = False
    c._logger = NS(warning=lambda *args: None)
    monkeypatch.setattr(module, "get_startup_state", lambda name: module.STARTUP_STATE_ENABLED)
    monkeypatch.setattr(module, "set_startup_enabled", lambda *args: pytest.fail("observer wrote to Windows"))
    assert AppController._sync_startup_state_from_windows(c) is False


def test_subscription_preparation_failure_settles_batch_and_is_scrubbed(monkeypatch):
    import xray_fluent.subscription_worker as module
    monkeypatch.setattr(module, "fetch_subscription_payload_result", lambda *args, **kw: NS(
        text="not-a-real-subscription", userinfo={}, errors=[], headers={}, status=200, not_modified=False))
    def fail(*args):
        raise ValueError("password=sentinel-secret")
    monkeypatch.setattr(module, "prepare_subscription_payload", fail)
    worker = module.SubscriptionFetchWorker()
    fetched, completed = [], []
    worker.fetched.connect(lambda *args: fetched.append(args))
    worker.completed.connect(lambda *args: completed.append(args))
    worker.run_batch([module.SubscriptionJob("https://example.invalid/", "update")], 11)
    assert completed == [(11, 1)] and fetched[0][2] == ""
    assert "sentinel-secret" not in str(fetched[0][4])
    assert fetched[0][5]["status"] == 0


def test_snapshot_failure_reaches_ui_without_secret(monkeypatch):
    import xray_fluent.app_controller as module
    def fail(state):
        raise ValueError("password=sentinel-secret")
    monkeypatch.setattr(module, "snapshot_state", fail)
    messages = []
    c = NS(profile_loaded=True, _save_executor_shutdown=False, thread=QThread.currentThread,
           state=object(), _logger=NS(error=lambda *args: None), _save_failed=NS(emit=messages.append))
    assert AppController._enqueue_state_save(c) is None
    assert len(messages) == 1 and "sentinel-secret" not in messages[0]


def test_path_without_process_ownership_never_authorizes_a_kill(monkeypatch, tmp_path):
    from xray_fluent import subprocess_utils
    monkeypatch.setattr(subprocess_utils, "run_text_pumped", lambda *args, **kw: pytest.fail("unowned process kill"))
    assert not subprocess_utils.kill_processes_by_path("xray.exe", tmp_path / "xray.exe")


def test_packaging_refuses_personal_data_without_deleting_it(monkeypatch, tmp_path):
    import build_qml
    app = tmp_path / "app"
    data = app / "data"
    data.mkdir(parents=True)
    personal = data / "state.json"
    personal.write_text("private-sentinel")
    with pytest.raises(RuntimeError, match="Personal or stale"):
        build_qml._validate_release_data_tree(app)
    assert personal.read_text() == "private-sentinel"


def test_packaging_accepts_only_matching_shipped_templates(monkeypatch, tmp_path):
    import build_qml
    source = tmp_path / "templates"
    source.mkdir()
    (source / "template.json").write_text("{}")
    app = tmp_path / "app"
    target = app / "data" / "templates"
    target.mkdir(parents=True)
    (target / "template.json").write_text("{}")
    monkeypatch.setattr(build_qml, "DATA_TEMPLATES_DIR", source)
    build_qml._validate_release_data_tree(app)
    (target / "template.json").write_text("changed")
    with pytest.raises(RuntimeError, match="modified"):
        build_qml._validate_release_data_tree(app)


def test_startup_failure_file_redacts_credentials(monkeypatch, tmp_path):
    import run_qml
    target = tmp_path / "startup.log"
    monkeypatch.setattr(run_qml, "_startup_log_path", lambda: target)
    run_qml._report_startup_failure(ValueError("password=private-sentinel"), show_dialog=False)
    text = target.read_text(encoding="utf-8")
    assert "ValueError" in text and "private-sentinel" not in text


def test_queued_metrics_with_deleted_or_retired_sender_are_ignored(monkeypatch):
    import xray_fluent.app_controller as module
    received = []
    monkeypatch.setattr(module, "on_live_metrics_operation", lambda *args: received.append(args))
    active, retired = object(), object()
    c = NS(_shutting_down=False, _metrics_worker=active, sender=lambda: retired)
    AppController._on_live_metrics(c, {})
    c.sender = lambda: None
    AppController._on_live_metrics(c, {})
    c.sender = lambda: active
    c._shutting_down = True
    AppController._on_live_metrics(c, {})
    assert received == []
    c._shutting_down = False
    AppController._on_live_metrics(c, {})
    assert len(received) == 1


def test_same_profile_reconfiguration_invalidates_health_generation():
    from xray_fluent.live_metrics_worker import LiveMetricsWorker
    worker = LiveMetricsWorker("", 0, active_profile_id="same")
    try:
        generation = worker._health_generation
        worker.set_active_profile("same")
        assert worker._health_generation == generation + 1
    finally:
        worker.stop()
