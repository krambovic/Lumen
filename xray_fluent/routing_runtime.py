from __future__ import annotations

import json
import ntpath
from copy import deepcopy
from ipaddress import ip_network
from pathlib import Path
from typing import Any

from .constants import (
    DATA_DIR,
    ROUTING_DIRECT,
    ROUTING_GLOBAL,
    SUBSCRIPTION_FETCHER_EXE_NAME,
)
from .geodata_resources import singbox_rule_set_path
from .models import AppSettings, RoutingSettings
from .process_presets import PROCESS_PRESETS_BY_ID
from .routing_presets import ROUTING_PRESET_BLOCKED, custom_domain_rules, preset_domain_rules
from .service_presets import SERVICE_PRESETS_BY_ID

_SINGBOX_RULE_SET_DIR = DATA_DIR / "runtime" / "sing-box-rule-sets"
_SINGBOX_RULE_SET_SOURCES = {
    "geosite:lumen-exclude": {
        "domains": ("zapret/lists/list-exclude.txt",),
        "domain_suffix": (),
        "ips": (),
    },
    "geoip:lumen-exclude": {
        "domains": (),
        "domain_suffix": (),
        "ips": ("zapret/lists/ipset-exclude.txt",),
    },
    "geosite:category-ru": {
        "domains": (),
        "domain_suffix": ("ru", "su", "рф"),
        "ips": (),
    },
    "geosite:category-media-ru-blocked": {
        "domains": (
            "zapret/lists/discord.txt",
            "zapret/lists/google.txt",
            "zapret/lists/list-general.txt",
            "zapret/lists/list-google.txt",
            "zapret/lists/russia-discord.txt",
            "zapret/lists/russia-youtube.txt",
            "zapret/lists/youtube.txt",
        ),
        "domain_suffix": (),
        "ips": (),
    },
    "geosite:ru-blocked": {
        "domains": (
            "zapret/lists/list-general.txt",
            "zapret/lists/list-google.txt",
        ),
        "domain_suffix": (),
        "ips": (),
    },
    "geoip:ru": {
        "domains": (),
        "domain_suffix": (),
        "ips": ("zapret/lists/ipset-ru.txt",),
    },
    "geoip:ru-blocked": {
        "domains": (),
        "domain_suffix": (),
        "ips": (
            "zapret/lists/ipset-all.txt",
            "zapret/lists/ipset-zapretkvn.txt",
            "zapret/lists/russia-discord-ipset.txt",
        ),
    },
    "geoip:ru-blocked-community": {
        "domains": (),
        "domain_suffix": (),
        "ips": (
            "zapret/lists/ipset-all.txt",
            "zapret/lists/ipset-zapretkvn.txt",
            "zapret/lists/russia-discord-ipset.txt",
        ),
    },
}
_SINGBOX_MANAGED_RULE_SET_TAGS = {key.replace(":", "-") for key in _SINGBOX_RULE_SET_SOURCES}
_BROWSER_DOH_DOMAIN_SUFFIXES = [
    "cloudflare-dns.com",
    "dns.adguard.com",
    "dns.alidns.com",
    "dns.cloudflare.com",
    "dns.google",
    "dns.nextdns.io",
    "dns.quad9.net",
    "doh.dns.sb",
    "doh.opendns.com",
    "doh.pub",
    "doh.sb",
    "mozilla.cloudflare-dns.com",
    "one.one.one.one",
]

_XRAY_UNSUPPORTED_GEOIP_CODES = {
    "geoip:lumen-exclude",
    "geoip:ru-blocked",
    "geoip:ru-blocked-community",
}
_XRAY_UNSUPPORTED_GEOSITE_CODES = {"geosite:lumen-exclude"}
_XRAY_EXPANDED_GEOIP_CACHE: dict[str, tuple[str, ...]] = {}
_XRAY_EXPANDED_GEOSITE_CACHE: dict[str, tuple[str, ...]] = {}


