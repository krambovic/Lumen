from __future__ import annotations

from types import SimpleNamespace

import pytest

from xray_fluent.application.server_preflight import validate_server_preflight


SETTINGS = SimpleNamespace(tun_mode=False)


def _xray_auto_node(
    strategy_type: str | None,
    *,
    observer: str | None = None,
) -> SimpleNamespace:
    strategy = {} if strategy_type is None else {"type": strategy_type}
    config = {
        "outbounds": [{"protocol": "vless", "tag": "proxy"}],
        "routing": {
            "balancers": [
                {
                    "tag": "auto",
                    "selector": ["proxy"],
                    "strategy": strategy,
                }
            ]
        },
    }
    if observer is not None:
        config[observer] = {}
    return SimpleNamespace(
        scheme="auto",
        outbound={"protocol": "xray_config", "xray_config": config},
    )


@pytest.mark.parametrize("strategy_type", [None, "random", "roundRobin"])
def test_xray_auto_does_not_require_observatory_for_non_probe_strategies(
    strategy_type: str | None,
) -> None:
    assert validate_server_preflight(_xray_auto_node(strategy_type), SETTINGS) is None


@pytest.mark.parametrize("strategy_type", ["leastPing", "leastLoad"])
@pytest.mark.parametrize("observer", ["observatory", "burstObservatory"])
def test_xray_auto_accepts_supported_observers(
    strategy_type: str,
    observer: str,
) -> None:
    node = _xray_auto_node(strategy_type, observer=observer)

    assert validate_server_preflight(node, SETTINGS) is None


@pytest.mark.parametrize("strategy_type", ["leastPing", "leastLoad"])
def test_xray_auto_requires_observer_for_probe_strategies(strategy_type: str) -> None:
    problem = validate_server_preflight(_xray_auto_node(strategy_type), SETTINGS)

    assert problem is not None
    assert "observatory или burstObservatory" in problem
