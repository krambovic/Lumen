"""Pure tests; never launch or stop a real VPN core."""
from __future__ import annotations

import io
from types import SimpleNamespace

from xray_fluent.core_log_limiter import CoreLogLimiter
from xray_fluent.engines.singbox.manager import SingBoxManager
from xray_fluent.engines.xray.manager import XrayManager
from xray_fluent.runtime_priority import ABOVE_NORMAL_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS
import xray_fluent.runtime_priority as priority_module


def test_core_log_limiter_preserves_first_lines_and_summarizes_burst() -> None:
    gate = CoreLogLimiter("core", max_lines_per_second=3)
    assert [gate.admit(now=10.0) for _ in range(5)] == [
        (None, True), (None, True), (None, True), (None, False), (None, False),
    ]
    summary, visible = gate.admit(now=11.1)
    assert visible
    assert summary == "INFO [core] 2 additional core log lines hidden in the previous second"


def test_critical_core_failure_is_visible_even_after_info_quota() -> None:
    gate = CoreLogLimiter("core", max_lines_per_second=2, max_critical_per_second=1)
    assert gate.admit("INFO connected", now=10.0)[1]
    assert gate.admit("INFO routed", now=10.0)[1]
    assert not gate.admit("INFO routed", now=10.0)[1]
    assert gate.admit("ERROR outbound failed", now=10.0)[1]
    assert not gate.admit("ERROR outbound failed", now=10.0)[1]


def test_xray_reader_drains_burst_without_forwarding_every_line() -> None:
    manager = XrayManager()
    received: list[str] = []
    manager.log_received.connect(received.append)
    proc = SimpleNamespace(
        stdout=io.BytesIO(b"INFO connection opened\n" * 300),
        returncode=0,
        poll=lambda: 0,
        wait=lambda: 0,
    )
    manager._proc = proc
    manager._read_output(proc)
    assert proc.stdout.tell() == len(proc.stdout.getvalue())
    assert received.count("INFO connection opened") <= 30


def test_singbox_reader_drains_error_burst_without_ui_flood() -> None:
    manager = SingBoxManager()
    received: list[str] = []
    manager.log_received.connect(received.append)
    proc = SimpleNamespace(
        stdout=io.BytesIO(b"ERROR outbound: connection failed\n" * 300),
        returncode=0,
        poll=lambda: 0,
        wait=lambda: 0,
    )
    manager._proc = proc
    manager._read_output(proc)
    assert proc.stdout.tell() == len(proc.stdout.getvalue())
    assert received.count("ERROR outbound: connection failed") <= 10
    assert manager._last_output_lines[-1] == "ERROR outbound: connection failed"


def test_ui_priority_raises_normal_class_only(monkeypatch) -> None:
    state = {"priority": NORMAL_PRIORITY_CLASS, "set": []}

    class FakeCall:
        def __init__(self, func):
            self.func = func

        def __call__(self, *args):
            return self.func(*args)

    class FakeKernel32:
        def __init__(self):
            self.GetCurrentProcess = FakeCall(lambda: 7)
            self.GetPriorityClass = FakeCall(lambda _handle: state["priority"])
            self.SetPriorityClass = FakeCall(lambda handle, value: state["set"].append((handle, value)) or 1)

    monkeypatch.setattr(priority_module, "os", SimpleNamespace(name="nt"))
    monkeypatch.setattr(priority_module.ctypes, "WinDLL", lambda *_args, **_kwargs: FakeKernel32(), raising=False)
    assert priority_module.prioritize_lumen_ui() is True
    assert state["set"] == [(7, ABOVE_NORMAL_PRIORITY_CLASS)]
    state["priority"] = ABOVE_NORMAL_PRIORITY_CLASS
    assert priority_module.prioritize_lumen_ui() is False
    assert len(state["set"]) == 1