def _routing_final_outbound(routing: RoutingSettings, *, use_rule_default: bool = True) -> str:
    if routing.mode == ROUTING_GLOBAL:
        return "proxy"
    if routing.mode == ROUTING_DIRECT:
        return "direct"
    if not use_rule_default:
        return "direct"
    # The removed "TUN default" UI setting is no longer user-controlled.
    # Custom/rule routing falls back to proxy. The built-in "blocked only"
    # preset is the deliberate exception: unmatched traffic must stay direct.
    if str(routing.preset_id).strip().lower() == ROUTING_PRESET_BLOCKED:
        return "direct"
    return "proxy"


def split_xray_domain_ip(items: list[str]) -> tuple[list[str], list[str]]:
    domains: list[str] = []
    ips: list[str] = []
    for raw in items:
        value = raw.strip()
        if not value:
            continue
        lowered = value.lower()
        if lowered in _XRAY_UNSUPPORTED_GEOSITE_CODES:
            domains.extend(_expand_xray_local_geosite(lowered))
            continue
        if value.startswith(("domain:", "full:", "regexp:", "keyword:", "geosite:", "ext:")):
            domains.append(value)
            continue
        if value.startswith(("geoip:", "ip:")):
            if lowered in _XRAY_UNSUPPORTED_GEOIP_CODES:
                ips.extend(_expand_xray_unsupported_geoip(lowered))
                continue
            ips.append(value)
            continue
        try:
            ip_network(value, strict=False)
            ips.append(value)
            continue
        except ValueError:
            domains.append(f"domain:{value}")
    return domains, ips


def append_xray_domain_ip_rule(rules: list[dict[str, Any]], items: list[str], outbound_tag: str) -> None:
    domains, ips = split_xray_domain_ip(items)
    if domains:
        rules.append({"type": "field", "domain": sorted(set(domains)), "outboundTag": outbound_tag})
    if ips:
        rules.append({"type": "field", "ip": sorted(set(ips)), "outboundTag": outbound_tag})


def _expand_xray_unsupported_geoip(value: str) -> tuple[str, ...]:
    cached = _XRAY_EXPANDED_GEOIP_CACHE.get(value)
    if cached is not None:
        return cached
    source = _SINGBOX_RULE_SET_SOURCES.get(value)
    if not source:
        _XRAY_EXPANDED_GEOIP_CACHE[value] = ()
        return ()
    expanded = tuple(_read_ip_lists(tuple(source.get("ips") or ())))
    _XRAY_EXPANDED_GEOIP_CACHE[value] = expanded
    return expanded


def _expand_xray_local_geosite(value: str) -> tuple[str, ...]:
    cached = _XRAY_EXPANDED_GEOSITE_CACHE.get(value)
    if cached is not None:
        return cached
    source = _SINGBOX_RULE_SET_SOURCES.get(value)
    if not source:
        _XRAY_EXPANDED_GEOSITE_CACHE[value] = ()
        return ()
    domains = _read_domain_lists(tuple(source.get("domains") or ()))
    suffixes = [
        str(item).strip().lstrip(".").lower()
        for item in source.get("domain_suffix") or ()
        if str(item).strip()
    ]
    expanded = tuple(f"domain:{item}" for item in sorted(set(domains + suffixes)))
    _XRAY_EXPANDED_GEOSITE_CACHE[value] = expanded
    return expanded


def resolve_xray_process_name(rule: dict[str, str]) -> str:
    value = str(rule.get("process", "")).strip()
    if not value:
        return ""
    match = str(rule.get("match", "")).strip().lower()
    if match == "path_regex":
        return ""
    if match == "path" or "\\" in value or "/" in value or (len(value) > 1 and value[1] == ":"):
        return ntpath.basename(value)
    return value


def append_xray_process_rule(rules: list[dict[str, Any]], processes: list[str], action: str) -> None:
    names = sorted({name.strip() for name in processes if name.strip()})
    if not names:
        return
    outbound = action if action in ("direct", "proxy", "block") else "direct"
    rules.append({"type": "field", "process": names, "network": "tcp,udp", "outboundTag": outbound})


