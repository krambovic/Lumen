from __future__ import annotations

import json
from pathlib import Path

from xray_fluent.engines.singbox.config_builder import build_singbox_outbound
from xray_fluent.engines.singbox.runtime_planner import parse_singbox_document, plan_singbox_runtime
from xray_fluent.link_parser import parse_links_text, repair_node_outbound_from_link
from xray_fluent.models import RoutingSettings


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


def test_mkcp_transport_is_converted_for_singbox_extended() -> None:
    nodes, errors = parse_links_text(
        "vless://00000000-0000-0000-0000-000000000000@example.com:443"
        "?type=kcp&headerType=wireguard&seed=secret&congestion=false&mtu=1350#mkcp"
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")

    assert outbound["transport"]["type"] == "mkcp"
    assert outbound["transport"]["header_type"] == "wireguard"
    assert outbound["transport"]["seed"] == "secret"
    assert outbound["transport"]["congestion"] is False
    assert outbound["transport"]["mtu"] == 1350


def test_mieru_native_json_import_is_tun_only_singbox_outbound() -> None:
    nodes, errors = parse_links_text(
        json.dumps(
            {
                "type": "mieru",
                "tag": "proxy",
                "server": "example.com",
                "server_port": 27017,
                "transport": "TCP",
                "username": "user",
                "password": "pass",
            }
        )
    )

    assert errors == []
    assert nodes[0].scheme == "mieru"
    assert build_singbox_outbound(nodes[0], tag="proxy")["type"] == "mieru"


def test_mierus_link_imports_as_mieru_extended_outbound() -> None:
    nodes, errors = parse_links_text(
        "mierus://user:pass@example.com:27017?transport=TCP&multiplexing=MULTIPLEXING_LOW#secure-mieru"
    )

    assert errors == []
    assert nodes[0].scheme == "mieru"
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert outbound["type"] == "mieru"
    assert outbound["server"] == "example.com"
    assert outbound["server_port"] == 27017
    assert outbound["transport"] == "TCP"
    assert outbound["username"] == "user"
    assert outbound["password"] == "pass"
    assert outbound["multiplexing"] == "MULTIPLEXING_LOW"


def test_clash_yaml_imports_wireguard_and_amnezia_warp(tmp_path) -> None:
    config_path = tmp_path / "warp.yaml"
    config_path.write_text(
        ("# long generated header\n" * 300) + """
warp-common: &warp-common
  type: wireguard
  ip: 172.16.0.2/32
  ipv6: '2606:4700:110::2/128'
  private-key: private-key=
  public-key: public-key=
  allowed-ips: [0.0.0.0/0, '::/0']
  reserved: [14, 84, 156]
  persistent-keepalive: 25
  mtu: 1280

proxies:
  - name: WARP WG
    <<: *warp-common
    server: engage.cloudflareclient.com
    port: 2408
  - name: WARP AWG
    <<: *warp-common
    server: 162.159.192.1
    port: 500
    amnezia-wg-option:
      jc: 4
      jmin: 64
      jmax: 256
      h1: 1
      h2: 2
      h3: 3
      h4: 4
      i1: '<b 0x01020304>'
""".strip(),
        encoding="utf-8",
    )

    nodes, errors = parse_links_text(str(config_path))

    assert errors == []
    assert [node.scheme for node in nodes] == ["warp", "awg"]
    assert all("WARP" in node.tags for node in nodes)
    plain = build_singbox_outbound(nodes[0], tag="proxy")
    assert plain["type"] == "warp"
    assert plain["profile"]["private_key"] == "private-key="
    assert plain["reserved"] == [14, 84, 156]
    assert plain["persistent_keepalive_interval"] == 25
    amnezia = build_singbox_outbound(nodes[1], tag="proxy")["amnezia"]
    assert amnezia["jc"] == 4
    assert amnezia["i1"] == "<b 0x01020304>"

    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    runtime = plan_singbox_runtime(
        document,
        nodes[1],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
    ).singbox_config
    endpoint = runtime["endpoints"][0]
    assert "ip" not in endpoint
    assert "server" not in endpoint
    assert endpoint["type"] == "warp"
    assert runtime["experimental"]["cache_file"]["store_warp_config"] is True


def test_legacy_clash_wireguard_link_is_migrated_before_runtime() -> None:
    legacy_payload = {
        "name": "Legacy WARP",
        "type": "wireguard",
        "server": "engage.cloudflareclient.com",
        "port": 2408,
        "ip": "172.16.0.2/32",
        "private-key": "private-key=",
        "public-key": "public-key=",
        "allowed-ips": ["0.0.0.0/0", "::/0"],
        "reserved": [14, 84, 156],
    }
    nodes, errors = parse_links_text("proxies:\n  - " + json.dumps(legacy_payload))
    assert errors == []
    node = nodes[0]
    node.link = json.dumps(legacy_payload)
    node.outbound = {"protocol": "wireguard", "singbox": dict(legacy_payload)}

    assert repair_node_outbound_from_link(node) is True

    outbound = build_singbox_outbound(node, tag="proxy")
    assert node.scheme == "warp"
    assert "ip" not in outbound
    assert "server" not in outbound
    assert outbound["type"] == "warp"
    assert outbound["profile"]["private_key"] == "private-key="
    assert outbound["reserved"] == [14, 84, 156]


def test_awg_wgquick_config_normalizes_bare_addresses_and_awg20_fields() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = QGg8AFRn6qKfTB7cT3FWH1WGx3np+OKzlNuQUrqIBmI=
        Address = 172.16.0.2, 2606:4700:110:80b2::2
        DNS = 1.1.1.1, 1.0.0.1
        MTU = 1280
        Jc = 4
        Jmin = 40
        Jmax = 70
        S1 = 0
        S2 = 0
        S3 = 0
        S4 = 0
        H1 = 1
        H2 = 2
        H3 = 3
        H4 = 4
        I1 = <b 0x01020304>
        I5 = <b 0x05060708>
        J3 = <b 0x090a0b0c>
        Itime = 50
        Id = www.pochta.ru
        Ip = quic
        Ib = curl

        [Peer]
        PublicKey = 3nk7jdnkcL95Fc/z+GCiH7jOovEKhFkLIGPT+U/uLEQ=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = 162.159.192.1:8886
        PersistentKeepalive = 25
        """
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert nodes[0].scheme == "awg"
    assert outbound["type"] == "wireguard"
    assert outbound["address"] == ["172.16.0.2/32", "2606:4700:110:80b2::2/128"]
    assert "listen_port" not in outbound
    assert outbound["peers"][0]["persistent_keepalive_interval"] == 25
    assert outbound["amnezia"]["i1"] == "<b 0x01020304>"
    assert outbound["amnezia"]["i5"] == "<b 0x05060708>"
    assert outbound["amnezia"]["j3"] == "<b 0x090a0b0c>"
    assert outbound["amnezia"]["itime"] == 50
    assert nodes[0].outbound["_dns"] == ["1.1.1.1", "1.0.0.1"]
    assert nodes[0].outbound["_protocol_masking"] == {
        "id": "www.pochta.ru",
        "ip": "quic",
        "ib": "curl",
    }


def test_legacy_awg_wgquick_node_is_migrated_before_runtime() -> None:
    source = """
        [Interface]
        PrivateKey = private-key=
        Address = 172.16.0.2
        DNS = 1.1.1.1, 1.0.0.1
        MTU = 1280
        Jc = 4
        Jmin = 40
        Jmax = 70
        H1 = 1
        H2 = 2
        H3 = 3
        H4 = 4

        [Peer]
        PublicKey = public-key=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = 8.47.69.2:1074
        """
    nodes, errors = parse_links_text(source)
    assert errors == []
    node = nodes[0]
    native = node.outbound["singbox"]
    native["listen_port"] = 10000
    node.outbound.pop("_dns")

    assert repair_node_outbound_from_link(node) is True

    assert "listen_port" not in node.outbound["singbox"]
    assert node.outbound["_dns"] == ["1.1.1.1", "1.0.0.1"]


def test_awg_runtime_uses_profile_dns_and_endpoint_mtu() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = private-key=
        Address = 172.16.0.2, 2606:4700:110::2
        DNS = 1.1.1.1, 1.0.0.1
        MTU = 1280
        Jc = 4
        Jmin = 40
        Jmax = 70
        S1 = 0
        S2 = 0
        S3 = 0
        S4 = 0
        H1 = 1
        H2 = 2
        H3 = 3
        H4 = 4

        [Peer]
        PublicKey = public-key=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = 8.47.69.2:1074
        """
    )

    assert errors == []
    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    runtime = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
    ).singbox_config

    tun = next(item for item in runtime["inbounds"] if item.get("type") == "tun")
    proxy_dns = next(item for item in runtime["dns"]["servers"] if item.get("tag") == "proxy-dns")
    endpoint = next(item for item in runtime["endpoints"] if item.get("tag") == "proxy")
    assert tun["mtu"] == 1280
    assert proxy_dns == {
        "tag": "proxy-dns",
        "type": "udp",
        "server": "1.1.1.1",
        "server_port": 53,
        "detour": "proxy",
    }
    assert endpoint["peers"][0]["address"] == "8.47.69.2"


