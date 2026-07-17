from __future__ import annotations

import hashlib
import io
import zipfile
from pathlib import Path
from types import SimpleNamespace

import pytest

from xray_fluent import core_resource_updater
from xray_fluent.app_controller import AppController
from xray_fluent.application import update_service
from xray_fluent.core_resource_updater import (
    ResourceUpdateResult,
    ResourceUpdateWorker,
    _atomic_replace_files,
    regional_geodata_installed,
)
from xray_fluent.engines.singbox import manager as singbox_manager
from xray_fluent.engines.xray import core_updater as xray_core_updater
from xray_fluent.engines.xray.core_updater import (
    UpdateCancelled,
    XrayCoreRelease,
    XrayCoreUpdateResult,
    XrayCoreUpdateWorker,
)
from xray_fluent.qml_app.bridge.app_bridge import AppBridge


def test_atomic_replace_rolls_every_file_back_when_validation_fails(tmp_path) -> None:
    first = tmp_path / "first.dat"
    second = tmp_path / "second.dat"
    first.write_bytes(b"old-first")
    second.write_bytes(b"old-second")
    new_first = tmp_path / "new-first.dat"
    new_second = tmp_path / "new-second.dat"
    new_first.write_bytes(b"new-first")
    new_second.write_bytes(b"new-second")

    def reject_install() -> None:
        raise RuntimeError("invalid install")

    with pytest.raises(RuntimeError, match="invalid install"):
        _atomic_replace_files(
            [(new_first, first), (new_second, second)],
            validator=reject_install,
        )

    assert first.read_bytes() == b"old-first"
    assert second.read_bytes() == b"old-second"
    assert not list(tmp_path.glob("*.rollback"))
    assert not list(tmp_path.glob("*.new"))


def test_atomic_replace_writes_backup_only_after_success(tmp_path) -> None:
    target = tmp_path / "sing-box.exe"
    source = tmp_path / "downloaded.exe"
    target.write_bytes(b"old")
    source.write_bytes(b"new")

    _atomic_replace_files([(source, target)], backup_targets={target})

    assert target.read_bytes() == b"new"
    assert target.with_suffix(".exe.bak").read_bytes() == b"old"


def test_lumen_singbox_build_is_not_replaced_by_unpatched_upstream(
    monkeypatch, tmp_path
) -> None:
    exe = tmp_path / "sing-box.exe"
    exe.write_bytes(b"lumen-sing-box")
    monkeypatch.setattr(
        core_resource_updater,
        "resolve_configured_path",
        lambda *_args, **_kwargs: exe,
    )
    monkeypatch.setattr(
        core_resource_updater,
        "get_singbox_version",
        lambda _path: "sing-box version 1.13.14-extended-2.5.1-lumen.1",
    )
    monkeypatch.setattr(
        core_resource_updater,
        "_request_json",
        lambda *_args, **_kwargs: {
            "tag_name": "v1.13.15-extended-2.6.0",
            "assets": [],
        },
    )
    monkeypatch.setattr(
        core_resource_updater,
        "_pick_singbox_asset",
        lambda _release: ("sing-box-windows-amd64.zip", "https://example.invalid/core.zip"),
    )
    monkeypatch.setattr(
        core_resource_updater,
        "_download_file",
        lambda *_args, **_kwargs: pytest.fail("unpatched upstream core must not be downloaded"),
    )

    result = core_resource_updater.check_or_update_singbox(str(exe), True)

    assert result.status == "available"
    assert result.current_version == "1.13.14-extended-2.5.1"
    assert result.latest_version == "1.13.15-extended-2.6.0"


@pytest.mark.parametrize(
    "worker",
    [
        lambda: ResourceUpdateWorker("geodata"),
        lambda: XrayCoreUpdateWorker("xray.exe", "stable", "", True),
    ],
)
def test_updater_cancel_unblocks_disconnect_handshake(worker) -> None:
    worker = worker()
    worker.cancel()

    with pytest.raises(UpdateCancelled):
        worker._trigger_disconnect_request()