def _collect_service_route_domains(routing: RoutingSettings) -> tuple[list[str], list[str], list[str]]:
    service_direct: list[str] = []
    service_proxy: list[str] = []
    service_block: list[str] = []
    for svc_id, raw_action in routing.service_routes.items():
        action = str(raw_action or "").strip().lower()
        if action not in {"direct", "proxy"}:
            continue
        preset = SERVICE_PRESETS_BY_ID.get(svc_id)
        if not preset:
            continue
        if action == "direct":
            service_direct.extend(preset.domains)
        else:
            service_proxy.extend(preset.domains)
    return service_direct, service_proxy, service_block


def build_xray_gui_routing_rules(routing: RoutingSettings, settings: AppSettings) -> list[dict[str, Any]]:
    rules: list[dict[str, Any]] = []
    if routing.bypass_lan:
        rules.append({"type": "field", "ip": ["geoip:private"], "outboundTag": "direct"})
        rules.append({"type": "field", "domain": ["geosite:private"], "outboundTag": "direct"})

    if not settings.tun_mode:
        preset_processes: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
        for preset_id, action in routing.process_preset_routes.items():
            preset = PROCESS_PRESETS_BY_ID.get(preset_id)
            if preset and action in preset_processes:
                preset_processes[action].extend(preset.processes)
        for action, processes in preset_processes.items():
            append_xray_process_rule(rules, processes, action)

        manual_processes: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
        for pr in routing.process_rules:
            name = resolve_xray_process_name(pr)
            action = pr.get("action", "direct")
            if name and action in manual_processes:
                manual_processes[action].append(name)
        for action, processes in manual_processes.items():
            append_xray_process_rule(rules, processes, action)

    service_direct, service_proxy, service_block = _collect_service_route_domains(routing)
    preset_direct, preset_proxy = preset_domain_rules(routing.preset_id)

    append_xray_domain_ip_rule(rules, service_direct, "direct")
    append_xray_domain_ip_rule(rules, custom_domain_rules(routing.direct_domains), "direct")
    append_xray_domain_ip_rule(rules, service_block, "block")
    append_xray_domain_ip_rule(rules, custom_domain_rules(routing.block_domains), "block")
    append_xray_domain_ip_rule(rules, service_proxy, "proxy")
    append_xray_domain_ip_rule(rules, custom_domain_rules(routing.proxy_domains), "proxy")
    append_xray_domain_ip_rule(rules, preset_direct, "direct")
    append_xray_domain_ip_rule(rules, preset_proxy, "proxy")

    rules.append(
        {
            "type": "field",
            "network": "tcp,udp",
            "outboundTag": _routing_final_outbound(routing, use_rule_default=settings.tun_mode),
        }
    )
    return rules


def apply_xray_gui_routing(payload: dict[str, Any], routing: RoutingSettings, settings: AppSettings) -> None:
    route = payload.setdefault("routing", {})
    if not isinstance(route, dict):
        route = {}
        payload["routing"] = route
    rules = route.setdefault("rules", [])
    if not isinstance(rules, list):
        rules = []
        route["rules"] = rules
    rules[:] = [rule for rule in rules if not _is_legacy_lumen_xray_route_rule(rule)]
    insert_at = _xray_runtime_rule_insert_index(rules)
    rules[insert_at:insert_at] = build_xray_gui_routing_rules(routing, settings)


def _is_legacy_lumen_xray_route_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    outbound = str(rule.get("outboundTag") or "")
    domains = {str(item) for item in rule.get("domain") or []} if isinstance(rule.get("domain"), list) else set()
    ips = {str(item) for item in rule.get("ip") or []} if isinstance(rule.get("ip"), list) else set()
    processes = {str(item).lower() for item in rule.get("process") or []} if isinstance(rule.get("process"), list) else set()
    network = str(rule.get("network") or "")
    port = str(rule.get("port") or "")

    if outbound == "block" and network == "udp" and port == "443":
        return True
    if outbound == "direct" and (ips == {"geoip:private"} or domains == {"geosite:private"}):
        return True
    if outbound == "direct" and (ips == {"geoip:ru"} or domains == {"geosite:category-ru"}):
        return True
    if outbound == "proxy" and network == "tcp,udp" and not domains and not ips and not processes:
        return True
    legacy_domains = {
        "regexp:(^|\\.)facebook\\.com$",
        "regexp:(^|\\.)fbcdn\\.net$",
        "regexp:(^|\\.)instagram\\.com$",
        "full:ntc.party",
        "full:rutracker.org",
        "full:static.rutracker.cc",
    }
    if outbound == "proxy" and domains == legacy_domains:
        return True
    legacy_proxy_processes = {
        "telegram.exe",
        "ayugram.exe",
        "discord.exe",
        "vesktop.exe",
        "spotify.exe",
        "chrome.exe",
        "firefox.exe",
        "waterfox.exe",
        "librewolf.exe",
        "msedge.exe",
        "opera.exe",
        "brave.exe",
        "vivaldi.exe",
        "browser.exe",
    }
    return outbound == "proxy" and bool(processes) and processes.issubset(legacy_proxy_processes)


