import base64

from xray_fluent import http_latency
from xray_fluent.active_profile_health import probe_active_profile


def test_warm_latency_reuses_connection_and_excludes_connect_and_first_get(monkeypatch):
    clock = [0.0]
    connections = []
    delays = iter([.9, .06, .04, .05, .03])

    class Response:
        status = 204
        will_close = False

        def read(self, _limit):
            return b""

        def isclosed(self):
            return True

        def close(self):
            pass

    class Connection:
        sock = None

        def __init__(self, host, port, **kwargs):
            assert (host, port) == ("127.0.0.1", 18080)
            connections.append(self)
            self.requests = 0
            self.closed = False

        def set_tunnel(self, host, port, headers):
            assert (host, port) == ("health.example", 443)
            expected = base64.b64encode(b"user name:pass:word").decode()
            assert headers == {"Proxy-Authorization": "Basic " + expected}

        def connect(self):
            clock[0] += 2.0
            self.sock = object()

        def request(self, *args, **kwargs):
            self.requests += 1
            clock[0] += next(delays)

        def getresponse(self):
            return Response()

        def close(self):
            self.closed = True
            self.sock = None

    monkeypatch.setattr(http_latency, "HTTPSConnection", Connection)
    monkeypatch.setattr(http_latency.time, "perf_counter", lambda: clock[0])
    probe = http_latency.WarmHTTPProbe()
    arguments = ("http://user%20name:pass%3Aword@127.0.0.1:18080", "https://health.example/generate_204", 3)
    assert probe.measure(*arguments) == 40
    assert probe.measure(*arguments) == 30
    assert len(connections) == 1 and connections[0].requests == 5
    probe.close()
    assert connections[0].closed


def test_dashboard_uses_warm_http_probe_instead_of_cold_clash_delay():
    class Probe:
        def measure(self, proxy, url, timeout):
            assert proxy == "http://127.0.0.1:18080"
            return 47

    class Api:
        def get(self, *args, **kwargs):
            raise AssertionError("cold Clash delay must not be used")

    result = probe_active_profile(profile_id="active", clash_client=Api(),
                                  proxy_url="http://127.0.0.1:18080", warm_probe=Probe())
    assert result.status == "HEALTHY" and result.latency_ms == 47
    assert result.source == "active-http-proxy-warm"


def test_singbox_health_listener_is_authenticated_and_pinned_before_direct_rules():
    from urllib.parse import urlsplit
    from xray_fluent.application.health_listener import add_singbox_health_listener

    payload = {
        "outbounds": [
            {"type": "selector", "tag": "proxy", "outbounds": ["vpn", "direct"], "default": "vpn"},
            {"type": "vless", "tag": "vpn"}, {"type": "direct", "tag": "direct"},
        ],
        "route": {"rules": [{"domain_suffix": ["com"], "outbound": "direct"}]},
    }
    url = urlsplit(add_singbox_health_listener(payload, port=18080))
    assert url.hostname == "127.0.0.1" and url.username and url.password
    inbound = payload["inbounds"][0]
    assert inbound["users"] == [{"username": url.username, "password": url.password}]
    assert payload["route"]["rules"][0] == {"inbound": [inbound["tag"]], "action": "route", "outbound": "proxy"}
    payload["outbounds"][0]["default"] = "direct"
    assert add_singbox_health_listener(payload, port=18081) == ""