def test_wireguard_config_supports_multiple_peers() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = private-key=
        Address = 10.0.0.2
        ListenPort = 51821

        [Peer]
        PublicKey = first-public-key=
        AllowedIPs = 10.1.0.0/16
        Endpoint = first.example.com:51820

        [Peer]
        PublicKey = second-public-key=
        PresharedKey = shared-key=
        AllowedIPs = 10.2.0.1
        Endpoint = [2001:db8::1]:51822
        """
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert outbound["listen_port"] == 51821
    assert outbound["address"] == ["10.0.0.2/32"]
    assert len(outbound["peers"]) == 2
    assert outbound["peers"][1]["address"] == "2001:db8::1"
    assert outbound["peers"][1]["allowed_ips"] == ["10.2.0.1/32"]
    assert outbound["peers"][1]["pre_shared_key"] == "shared-key="


def test_cloudflare_wireguard_reserved_config_uses_native_warp_endpoint() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = private-key=
        Address = 172.16.0.2
        Reserved = DlSc

        [Peer]
        PublicKey = public-key=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = engage.cloudflareclient.com:2408
        PersistentKeepalive = 25
        """
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert nodes[0].scheme == "warp"
    assert outbound["type"] == "warp"
    assert outbound["reserved"] == [14, 84, 156]
    assert outbound["persistent_keepalive_interval"] == 25
    assert outbound["profile"]["private_key"] == "private-key="