def _xray_runtime_rule_insert_index(rules: list[Any]) -> int:
    index = 0
    while index < len(rules):
        rule = rules[index]
        inbound = rule.get("inboundTag") if isinstance(rule, dict) else None
        if not isinstance(inbound, list):
            break
        inbound_text = {str(item) for item in inbound}
        if any(tag.startswith("__app_") or tag in {"api", "discord-socks-in"} for tag in inbound_text):
            index += 1
            continue
        break
    return index


def apply_singbox_gui_routing(payload: dict[str, Any], routing: RoutingSettings) -> None:
    route = payload.setdefault("route", {})
    if not isinstance(route, dict):
        route = {}
        payload["route"] = route
    rules = route.setdefault("rules", [])
    if not isinstance(rules, list):
        rules = []
        route["rules"] = rules

    rules[:] = [
        rule
        for rule in rules
        if not _is_legacy_lumen_singbox_route_rule(rule)
        and not _is_subscription_fetcher_direct_rule(rule)
    ]
    gui_rules, route_rule_sets = build_singbox_gui_route_rules(routing)
    dns_rules, dns_rule_sets = build_singbox_gui_dns_rules(routing)
    _ensure_singbox_rule_sets(route, route_rule_sets | dns_rule_sets)
    insert_at = _singbox_runtime_rule_insert_index(rules)
    rules[insert_at:insert_at] = [_subscription_fetcher_direct_rule(), *gui_rules]
    final_outbound = _routing_final_outbound(routing)
    route["final"] = final_outbound
    dns = payload.get("dns")
    if isinstance(dns, dict):
        dns_tags = {
            str(server.get("tag") or "")
            for server in dns.get("servers") or []
            if isinstance(server, dict)
        }
        target_dns = "bootstrap-dns" if final_outbound == "direct" else "proxy-dns"
        target_strategy = (
            routing.dns_bootstrap_strategy if final_outbound == "direct" else routing.dns_proxy_strategy
        )
        target_strategy = _singbox_dns_strategy(target_strategy)
        if target_dns in dns_tags:
            dns["final"] = target_dns
            route["default_domain_resolver"] = {"server": target_dns, "strategy": target_strategy}
        existing_dns_rules = dns.setdefault("rules", [])
        if not isinstance(existing_dns_rules, list):
            existing_dns_rules = []
            dns["rules"] = existing_dns_rules
        existing_dns_rules[:] = [
            rule for rule in existing_dns_rules if not _is_lumen_singbox_dns_rule(rule)
        ]
        if "fake-dns" in dns_tags:
            existing_dns_rules.insert(
                0,
                {
                    "query_type": ["A", "AAAA"],
                    "server": "fake-dns",
                    "rewrite_ttl": 1,
                },
            )
        if dns_rules:
            existing_dns_rules[0:0] = dns_rules
        # Keep the HTTPS/SVCB (type 64/65) reject ahead of every domain rule so
        # ECH config and HTTP/3 hints are never answered for routed domains.
        existing_dns_rules[:] = [
            rule for rule in existing_dns_rules if not _is_https_dns_reject_rule(rule)
        ]
        existing_dns_rules.insert(0, {"query_type": ["HTTPS", "SVCB"], "action": "reject"})
        existing_dns_rules.insert(
            1,
            {
                "domain_suffix": list(_BROWSER_DOH_DOMAIN_SUFFIXES),
                "action": "reject",
            },
        )


