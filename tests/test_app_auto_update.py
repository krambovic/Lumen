import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from xray_fluent.app_controller import apply_masque_direct_update_once
from xray_fluent.app_updater import AppUpdate, UpdateDownloader, should_auto_install
from xray_fluent.models import AppSettings, AppState, Node, RoutingSettings


def _update(*, downgrade: bool = False) -> AppUpdate:
    return AppUpdate(
        version="2.0.0",
        tag="v2.0.0",
        download_url="https://example.test/setup.exe",
        size=1,
        notes="",
        is_downgrade=downgrade,
    )


def test_app_auto_update_setting_round_trip() -> None:
    settings = AppSettings(app_auto_update=True)
    restored = AppSettings.from_dict(settings.to_dict())
    assert restored.app_auto_update is True


def test_app_auto_update_defaults_to_disabled() -> None:
    assert AppSettings.from_dict({}).app_auto_update is False


def test_diagnostics_upload_defaults_to_enabled() -> None:
    assert AppSettings.from_dict({}).diagnostics_upload_enabled is True
    restored = AppSettings.from_dict(AppSettings(diagnostics_upload_enabled=False).to_dict())
    assert restored.diagnostics_upload_enabled is False


def test_legacy_xray_tun_engine_is_ignored() -> None:
    settings = AppSettings.from_dict({"tun_mode": True, "tun_engine": "xray"})

    assert settings.tun_mode is True
    assert "tun_engine" not in settings.to_dict()


def test_fake_dns_defaults_to_disabled_like_v2rayn() -> None:
    assert RoutingSettings().dns_fake_enabled is False
    assert RoutingSettings.from_dict({}).dns_fake_enabled is False


def test_applied_migrations_round_trip() -> None:
    state = AppState()
    state.applied_migrations["enable_fake_dns_by_default"] = True
    restored = AppState.from_dict(state.to_dict())
    assert restored.applied_migrations["enable_fake_dns_by_default"] is True


def test_saved_masque_nodes_are_migrated_once_on_update() -> None:
    native = {
        "type": "masque",
        "tag": "proxy",
        "server": "162.159.198.2",
        "server_port": 443,
        "public_key": "public",
        "address": ["172.16.0.2/32"],
        "profile": {"detour": "direct", "private_key": "private"},
    }
    node = Node(
        name="Legacy MASQUE",
        scheme="masque",
        server="162.159.198.2",
        port=443,
        link="",
        outbound={"protocol": "masque", "singbox": native},
    )
    state = AppState(nodes=[node])

    assert apply_masque_direct_update_once(state) is True
    assert native["private_key"] == "private"
    assert native["profile"] == {"detour": "direct"}
    assert apply_masque_direct_update_once(state) is False


def test_fragmentation_defaults_to_disabled() -> None:
    settings = AppSettings.from_dict({})
    assert settings.enable_xray_fragment is False
    assert settings.enable_final_fragment is False


def test_window_size_defaults_to_widescreen() -> None:
    settings = AppSettings.from_dict({})
    assert (settings.window_width, settings.window_height) == (1280, 720)


def test_legacy_square_window_default_migrates_to_widescreen() -> None:
    settings = AppSettings.from_dict({"window_width": 1024, "window_height": 768})
    assert (settings.window_width, settings.window_height) == (1280, 720)


def test_saved_window_size_is_preserved() -> None:
    settings = AppSettings.from_dict({"window_width": 1440, "window_height": 900})
    assert (settings.window_width, settings.window_height) == (1440, 900)


def test_window_size_is_not_rebound_on_every_settings_change() -> None:
    main_qml = (
        Path(__file__).parents[1] / "xray_fluent" / "qml_app" / "qml" / "Main.qml"
    ).read_text(encoding="utf-8")

    assert "width: App.windowWidth" not in main_qml
    assert "height: App.windowHeight" not in main_qml
    assert "win.width = Math.max(win.minimumWidth, App.windowWidth" in main_qml
    assert "win.height = Math.max(win.minimumHeight, App.windowHeight" in main_qml


def test_negative_window_position_is_preserved_for_left_monitor() -> None:
    settings = AppSettings.from_dict({"window_x": -1920, "window_y": 80})
    assert (settings.window_x, settings.window_y) == (-1920, 80)


def test_auto_install_requires_permission_and_never_downgrades() -> None:
    assert should_auto_install(_update(), enabled=True)
    assert not should_auto_install(_update(), enabled=False)
    assert not should_auto_install(_update(downgrade=True), enabled=True)


def test_downloader_cancel_interrupts_blocking_response() -> None:
    request_started = threading.Event()
    release_response = threading.Event()

    class _Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args) -> None:
            pass

        def do_HEAD(self) -> None:
            self.send_response(200)
            self.send_header("Content-Length", str(1024 * 1024))
            self.end_headers()

        def do_GET(self) -> None:
            self.send_response(200)
            self.send_header("Content-Length", str(1024 * 1024))
            self.end_headers()
            request_started.set()
            release_response.wait(5)

    server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    update = _update()
    update.download_url = f"http://127.0.0.1:{server.server_port}/setup.exe"
    update.digest_sha256 = "0" * 64
    update.asset_name = "setup.exe"
    worker = UpdateDownloader(update)

    try:
        worker.start()
        assert request_started.wait(3)
        started = time.monotonic()
        worker.cancel()
        assert worker.wait(3000)
        assert time.monotonic() - started < 1.0
    finally:
        release_response.set()
        if worker.isRunning():
            worker.wait(6000)
        server.shutdown()
        server.server_close()
        server_thread.join(2)
