from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading
from types import SimpleNamespace

import pytest

from xray_fluent import live_metrics_worker as live
from xray_fluent import process_traffic_collector as collector
from xray_fluent import xray_stats_client as stats
from xray_fluent.metrics_api import ClashApiClient, MetricsApiError, freeze_document
from xray_fluent.traffic_route_classifier import RouteClassifier


GRAPH = {"proxies": {
    "GLOBAL": {"type": "Selector", "all": ["AUTO", "plain"], "now": "AUTO"},
    "AUTO": {"type": "URLTest", "all": ["Japan"], "now": "Japan"},
    "Japan": {"type": "Hysteria2"}, "plain": {"type": "Direct"},
    "proxy": {"type": "Selector", "all": ["Japan"], "now": "Japan"},
}}


@pytest.mark.parametrize("chain,expected", [
    (["Japan", "AUTO", "GLOBAL"], "proxy"), (["GLOBAL", "AUTO", "Japan"], "proxy"),
    (["GLOBAL"], "proxy"), (["plain"], "direct"), (["plain", "GLOBAL"], "direct"),
    (["proxy"], "proxy"), ([], "unknown"), (None, "unknown"),
    (["unrecognized-proxy"], "unknown"), (["Japan", "plain"], "unknown"),
    ([None], "unknown"), ("plain", "unknown"),
])
def test_complete_chain_is_classified_by_outbound_type(chain, expected):
    assert RouteClassifier(GRAPH).classify(chain) == expected


@pytest.mark.parametrize("kind", ["VLESS", "VMess", "Trojan", "WireGuard", "TUIC", "Naive", "Socks", "Shadowsocks"])
def test_proxy_types_need_no_proxy_substring_in_label(kind):
    assert RouteClassifier({"region": {"type": kind}}).classify(["region"]) == "proxy"


def test_tag_names_do_not_override_direct_freedom_or_unknown_types():
    graph = RouteClassifier({"proxy": {"type": "Direct"}, "GLOBAL": {"protocol": "freedom"},
                             "direct": {"type": "future-unknown"}})
    assert graph.classify(["proxy"]) == "direct"
    assert graph.classify(["GLOBAL"]) == "direct"
    assert graph.classify(["direct"]) == "unknown"
    assert RouteClassifier().classify(["direct"]) == "unknown"


def test_ambiguous_group_and_cyclic_graph_are_unknown():
    graph = RouteClassifier({"g": {"type": "Selector", "all": ["p", "d"]},
                             "p": {"type": "VLESS"}, "d": {"type": "Direct"},
                             "cycle": {"type": "Selector", "all": ["cycle"]}})
    assert graph.classify(["g"]) == "unknown"
    assert graph.classify(["cycle"]) == "unknown"
    assert graph.classify(["d", "g"]) == "direct"


@pytest.fixture(autouse=True)
def reset_tracking():
    collector.reset_connection_tracking()
    yield
    collector.reset_connection_tracking()


def document(up=0, down=0, chain=None, active=True):
    return freeze_document({"uploadTotal": up, "downloadTotal": down, "connections": [{
        "id": "id", "upload": up, "download": down,
        "metadata": {"processPath": "C:\\Apps\\client.exe", "host": "example.invalid"},
        "chains": [] if chain is None else chain,
    }] if active else []})


def collect(doc, graph=GRAPH, **kwargs):
    return collector.collect_process_stats(connections_document=doc, outbound_graph=graph, **kwargs)


def test_closed_unknown_and_known_byte_totals_are_kept_separately():
    first = collect(document())[0]
    assert first.upload == first.download == first.unknown_bytes == 0
    second = collect(document(10, 20))[0]
    assert second.unknown_bytes == 30 and second.route == "unknown"
    third = collect(document(20, 30, ["Japan", "AUTO", "GLOBAL"]))[0]
    assert third.unknown_bytes == 30 and third.proxy_bytes == 20
    closed = collect(document(active=False))[0]
    assert closed.connections == 0 and closed.total_connections == 1
    assert closed.upload == 20 and closed.download == 30
    assert closed.proxy_bytes + closed.direct_bytes + closed.unknown_bytes == 50
    assert closed.unknown_bytes == 30 and closed.proxy_bytes == 20
    assert closed.down_speed == closed.up_speed == 0