def _subscription_fetcher_direct_rule() -> dict[str, Any]:
    return {
        "process_name": [SUBSCRIPTION_FETCHER_EXE_NAME],
        "action": "route",
        "outbound": "direct",
    }


def _is_subscription_fetcher_direct_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    names = {
        str(value or "").strip().casefold()
        for value in (rule.get("process_name") or [])
    }
    return (
        names == {SUBSCRIPTION_FETCHER_EXE_NAME.casefold()}
        and rule.get("action") == "route"
        and rule.get("outbound") == "direct"
    )


def build_singbox_gui_route_rules(routing: RoutingSettings) -> tuple[list[dict[str, Any]], set[str]]:
    rules: list[dict[str, Any]] = []
    rule_sets: set[str] = set()

    preset_processes: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
    for preset_id, action in routing.process_preset_routes.items():
        preset = PROCESS_PRESETS_BY_ID.get(preset_id)
        if preset and action in preset_processes:
            preset_processes[action].extend(preset.processes)
    for action, processes in preset_processes.items():
        _append_singbox_process_rule(rules, processes, action)

    manual_names: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
    manual_paths: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
    manual_regex: dict[str, list[str]] = {"direct": [], "proxy": [], "block": []}
    for pr in routing.process_rules:
        action = pr.get("action", "direct")
        if action not in manual_names:
            continue
        value = str(pr.get("process", "")).strip()
        if not value:
            continue
        match = str(pr.get("match", "")).strip().lower()
        if match == "path_regex":
            manual_regex[action].append(value)
        elif match == "path" or "\\" in value or "/" in value or (len(value) > 1 and value[1] == ":"):
            manual_paths[action].append(value)
        else:
            manual_names[action].append(value)
    for action in ("direct", "proxy", "block"):
        _append_singbox_process_rule(rules, manual_names[action], action)
        _append_singbox_process_rule(rules, manual_paths[action], action, key="process_path")
        _append_singbox_process_rule(rules, manual_regex[action], action, key="process_path_regex")

    service_direct, service_proxy, service_block = _collect_service_route_domains(routing)
    preset_direct, preset_proxy = preset_domain_rules(routing.preset_id)

    for items, outbound in (
        (service_direct, "direct"),
        (custom_domain_rules(routing.direct_domains), "direct"),
        (service_block, "block"),
        (custom_domain_rules(routing.block_domains), "block"),
        (service_proxy, "proxy"),
        (custom_domain_rules(routing.proxy_domains), "proxy"),
        (preset_direct, "direct"),
        (preset_proxy, "proxy"),
    ):
        rule, used_sets = _singbox_domain_ip_rule(items, outbound)
        if rule:
            rules.append(rule)
            rule_sets.update(used_sets)

    if routing.dns_fake_enabled:
        rules.append(
            {
                "ip_cidr": ["198.18.0.0/15", "fc00::/18"],
                "action": "route",
                "outbound": _routing_final_outbound(routing),
            }
        )

    if routing.bypass_lan:
        rules.append({"ip_is_private": True, "action": "route", "outbound": "direct"})
    return rules, rule_sets


def build_singbox_gui_dns_rules(routing: RoutingSettings) -> tuple[list[dict[str, Any]], set[str]]:
    rules: list[dict[str, Any]] = []
    rule_sets: set[str] = set()

    service_direct, service_proxy, service_block = _collect_service_route_domains(routing)
    if routing.dns_geo_check:
        preset_direct, preset_proxy = preset_domain_rules(routing.preset_id)
    else:
        preset_direct, preset_proxy = [], []

    for items, dns_action in (
        (service_direct, "bootstrap-dns"),
        (custom_domain_rules(routing.direct_domains), "bootstrap-dns"),
        (service_block, "reject"),
        (custom_domain_rules(routing.block_domains), "reject"),
        (service_proxy, "proxy-dns"),
        (custom_domain_rules(routing.proxy_domains), "proxy-dns"),
        (preset_direct, "bootstrap-dns"),
        (preset_proxy, "proxy-dns"),
    ):
        rule, used_sets = _singbox_domain_dns_rule(items, dns_action)
        if rule:
            rules.append(rule)
            rule_sets.update(used_sets)
    return rules, rule_sets


