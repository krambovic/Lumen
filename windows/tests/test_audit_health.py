from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading
from types import SimpleNamespace

import pytest

from xray_fluent import active_profile_health as health
from xray_fluent.application import auto_switch_service as service
from xray_fluent.metrics_api import MetricsApiError


class Signal:
    def __init__(self):
        self.values = []

    def emit(self, *values):
        self.values.append(values)


def controller(tun=False):
    nodes = [SimpleNamespace(id=str(i), name=f"node{i}", group="test", is_alive=True,
                             ping_ms=10, speed_mbps=10) for i in range(3)]
    settings = SimpleNamespace(auto_switch_enabled=True, tun_mode=tun,
                               auto_switch_cooldown_sec=30, auto_switch_threshold_kbps=512,
                               auto_switch_delay_sec=10)
    result = SimpleNamespace(
        state=SimpleNamespace(settings=settings, nodes=nodes, selected_node_id="0"),
        connected=True, _switching=False, _reconnecting=False,
        _auto_switch_exhausted=False, _auto_switch_cycle_attempts=0,
        _auto_switch_last_switch=0.0, _auto_switch_high_ticks=0,
        _auto_switch_active_download=False, _auto_switch_low_since=0.0,
        _health_down_since=0.0, _log=lambda message: None, save=lambda: None,
        status=Signal(), auto_switch_triggered=Signal(), selection_changed=Signal(), transitions=[],
    )
    result._request_transition = result.transitions.append
    return result


def check(monkeypatch, c, now, status="FAILED", stamp=None, profile="0", down=0, up=0, latency=None):
    monkeypatch.setattr(service.time, "monotonic", lambda: now)
    service.check_auto_switch(c, down, latency, up_bps=up, health_status=status,
                              health_checked_at=now if stamp is None else stamp,
                              health_profile_id=profile)


@pytest.mark.parametrize("tun", [False, True])
def test_three_distinct_http_failures_switch_in_proxy_and_tun(monkeypatch, tun):
    c = controller(tun)
    check(monkeypatch, c, 100)
    check(monkeypatch, c, 104)
    assert c.transitions == []
    check(monkeypatch, c, 108)
    assert c.transitions == ["auto-switch: profile HTTP health"]


def test_missing_tcp_latency_never_means_failed_health(monkeypatch):
    c = controller()
    for stamp in range(100, 300, 20):
        monkeypatch.setattr(service.time, "monotonic", lambda: stamp)
        service.check_auto_switch(c, 0, None)
    assert c.transitions == []
    assert c._health_failure_count == 0


@pytest.mark.parametrize("down,up", [(1, 0), (0, 1), (2000000, 10)])
def test_positive_traffic_resets_before_failed_health_or_missing_latency(monkeypatch, down, up):
    c = controller()
    check(monkeypatch, c, 100)
    check(monkeypatch, c, 104)
    check(monkeypatch, c, 108, down=down, up=up)
    assert c._health_failure_count == 0
    assert c.transitions == []
    check(monkeypatch, c, 109, stamp=108)
    assert c._health_failure_count == 0
    check(monkeypatch, c, 112)
    assert c._health_failure_count == 1


@pytest.mark.parametrize("state,stamp,profile", [
    (None, 108, "0"), ("UNKNOWN", 108, "0"), ("unsupported", 108, "0"),
    ("FAILED", 50, "0"), ("FAILED", 200, "0"), ("FAILED", float("nan"), "0"),
    ("FAILED", 108, "different"), ("FAILED", 108, ""),
])
def test_unknown_stale_wrong_profile_and_invalid_probes_break_streak(monkeypatch, state, stamp, profile):
    c = controller()
    check(monkeypatch, c, 100)
    check(monkeypatch, c, 104)
    check(monkeypatch, c, 108, state, stamp, profile)
    assert c._health_failure_count == 0
    check(monkeypatch, c, 112)
    assert c.transitions == []


def test_repeated_sample_counted_once_and_cooldown_enforced(monkeypatch):
    c = controller()
    c._auto_switch_last_switch = 90
    for stamp in range(100, 109):
        check(monkeypatch, c, stamp, stamp=100)
    assert c._health_failure_count == 1
    check(monkeypatch, c, 112)
    check(monkeypatch, c, 116)
    assert c.transitions == []
    check(monkeypatch, c, 120)
    assert len(c.transitions) == 1


def test_healthy_and_hot_switch_clear_failure_streak(monkeypatch):
    c = controller()
    check(monkeypatch, c, 100)
    check(monkeypatch, c, 104, "HEALTHY")
    check(monkeypatch, c, 108)
    assert c._health_failure_count == 1
    c.state.selected_node_id = "1"
    check(monkeypatch, c, 112, profile="1")
    assert c._health_failure_count == 1
    assert c.transitions == []