def test_supplied_document_is_reused_without_any_second_api_fetch(monkeypatch):
    monkeypatch.setattr(collector, "ClashApiClient", lambda *args: pytest.fail("unexpected network fetch"))
    sample = document(100, 200, ["Japan"])
    result = collect(sample)
    assert result[0].route == "proxy"
    with pytest.raises(TypeError):
        sample["connections"][0]["upload"] = 500


def test_old_worker_epoch_cannot_resurrect_reset_connection_tracking():
    old_epoch = collector.connection_tracking_epoch()
    collect(document(), tracking_epoch=old_epoch)
    collector.reset_connection_tracking()
    assert collect(document(10, 20), tracking_epoch=old_epoch) == []
    assert collector._conn_bytes == {}


def test_collector_mutations_hold_the_shared_lock(monkeypatch):
    original = collector._process_name_from_metadata
    checked = []

    def inspect(meta):
        free = collector._lock.acquire(blocking=False)
        if free:
            collector._lock.release()
        checked.append(not free)
        return original(meta)

    monkeypatch.setattr(collector, "_process_name_from_metadata", inspect)
    collect(document())
    assert checked == [True]


def test_pid_lookup_is_not_hidden_by_a_pid_only_cache(monkeypatch):
    names = iter(("old.exe", "new.exe"))
    monkeypatch.setattr(collector, "process_name_from_pid", lambda _pid: next(names))
    metadata = {"processID": 4242}
    assert collector._process_name_from_metadata(metadata) == ("old.exe", "old.exe")
    # win_proc_monitor validates the process creation time; the collector must
    # ask it again so a recycled numeric PID cannot keep the old application.
    assert collector._process_name_from_metadata(metadata) == ("new.exe", "new.exe")


def test_first_baseline_and_implausible_jumps_do_not_create_traffic():
    assert collect(document(10**15, 10**15))[0].upload == 0
    assert collect(document(10**18, 10**18))[0].download == 0
    assert collect(document(1, 1))[0].upload == 0
    assert collect(document(2, 3))[0].upload == 1


def _stat_message(name, value):
    encoded = name.encode()
    message = b"\x0a" + stats._varint(len(encoded)) + encoded + b"\x10" + stats._varint(value)
    return b"\x0a" + stats._varint(len(message)) + message


def test_query_stats_codec_matches_official_protobuf_field_numbers():
    assert stats.STATS_METHOD == "/xray.app.stats.command.StatsService/QueryStats"
    assert stats.encode_query_stats_request("x") == b"\x0a\x01x"  # reset=false omitted
    assert stats.decode_query_stats_response(b"\x0a\x05\x0a\x01x\x10\x7b") == {"x": 123}
    assert stats.decode_query_stats_response(b"\x0a\x03\x0a\x01x") == {"x": 0}
    assert stats.decode_query_stats_response(_stat_message("large", 2**55)) == {"large": 2**55}


@pytest.mark.parametrize("payload", [b"\x0a\xff", b"\x00", b"\x08\x01", b"\x0a\x01\x80",
                                     _stat_message("negative", 2**64 - 1),
                                     _stat_message("x", 1) + _stat_message("x", 2)])
def test_invalid_or_negative_protobuf_counters_are_rejected(payload):
    with pytest.raises(ValueError):
        stats.decode_query_stats_response(payload)


class Future:
    def __init__(self, response=None):
        self.response = {"x": 1} if response is None else response
        self.cancelled = False

    def result(self, **kwargs):
        return self.response

    def cancel(self):
        self.cancelled = True


class Channel:
    def __init__(self, future=None):
        self.next_future = future or Future()
        self.calls = []
        self.closed = False

    def unary_unary(self, method, **kwargs):
        assert method == stats.STATS_METHOD
        assert kwargs["request_serializer"](b"test") == b"test"
        assert kwargs["response_deserializer"](b"") == {}
        return self

    def future(self, request, **kwargs):
        self.calls.append((request, kwargs))
        return self.next_future

    def close(self):
        self.closed = True