def _is_legacy_lumen_singbox_route_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    network = rule.get("network")
    if isinstance(network, list):
        is_udp = {str(item).lower() for item in network} == {"udp"}
    else:
        is_udp = str(network or "").lower() == "udp"
    port = rule.get("port")
    if isinstance(port, list):
        is_443 = {str(item) for item in port} == {"443"}
    else:
        is_443 = str(port or "") == "443"
    outbound = str(rule.get("outbound") or "")
    if is_udp and is_443 and outbound == "block":
        return True
    if outbound not in {"direct", "proxy", "block"}:
        return False
    rule_set = rule.get("rule_set")
    if isinstance(rule_set, str):
        if rule_set in _SINGBOX_MANAGED_RULE_SET_TAGS:
            return True
    elif isinstance(rule_set, list) and any(str(item) in _SINGBOX_MANAGED_RULE_SET_TAGS for item in rule_set):
        return True
    if rule.get("ip_is_private") is True and outbound == "direct":
        return True
    generated_keys = {
        "domain_suffix",
        "domain",
        "domain_keyword",
        "domain_regex",
        "ip_cidr",
        "process_name",
        "process_path",
        "process_path_regex",
    }
    allowed_keys = generated_keys | {"rule_set", "ip_is_private", "action", "outbound"}
    return bool(generated_keys & set(rule)) and set(rule).issubset(allowed_keys)


def _is_lumen_singbox_dns_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    if _is_https_dns_reject_rule(rule):
        return True
    if _is_browser_doh_dns_reject_rule(rule):
        return True
    if (
        str(rule.get("server") or "") == "fake-dns"
        and set(str(item) for item in rule.get("query_type") or []) == {"A", "AAAA"}
        and set(rule).issubset({"query_type", "server", "rewrite_ttl", "action"})
    ):
        return True
    action = str(rule.get("action") or "")
    if action not in {"route", "reject"}:
        return False
    server = str(rule.get("server") or "")
    if action == "route" and server not in {"bootstrap-dns", "proxy-dns"}:
        return False
    rule_set = rule.get("rule_set")
    if isinstance(rule_set, str):
        if rule_set in _SINGBOX_MANAGED_RULE_SET_TAGS:
            return True
    elif isinstance(rule_set, list) and any(str(item) in _SINGBOX_MANAGED_RULE_SET_TAGS for item in rule_set):
        return True
    generated_keys = {
        "domain_suffix",
        "domain",
        "domain_keyword",
        "domain_regex",
    }
    allowed_keys = generated_keys | {"rule_set", "action", "server"}
    return bool(generated_keys & set(rule)) and set(rule).issubset(allowed_keys)


def _is_browser_doh_dns_reject_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    if str(rule.get("action") or "") != "reject":
        return False
    suffixes = rule.get("domain_suffix")
    if not isinstance(suffixes, list):
        return False
    return [str(item) for item in suffixes] == list(_BROWSER_DOH_DOMAIN_SUFFIXES)


def _is_https_dns_reject_rule(rule: Any) -> bool:
    if not isinstance(rule, dict):
        return False
    if str(rule.get("action") or "") != "reject":
        return False
    query_type = rule.get("query_type")
    if not isinstance(query_type, list):
        return False
    return {str(item).strip().upper() for item in query_type} == {"HTTPS", "SVCB"}


def _singbox_dns_strategy(value: str) -> str:
    strategy = str(value or "").strip().lower().replace("-", "_")
    return strategy if strategy in {"prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"} else "prefer_ipv4"


def routing_with_ip_preference(routing: RoutingSettings, *, prefer_ipv6: bool = False) -> RoutingSettings:
    effective = deepcopy(routing)
    preferred_strategy = "prefer_ipv6" if prefer_ipv6 else "prefer_ipv4"
    for attr in ("dns_bootstrap_strategy", "dns_proxy_strategy"):
        current = _singbox_dns_strategy(str(getattr(effective, attr, "")))
        if current in {"prefer_ipv4", "prefer_ipv6"}:
            setattr(effective, attr, preferred_strategy)
    return effective


