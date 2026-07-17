from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application.worker_service import (
    _clear_ping_measurements,
    _clear_speed_measurements,
    ping_nodes,
    speed_test_nodes,
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


class _CompatibilityController(_Controller):
    def __init__(self, nodes: list[Node]) -> None:
        super().__init__()
        self.state = SimpleNamespace(
            nodes=nodes,
            settings=SimpleNamespace(ping_method="tcping"),
        )
        self.status = _Signal()
        self._ping_worker = None
        self._speed_worker = None


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


def _native_node(node_id: str, protocol: str) -> Node:
    return Node(
        id=node_id,
        name=node_id.upper(),
        scheme=protocol,
        server="203.0.113.10",
        port=443,
        outbound={"protocol": protocol, "singbox": {"type": protocol}},
    )


def test_awg_and_masque_ping_warning_is_emitted_once_for_bulk_selection() -> None:
    nodes = [_native_node("awg", "awg"), _native_node("masque", "masque")]
    controller = _CompatibilityController(nodes)

    ping_nodes(controller, {node.id for node in nodes}, method="tcping")

    assert len(controller.status.calls) == 1
    level, message = controller.status.calls[0]
    assert level == "warning"
    assert "AWG" in message and "MASQUE" in message
    assert controller._ping_worker is None
    assert controller.ping_updated.calls == []
    assert all(node.is_alive is None and node.ping_ms is None for node in nodes)


def test_native_speed_warning_is_emitted_once_for_bulk_selection() -> None:
    nodes = [_native_node("awg", "awg"), _native_node("masque", "masque")]
    controller = _CompatibilityController(nodes)

    assert speed_test_nodes(controller, {node.id for node in nodes}) is False

    assert len(controller.status.calls) == 1
    assert controller.status.calls[0][0] == "warning"
    assert controller._speed_worker is None
    assert controller.speed_updated.calls == []
    assert all(node.is_alive is None and node.speed_mbps is None for node in nodes)


def test_hysteria2_and_tuic_real_ping_warning_is_emitted_once() -> None:
    nodes = [_native_node("hy2", "hysteria2"), _native_node("tuic", "tuic")]
    controller = _CompatibilityController(nodes)

    ping_nodes(controller, {node.id for node in nodes}, method="real")

    assert len(controller.status.calls) == 1
    assert "HY2" in controller.status.calls[0][1]
    assert "TUIC" in controller.status.calls[0][1]
    assert controller.ping_updated.calls == []
    assert all(node.is_alive is None and node.ping_ms is None for node in nodes)
