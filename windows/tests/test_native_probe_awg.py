import base64
from copy import deepcopy

import pytest

from xray_fluent.models import Node
from xray_fluent.native_test_config import build_native_test_config
from xray_fluent.link_parser import parse_single, parse_links_text


@pytest.mark.parametrize("enabled", [True, False])
def test_awg31_link_and_conf_preserve_boolean_options(enabled):
    key = base64.b64encode(bytes(range(32))).decode()
    node = parse_single(f"awg://{key}@192.0.2.1:51820?public_key={key}&address=10.0.0.2/32"
                        f"&random_trailers={enabled}&disable_cookies={enabled}")
    amnezia = node.outbound["singbox"]["amnezia"]
    assert amnezia["random_trailers"] is enabled
    assert amnezia["disable_cookies"] is enabled
    nodes, errors = parse_links_text(f"""[Interface]
PrivateKey = {key}
Address = 10.0.0.2/32
RandomTrailers = {enabled}
DisableCookies = {enabled}
[Peer]
PublicKey = {key}
Endpoint = 192.0.2.1:51820
AllowedIPs = 0.0.0.0/0
""")
    assert not errors
    assert nodes[0].outbound["singbox"]["amnezia"] == amnezia


@pytest.mark.parametrize("version31", [False, True])
@pytest.mark.parametrize("full_profile", [False, True])
def test_awg3_probe_uses_the_same_windows_bind_as_normal_connections(version31, full_profile):
    key = base64.b64encode(bytes(range(32))).decode()
    amnezia = {"header_protection_key": key, "h1": "100-200", "content_padding_addition": "0-64"}
    if version31:
        amnezia.update(random_trailers=True, disable_cookies=True)
    endpoint = {
        "type": "wireguard", "tag": "vpn", "system": False,
        "address": ["10.0.0.2/32"], "private_key": key, "amnezia": amnezia,
        "peers": [{"address": "192.0.2.1", "port": 51820, "public_key": key,
                   "allowed_ips": ["0.0.0.0/0"]}],
    }
    outbound = {"protocol": "awg", "singbox": endpoint}
    if full_profile:
        outbound = {"protocol": "singbox_config", "singbox_config": {
            "endpoints": [endpoint], "outbounds": [], "route": {"final": "vpn"},
        }}
    original = deepcopy(outbound)
    config = build_native_test_config(Node(scheme="awg", outbound=outbound), 19080)
    tested = config["endpoints"][0]
    detour = tested.get("detour")
    assert detour
    assert {"type": "direct", "tag": detour} in config["outbounds"]
    assert tested["amnezia"] == amnezia
    assert tested["system"] is False
    assert outbound == original


def test_explicit_awg_detour_is_preserved():
    key = base64.b64encode(bytes(range(32))).decode()
    config = {"endpoints": [{"type": "wireguard", "tag": "vpn", "detour": "custom",
                             "amnezia": {"header_protection_key": key}}],
              "outbounds": [{"type": "direct", "tag": "custom"}], "route": {"final": "vpn"}}
    node = Node(outbound={"protocol": "singbox_config", "singbox_config": config})
    assert build_native_test_config(node, 19080)["endpoints"][0]["detour"] == "custom"
