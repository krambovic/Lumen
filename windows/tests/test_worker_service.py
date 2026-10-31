from __future__ import annotations

from types import SimpleNamespace

from xray_fluent.application.worker_service import (
    _clear_ping_measurements,
    _clear_speed_measurements,
    _filter_testable_nodes,
    _node_supports_test,
    speed_test_nodes,
)
from xray_fluent.models import Node
from xray_fluent import ping_worker
from xray_fluent.native_test_config import build_native_test_config
from xray_fluent.probe_capabilities import endpoint_method


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


def test_awg_and_masque_endpoint_ping_are_supported() -> None:
    nodes = [_native_node("awg", "awg"), _native_node("masque", "masque")]

    assert all(_node_supports_test(node, "ping", ping_method="tcping") for node in nodes)


def test_unknown_protocol_warning_is_emitted_once_for_bulk_selection() -> None:
    nodes = [_native_node("unknown1", "future-unknown"), _native_node("unknown2", "future-unknown")]
    controller = _CompatibilityController(nodes)

    assert speed_test_nodes(controller, {node.id for node in nodes}) is False

    assert len(controller.status.calls) == 1
    assert controller.status.calls[0][0] == "warning"
    assert controller._speed_worker is None
    assert controller.speed_updated.calls == []
    assert all(node.is_alive is None and node.speed_mbps is None for node in nodes)


def test_hysteria2_and_tuic_real_ping_have_native_profile_adapters() -> None:
    nodes = [_native_node("hy2", "hysteria2"), _native_node("tuic", "tuic")]

    assert all(_node_supports_test(node, "ping", ping_method="real") for node in nodes)


def test_hysteria_endpoint_probe_uses_honestly_labelled_icmp(monkeypatch) -> None:
    calls: list[tuple[str, int, float]] = []

    monkeypatch.setattr(ping_worker, "udp_ping", lambda host, port, timeout: calls.append((host, port, timeout)) or None)
    monkeypatch.setattr(ping_worker, "icmp_ping", lambda host, timeout_ms: 37)

    node = _native_node("hy2", "hysteria2")
    worker = ping_worker.PingWorker([node], timeout=1.5, method="tcping")

    assert worker._measure(node) == 37
    assert calls == []  # An unauthenticated UDP socket is not a profile check.


def _xray_auto_node(*outbounds: dict) -> Node:
    return Node(
        id="auto",
        name="AUTO",
        scheme="auto",
        server="first.invalid",
        port=443,
        outbound={
            "protocol": "xray_config",
            "xray_config": {
                "outbounds": list(outbounds),
                "routing": {
                    "balancers": [{"tag": "auto", "selector": ["proxy"]}],
                    "rules": [{"type": "field", "balancerTag": "auto"}],
                },
                "observatory": {"subjectSelector": ["proxy"]},
            },
        },
    )


def test_xray_auto_is_testable_when_its_candidates_support_the_test() -> None:
    auto = _xray_auto_node(
        {
            "tag": "proxy-1",
            "protocol": "vless",
            "settings": {"vnext": [{"address": "one.example", "port": 443}]},
        },
        {"tag": "direct", "protocol": "freedom"},
    )

    assert _node_supports_test(auto, "ping", ping_method="tcping") is True
    assert _node_supports_test(auto, "ping", ping_method="http") is True
    assert _node_supports_test(auto, "ping", ping_method="real") is True
    assert _node_supports_test(auto, "speed") is True


def test_xray_auto_endpoint_ping_uses_a_supported_inner_server() -> None:
    auto = _xray_auto_node(
        {
            "tag": "proxy-awg",
            "protocol": "awg",
            "settings": {"address": "awg.example", "port": 443},
        },
        {
            "tag": "proxy-vless",
            "protocol": "vless",
            "settings": {"vnext": [{"address": "vless.example", "port": 8443}]},
        },
    )
    controller = _CompatibilityController([auto])

    prepared = _filter_testable_nodes(controller, [auto], "ping", ping_method="tcping")

    assert len(prepared) == 1
    assert prepared[0].id == auto.id
    assert (prepared[0].server, prepared[0].port) == ("vless.example", 8443)


def test_auto_with_only_unsupported_candidates_stays_untested() -> None:
    auto = _xray_auto_node(
        {
            "tag": "proxy-awg",
            "protocol": "awg",
            "settings": {"address": "awg.example", "port": 443},
        }
    )

    assert _node_supports_test(auto, "ping", ping_method="tcping") is False
    assert _node_supports_test(auto, "speed") is False


