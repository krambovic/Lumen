from types import SimpleNamespace

from xray_fluent import app_controller, network_monitor
from xray_fluent.app_controller import AppController


def test_network_monitor_does_not_wait_for_hostname_dns_when_offline(monkeypatch):
    def unavailable(*_args, **_kwargs):
        raise OSError("network is not ready")

    def blocking_dns_must_not_run(*_args, **_kwargs):
        raise AssertionError("synchronous hostname DNS would block startup")

    monkeypatch.setattr(network_monitor.socket, "socket", unavailable)
    monkeypatch.setattr(network_monitor.socket, "gethostbyname", blocking_dns_must_not_run)
    assert network_monitor.NetworkMonitor._fingerprint() == "0.0.0.0"


def test_network_monitor_constructor_does_not_probe_before_first_frame(monkeypatch):
    def unexpected_probe():
        raise AssertionError("constructor must not probe the network")

    monkeypatch.setattr(network_monitor.NetworkMonitor, "_fingerprint", staticmethod(unexpected_probe))
    monitor = network_monitor.NetworkMonitor()
    assert monitor._last_fingerprint == ""


def test_startup_country_detection_yields_between_server_batches(monkeypatch):
    nodes = [SimpleNamespace(name=f"Server {index}", server="example.org", country_code="") for index in range(70)]
    timer_starts = []
    scheduled = []
    saves = []
    emissions = []
    controller = SimpleNamespace(
        _shutting_down=False,
        _country_detect_nodes=nodes,
        _country_detect_cursor=0,
        _country_detect_changed=False,
        _country_detect_timer=SimpleNamespace(start=timer_starts.append),
        state=SimpleNamespace(nodes=nodes),
        schedule_save=lambda: saves.append(True),
        nodes_changed=SimpleNamespace(emit=emissions.append),
        _start_country_ip_resolution=lambda: None,
    )
    monkeypatch.setattr(app_controller, "detect_country", lambda _name, _server: "NL")
    monkeypatch.setattr(
        app_controller,
        "QTimer",
        SimpleNamespace(singleShot=lambda delay, callback: scheduled.append((delay, callback))),
    )

    AppController._process_country_detection_batch(controller)
    assert controller._country_detect_cursor == 32
    assert timer_starts == [10]
    assert not saves and not emissions

    AppController._process_country_detection_batch(controller)
    assert controller._country_detect_cursor == 64
    assert timer_starts == [10, 10]

    AppController._process_country_detection_batch(controller)
    assert controller._country_detect_nodes == []
    assert all(node.country_code == "NL" for node in nodes)
    assert saves == [True]
    assert emissions == [nodes]
    assert scheduled == [(500, controller._start_country_ip_resolution)]
