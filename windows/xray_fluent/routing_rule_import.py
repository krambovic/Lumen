from __future__ import annotations

import json
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - optional in source-only environments
    yaml = None


_ACTIONS = {
    "direct": "direct",
    "freedom": "direct",
    "proxy": "proxy",
    "vpn": "proxy",
    "reject": "block",
    "block": "block",
    "blocked": "block",
    "blackhole": "block",
}


def _action(value: Any) -> str:
    normalized = str(value or "proxy").strip().lower()
    return _ACTIONS.get(normalized, "proxy")


def _append(result: list[tuple[str, str]], address: Any, action: Any) -> None:
    value = str(address or "").strip()
    if value:
        result.append((value, _action(action)))


def _parse_clash_rule(value: str) -> tuple[str, str] | None:
    parts = [part.strip() for part in value.split(",")]
    if len(parts) < 2:
        return None
    kind = parts[0].upper()
    action = parts[2] if len(parts) >= 3 else "proxy"
    target = parts[1]
    prefixes = {
        "DOMAIN": "full:",
        "DOMAIN-SUFFIX": "domain:",
        "DOMAIN-KEYWORD": "keyword:",
        "DOMAIN-REGEX": "regexp:",
        "GEOSITE": "geosite:",
        "GEOIP": "geoip:",
    }
    if kind in prefixes:
        return f"{prefixes[kind]}{target}", _action(action)
    if kind in {"IP-CIDR", "IP-CIDR6"}:
        return target, _action(action)
    return None


def _parse_rule_object(value: dict[str, Any], result: list[tuple[str, str]]) -> None:
    action = value.get("action") or value.get("outboundTag") or value.get("outbound")
    for key in ("address", "domain", "domains", "ip", "ips"):
        addresses = value.get(key)
        if isinstance(addresses, str):
            _append(result, addresses, action)
        elif isinstance(addresses, list):
            for address in addresses:
                _append(result, address, action)


def _parse_structured(payload: Any) -> list[tuple[str, str]]:
    result: list[tuple[str, str]] = []
    if isinstance(payload, list):
        for item in payload:
            if isinstance(item, str):
                parsed = _parse_clash_rule(item)
                if parsed:
                    result.append(parsed)
                else:
                    _append(result, item, "proxy")
            elif isinstance(item, dict):
                _parse_rule_object(item, result)
        return result
    if not isinstance(payload, dict):
        return result

    for key, action in (("direct", "direct"), ("proxy", "proxy"), ("block", "block")):
        values = payload.get(key)
        if isinstance(values, list):
            for value in values:
                _append(result, value, action)

    rules = payload.get("rules")
    if rules is None and isinstance(payload.get("routing"), dict):
        rules = payload["routing"].get("rules")
    if isinstance(rules, list):
        result.extend(_parse_structured(rules))
    return result


def parse_routing_rules(text: str, *, suffix: str = "") -> list[tuple[str, str]]:
    source = str(text or "").strip()
    if not source:
        return []

    extension = suffix.strip().lower()
    structured: Any = None
    if extension == ".json" or source[:1] in {"{", "["}:
        try:
            structured = json.loads(source)
        except (TypeError, ValueError):
            if extension == ".json":
                raise ValueError("Некорректный JSON-файл правил") from None
    elif extension in {".yaml", ".yml"}:
        if yaml is None:
            raise ValueError("Для импорта YAML требуется PyYAML")
        try:
            structured = yaml.safe_load(source)
        except Exception as exc:
            raise ValueError(f"Некорректный YAML-файл правил: {exc}") from exc

    if structured is not None:
        return _parse_structured(structured)

    result: list[tuple[str, str]] = []
    for raw_line in source.splitlines():
        line = raw_line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if "|" in line:
            address, _, action = line.partition("|")
            _append(result, address, action)
            continue
        clash = _parse_clash_rule(line)
        if clash:
            result.append(clash)
        else:
            _append(result, line, "proxy")
    return result
