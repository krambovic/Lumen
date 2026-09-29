"""Bound live core-log delivery so a traffic burst cannot starve the UI.

The core's stdout is still drained and recent lines are retained for startup
diagnostics. Only forwarding into the Qt/UI/logging pipeline is rate-limited.
"""
from __future__ import annotations

import re
import time

_CRITICAL_LEVEL = re.compile(r"(?:^|[\s\[])\s*(?:ERROR|FATAL|PANIC|WARN(?:ING)?)(?:\b|\])", re.IGNORECASE)


class CoreLogLimiter:
    def __init__(
        self, source: str, *, max_lines_per_second: int = 40,
        max_critical_per_second: int = 10,
    ) -> None:
        self.source = source
        self.limit = max(1, max_lines_per_second)
        self.critical_limit = max(1, max_critical_per_second)
        self._window_started = 0.0
        self._visible = 0
        self._critical_visible = 0
        self._hidden = 0

    def reset(self) -> None:
        self._window_started = 0.0
        self._visible = 0
        self._critical_visible = 0
        self._hidden = 0

    def admit(self, line: str = "", *, now: float | None = None) -> tuple[str | None, bool]:
        """Return (previous-window summary, whether to forward this line)."""
        now = time.monotonic() if now is None else now
        summary = None
        if self._window_started == 0.0 or now - self._window_started >= 1.0:
            if self._hidden:
                summary = f"INFO [{self.source}] {self._hidden} additional core log lines hidden in the previous second"
            self._window_started = now
            self._visible = 0
            self._critical_visible = 0
            self._hidden = 0
        if _CRITICAL_LEVEL.search(line):
            if self._critical_visible >= self.critical_limit:
                self._hidden += 1
                return summary, False
            self._critical_visible += 1
            return summary, True
        if self._visible >= self.limit:
            self._hidden += 1
            return summary, False
        self._visible += 1
        return summary, True