def test_resource_worker_cancel_closes_active_response() -> None:
    class _Response:
        closed = False

        def close(self) -> None:
            self.closed = True

    worker = ResourceUpdateWorker("geodata")
    response = _Response()
    worker._register_response(response)

    worker.cancel()

    assert response.closed is True


def _write_droute_bundle(directory: Path, version: str, payload: bytes) -> None:
    directory.mkdir(parents=True)
    (directory / "droute.exe").write_bytes(payload)
    (directory / "droute.exe.config").write_text("config", encoding="utf-8")
    (directory / "LICENSE.txt").write_text("GPL-3.0", encoding="utf-8")
    (directory / "version.txt").write_text(version + "\n", encoding="utf-8")
    resources = directory / "ru-RU"
    resources.mkdir()
    (resources / "droute.resources.dll").write_bytes(b"resource")
    checksums = []
    for path in sorted(item for item in directory.rglob("*") if item.is_file()):
        relative = path.relative_to(directory).as_posix()
        checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {relative}")
    (directory / "SHA256SUMS.txt").write_text("\n".join(checksums) + "\n", encoding="utf-8")


def test_droute_check_reports_available_bundled_release(tmp_path) -> None:
    target_dir = tmp_path / "installed"
    bundled_dir = tmp_path / "bundled"
    _write_droute_bundle(target_dir, "1.1.2", b"old" * 512)
    _write_droute_bundle(bundled_dir, "2.0.0", b"new" * 1024)

    result = core_resource_updater.check_or_update_droute(
        False,
        bundle_dir=target_dir,
        bundled_dir=bundled_dir,
    )

    assert result.status == "available"
    assert result.current_version == "1.1.2"
    assert result.latest_version == "2.0.0"


def test_droute_update_installs_bundled_release_without_network(tmp_path) -> None:
    target_dir = tmp_path / "installed"
    bundled_dir = tmp_path / "bundled"
    _write_droute_bundle(target_dir, "1.1.2", b"old" * 512)
    _write_droute_bundle(bundled_dir, "2.0.0", b"new" * 1024)

    result = core_resource_updater.check_or_update_droute(
        True,
        bundle_dir=target_dir,
        bundled_dir=bundled_dir,
    )

    assert result.status == "updated"
    assert (target_dir / "version.txt").read_text(encoding="utf-8").strip() == "2.0.0"
    assert (target_dir / "droute.exe").read_bytes() == b"new" * 1024
    assert (target_dir / "ru-RU" / "droute.resources.dll").read_bytes() == b"resource"


def test_geodata_same_verified_release_is_not_downloaded(monkeypatch, tmp_path) -> None:
    core_dir = tmp_path / "core"
    rules_dir = core_dir / "rule-sets"
    core_dir.mkdir()
    rules_dir.mkdir()
    geoip = b"geoip-current"
    geosite = b"geosite-current"
    assets = {
        "geoip.dat": ("https://example.invalid/geoip.dat", hashlib.sha256(geoip).hexdigest()),
        "geosite.dat": ("https://example.invalid/geosite.dat", hashlib.sha256(geosite).hexdigest()),
        "sing-box.zip": ("https://example.invalid/sing-box.zip", "a" * 64),
    }
    (core_dir / "geoip.dat").write_bytes(geoip)
    (core_dir / "geosite.dat").write_bytes(geosite)
    (core_dir / "lumen-geodata.json").write_text(
        '{"version":"202607132228","sha256":{'
        f'"geoip.dat":"{assets["geoip.dat"][1]}",'
        f'"geosite.dat":"{assets["geosite.dat"][1]}",'
        f'"sing-box.zip":"{assets["sing-box.zip"][1]}"'
        "}}",
        encoding="utf-8",
    )
    for member in core_resource_updater.SINGBOX_BINARY_RULE_SETS.values():
        (rules_dir / Path(member).name).write_bytes(b"rules")

    monkeypatch.setattr(core_resource_updater, "_core_dir", lambda: core_dir)
    monkeypatch.setattr(core_resource_updater, "SINGBOX_RULE_SET_DIR", rules_dir)
    monkeypatch.setattr(
        core_resource_updater,
        "_resolve_geodata_release",
        lambda **_kwargs: ("202607132228", assets),
    )
    monkeypatch.setattr(
        core_resource_updater,
        "_download_direct",
        lambda *_args, **_kwargs: pytest.fail("same geodata release must not download again"),
    )

    result = core_resource_updater.update_geodata()

    assert result.status == "up_to_date"
    assert result.current_version == "202607132228"
    assert result.latest_version == "202607132228"