def test_grpc_channel_is_lazy_reused_deadlined_and_ignores_ambient_proxy(monkeypatch):
    channel, creations = Channel(), []

    def create(target, **kwargs):
        creations.append((target, kwargs))
        return channel

    monkeypatch.setattr(stats.importlib, "import_module", lambda name: SimpleNamespace(insecure_channel=create))
    client = stats.XrayStatsClient(12345)
    assert creations == []
    assert client.query() == {"x": 1} and client.query() == {"x": 1}
    assert len(creations) == 1 and creations[0][0] == "127.0.0.1:12345"
    assert ("grpc.enable_http_proxy", 0) in creations[0][1]["options"]
    assert len(channel.calls) == 2 and channel.calls[0][1]["timeout"] <= 2
    assert channel.calls[0][1]["wait_for_ready"] is False
    client.close()
    assert channel.closed and client.query() is None


def test_grpc_close_cancels_pending_rpc_without_a_core_process(monkeypatch):
    started, released = threading.Event(), threading.Event()

    class BlockingFuture(Future):
        def result(self, **kwargs):
            started.set()
            assert released.wait(3)
            raise RuntimeError("cancelled")

        def cancel(self):
            super().cancel()
            released.set()

    future = BlockingFuture()
    channel = Channel(future)
    monkeypatch.setattr(stats.importlib, "import_module", lambda name: SimpleNamespace(insecure_channel=lambda *a, **k: channel))
    client = stats.XrayStatsClient(12345)
    thread = threading.Thread(target=client.query)
    thread.start()
    try:
        assert started.wait(2)
        client.close()
    finally:
        released.set()
        thread.join(3)
    assert future.cancelled and channel.closed and not thread.is_alive()


def test_missing_grpc_dependency_is_explicitly_degraded(monkeypatch):
    def unavailable(name):
        raise ImportError("not installed")

    monkeypatch.setattr(stats.importlib, "import_module", unavailable)
    client = stats.XrayStatsClient(12345)
    assert client.query() is None
    assert "grpcio unavailable" in client.reason
    client.close()


def test_xray_worker_uses_only_exact_active_inbound_counters(monkeypatch):
    worker = live.LiveMetricsWorker("not-an-executable", 12345, xray_inbound_tags=["active"])
    payload = {"inbound>>>active>>>traffic>>>uplink": 12, "inbound>>>active>>>traffic>>>downlink": 34,
               "inbound>>>unrelated>>>traffic>>>uplink": 999999,
               "outbound>>>proxy>>>traffic>>>downlink": 999999}
    monkeypatch.setattr(worker._stats_client, "query", lambda: payload)
    try:
        assert worker._query_xray_stats() == (12, 34)
        payload.pop("inbound>>>active>>>traffic>>>downlink")
        assert worker._query_xray_stats() == (None, None)
        assert "unavailable" in worker._traffic_reason
    finally:
        worker.stop()


def test_actual_loopback_grpc_framing_when_dependency_is_installed():
    grpc = pytest.importorskip("grpc", reason="grpcio not installed; codec/client unit tests remain mandatory")
    captured = []

    def query(request, context):
        captured.append(request)
        return _stat_message("inbound>>>active>>>traffic>>>uplink", 42)

    executor = ThreadPoolExecutor(max_workers=1)
    server = grpc.server(executor)
    server.add_generic_rpc_handlers((grpc.method_handlers_generic_handler(
        "xray.app.stats.command.StatsService", {"QueryStats": grpc.unary_unary_rpc_method_handler(
            query, request_deserializer=lambda data: data, response_serializer=lambda data: data)}),))
    port = server.add_insecure_port("127.0.0.1:0")
    server.start()
    client = stats.XrayStatsClient(port)
    try:
        assert client.query() == {"inbound>>>active>>>traffic>>>uplink": 42}
        assert captured == [stats.encode_query_stats_request()]
    finally:
        client.close()
        server.stop(0).wait(2)
        executor.shutdown(wait=True)


