from __future__ import annotations

import zipfile
from types import SimpleNamespace

import pytest

from xray_fluent import core_resource_updater
from xray_fluent.core_resource_updater import ResourceUpdateWorker, _atomic_replace_files
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


def test_droute_check_reports_available_release(monkeypatch, tmp_path) -> None:
    bundle_dir = tmp_path / "droute"
    bundle_dir.mkdir()
    (bundle_dir / "droute.exe").write_bytes(b"old" * 512)
    (bundle_dir / "version.txt").write_text("1.1.2\n", encoding="utf-8")
    monkeypatch.setattr(
        core_resource_updater,
        "_request_json",
        lambda *_args, **_kwargs: {
            "tag_name": "1.2.0",
            "assets": [
                {
                    "name": "droute-1.2.0.zip",
                    "browser_download_url": "https://example.invalid/droute-1.2.0.zip",
                }
            ],
        },
    )

    result = core_resource_updater.check_or_update_droute(False, bundle_dir=bundle_dir)

    assert result.status == "available"
    assert result.current_version == "1.1.2"
    assert result.latest_version == "1.2.0"


def test_droute_update_installs_latest_bundle(monkeypatch, tmp_path) -> None:
    bundle_dir = tmp_path / "droute"
    bundle_dir.mkdir()
    (bundle_dir / "droute.exe").write_bytes(b"old" * 512)
    (bundle_dir / "version.txt").write_text("1.1.2\n", encoding="utf-8")
    monkeypatch.setattr(
        core_resource_updater,
        "_request_json",
        lambda *_args, **_kwargs: {
            "tag_name": "1.2.0",
            "assets": [
                {
                    "name": "droute-1.2.0.zip",
                    "browser_download_url": "https://example.invalid/droute-1.2.0.zip",
                }
            ],
        },
    )

    def fake_download(_url, destination, *_args, **_kwargs) -> None:
        with zipfile.ZipFile(destination, "w") as archive:
            archive.writestr("droute.exe", b"new" * 1024)
            archive.writestr("droute.exe.config", "config")
            archive.writestr("ru-RU/droute.resources.dll", b"resource")

    monkeypatch.setattr(core_resource_updater, "_download_direct", fake_download)

    result = core_resource_updater.check_or_update_droute(True, bundle_dir=bundle_dir)

    assert result.status == "updated"
    assert (bundle_dir / "version.txt").read_text(encoding="utf-8").strip() == "1.2.0"
    assert (bundle_dir / "droute.exe").read_bytes() == b"new" * 1024
    assert (bundle_dir / "ru-RU" / "droute.resources.dll").read_bytes() == b"resource"


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
