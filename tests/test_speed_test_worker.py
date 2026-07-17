from __future__ import annotations

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
