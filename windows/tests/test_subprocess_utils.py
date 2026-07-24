from __future__ import annotations

import subprocess

from xray_fluent import subprocess_utils


def test_run_text_pumped_runs_directly_without_gui_thread(monkeypatch) -> None:
    expected = subprocess.CompletedProcess(["tool"], 0, b"ok", b"")
    calls: list[tuple[list[str], float, bool, int | None]] = []

    def fake_run_text(command, *, timeout, check, creationflags):
        calls.append((command, timeout, check, creationflags))
        return expected

    monkeypatch.setattr(subprocess_utils, "_qt_app_on_current_thread", lambda: None)
    monkeypatch.setattr(subprocess_utils, "run_text", fake_run_text)

    result = subprocess_utils.run_text_pumped(
        ["tool"],
        timeout=2.5,
        check=True,
        creationflags=123,
    )

    assert result is expected
    assert calls == [(["tool"], 2.5, True, 123)]