def test_singbox_nested_auto_supports_endpoint_and_proxy_checks_without_direct_fallback() -> None:
    config = {
        "outbounds": [
            {"type": "selector", "tag": "main", "outbounds": ["fast", "direct"]},
            {"type": "urltest", "tag": "fast", "outbounds": ["ovpn", "vless"]},
            {"type": "direct", "tag": "direct"},
            {"type": "vless", "tag": "vless", "server": "vless.example", "server_port": 443},
            {"type": "selector", "tag": "unused", "outbounds": ["missing"]},
        ],
        "endpoints": [
            {"type": "openvpn-client", "tag": "ovpn", "servers": [
                {"server": "openvpn.example", "server_port": 1194}
            ]},
        ],
        "route": {"final": "main"},
    }
    auto = Node(
        id="native-auto", name="AUTO", scheme="auto", server="", port=0,
        outbound={"protocol": "singbox_config", "singbox_config": config},
    )

    assert _node_supports_test(auto, "ping", ping_method="tcping")
    assert _node_supports_test(auto, "ping", ping_method="http")
    assert _node_supports_test(auto, "ping", ping_method="real")
    assert _node_supports_test(auto, "speed")
    prepared = _filter_testable_nodes(_CompatibilityController([auto]), [auto], "ping")
    assert (prepared[0].server, prepared[0].port) == ("openvpn.example", 1194)
    assert endpoint_method(prepared[0], "tcping") == "icmp"
    probe = build_native_test_config(auto, 19876)
    main = next(item for item in probe["outbounds"] if item.get("tag") == "main")
    assert main["outbounds"] == ["fast"]
    assert config["outbounds"][0]["outbounds"] == ["fast", "direct"]


def test_icmp_falls_back_to_tcp_handshake_like_android(monkeypatch) -> None:
    tcp_calls: list[tuple[str, int]] = []
    monkeypatch.setattr(ping_worker, "icmp_ping", lambda host, timeout_ms: None)
    monkeypatch.setattr(ping_worker, "tcp_ping", lambda host, port, timeout: tcp_calls.append((host, port)) or 42)

    tcp_node = Node(id="vless", name="vless", scheme="vless", server="tcp.example", port=443, outbound={"protocol": "vless"})
    udp_node = _native_node("hy2", "hysteria2")
    worker = ping_worker.PingWorker([tcp_node, udp_node], timeout=1.0, method="icmp")

    assert worker._measure(tcp_node) == 42
    assert worker._measure(udp_node) is None  # a UDP-only endpoint has no TCP listener
    assert tcp_calls == [("tcp.example", 443)]


def test_icmp_reply_is_parsed_without_localized_ping_output() -> None:
    import sys

    if sys.platform != "win32":
        return
    assert ping_worker.icmp_ping("127.0.0.1", 1000) is not None


def test_icmpv6_reply_uses_aligned_status_and_rtt(monkeypatch):
    import struct

    class Function:
        def __init__(self, callback):
            self.callback = callback

        def __call__(self, *args):
            return self.callback(*args)

    closed = []
    status = [0]

    def send(*args):
        struct.pack_into("<28xII", args[9], 0, status[0], 42)
        return 1

    api = SimpleNamespace(
        Icmp6CreateFile=Function(lambda: 123),
        Icmp6SendEcho2=Function(send),
        IcmpCloseHandle=Function(closed.append),
    )
    monkeypatch.setattr(ping_worker, "_iphlpapi", lambda: api)
    assert ping_worker._icmp_echo_v6("::1", 1000) == 42
    status[0] = 11003
    assert ping_worker._icmp_echo_v6("::1", 1000) is None
    assert closed == [123, 123]


def test_cancel_tests_stops_ping_and_speed_workers() -> None:
    from xray_fluent.application import worker_service

    class Worker:
        def __init__(self, running: bool) -> None:
            self.running = running
            self.cancelled = False

        def isRunning(self) -> bool:
            return self.running

        def cancel(self) -> None:
            self.cancelled = True

    statuses: list[tuple[str, str]] = []
    ping, speed = Worker(True), Worker(True)
    controller = SimpleNamespace(_ping_worker=ping, _speed_worker=speed, status=SimpleNamespace(emit=lambda *a: statuses.append(a)))

    assert worker_service.cancel_tests(controller)
    assert ping.cancelled and speed.cancelled

    idle = SimpleNamespace(_ping_worker=None, _speed_worker=Worker(False), status=SimpleNamespace(emit=lambda *a: statuses.append(a)))
    assert not worker_service.cancel_tests(idle)
