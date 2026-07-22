from __future__ import annotations

import json
from pathlib import Path

from xray_fluent.application.node_runtime_service import is_native_singbox_only_node
from xray_fluent.application.worker_service import _node_supports_test
from xray_fluent.engines.singbox.config_builder import build_singbox_outbound
from xray_fluent.engines.singbox.runtime_planner import parse_singbox_document, plan_singbox_runtime
from xray_fluent.link_parser import parse_links_text, validate_node_outbound
from xray_fluent.models import RoutingSettings
from xray_fluent.node_transport import node_transport
from xray_fluent.qml_app.bridge.node_edit_helpers import (
    MANUAL_NODE_PROTOCOLS,
    build_node_updates,
    load_node_edit_fields,
    new_node_edit_fields,
)


def _base_config() -> dict:
    return {
        "inbounds": [{"type": "tun", "tag": "tun-in"}],
        "outbounds": [
            {"type": "direct", "tag": "proxy"},
            {"type": "direct", "tag": "direct"},
            {"type": "block", "tag": "block"},
        ],
        "route": {"rules": [], "final": "direct"},
        "dns": {"servers": [{"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"}]},
    }


def test_ovpn_inline_profile_imports_supported_extended_fields() -> None:
    nodes, errors = parse_links_text(
        """client
proto udp
remote vpn-one.example.com 1194
remote vpn-two.example.com 443 udp
data-ciphers AES-256-GCM:AES-128-GCM
auth SHA256
auth-user-pass
verify-x509-name vpn.example.com name
key-direction 1
ping 10
ping-restart 60
connect-retry 5
dhcp-option DNS 10.8.0.1
<auth-user-pass>
alice
secret
</auth-user-pass>
<ca>
-----BEGIN CERTIFICATE-----
CA
-----END CERTIFICATE-----
</ca>
<tls-crypt-v2>
-----BEGIN OpenVPN tls-crypt-v2 client key-----
KEY
-----END OpenVPN tls-crypt-v2 client key-----
</tls-crypt-v2>
"""
    )

    assert errors == []
    node = nodes[0]
    assert node.scheme == "openvpn"
    assert node.server == "vpn-one.example.com"
    assert node.port == 1194
    assert node_transport(node) == "UDP"
    assert is_native_singbox_only_node(node) is True
    assert _node_supports_test(node, "ping", ping_method="tcping") is False
    assert _node_supports_test(node, "speed") is False
    outbound = build_singbox_outbound(node)
    assert outbound["type"] == "openvpn"
    assert outbound["servers"] == [
        {"server": "vpn-one.example.com", "server_port": 1194},
        {"server": "vpn-two.example.com", "server_port": 443},
    ]
    assert outbound["proto"] == "udp"
    assert outbound["cipher"] == "AES-256-GCM"
    assert outbound["auth"] == "SHA256"
    assert outbound["username"] == "alice"
    assert outbound["password"] == "secret"
    assert outbound["tls_crypt_v2"] is True
    assert outbound["reconnect_delay"] == "5s"
    assert outbound["ping_interval"] == "10s"
    assert outbound["ping_restart"] == "60s"
    assert outbound["tls"]["verify_x509_name_mode"] == "exact"
    assert node.outbound["_dns"] == ["10.8.0.1"]
    assert validate_node_outbound(node) is None


def test_ovpn_file_embeds_sibling_resources(tmp_path: Path) -> None:
    (tmp_path / "ca.crt").write_text("CA DATA\n", encoding="utf-8")
    (tmp_path / "client.crt").write_text("CERT DATA\n", encoding="utf-8")
    (tmp_path / "client.key").write_text("KEY DATA\n", encoding="utf-8")
    (tmp_path / "ta.key").write_text("TLS AUTH DATA\n", encoding="utf-8")
    (tmp_path / "auth.txt").write_text("user\npass\n", encoding="utf-8")
    profile = tmp_path / "provider.ovpn"
    profile.write_text(
        """client
remote 198.51.100.10 443
proto tcp-client
ca ca.crt
cert client.crt
key client.key
tls-auth ta.key 1
auth-user-pass auth.txt
""",
        encoding="utf-8",
    )

    nodes, errors = parse_links_text(str(profile))

    assert errors == []
    node = nodes[0]
    native = node.outbound["singbox"]
    assert node.name == "provider"
    assert native["proto"] == "tcp"
    assert native["tls_auth"] == "TLS AUTH DATA\n"
    assert native["key_direction"] == 1
    assert native["username"] == "user"
    assert native["password"] == "pass"
    assert native["tls"] == {
        "certificate": "CERT DATA\n",
        "key": "KEY DATA\n",
        "ca": "CA DATA\n",
    }
    assert not any(key.endswith("_path") for key in native)