@pytest.mark.parametrize("details", [True, False])
def test_live_tick_fetches_connections_once_and_health_continues_when_hidden(monkeypatch, details):
    clock = [10.0]
    monkeypatch.setattr(live.time, "monotonic", lambda: clock[0])
    calls, documents, payloads = [], [], []

    class Api:
        def get(self, path, **kwargs):
            calls.append(path)
            if path == "/connections":
                return document()
            if path == "/proxies":
                return freeze_document(GRAPH)
            return {"delay": 25}

        def close(self):
            pass

    worker = live.LiveMetricsWorker("", 0, mode="singbox", clash_api_secret="test-secret",
                                    active_profile_id="profile", process_stats_enabled=details)
    monkeypatch.setattr(worker, "_clash_client", Api())
    monkeypatch.setattr(live, "collect_process_stats", lambda *a, **k: documents.append(k["connections_document"]) or [])
    monkeypatch.setattr(worker._stop_event, "wait", lambda delay: clock.__setitem__(0, clock[0] + 1))

    def emitted(payload):
        payloads.append(payload)
        if len(payloads) == 6:
            worker.stop()

    worker.metrics.connect(emitted)
    worker.run()
    assert calls.count("/connections") == 6
    assert sum(path.startswith("/proxies/proxy/delay?") for path in calls) == 2
    assert len(documents) == (2 if details else 0)
    assert all(row["health_status"] == "HEALTHY" for row in payloads)
    assert all(row["traffic_available"] for row in payloads)


def test_loopback_api_auth_and_redirect_never_follow_ambient_proxy(monkeypatch):
    received = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            received.append((self.path, self.headers.get("Authorization")))
            if self.path == "/redirect":
                self.send_response(302)
                self.send_header("Location", "http://must-not-be-dialed.invalid/")
                self.end_headers()
            else:
                data = json.dumps({"connections": []}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    monkeypatch.setenv("HTTP_PROXY", "http://must-not-be-dialed.invalid:9999")
    monkeypatch.setenv("NO_PROXY", "")
    client = ClashApiClient(server.server_port, "test-secret")
    try:
        assert client.get("/connections")["connections"] == ()
        with pytest.raises(MetricsApiError) as exc:
            client.get("/redirect")
        assert exc.value.status == 302
        assert received == [("/connections", "Bearer test-secret"), ("/redirect", "Bearer test-secret")]
        with pytest.raises(MetricsApiError):
            ClashApiClient(server.server_port, "").get("/connections")
        assert len(received) == 2
    finally:
        client.close()
        server.shutdown()
        server.server_close()
        thread.join(2)


def test_failed_grpc_sample_reconnects_lazily_after_backoff(monkeypatch):
    clock, channels = [10.0], []

    class Broken(Future):
        def result(self, **kwargs):
            raise OSError("unavailable")

    def create(*args, **kwargs):
        channel = Channel(Broken() if not channels else Future())
        channels.append(channel)
        return channel

    monkeypatch.setattr(stats.time, "monotonic", lambda: clock[0])
    monkeypatch.setattr(stats.importlib, "import_module", lambda name: SimpleNamespace(insecure_channel=create))
    client = stats.XrayStatsClient(12345)
    assert client.query() is None and channels[0].closed
    assert client.query() is None and len(channels) == 1
    clock[0] = 14
    assert client.query() == {"x": 1} and len(channels) == 2
    client.close()


@pytest.mark.parametrize("detour", [{"detour": "exit"}, {"proxySettings": {"tag": "exit"}},
                                     {"streamSettings": {"sockopt": {"dialerProxy": "exit"}}}])
def test_direct_type_with_a_known_proxy_detour_is_not_direct(detour):
    graph = RouteClassifier({"innocent": {"type": "direct", **detour}, "exit": {"type": "VLESS"}})
    assert graph.classify(["innocent"]) == "proxy"
    assert graph.classify(["innocent", "exit"]) == "proxy"


def test_clash_close_handles_a_socket_cleared_by_the_reader():
    closed = []
    sock = SimpleNamespace(shutdown=lambda how: closed.append("socket"))

    class RacingConnection:
        reads = 0

        @property
        def sock(self):
            self.reads += 1
            return sock if self.reads == 1 else None

        def close(self):
            closed.append("connection")

    client = ClashApiClient(12345, "secret")
    client._active = RacingConnection()
    client.close()
    assert closed == ["socket", "connection"]
