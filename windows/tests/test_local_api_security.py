from __future__ import annotations

import json
from types import SimpleNamespace
from typing import Any

import pytest

from xray_fluent.application.runtime_introspection import (
    collect_xray_inbound_ports,
    ensure_dict,
    ensure_list,
    replace_or_append_tagged,
)
from xray_fluent.application.xray_runtime_service import (
    APP_METRICS_API_INBOUND_TAG,
    APP_METRICS_API_TAG,
    ensure_xray_metrics_contract,
)
from xray_fluent.engines.singbox.runtime_planner import _ensure_singbox_metrics_contract
from xray_fluent.live_metrics_worker import LiveMetricsWorker
from xray_fluent.process_traffic_collector import collect_process_stats


class _Response:
    def __init__(self, payload: dict[str, Any]) -> None:
        self._payload = json.dumps(payload).encode("utf-8")

    def __enter__(self) -> _Response:
        return self

    def __exit__(self, *_args: object) -> None:
        return None

    def read(self) -> bytes:
        return self._payload


def _xray_controller() -> SimpleNamespace:
    return SimpleNamespace(
        state=SimpleNamespace(
            settings=SimpleNamespace(
                discord_proxy_enabled=False,
                proxy_allow_lan=False,
            )
        ),
        _ensure_dict=ensure_dict,
        _ensure_list=ensure_list,
        _collect_xray_inbound_ports=collect_xray_inbound_ports,
        _replace_or_append_tagged=replace_or_append_tagged,
        _log=lambda _message: None,
    )