GRAPH = {"proxies": {"proxy": {"type": "Selector", "all": ["region"], "now": "region"},
                     "region": {"type": "WireGuard"}}}


class Api:
    def __init__(self, result=None, error=None):
        self.result = result or {"delay": 31}
        self.error = error
        self.calls = []

    def get(self, path, **kwargs):
        self.calls.append((path, kwargs))
        if self.error:
            raise self.error
        return self.result


def test_udp_native_outbound_health_uses_explicit_authenticated_api_path():
    api = Api()
    result = health.probe_active_profile(profile_id="id", clash_client=api, outbound_graph=GRAPH)
    assert result.status == "HEALTHY" and result.latency_ms == 31
    assert api.calls[0][0].startswith("/proxies/proxy/delay?")
    assert "timeout=" in api.calls[0][0] and "url=https%3A%2F%2F" in api.calls[0][0]


@pytest.mark.parametrize("code,expected", [(0, "UNKNOWN"), (401, "UNKNOWN"), (404, "UNKNOWN"),
                                           (405, "UNKNOWN"), (501, "UNKNOWN"), (503, "FAILED"), (504, "FAILED")])
def test_api_capability_errors_are_not_profile_failures(code, expected):
    api = Api(error=MetricsApiError("safe diagnostic", code))
    result = health.probe_active_profile(profile_id="id", clash_client=api, outbound_graph=GRAPH)
    assert result.status == expected


@pytest.mark.parametrize("graph", [None, {}, {"proxy": {"type": "direct"}}, {"proxy": {"type": "unknown"}}])
def test_unverified_outbound_cannot_be_probed_as_profile(graph):
    api = Api()
    assert health.probe_active_profile(profile_id="id", clash_client=api, outbound_graph=graph).status == "UNKNOWN"
    assert api.calls == []


def test_stale_health_payload_discards_display_latency():
    sample = health.HealthSample("FAILED", 10, "id")
    assert sample.payload(now=21)["health_status"] == "UNKNOWN"
    assert health.HealthSample("HEALTHY", 10, "id", 25).payload(now=21)["latency_ms"] is None


def test_no_active_listener_and_no_profile_are_unknown():
    assert health.probe_active_profile(profile_id="id").status == "UNKNOWN"
    assert health.probe_active_profile(profile_id="", proxy_url="http://127.0.0.1:9").status == "UNKNOWN"
    assert health.probe_active_profile(profile_id="id", proxy_url="http://remote.invalid:8080").status == "UNKNOWN"


def test_explicit_proxy_health_ignores_hostile_no_proxy(monkeypatch):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(self.path)
            self.send_response(204)
            self.end_headers()

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    monkeypatch.setenv("NO_PROXY", "*")
    monkeypatch.setenv("no_proxy", "*")
    try:
        result = health.probe_active_profile(
            profile_id="id", proxy_url=f"http://127.0.0.1:{server.server_port}",
            health_url="http://health.invalid/generate_204",
        )
        assert result.status == "HEALTHY"
        assert requests == ["http://health.invalid/generate_204"]
    finally:
        server.shutdown()
        server.server_close()
        thread.join(2)


def test_unsupported_delay_is_unknown_even_if_api_uses_gateway_timeout():
    api = Api(error=MetricsApiError("HTTP 504", 504, unsupported=True))
    assert health.probe_active_profile(profile_id="id", clash_client=api, outbound_graph=GRAPH).status == "UNKNOWN"


@pytest.mark.parametrize("delay", [0, None, False, "25"])
def test_missing_zero_or_unsupported_delay_is_unknown(delay):
    result = health.probe_active_profile(profile_id="id", clash_client=Api(result={"delay": delay}), outbound_graph=GRAPH)
    assert result.status == "UNKNOWN"


def test_malformed_health_url_is_unknown_without_dialing():
    assert health.probe_active_profile(profile_id="id", health_url="http://[").status == "UNKNOWN"


def test_long_sample_gap_and_legacy_reset_break_failure_streak(monkeypatch):
    c = controller()
    check(monkeypatch, c, 100)
    check(monkeypatch, c, 104)
    check(monkeypatch, c, 200)
    assert c._health_failure_count == 1 and c.transitions == []
    check(monkeypatch, c, 204)
    c._health_down_since = 0.0  # legacy AppController reset remains safe
    check(monkeypatch, c, 208)
    assert c._health_failure_count == 1 and c.transitions == []
