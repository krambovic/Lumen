from __future__ import annotations

from xray_fluent.application.runtime_security import (
    clamp_singbox_local_inbounds,
    clamp_xray_local_inbounds,
)


def test_clamp_xray_local_inbounds_keeps_proxy_and_api_on_loopback() -> None:
    payload = {
        "inbounds": [
            {"tag": "socks-in", "protocol": "mixed", "listen": "0.0.0.0", "settings": {"auth": "noauth"}},
            {"tag": "http-in", "protocol": "http", "settings": {}},
            {
                "tag": "api",
                "protocol": "dokodemo-door",
                "listen": "::",
                "settings": {"address": "0.0.0.0"},
            },
            {"tag": "public-vless", "protocol": "vless", "listen": "0.0.0.0"},
        ]
    }

    changed = clamp_xray_local_inbounds(payload)

    assert changed == 4
    assert payload["inbounds"][0]["listen"] == "127.0.0.1"
    assert payload["inbounds"][0]["settings"]["auth"] == "noauth"
    assert payload["inbounds"][1]["listen"] == "127.0.0.1"
    assert payload["inbounds"][2]["listen"] == "127.0.0.1"
    assert payload["inbounds"][2]["settings"]["address"] == "127.0.0.1"
    assert payload["inbounds"][3]["listen"] == "0.0.0.0"


def test_clamp_singbox_local_inbounds_keeps_proxy_inbounds_on_loopback() -> None:
    payload = {
        "inbounds": [
            {"tag": "mixed-in", "type": "mixed", "listen": "0.0.0.0", "listen_port": 10808},
            {"tag": "discord-socks-in", "type": "socks", "listen": "::", "listen_port": 10818},
            {"tag": "__app_hybrid_protect_in", "type": "shadowsocks", "listen_port": 19090},
            {"tag": "tun-in", "type": "tun", "interface_name": "singbox_tun"},
        ]
    }

    changed = clamp_singbox_local_inbounds(payload)

    assert changed == 3
    assert payload["inbounds"][0]["listen"] == "127.0.0.1"
    assert payload["inbounds"][1]["listen"] == "127.0.0.1"
    assert payload["inbounds"][2]["listen"] == "127.0.0.1"
    assert "listen" not in payload["inbounds"][3]