def test_regional_geodata_installed_requires_matching_active_data_and_all_rules(tmp_path) -> None:
    core_dir = tmp_path / "core"
    rule_set_dir = core_dir / "rule-sets"
    rule_set_dir.mkdir(parents=True)
    (core_dir / "geoip.dat").write_bytes(b"i" * 2048)
    (core_dir / "geosite.dat").write_bytes(b"s" * 2048)
    (core_dir / "lumen-geodata.json").write_text(
        '{"schema":2,"region":"china","version":"test"}',
        encoding="utf-8",
    )
    for key, source in core_resource_updater.REGIONAL_SINGBOX_RULE_SETS["china"].items():
        filename = (
            Path(source.removeprefix("archive:")).name
            if source.startswith("archive:")
            else f"{key.replace(':', '-')}.srs"
        )
        (rule_set_dir / filename).write_bytes(b"r" * 80)

    assert regional_geodata_installed(
        "china",
        target_dir=core_dir,
        rule_set_dir=rule_set_dir,
    )
    assert not regional_geodata_installed(
        "iran",
        target_dir=core_dir,
        rule_set_dir=rule_set_dir,
    )

    (rule_set_dir / "geosite-gfw.srs").unlink()
    assert not regional_geodata_installed(
        "china",
        target_dir=core_dir,
        rule_set_dir=rule_set_dir,
    )


def test_resource_card_keeps_installed_version_when_update_is_only_available() -> None:
    emitted: list[dict] = []
    bridge = SimpleNamespace(
        resourceUpdateState=SimpleNamespace(emit=emitted.append),
        _localized_backend_message=lambda message: message,
    )
    result = ResourceUpdateResult(
        kind="droute",
        status="available",
        message="update available",
        current_version="1.2.0",
        latest_version="2.0.0",
    )

    AppBridge._on_resource_update_result(bridge, result)

    assert emitted[0]["version"] == "1.2.0"
    assert emitted[0]["currentVersion"] == "1.2.0"
    assert emitted[0]["latestVersion"] == "2.0.0"


def test_resource_checks_for_different_components_can_run_together(monkeypatch) -> None:
    created = []

    class Signal:
        def __init__(self) -> None:
            self.callbacks = []

        def connect(self, callback) -> None:
            self.callbacks.append(callback)

    class Worker:
        def __init__(self, kind, **kwargs) -> None:
            self._kind = kind
            self._apply_update = kwargs["apply_update"]
            self.proxy_url = kwargs["proxy_url"]
            self.region = kwargs["region"]
            self.progress = Signal()
            self.done = Signal()
            self.request_disconnect = Signal()
            self.finished = Signal()
            self.running = False
            created.append(self)

        def isRunning(self) -> bool:
            return self.running

        def start(self) -> None:
            self.running = True

    monkeypatch.setattr(core_resource_updater, "ResourceUpdateWorker", Worker)
    monkeypatch.setattr(
        "xray_fluent.qthread_utils.retain_thread_until_finished",
        lambda _owner, workers, worker: workers.append(worker),
    )
    controller = SimpleNamespace(
        _shutting_down=False,
        _resource_update_workers=[],
        connected=True,
        state=SimpleNamespace(settings=SimpleNamespace(singbox_path="", tun_mode=True)),
        status=SimpleNamespace(emit=lambda *_args: None),
        resource_update_progress=SimpleNamespace(emit=lambda *_args: None),
        get_effective_http_proxy_port=lambda: None,
        _on_resource_update_worker_finished=lambda: None,
        _on_resource_update_done=lambda _result: None,
        _on_update_disconnect_request=lambda: None,
    )

    assert AppController.run_resource_update(
        controller,
        "geodata",
        apply_update=False,
        region="iran",
    ) is True
    assert AppController.run_resource_update(controller, "singbox", apply_update=False) is True
    assert AppController.run_resource_update(controller, "droute", apply_update=False) is True
    assert AppController.run_resource_update(controller, "singbox", apply_update=False) is False
    assert [worker._kind for worker in created] == ["geodata", "singbox", "droute"]
    assert all(worker.proxy_url is None for worker in created)
    assert created[0].region == "iran"


