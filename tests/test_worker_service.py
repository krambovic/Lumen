from __future__ import annotations

from xray_fluent.application.worker_service import (
    _clear_ping_measurements,
    _clear_speed_measurements,
)
from xray_fluent.models import Node


class _Signal:
    def __init__(self) -> None:
        self.calls: list[tuple] = []

    def emit(self, *args) -> None:
        self.calls.append(args)


class _Controller:
    def __init__(self) -> None:
        self.ping_updated = _Signal()
        self.speed_updated = _Signal()
        self.speed_progress_updated = _Signal()
        self.save_count = 0

    def schedule_save(self) -> None:
        self.save_count += 1


def test_clear_ping_measurements_resets_saved_values_before_worker_starts() -> None:
    node = Node(id="node-1", ping_ms=123, speed_mbps=45.0, is_alive=True)
    controller = _Controller()

    _clear_ping_measurements(controller, [node])

    assert node.ping_ms is None
    assert node.speed_mbps == 45.0
    assert node.is_alive is None
    assert controller.ping_updated.calls == [("node-1", None)]
    assert controller.save_count == 1


def test_clear_speed_measurements_resets_saved_values_before_worker_starts() -> None:
    node = Node(id="node-1", ping_ms=123, speed_mbps=45.0, is_alive=True)
    controller = _Controller()

    _clear_speed_measurements(controller, [node])

    assert node.ping_ms == 123
    assert node.speed_mbps is None
    assert node.is_alive is None
    assert controller.speed_updated.calls == [("node-1", None, False)]
    assert controller.speed_progress_updated.calls == [("node-1", 0)]
    assert controller.save_count == 1