def test_xray_runtime_keeps_only_stats_service_on_an_app_owned_loopback_api(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import xray_fluent.application.xray_runtime_service as runtime_service

    payload = {
        "api": {
            "tag": "api",
            "listen": "0.0.0.0:10085",
            "services": [
                "HandlerService",
                "RoutingService",
                "ReflectionService",
                "StatsService",
            ],
        },
        "inbounds": [
            {
                "tag": "api-in",
                "listen": "0.0.0.0",
                "port": 10085,
                "protocol": "dokodemo-door",
                "settings": {"address": "127.0.0.1"},
            },
            {"tag": "mixed-in", "listen": "127.0.0.1", "port": 10808, "protocol": "socks"},
        ],
        "outbounds": [
            {"tag": "proxy", "protocol": "vless"},
            {"tag": "direct", "protocol": "freedom"},
            {"tag": "api", "protocol": "freedom"},
        ],
        "routing": {
            "rules": [
                {"type": "field", "inboundTag": ["api-in"], "outboundTag": "api"},
                {"type": "field", "domain": ["example.com"], "outboundTag": "proxy"},
            ]
        },
    }
    monkeypatch.setattr(runtime_service, "find_free_api_port", lambda **_kwargs: 19085)

    api_port, inbound_tags = ensure_xray_metrics_contract(
        _xray_controller(),
        payload,
        allocate_port=True,
    )

    assert api_port == 19085
    assert inbound_tags == ("mixed-in",)
    assert payload["api"] == {
        "tag": APP_METRICS_API_TAG,
        "services": ["StatsService"],
    }
    serialized = json.dumps(payload)
    assert "HandlerService" not in serialized
    assert "RoutingService" not in serialized
    assert "ReflectionService" not in serialized
    assert '"listen": "0.0.0.0:10085"' not in serialized

    metrics_inbounds = [
        inbound
        for inbound in payload["inbounds"]
        if inbound.get("tag") == APP_METRICS_API_INBOUND_TAG
    ]
    assert metrics_inbounds == [
        {
            "tag": APP_METRICS_API_INBOUND_TAG,
            "listen": "127.0.0.1",
            "port": 19085,
            "protocol": "dokodemo-door",
            "settings": {"address": "127.0.0.1"},
        }
    ]
    assert not any(inbound.get("tag") == "api-in" for inbound in payload["inbounds"])
    assert not any(outbound.get("tag") == "api" for outbound in payload["outbounds"])
    assert sum(outbound.get("tag") == APP_METRICS_API_TAG for outbound in payload["outbounds"]) == 1
    assert payload["routing"]["rules"][0] == {
        "type": "field",
        "inboundTag": [APP_METRICS_API_INBOUND_TAG],
        "outboundTag": APP_METRICS_API_TAG,
    }


def test_xray_runtime_does_not_delete_user_routing_when_imported_api_tag_collides() -> None:
    payload = {
        "api": {
            "tag": "direct",
            "listen": "0.0.0.0:10085",
            "services": ["HandlerService", "RoutingService", "ReflectionService"],
        },
        "inbounds": [
            {"tag": "mixed-in", "listen": "127.0.0.1", "port": 10808, "protocol": "socks"},
        ],
        "outbounds": [
            {"tag": "proxy", "protocol": "vless"},
            {"tag": "direct", "protocol": "freedom"},
        ],
        "routing": {
            "rules": [
                {"type": "field", "domain": ["example.com"], "outboundTag": "direct"},
            ]
        },
    }

    ensure_xray_metrics_contract(_xray_controller(), payload, allocate_port=False)

    assert {outbound.get("tag") for outbound in payload["outbounds"]} >= {"proxy", "direct", APP_METRICS_API_TAG}
    assert any(rule.get("domain") == ["example.com"] for rule in payload["routing"]["rules"])
    assert payload["api"] == {"tag": APP_METRICS_API_TAG, "services": ["StatsService"]}


def test_singbox_clash_api_secret_is_strong_rotated_and_overwrites_imported_value() -> None:
    first_payload = {
        "experimental": {
            "clash_api": {
                "external_controller": "0.0.0.0:9090",
                "external_ui": "attacker-controlled-ui",
                "secret": "imported-secret",
            }
        }
    }
    second_payload: dict[str, Any] = {}

    first_secret = _ensure_singbox_metrics_contract(first_payload)
    second_secret = _ensure_singbox_metrics_contract(second_payload)

    assert len(first_secret) >= 40
    assert first_secret != "imported-secret"
    assert first_secret != second_secret
    assert first_payload["experimental"]["clash_api"] == {
        "external_controller": "127.0.0.1:19090",
        "secret": first_secret,
    }
    assert second_payload["experimental"]["clash_api"]["secret"] == second_secret


def test_live_metrics_worker_uses_bearer_auth_and_fails_closed_without_secret(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import xray_fluent.live_metrics_worker as metrics_module

    requests = []

    def fake_urlopen(request, *, timeout: int):
        requests.append((request, timeout))
        return _Response({"uploadTotal": 12, "downloadTotal": 34})

    monkeypatch.setattr(metrics_module, "urlopen", fake_urlopen)
    worker = LiveMetricsWorker("", 0, mode="singbox", clash_api_secret="runtime-secret")

    assert worker._query_clash_api_totals() == (12, 34)
    assert requests[0][0].get_header("Authorization") == "Bearer runtime-secret"

    requests.clear()
    unauthenticated_worker = LiveMetricsWorker("", 0, mode="singbox")
    assert unauthenticated_worker._query_clash_api_totals() == (None, None)
    assert requests == []


def test_process_traffic_collector_uses_bearer_auth_and_fails_closed_without_secret(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import xray_fluent.process_traffic_collector as collector_module

    requests = []

    def fake_urlopen(request, *, timeout: int):
        requests.append((request, timeout))
        return _Response({"connections": []})

    monkeypatch.setattr(collector_module.urllib.request, "urlopen", fake_urlopen)

    assert collect_process_stats(clash_api_secret="runtime-secret") == []
    assert requests[0][0].get_header("Authorization") == "Bearer runtime-secret"

    requests.clear()
    assert collect_process_stats() == []
    assert requests == []
