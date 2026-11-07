import io
import json
import zipfile

from xray_fluent import diagnostics as diag
from xray_fluent.models import AppState, Node


def test_state_retains_actual_settings_and_masks_private_values():
    state = AppState()
    state.settings.tun_mode = True
    state.settings.core_preference = "singbox"
    state.settings.tun_mtu = 1400
    state.settings.language = "fa"
    state.settings.proxy_auth_username = "private-user"
    state.settings.proxy_auth_password = "private-password"
    state.settings.xray_config_file = "C:/private-user/config.json"
    state.settings.ui_wallpaper = "C:/private-user/picture.png"
    state.settings.subscription_converter_url = "https://private.example/token"
    source = state.to_dict()
    source["settings"]["future_unknown"] = "private-future"
    safe = diag._sanitize_state(source)
    assert set(state.settings.to_dict()) <= safe["settings"].keys()
    for key in ("tun_mode", "core_preference", "tun_mtu", "auto_connect_last", "enable_system_proxy", "language", "local_http_port", "subscription_use_real_hwid"):
        assert safe["settings"][key] == source["settings"][key]
    for key in ("proxy_auth_username", "proxy_auth_password", "xray_config_file", "ui_wallpaper", "subscription_converter_url"):
        assert safe["settings"][key] == "***"
    assert "future_unknown" not in safe["settings"]
    assert "private-" not in json.dumps(safe)


def test_routing_and_group_counts_use_the_real_model_and_are_idempotent():
    state = AppState(nodes=[Node(name="private-node", server="private-server")],
                     manual_groups=["private-group"], subscriptions=[{"url": "private-subscription"}])
    state.routing.direct_domains = ["private-domain-1", "private-domain-2"]
    state.routing.process_rules = [{"process": "C:/private-user/app.exe", "action": "proxy"}]
    state.routing.dns_hosts = {"private-host": ["192.0.2.1"]}
    state.routing.service_routes = {"youtube": "proxy"}
    state.routing.dns_proxy_type = "tls"
    safe = diag._sanitize_state(state.to_dict())
    assert safe["group_count"] == 1
    assert safe["node_count"] == 1 and safe["subscription_count"] == 1
    assert safe["routing_rule_count"] == 4
    assert safe["routing"]["dns_proxy_type"] == "tls"
    assert safe["routing"]["service_routes"] == {"youtube": "proxy"}
    assert safe["routing"]["direct_domain_count"] == 2
    assert safe["routing"]["process_rule_count"] == 1
    assert safe["routing"]["dns_host_count"] == 1
    assert "private-" not in json.dumps(safe)
    assert "security" not in safe
    assert diag._sanitize_state(safe) == safe


def test_state_survives_export_and_repeated_upload_filtering(tmp_path, monkeypatch):
    monkeypatch.setattr(diag, "collect_runtime_context", lambda state: {})
    state = AppState()
    state.settings.kill_switch = True
    state.settings.proxy_auth_password = "private-password"
    expected = diag._sanitize_state(state.to_dict())
    path = diag.export_diagnostics(tmp_path / "out.zip", state, [], {"state": True})
    data = path.read_bytes()
    for _ in range(3):
        data = diag.sanitize_diagnostic_zip(data)
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            assert json.loads(archive.read("state_redacted.json")) == expected
    assert expected["settings"]["kill_switch"] is True
    assert expected["settings"]["proxy_auth_password"] == "***"


def test_large_routing_maps_keep_original_counts_on_repeated_sanitization():
    state = AppState()
    state.routing.service_routes = {f"service_{index}": "proxy" for index in range(200)}
    safe = diag._sanitize_state(state.to_dict())
    assert safe["routing"]["service_route_count"] == 200
    assert safe["routing_rule_count"] == 200
    assert diag._sanitize_state(safe) == safe
