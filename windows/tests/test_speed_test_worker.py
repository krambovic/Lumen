from __future__ import annotations

import pytest

from xray_fluent.models import Node
from xray_fluent.speed_test_worker import (
    SpeedTestWorker,
    _SpeedTestTarget,
    _resolve_speed_test_concurrency,
)


def test_configured_speed_concurrency_is_not_capped_at_six() -> None:
    assert _resolve_speed_test_concurrency(node_count=30, configured=10) == 10


def test_auto_speed_concurrency_uses_default_ten() -> None:
    assert _resolve_speed_test_concurrency(node_count=30, configured=0) == 10


def test_speed_concurrency_never_exceeds_node_count() -> None:
    assert _resolve_speed_test_concurrency(node_count=3, configured=10) == 3


def test_xray_auto_speed_config_preserves_balancer_and_observatory() -> None:
    full_config = {
        "outbounds": [
            {
                "tag": "proxy-1",
                "protocol": "vless",
                "settings": {"vnext": [{"address": "one.example", "port": 443}]},
            },
            {"tag": "direct", "protocol": "freedom"},
        ],
        "observatory": {"subjectSelector": ["proxy"], "probeInterval": "30s"},
        "routing": {
            "balancers": [{"tag": "auto", "selector": ["proxy"], "strategy": {"type": "leastPing"}}],
            "rules": [{"type": "field", "network": "tcp,udp", "balancerTag": "auto"}],
        },
    }
    node = Node(
        id="auto",
        name="AUTO",
        scheme="auto",
        outbound={"protocol": "xray_config", "xray_config": full_config},
    )
    worker = SpeedTestWorker([node], xray_path="xray.exe")

    config = worker._build_config(_SpeedTestTarget(node=node, http_port=19080))

    assert config["observatory"] == full_config["observatory"]
    assert config["routing"]["balancers"] == full_config["routing"]["balancers"]
    assert config["routing"]["rules"] == [
        {"type": "field", "inboundTag": ["speed-http"], "balancerTag": "auto"}
    ]
    assert config["inbounds"][0]["port"] == 19080


@pytest.mark.parametrize("closed,status,expected", [(False, 204, 40), (True, 204, -1), (False, 302, -1)])
def test_http_get_excludes_handshakes_and_warmup(monkeypatch, closed, status, expected):
    from xray_fluent import speed_test_worker as module

    clock = [0.0]
    connections = []

    class Response:
        will_close = closed

        def __init__(self):
            self.status = status

        def read(self, size):
            return b""

        def isclosed(self):
            return True

        def close(self):
            pass

    class Connection:
        def __init__(self, host, port, **kwargs):
            assert host == "127.0.0.1" and port == 19080
            assert kwargs["context"] is module.get_ssl_context()
            self.sock = None
            self.requests = 0
            self.closed = False
            connections.append(self)

        def set_tunnel(self, host, port):
            assert host == "www.google.com" and port == 443

        def connect(self):
            clock[0] += 2.0  # CONNECT and TLS never enter the measured samples.
            self.sock = object()

        def request(self, method, path, headers):
            assert method == "GET" and path == "/generate_204"
            assert self.sock is not None
            clock[0] += [0.9, 0.06, 0.04][self.requests]
            self.requests += 1

        def getresponse(self):
            return Response()

        def close(self):
            self.closed = True

    monkeypatch.setattr(module, "HTTPSConnection", Connection)
    monkeypatch.setattr(module.time, "perf_counter", lambda: clock[0])
    node = Node(id="one", server="node.example", port=443)
    worker = SpeedTestWorker([node], xray_path="xray.exe", mode="ping", warm_ping=True)
    assert worker._real_ping(_SpeedTestTarget(node, 19080)) == expected
    assert len(connections) == 1 and connections[0].closed
    assert not worker._responses
    assert connections[0].requests == (3 if expected > 0 else 1)


def test_auto_probe_uses_main_balancer_and_preserves_loopback_rescue():
    config = {
        "outbounds": [
            {"tag": "proxy-1", "protocol": "vless"},
            {"tag": "rescue", "protocol": "loopback", "settings": {"inboundTag": "rescue-in"}},
            {"tag": "block", "protocol": "blackhole"},
        ],
        "routing": {
            "balancers": [
                {"tag": "main", "selector": ["proxy-"], "fallbackTag": "rescue"},
                {"tag": "backup", "selector": ["proxy-"], "fallbackTag": "block"},
            ],
            "rules": [
                {"type": "field", "inboundTag": ["rescue-in"], "balancerTag": "backup"},
                {"type": "field", "network": "tcp,udp", "balancerTag": "main"},
            ],
        },
    }
    node = Node(scheme="auto", outbound={"protocol": "xray_config", "xray_config": config})
    worker = SpeedTestWorker([node], xray_path="xray.exe")
    result = worker._build_config(_SpeedTestTarget(node, 19080))
    assert result["routing"]["rules"] == [
        {"type": "field", "inboundTag": ["speed-http"], "balancerTag": "main"},
        config["routing"]["rules"][0],
    ]
    config["outbounds"][-1]["protocol"] = "freedom"
    with pytest.raises(ValueError, match="cannot bypass"):
        worker._build_config(_SpeedTestTarget(node, 19080))


def test_auto_ping_retries_observer_startup_outside_measurement(monkeypatch):
    from xray_fluent import speed_test_worker as module

    node = Node(scheme="auto", outbound={"protocol": "xray_config"})
    worker = SpeedTestWorker([node], xray_path="xray.exe", mode="ping", warm_ping=True)
    clock = [0.0]
    monkeypatch.setattr(module.time, "monotonic", lambda: clock[0])
    monkeypatch.setattr(module.time, "sleep", lambda delay: clock.__setitem__(0, clock[0] + delay))
    results = iter([-1, -1, 45])
    monkeypatch.setattr(worker, "_warm_http_ping", lambda _: next(results))
    assert worker._real_ping(_SpeedTestTarget(node, 19080)) == 45
    assert clock[0] == pytest.approx(.4)

    monkeypatch.setattr(worker, "_warm_http_ping", lambda _: -1)
    assert worker._real_ping(_SpeedTestTarget(node, 19080)) == -1
    assert 12 <= clock[0] <= 12.7
