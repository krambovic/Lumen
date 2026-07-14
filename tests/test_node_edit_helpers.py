from __future__ import annotations

import pytest

from xray_fluent.models import Node
from xray_fluent.qml_app.bridge.node_edit_helpers import build_node_updates, load_node_edit_fields


def test_awg_editor_hides_xray_fields_and_updates_native_endpoint() -> None:
    node = Node(
        name="WARP",
        scheme="awg",
        server="engage.cloudflareclient.com",
        port=2408,
        outbound={
            "protocol": "awg",
            "singbox": {
                "type": "wireguard",
                "peers": [{"server": "engage.cloudflareclient.com", "server_port": 2408}],
                "amnezia": {"jc": 4},
            },
        },
    )

    fields = load_node_edit_fields(node)

    assert fields["capabilities"]["nativeSingbox"] is True
    assert fields["capabilities"]["xrayAdvanced"] is False
    assert fields["capabilities"]["identity"] is False
    assert fields["capabilities"]["endpoint"] is True

    updates = build_node_updates(
        node,
        {
            "name": "WARP new",
            "group": "Native",
            "server": "162.159.192.1",
            "port": "2408",
            "uuid": "must-not-be-used",
            "network": "xhttp",
            "security": "reality",
        },
    )

    assert updates["name"] == "WARP new"
    assert updates["server"] == "162.159.192.1"
    assert "settings" not in updates["outbound"]
    assert "streamSettings" not in updates["outbound"]
    peer = updates["outbound"]["singbox"]["peers"][0]
    assert peer["server"] == "162.159.192.1"
    assert peer["server_port"] == 2408


def test_singbox_config_editor_is_metadata_only() -> None:
    node = Node(
        name="Full config",
        scheme="singbox_config",
        server="",
        port=0,
        outbound={"protocol": "singbox_config", "singbox": {"outbounds": []}},
    )

    fields = load_node_edit_fields(node)
    updates = build_node_updates(node, {"name": "Renamed", "group": "Configs", "server": "bad", "port": "443"})

    assert fields["capabilities"]["readOnlyConfig"] is True
    assert fields["capabilities"]["endpoint"] is False
    assert "tags" not in fields
    assert updates == {
        "name": "Renamed",
        "group": "Configs",
        "outbound": {"protocol": "singbox_config", "singbox": {"outbounds": []}},
    }


def test_vless_editor_keeps_advanced_fields() -> None:
    node = Node(
        name="VLESS",
        scheme="vless",
        server="old.example",
        port=443,
        outbound={
            "protocol": "vless",
            "settings": {"vnext": [{"address": "old.example", "port": 443, "users": [{"id": "old"}]}]},
            "streamSettings": {"network": "tcp", "security": "none"},
        },
    )

    fields = load_node_edit_fields(node)

    assert fields["capabilities"]["xrayAdvanced"] is True
    assert fields["capabilities"]["identity"] is True

    updates = build_node_updates(
        node,
        {
            "name": "VLESS new",
            "group": "Default",
            "server": "new.example",
            "port": "8443",
            "uuid": "new-id",
            "encryption": "none",
            "flow": "",
            "network": "xhttp",
            "security": "tls",
            "sni": "front.example",
            "fingerprint": "edge",
        },
    )

    outbound = updates["outbound"]
    user = outbound["settings"]["vnext"][0]["users"][0]
    assert updates["server"] == "new.example"
    assert updates["port"] == 8443
    assert user["id"] == "new-id"
    assert outbound["streamSettings"]["network"] == "xhttp"
    assert outbound["streamSettings"]["tlsSettings"]["serverName"] == "front.example"


def test_vless_editor_validates_tls_certificate_pin() -> None:
    node = Node(
        name="VLESS",
        scheme="vless",
        server="example.com",
        port=443,
        outbound={
            "protocol": "vless",
            "settings": {"vnext": [{"address": "example.com", "port": 443, "users": [{"id": "id"}]}]},
            "streamSettings": {"network": "tcp", "security": "tls"},
        },
    )

    with pytest.raises(ValueError, match="64 hex"):
        build_node_updates(node, {"pinnedPeerCertSha256": "not-a-digest"})

    updates = build_node_updates(node, {"pinnedPeerCertSha256": "A" * 64, "security": "tls"})
    assert updates["outbound"]["streamSettings"]["tlsSettings"][
        "pinnedPeerCertSha256"
    ] == "a" * 64