def test_regional_profile_waits_for_target_download_before_commit(monkeypatch) -> None:
    updates: list[tuple[str, bool, str]] = []
    commits: list[tuple[str, bool]] = []
    emitted: list[ResourceUpdateResult] = []
    monkeypatch.setattr(core_resource_updater, "regional_geodata_installed", lambda _region: False)
    controller = SimpleNamespace(
        state=SimpleNamespace(
            settings=SimpleNamespace(regional_preset="russia"),
        ),
        _pending_regional_preset="",
        run_resource_update=lambda kind, *, apply_update, region: (
            updates.append((kind, apply_update, region)) or True
        ),
    )

    outcome = AppController.request_regional_preset_change(controller, "iran")

    assert outcome == "downloading"
    assert controller.state.settings.regional_preset == "russia"
    assert controller._pending_regional_preset == "iran"
    assert updates == [("geodata", True, "iran")]

    controller._reconnect_after_resource_updates = True
    controller._commit_regional_preset = lambda region, *, restart_runtime: commits.append(
        (region, restart_runtime)
    )
    worker = SimpleNamespace(_region="iran", isRunning=lambda: False)
    controller.sender = lambda: worker
    controller._resource_update_workers = [worker]
    controller._desired_connected = False
    transitions: list[str] = []
    controller._request_transition = transitions.append
    controller.resource_update_result = SimpleNamespace(emit=emitted.append)
    controller.status = SimpleNamespace(emit=lambda *_args: None)
    controller._logger = SimpleNamespace(
        info=lambda *_args: None,
        warning=lambda *_args: None,
        error=lambda *_args: None,
    )
    controller._shutting_down = False
    controller.apply_discord_proxy = lambda: None
    result = ResourceUpdateResult("geodata", "updated", "updated")

    AppController._on_resource_update_done(controller, result)

    assert commits == [("iran", False)]
    assert controller._pending_regional_preset == ""
    assert emitted == [result]

    AppController._on_resource_update_worker_finished(controller)

    assert controller._desired_connected is True
    assert transitions == ["resource update reconnect"]


def test_ready_regional_profile_commits_immediately_without_download(monkeypatch) -> None:
    commits: list[tuple[str, bool]] = []
    monkeypatch.setattr(core_resource_updater, "regional_geodata_installed", lambda region: region == "china")
    controller = SimpleNamespace(
        state=SimpleNamespace(settings=SimpleNamespace(regional_preset="russia")),
        _pending_regional_preset="",
        _commit_regional_preset=lambda region, *, restart_runtime: commits.append(
            (region, restart_runtime)
        ),
        run_resource_update=lambda *_args, **_kwargs: pytest.fail(
            "installed regional data must not be downloaded again"
        ),
    )

    outcome = AppController.request_regional_preset_change(controller, "china")

    assert outcome == "applied"
    assert commits == [("china", True)]


