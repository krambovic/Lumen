import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from xray_fluent.app_updater import AppUpdate, UpdateDownloader, should_auto_install
from xray_fluent.models import AppSettings


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