def test_ovpn_file_cannot_read_resources_outside_profile_directory(tmp_path: Path) -> None:
    profile_dir = tmp_path / "profile"
    profile_dir.mkdir()
    (tmp_path / "outside-ca.crt").write_text("PRIVATE DATA", encoding="utf-8")
    profile = profile_dir / "unsafe.ovpn"
    profile.write_text(
        "client\nremote vpn.example.com 1194\nca ../outside-ca.crt\n",
        encoding="utf-8",
    )

    nodes, errors = parse_links_text(str(profile))

    assert nodes == []
    assert errors and "inside the profile directory" in errors[0]


def test_openvpn_native_json_and_urltest_auto_are_preserved() -> None:
    single_nodes, single_errors = parse_links_text(json.dumps({
        "type": "openvpn",
        "tag": "provider-openvpn",
        "servers": [{"server": "single.example.com", "server_port": 1194}],
        "proto": "udp",
        "tls": {"ca": "CA"},
    }))
    assert single_errors == []
    assert single_nodes[0].scheme == "openvpn"
    assert single_nodes[0].server == "single.example.com"

    config = {
        "remarks": "OpenVPN AUTO",
        "outbounds": [
            {"type": "openvpn", "tag": "ovpn-1", "servers": [{"server": "one.example.com", "server_port": 1194}], "proto": "udp"},
            {"type": "openvpn", "tag": "ovpn-2", "servers": [{"server": "two.example.com", "server_port": 443}], "proto": "tcp"},
            {"type": "urltest", "tag": "auto", "outbounds": ["ovpn-1", "ovpn-2"], "url": "https://www.gstatic.com/generate_204", "interval": "3m"},
        ],
        "route": {"final": "auto"},
    }

    nodes, errors = parse_links_text(json.dumps(config))

    assert errors == []
    assert len(nodes) == 1
    node = nodes[0]
    assert node.scheme == "auto"
    assert node.name == "OpenVPN AUTO"
    assert node.server == "one.example.com"
    assert node.outbound["protocol"] == "singbox_config"
    stored = node.outbound["singbox_config"]
    stored_openvpn = [item for item in stored["outbounds"] if item.get("tag", "").startswith("ovpn-")]
    assert [item["type"] for item in stored_openvpn] == ["openvpn", "openvpn"]
    assert [item["name"] for item in stored_openvpn] == ["openvpn0", "openvpn1"]
    assert all(item["system"] is False for item in stored_openvpn)
    assert stored["route"]["final"] == "auto"


def test_openvpn_runtime_bootstraps_every_remote(monkeypatch) -> None:
    nodes, errors = parse_links_text(
        "client\nremote first.example.com 1194\nremote 198.51.100.20 443\nproto udp\n"
        "<ca>\n-----BEGIN CERTIFICATE-----\nCA\n-----END CERTIFICATE-----\n</ca>\n"
    )
    assert errors == []
    monkeypatch.setattr(
        "xray_fluent.engines.singbox.runtime_planner._resolve_endpoint_addresses",
        lambda host: ["203.0.113.10"] if host == "first.example.com" else [],
    )
    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))

    config = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
    ).singbox_config

    outbound = next(item for item in config["outbounds"] if item.get("tag") == "proxy")
    tun = next(item for item in config["inbounds"] if item.get("type") == "tun")
    assert outbound["type"] == "openvpn"
    assert outbound["domain_resolver"] == "bootstrap-dns"
    assert "203.0.113.10/32" in tun["route_exclude_address"]
    assert "198.51.100.20/32" in tun["route_exclude_address"]
    assert any("first.example.com" in rule.get("domain", []) for rule in config["route"]["rules"])


def test_openvpn_editor_round_trip_and_manual_protocol() -> None:
    assert "openvpn" in MANUAL_NODE_PROTOCOLS
    fields = new_node_edit_fields("openvpn", "Manual")
    keys = {item["key"] for item in fields["protocolFields"]}
    assert {"openvpnServersJson", "tlsCrypt", "tlsAuth", "certificate", "privateKey", "caCertificate"} <= keys

    updates = build_node_updates(
        type("Draft", (), {
            "name": "",
            "group": "Manual",
            "scheme": "openvpn",
            "server": "",
            "port": 0,
            "outbound": {"protocol": "openvpn"},
        })(),
        {
            "name": "My OpenVPN",
            "group": "Manual",
            "server": "vpn.example.com",
            "port": "1194",
            "openvpnProto": "udp",
            "openvpnCipher": "AES-256-GCM",
            "username": "user",
            "password": "pass",
            "tlsCrypt": "STATIC KEY",
            "caCertificate": "CA",
            "verifyX509Name": "vpn.example.com",
            "verifyX509NameMode": "exact",
        },
    )

    native = updates["outbound"]["singbox"]
    assert native["servers"] == [{"server": "vpn.example.com", "server_port": 1194}]
    assert native["tls_crypt"] == "STATIC KEY"
    assert native["tls"]["ca"] == "CA"
    loaded = load_node_edit_fields(type("Saved", (), {**updates, "scheme": "openvpn", "server": "vpn.example.com", "port": 1194})())
    assert loaded["username"] == "user"
    assert loaded["caCertificate"] == "CA"