def test_update_json_request_retries_transient_connection_reset(monkeypatch) -> None:
    calls = []

    class Response:
        def __init__(self, payload=None, error=None) -> None:
            self.payload = payload
            self.error = error

        def __enter__(self):
            return self

        def __exit__(self, *_args) -> None:
            return None

        def read(self):
            if self.error is not None:
                raise self.error
            return self.payload

    def fake_open(*_args, **_kwargs):
        calls.append(True)
        if len(calls) == 1:
            return Response(error=ConnectionResetError(10054, "connection reset"))
        return Response(payload=b'{"tag_name":"v1.2.3"}')

    monkeypatch.setattr(xray_core_updater, "urlopen_proxy_first", fake_open)
    monkeypatch.setattr(xray_core_updater.time, "sleep", lambda _seconds: None)

    assert xray_core_updater._request_json("https://example.test/release") == {"tag_name": "v1.2.3"}
    assert len(calls) == 2


def test_xray_update_does_not_force_local_proxy_in_tun_mode() -> None:
    controller = SimpleNamespace(
        connected=True,
        get_effective_http_proxy_port=lambda: None,
    )

    assert update_service._controller_proxy_url(controller) is None


def test_singbox_version_is_stable_before_and_after_update_check(monkeypatch, tmp_path) -> None:
    exe = tmp_path / "sing-box.exe"
    exe.write_bytes(b"sing-box")
    monkeypatch.setattr(singbox_manager, "resolve_configured_path", lambda *_args, **_kwargs: exe)
    monkeypatch.setattr(singbox_manager, "run_text_pumped", lambda *_args, **_kwargs: object())
    monkeypatch.setattr(
        singbox_manager,
        "result_output_text",
        lambda _result: "sing-box version 1.13.14-extended-2.5.0\nEnvironment: test",
    )

    assert singbox_manager.get_singbox_version(str(exe)) == "1.13.14-extended-2.5.0"


def test_droute_update_reapplies_enabled_discord_proxy(monkeypatch) -> None:
    callbacks: list[object] = []
    reapplied: list[bool] = []
    controller = SimpleNamespace(
        resource_update_result=SimpleNamespace(emit=lambda _result: None),
        status=SimpleNamespace(emit=lambda *_args: None),
        _logger=SimpleNamespace(info=lambda *_args: None),
        _reconnect_after_resource_updates=False,
        _shutting_down=False,
        apply_discord_proxy=lambda: reapplied.append(True),
    )
    monkeypatch.setattr(
        "xray_fluent.app_controller.QTimer.singleShot",
        lambda _delay, callback: callbacks.append(callback),
    )

    AppController._on_resource_update_done(
        controller,
        ResourceUpdateResult(
            kind="droute",
            status="updated",
            message="updated",
            current_version="1.2.0",
            latest_version="2.0.0",
        ),
    )

    assert len(callbacks) == 1
    callbacks[0]()
    assert reapplied == [True]


def test_xray_equal_versions_are_up_to_date(monkeypatch, tmp_path) -> None:
    exe = tmp_path / "xray.exe"
    exe.write_bytes(b"xray")
    monkeypatch.setattr(xray_core_updater, "resolve_configured_path", lambda *_args, **_kwargs: exe)
    monkeypatch.setattr(
        xray_core_updater,
        "get_xray_version",
        lambda _path: "Xray 26.7.11 (Xray, Penetrates Everything.)",
    )
    monkeypatch.setattr(
        xray_core_updater,
        "resolve_xray_release",
        lambda *_args, **_kwargs: XrayCoreRelease(
            version="v26.7.11",
            channel="stable",
            url="https://example.invalid/Xray-windows-64.zip",
        ),
    )

    result = xray_core_updater.check_and_update_xray_core(str(exe), "stable")

    assert result.status == "up_to_date"
    assert result.current_version == "26.7.11"
    assert result.latest_version == "26.7.11"


def test_xray_stable_channel_can_replace_newer_beta(monkeypatch, tmp_path) -> None:
    exe = tmp_path / "xray.exe"
    exe.write_bytes(b"xray")
    monkeypatch.setattr(xray_core_updater, "resolve_configured_path", lambda *_args, **_kwargs: exe)
    monkeypatch.setattr(xray_core_updater, "get_xray_version", lambda _path: "Xray 26.7.11")
    monkeypatch.setattr(
        xray_core_updater,
        "resolve_xray_release",
        lambda *_args, **_kwargs: XrayCoreRelease(
            version="v26.3.27",
            channel="stable",
            url="https://example.invalid/Xray-windows-64.zip",
        ),
    )

    result = xray_core_updater.check_and_update_xray_core(str(exe), "stable")

    assert result.status == "available"
    assert result.latest_version == "26.3.27"