def _append_singbox_process_rule(
    rules: list[dict[str, Any]],
    values: list[str],
    action: str,
    *,
    key: str = "process_name",
) -> None:
    names = sorted({value.strip() for value in values if value.strip()})
    if names:
        outbound = action if action in ("direct", "proxy", "block") else "direct"
        rules.append({key: names, "action": "route", "outbound": outbound})


def _singbox_domain_ip_rule(items: list[str], outbound: str) -> tuple[dict[str, Any] | None, set[str]]:
    domain_suffix: list[str] = []
    domain: list[str] = []
    domain_keyword: list[str] = []
    domain_regex: list[str] = []
    ip_cidr: list[str] = []
    rule_set: list[str] = []

    for raw in items:
        value = raw.strip()
        if not value:
            continue
        mapped = _singbox_rule_set_tag(value)
        if mapped:
            rule_set.append(mapped)
            continue
        if value.startswith("domain:"):
            domain_suffix.append(value.removeprefix("domain:"))
        elif value.startswith("full:"):
            domain.append(value.removeprefix("full:"))
        elif value.startswith("keyword:"):
            domain_keyword.append(value.removeprefix("keyword:"))
        elif value.startswith("regexp:"):
            domain_regex.append(value.removeprefix("regexp:"))
        elif value.startswith("ip:"):
            ip_cidr.append(value.removeprefix("ip:"))
        else:
            try:
                ip_network(value, strict=False)
                ip_cidr.append(value)
            except ValueError:
                domain_suffix.append(value)

    rule: dict[str, Any] = {}
    if domain_suffix:
        rule["domain_suffix"] = sorted(set(domain_suffix))
    if domain:
        rule["domain"] = sorted(set(domain))
    if domain_keyword:
        rule["domain_keyword"] = sorted(set(domain_keyword))
    if domain_regex:
        rule["domain_regex"] = sorted(set(domain_regex))
    if ip_cidr:
        rule["ip_cidr"] = sorted(set(ip_cidr))
    if rule_set:
        rule["rule_set"] = sorted(set(rule_set))
    if not rule:
        return None, set()
    rule["action"] = "route"
    rule["outbound"] = outbound
    return rule, set(rule_set)


def _singbox_domain_dns_rule(items: list[str], dns_action: str) -> tuple[dict[str, Any] | None, set[str]]:
    domain_suffix: list[str] = []
    domain: list[str] = []
    domain_keyword: list[str] = []
    domain_regex: list[str] = []
    rule_set: list[str] = []

    for raw in items:
        value = raw.strip()
        if not value:
            continue
        mapped = _singbox_dns_rule_set_tag(value)
        if mapped:
            rule_set.append(mapped)
            continue
        if value.startswith("domain:"):
            domain_suffix.append(value.removeprefix("domain:"))
        elif value.startswith("full:"):
            domain.append(value.removeprefix("full:"))
        elif value.startswith("keyword:"):
            domain_keyword.append(value.removeprefix("keyword:"))
        elif value.startswith("regexp:"):
            domain_regex.append(value.removeprefix("regexp:"))
        elif value.startswith(("ip:", "geoip:")):
            continue
        else:
            try:
                ip_network(value, strict=False)
                continue
            except ValueError:
                domain_suffix.append(value)

    rule: dict[str, Any] = {}
    if domain_suffix:
        rule["domain_suffix"] = sorted(set(domain_suffix))
    if domain:
        rule["domain"] = sorted(set(domain))
    if domain_keyword:
        rule["domain_keyword"] = sorted(set(domain_keyword))
    if domain_regex:
        rule["domain_regex"] = sorted(set(domain_regex))
    if rule_set:
        rule["rule_set"] = sorted(set(rule_set))
    if not rule:
        return None, set()
    if dns_action == "reject":
        rule["action"] = "reject"
    else:
        rule["action"] = "route"
        rule["server"] = dns_action
    return rule, set(rule_set)


