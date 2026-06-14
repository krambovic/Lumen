from __future__ import annotations

from xray_fluent.country_flags import get_flag_emoji
from xray_fluent.link_parser import parse_links_text


def test_flag_emoji_uses_real_regional_indicators() -> None:
    assert get_flag_emoji("US") == "🇺🇸"
    assert get_flag_emoji("GB") == "🇬🇧"
    assert get_flag_emoji("FI") == "🇫🇮"
    assert get_flag_emoji("de") == "🇩🇪"
    assert get_flag_emoji("USA") == ""


def test_amnezia_warp_config_gets_awg_warp_name() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=
        Address = 172.16.0.2/32
        Jc = 4
        Jmin = 40
        Jmax = 70
        S1 = 0
        S2 = 0
        H1 = 1
        H2 = 2
        H3 = 3
        H4 = 4

        [Peer]
        PublicKey = bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = engage.cloudflareclient.com:2408
        """
    )

    assert errors == []
    assert len(nodes) == 1
    assert nodes[0].scheme == "awg"
    assert nodes[0].name.startswith("AWG/WARP ")


def test_plain_cloudflare_wireguard_config_gets_warp_name() -> None:
    nodes, errors = parse_links_text(
        """
        [Interface]
        PrivateKey = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=
        Address = 172.16.0.2/32

        [Peer]
        PublicKey = bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb=
        AllowedIPs = 0.0.0.0/0, ::/0
        Endpoint = engage.cloudflareclient.com:2408
        """
    )

    assert errors == []
    assert len(nodes) == 1
    assert nodes[0].scheme == "wireguard"
    assert nodes[0].name.startswith("WARP ")
