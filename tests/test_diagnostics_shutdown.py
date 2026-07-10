from __future__ import annotations

import logging

from xray_fluent import diagnostics_uploader


def test_http_diagnostics_handler_joins_background_thread(monkeypatch) -> None:
    monkeypatch.setattr(diagnostics_uploader, "_send", lambda *_args, **_kwargs: None)
    handler = diagnostics_uploader.HttpDiagnosticsHandler(
        "https://diagnostics.example.test",
        flush_interval=60,
    )
    handler.emit(logging.LogRecord("test", logging.WARNING, __file__, 1, "message", (), None))

    handler.close()

    assert not handler._thread.is_alive()


def test_heartbeat_stop_joins_background_thread(monkeypatch) -> None:
    monkeypatch.setattr(diagnostics_uploader, "_send", lambda *_args, **_kwargs: None)
    heartbeat = diagnostics_uploader.HeartbeatSender(
        "https://diagnostics.example.test",
        interval=60,
    )

    heartbeat.stop()

    assert not heartbeat._thread.is_alive()
