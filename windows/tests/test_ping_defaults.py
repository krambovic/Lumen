from xray_fluent.models import AppSettings


def test_fresh_and_legacy_missing_ping_preferences_default_to_http_get():
    assert AppSettings().ping_method == "http"
    assert AppSettings.from_dict({}).ping_method == "http"
    assert AppSettings.from_dict({"ping_method": ""}).ping_method == "http"


def test_explicit_ping_preference_is_preserved():
    assert AppSettings.from_dict({"ping_method": "icmp"}).ping_method == "icmp"
    assert AppSettings.from_dict({"ping_method": "tcping"}).ping_method == "tcping"