def test_saved_wireguard_endpoint_is_normalized_again_before_runtime() -> None:
    nodes, errors = parse_links_text(
        json.dumps(
            {
                "type": "wireguard",
                "address": ["172.16.0.2", "2606:4700:110::2"],
                "private_key": "private-key=",
                "peers": [
                    {
                        "address": "162.159.192.1",
                        "port": 2408,
                        "public_key": "public-key=",
                        "allowed_ips": ["0.0.0.0", "::"],
                    }
                ],
            }
        )
    )

    assert errors == []
    outbound = build_singbox_outbound(nodes[0], tag="proxy")
    assert outbound["address"] == ["172.16.0.2/32", "2606:4700:110::2/128"]
    assert outbound["peers"][0]["allowed_ips"] == ["0.0.0.0/32", "::/128"]


def test_tuic_builds_native_proxy_runtime_without_xray_sidecar() -> None:
    nodes, errors = parse_links_text(
        "tuic://00000000-0000-0000-0000-000000000000:password@example.com:443"
        "?allow_insecure=1#tuic-proxy"
    )
    assert errors == []

    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    plan = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
        tun_mode=False,
    )

    assert plan.xray_sidecar is None
    assert not any(inbound.get("type") == "tun" for inbound in plan.singbox_config["inbounds"])
    proxy = next(outbound for outbound in plan.singbox_config["outbounds"] if outbound.get("tag") == "proxy")
    assert proxy["type"] == "tuic"


def test_insecure_trojan_builds_native_proxy_runtime_without_xray_sidecar() -> None:
    nodes, errors = parse_links_text(
        "trojan://password@example.com:443?security=tls&sni=example.com&allowInsecure=1#trojan-proxy"
    )
    assert errors == []

    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    plan = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
        tun_mode=False,
    )

    assert plan.xray_sidecar is None
    proxy = next(outbound for outbound in plan.singbox_config["outbounds"] if outbound.get("tag") == "proxy")
    assert proxy["type"] == "trojan"
    assert proxy["tls"]["insecure"] is True


def test_xray_verify_peer_names_remain_a_comma_separated_string() -> None:
    nodes, errors = parse_links_text(
        "trojan://password@example.com:443?security=tls&vcn=one.example.com,two.example.com#trojan-vcn"
    )

    assert errors == []
    tls = nodes[0].outbound["streamSettings"]["tlsSettings"]
    assert tls["verifyPeerCertByName"] == "one.example.com,two.example.com"


def test_full_provider_config_is_preserved_for_tun_runtime() -> None:
    provider_config = {
        "inbounds": [{"type": "mixed", "tag": "mixed-in", "listen_port": 7897}],
        "outbounds": [
            {"type": "direct", "tag": "direct"},
            {"type": "urltest", "tag": "auto", "outbounds": ["direct"], "use_all_providers": True},
        ],
        "providers": [
            {
                "type": "remote",
                "tag": "sub",
                "url": "https://example.com/sub.txt",
                "user_agent": "sing-box",
            }
        ],
        "route": {"final": "auto"},
        "dns": {"servers": [{"tag": "bootstrap-dns", "type": "udp", "server": "1.1.1.1"}]},
    }
    nodes, errors = parse_links_text(json.dumps(provider_config))
    assert errors == []
    assert nodes[0].scheme == "singbox_config"

    document = parse_singbox_document(Path("default.json"), json.dumps(_base_config()))
    planned = plan_singbox_runtime(
        document,
        nodes[0],
        routing=RoutingSettings(mode="global", tun_default_outbound="proxy"),
    ).singbox_config

    assert planned["providers"] == provider_config["providers"]
    assert any(outbound.get("tag") == "proxy" for outbound in planned["outbounds"])
