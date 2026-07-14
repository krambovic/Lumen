from __future__ import annotations

import json

from xray_fluent.routing_rule_import import parse_routing_rules


def test_imports_plain_and_clash_rules() -> None:
    rules = parse_routing_rules(
        "example.com|direct\n"
        "DOMAIN-SUFFIX,blocked.example,REJECT\n"
        "IP-CIDR,10.0.0.0/8,DIRECT,no-resolve\n"
    )

    assert rules == [
        ("example.com", "direct"),
        ("domain:blocked.example", "block"),
        ("10.0.0.0/8", "direct"),
    ]


def test_imports_v2ray_json_rules() -> None:
    payload = json.dumps(
        {
            "routing": {
                "rules": [
                    {"domain": ["full:one.example", "domain:two.example"], "outboundTag": "proxy"},
                    {"ip": ["192.0.2.0/24"], "outboundTag": "direct"},
                ]
            }
        }
    )

    assert parse_routing_rules(payload, suffix=".json") == [
        ("full:one.example", "proxy"),
        ("domain:two.example", "proxy"),
        ("192.0.2.0/24", "direct"),
    ]


def test_imports_clash_yaml_rules() -> None:
    payload = """
rules:
  - DOMAIN,one.example,DIRECT
  - DOMAIN-KEYWORD,ads,REJECT
"""

    assert parse_routing_rules(payload, suffix=".yaml") == [
        ("full:one.example", "direct"),
        ("keyword:ads", "block"),
    ]