def test_xray_release_channels_filter_prereleases() -> None:
    releases = [
        {"tag_name": "v26.7.11", "prerelease": True, "draft": False},
        {"tag_name": "v26.3.27", "prerelease": False, "draft": False},
    ]

    assert xray_core_updater._pick_release_from_github(releases, "stable")["tag_name"] == "v26.3.27"
    assert xray_core_updater._pick_release_from_github(releases, "beta")["tag_name"] == "v26.7.11"


def test_xray_check_does_not_replace_full_version_in_card() -> None:
    emitted: list[dict] = []
    bridge = SimpleNamespace(
        xrayUpdateState=SimpleNamespace(emit=emitted.append),
        _localized_backend_message=lambda message: message,
    )
    result = XrayCoreUpdateResult(
        status="available",
        message="update available",
        channel="stable",
        current_version="25.12.8",
        latest_version="26.7.11",
    )

    AppBridge._on_xray_update_result(bridge, result)

    assert emitted == [
        {
            "phase": "available",
            "version": "",
            "currentVersion": "25.12.8",
            "latestVersion": "26.7.11",
            "message": "update available",
            "percent": 0,
        }
    ]


def test_xray_updated_result_refreshes_full_version_in_card(monkeypatch) -> None:
    emitted: list[dict] = []
    bridge = SimpleNamespace(
        controller=SimpleNamespace(
            state=SimpleNamespace(settings=SimpleNamespace(xray_path="xray.exe")),
        ),
        xrayUpdateState=SimpleNamespace(emit=emitted.append),
        _localized_backend_message=lambda message: message,
    )
    monkeypatch.setattr(
        "xray_fluent.engines.xray.get_xray_version",
        lambda _path: "Xray 26.7.11 (Xray, Penetrates Everything.)",
    )
    result = XrayCoreUpdateResult(
        status="updated",
        message="updated",
        channel="stable",
        current_version="25.12.8",
        latest_version="26.7.11",
        updated=True,
    )

    AppBridge._on_xray_update_result(bridge, result)

    assert emitted[0]["version"] == "Xray 26.7.11 (Xray, Penetrates Everything.)"
    assert emitted[0]["currentVersion"] == "25.12.8"
    assert emitted[0]["latestVersion"] == "26.7.11"


def test_xray_install_preserves_lumen_geodata(tmp_path) -> None:
    target_dir = tmp_path / "core"
    target_dir.mkdir()
    target_xray = target_dir / "xray.exe"
    target_xray.write_bytes(b"old-xray")
    (target_dir / "geoip.dat").write_bytes(b"lumen-geoip")
    (target_dir / "geosite.dat").write_bytes(b"lumen-geosite-with-ru-blocked")
    (target_dir / "wintun.dll").write_bytes(b"old-wintun")
    archive_path = tmp_path / "Xray-windows-64.zip"
    with zipfile.ZipFile(archive_path, "w") as archive:
        archive.writestr("xray.exe", b"new-xray")
        archive.writestr("geoip.dat", b"official-geoip")
        archive.writestr("geosite.dat", b"official-geosite-without-custom-rules")
        archive.writestr("wintun.dll", b"new-wintun")

    xray_core_updater._install_zip_archive(archive_path, target_xray)

    assert target_xray.read_bytes() == b"new-xray"
    assert (target_dir / "wintun.dll").read_bytes() == b"new-wintun"
    assert (target_dir / "geoip.dat").read_bytes() == b"lumen-geoip"
    assert (target_dir / "geosite.dat").read_bytes() == b"lumen-geosite-with-ru-blocked"