def _singbox_rule_set_tag(value: str) -> str:
    if value not in _SINGBOX_RULE_SET_SOURCES:
        return ""
    return value.replace(":", "-")


def _singbox_dns_rule_set_tag(value: str) -> str:
    source = _SINGBOX_RULE_SET_SOURCES.get(value)
    if not source:
        return ""
    if source.get("domains") or source.get("domain_suffix"):
        return value.replace(":", "-")
    return ""


def _ensure_singbox_rule_sets(route: dict[str, Any], tags: set[str]) -> None:
    existing = route.setdefault("rule_set", [])
    if not isinstance(existing, list):
        existing = []
        route["rule_set"] = existing

    existing[:] = [
        item
        for item in existing
        if not (isinstance(item, dict) and str(item.get("tag") or "") in _SINGBOX_MANAGED_RULE_SET_TAGS)
    ]
    if not tags:
        return

    reverse_keys = {key.replace(":", "-"): key for key in _SINGBOX_RULE_SET_SOURCES}
    for tag in sorted(tags):
        key = reverse_keys.get(tag)
        if not key:
            continue
        binary_path = singbox_rule_set_path(key)
        if binary_path is not None and binary_path.is_file():
            path = binary_path
            rule_format = "binary"
        else:
            path = _ensure_singbox_local_rule_set(key)
            rule_format = "source"
        existing.append(
            {
                "type": "local",
                "tag": tag,
                "format": rule_format,
                "path": str(path),
            }
        )


def _ensure_singbox_local_rule_set(key: str) -> Path:
    source = _SINGBOX_RULE_SET_SOURCES[key]
    tag = key.replace(":", "-")
    path = _SINGBOX_RULE_SET_DIR / f"{tag}.json"
    domains = _read_domain_lists(source.get("domains") or ())
    domain_suffix = sorted(
        {
            value.strip().lstrip(".").lower()
            for value in source.get("domain_suffix") or ()
            if str(value).strip()
        }
    )
    ips = _read_ip_lists(source.get("ips") or ())

    rules: list[dict[str, Any]] = []
    if domains:
        rules.append({"domain_suffix": domains})
    if domain_suffix:
        rules.append({"domain_suffix": domain_suffix})
    if ips:
        rules.append({"ip_cidr": ips})
    if not rules:
        rules.append({"domain_suffix": [tag]})

    payload = {"version": 3, "rules": rules}
    _SINGBOX_RULE_SET_DIR.mkdir(parents=True, exist_ok=True)
    text = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    if not path.exists() or path.read_text(encoding="utf-8", errors="ignore") != text:
        path.write_text(text, encoding="utf-8")
    return path


def _read_domain_lists(paths: tuple[str, ...]) -> list[str]:
    values: set[str] = set()
    for relative in paths:
        path = DATA_DIR.parent / relative
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
            value = _normalize_list_entry(line)
            if not value:
                continue
            try:
                ip_network(value, strict=False)
                continue
            except ValueError:
                values.add(value.lstrip(".").lower())
    return sorted(values)


def _read_ip_lists(paths: tuple[str, ...]) -> list[str]:
    values: set[str] = set()
    for relative in paths:
        path = DATA_DIR.parent / relative
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
            value = _normalize_list_entry(line)
            if not value:
                continue
            try:
                values.add(str(ip_network(value, strict=False)))
            except ValueError:
                continue
    return sorted(values)


def _normalize_list_entry(line: str) -> str:
    value = line.split("#", 1)[0].strip()
    if not value:
        return ""
    if value.startswith(("domain:", "full:", "keyword:", "regexp:", "ip:")):
        value = value.split(":", 1)[1].strip()
    if value.startswith("||"):
        value = value[2:]
    value = value.strip("|^,; ")
    return value


def _singbox_runtime_rule_insert_index(rules: list[Any]) -> int:
    index = 0
    while index < len(rules):
        rule = rules[index]
        if not isinstance(rule, dict):
            break
        if rule.get("action") in {"sniff", "hijack-dns", "route-options"}:
            index += 1
            continue
        if rule.get("inbound"):
            index += 1
            continue
        break
    return index
